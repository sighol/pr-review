use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use tokio::process::Command;
use tracing::{debug, warn};

const FIELD_SEP: char = '\u{1f}';

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub oid: String,
    pub subject: String,
}

/// Runs git. A non-zero exit is not an error here (some callers probe), only a failure to start is.
async fn run(repo: &Path, args: &[&str]) -> Result<std::process::Output> {
    let started = Instant::now();
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .await
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;
    debug!(
        "git {} -> {} in {:?}",
        args.join(" "),
        out.status,
        started.elapsed()
    );
    Ok(out)
}

async fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let out = run(repo, args).await?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        warn!(
            "git {} failed ({}): {}",
            args.join(" "),
            out.status,
            stderr.trim()
        );
        bail!("git {} failed: {}", args.join(" "), stderr.trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Resolves `rev` (hash, tag, branch, ...) to a full commit oid.
pub async fn resolve_commit(repo: &Path, rev: &str) -> Result<String> {
    let out = run(
        repo,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{rev}^{{commit}}"),
        ],
    )
    .await?;
    if !out.status.success() {
        bail!("'{rev}' is not a valid commit in this repo");
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub async fn merge_base(repo: &Path, a: &str, b: &str) -> Result<String> {
    Ok(git(repo, &["merge-base", a, b]).await?.trim().to_string())
}

/// Commits in `base..head`, oldest first.
pub async fn commits_in_range(repo: &Path, base: &str, head: &str) -> Result<Vec<Commit>> {
    let format = format!("--format=%H{FIELD_SEP}%s");
    let out = git(
        repo,
        &["log", "--reverse", &format, &format!("{base}..{head}")],
    )
    .await?;
    Ok(out
        .lines()
        .map(|line| {
            let (oid, subject) = line.split_once(FIELD_SEP).unwrap_or((line, ""));
            Commit {
                oid: oid.to_string(),
                subject: subject.to_string(),
            }
        })
        .collect())
}

/// What changed in one file between the PR base and head.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStat {
    pub added: usize,
    pub removed: usize,
    /// The file's path at the base when git detected it was renamed (the key is the head path).
    pub old_path: Option<String>,
}

/// Lines added/removed per file between `base` and `head`, keyed by the head path, with renames
/// detected by git (`-M`, like GitHub does for the whole PR). Binary files count as 0/0.
pub async fn numstat(repo: &Path, base: &str, head: &str) -> Result<BTreeMap<String, FileStat>> {
    let out = git(repo, &["diff", "--numstat", "-z", "-M", base, head]).await?;
    let mut stats = BTreeMap::new();
    // With -z a rename is `added\tremoved\t\0old\0new\0`; anything else is `added\tremoved\tpath\0`.
    let mut records = out.split('\0').filter(|r| !r.is_empty());
    while let Some(record) = records.next() {
        let mut parts = record.splitn(3, '\t');
        let added = parts.next().unwrap_or("0").parse().unwrap_or(0);
        let removed = parts.next().unwrap_or("0").parse().unwrap_or(0);
        let path = parts.next().unwrap_or_default();
        let (path, old_path) = if path.is_empty() {
            let old = records.next().unwrap_or_default();
            let new = records.next().unwrap_or_default();
            (new.to_string(), Some(old.to_string()))
        } else {
            (path.to_string(), None)
        };
        stats.insert(
            path,
            FileStat {
                added,
                removed,
                old_path,
            },
        );
    }
    Ok(stats)
}

/// Paths touched by a single commit.
pub async fn files_in_commit(repo: &Path, oid: &str) -> Result<Vec<String>> {
    let out = git(
        repo,
        &[
            "diff-tree",
            "--no-commit-id",
            "--name-only",
            "-r",
            "-z",
            "--no-renames",
            oid,
        ],
    )
    .await?;
    Ok(out
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(String::from)
        .collect())
}

async fn exists_at(repo: &Path, oid: &str, path: &str) -> bool {
    match run(repo, &["cat-file", "-e", &format!("{oid}:{path}")]).await {
        Ok(out) => out.status.success(),
        Err(_) => false,
    }
}

/// The path to read at `oid` for a file whose head path is `path`: `old_path` (its path at the
/// PR base) when the file was renamed and only exists under the old name at `oid`.
async fn path_at<'a>(repo: &Path, oid: &str, path: &'a str, old_path: Option<&'a str>) -> &'a str {
    match old_path {
        Some(old) if !exists_at(repo, oid, path).await && exists_at(repo, oid, old).await => old,
        _ => path,
    }
}

/// The file's content at `oid`; empty if it does not exist there. For a renamed file pass its
/// `old_path`, which is read where the new name does not exist yet.
pub async fn file_text(
    repo: &Path,
    oid: &str,
    path: &str,
    old_path: Option<&str>,
) -> Result<String> {
    let path = path_at(repo, oid, path, old_path).await;
    let out = run(repo, &["show", &format!("{oid}:{path}")]).await?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    if stderr.contains("does not exist in") || stderr.contains("exists on disk, but not in") {
        return Ok(String::new());
    }
    bail!("git show {oid}:{path} failed: {}", stderr.trim());
}

/// `git diff` of one file between two commits. When the file is under a different name at each
/// end (see `file_text`), the two versions are compared directly, so a rename with edits is one
/// diff of the old content against the new rather than a deletion plus an addition.
pub async fn file_diff(
    repo: &Path,
    from: &str,
    to: &str,
    path: &str,
    old_path: Option<&str>,
) -> Result<String> {
    let from_path = path_at(repo, from, path, old_path).await;
    let path = path_at(repo, to, path, old_path).await;
    if from_path != path {
        return git(
            repo,
            &[
                "diff",
                "--no-color",
                "-U3",
                &format!("{from}:{from_path}"),
                &format!("{to}:{path}"),
            ],
        )
        .await;
    }
    git(
        repo,
        &[
            "diff",
            "--no-color",
            "--no-renames",
            "-U3",
            from,
            to,
            "--",
            path,
        ],
    )
    .await
}

/// Whether `path` differs between two commits. A commit that cannot be read (for example one
/// that was force-pushed away) counts as changed, since we cannot tell.
pub async fn file_changed(repo: &Path, from: &str, to: &str, path: &str) -> bool {
    match run(repo, &["diff", "--quiet", from, to, "--", path]).await {
        Ok(out) => !out.status.success(),
        Err(_) => true,
    }
}

/// Root of the git work tree containing `dir`.
pub async fn toplevel(dir: &Path) -> Result<PathBuf> {
    let out = run(dir, &["rev-parse", "--show-toplevel"]).await?;
    if !out.status.success() {
        bail!("{} is not inside a git repository", dir.display());
    }
    Ok(PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()))
}

/// GitHub `(owner, repo)` of the `origin` remote.
pub async fn origin_owner_repo(repo: &Path) -> Result<(String, String)> {
    let out = run(repo, &["remote", "get-url", "origin"]).await?;
    if !out.status.success() {
        bail!("this repo has no 'origin' remote");
    }
    let url = String::from_utf8_lossy(&out.stdout).trim().to_string();
    parse_github_owner_repo(&url)
        .with_context(|| format!("could not find a GitHub owner/repo in the origin URL '{url}'"))
}

fn parse_github_owner_repo(url: &str) -> Option<(String, String)> {
    let trimmed = url.strip_suffix(".git").unwrap_or(url);
    let idx = trimmed.find("github.com")?;
    let rest = trimmed[idx + "github.com".len()..]
        .trim_start_matches(':')
        .trim_start_matches('/');
    let (owner, repo) = rest.split_once('/')?;
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner.to_string(), repo.to_string()))
}

/// The commits a PR spans: `base` is the merge-base of the base branch and the PR head.
#[derive(Debug, PartialEq, Eq)]
pub struct PrRange {
    pub base: String,
    pub head: String,
}

/// Fetches the PR head (`refs/pull/N/head`, which also exists for fork PRs) and its base
/// branch from `origin`, then resolves the range. The head is stored under
/// `refs/pr-review/N/head` so it never touches the user's branches.
pub async fn fetch_pr(repo: &Path, number: u64, base_ref_name: &str) -> Result<PrRange> {
    let head_ref = format!("refs/pr-review/{number}/head");
    let base_ref = format!("refs/remotes/origin/{base_ref_name}");
    git(
        repo,
        &[
            "fetch",
            "--no-tags",
            "origin",
            &format!("+refs/pull/{number}/head:{head_ref}"),
            &format!("+refs/heads/{base_ref_name}:{base_ref}"),
        ],
    )
    .await?;
    let head = resolve_commit(repo, &head_ref).await?;
    let base = merge_base(repo, &base_ref, &head).await?;
    Ok(PrRange { base, head })
}

#[cfg(test)]
pub(crate) mod testutil {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    pub struct TestRepo {
        pub dir: PathBuf,
    }

    impl TestRepo {
        pub fn new() -> Self {
            let n = COUNTER.fetch_add(1, Ordering::SeqCst);
            let dir =
                std::env::temp_dir().join(format!("pr-review-core-{}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let repo = Self { dir };
            repo.git(&["init", "-q"]);
            repo.git(&["config", "user.email", "test@example.com"]);
            repo.git(&["config", "user.name", "Test"]);
            repo
        }

        pub fn path(&self) -> &Path {
            &self.dir
        }

        /// A clone of `other`, with `other` as its `origin`.
        pub fn clone_of(other: &TestRepo) -> Self {
            let n = COUNTER.fetch_add(1, Ordering::SeqCst);
            let dir =
                std::env::temp_dir().join(format!("pr-review-core-{}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            let out = std::process::Command::new("git")
                .arg("clone")
                .arg("-q")
                .arg(&other.dir)
                .arg(&dir)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            Self { dir }
        }

        pub fn git(&self, args: &[&str]) -> String {
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(&self.dir)
                .args(args)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        }

        /// Writes the files (a `None` content deletes the file), commits, and returns the new oid.
        pub fn commit(&self, message: &str, files: &[(&str, Option<&str>)]) -> String {
            for (path, content) in files {
                let full = self.dir.join(path);
                match content {
                    Some(c) => {
                        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
                        std::fs::write(full, c).unwrap();
                    }
                    None => std::fs::remove_file(full).unwrap(),
                }
            }
            self.git(&["add", "-A"]);
            self.git(&["commit", "-q", "-m", message]);
            self.git(&["rev-parse", "HEAD"])
        }
    }

    impl Drop for TestRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::TestRepo;
    use super::*;

    #[tokio::test]
    async fn commits_numstat_and_per_commit_files() {
        let repo = TestRepo::new();
        let base = repo.commit("base", &[("a.txt", Some("1\n2\n3\n"))]);
        let c1 = repo.commit("change a", &[("a.txt", Some("1\nx\n3\n4\n"))]);
        let c2 = repo.commit("add dir/b", &[("dir/b.txt", Some("hi\n"))]);

        let commits = commits_in_range(repo.path(), &base, &c2).await.unwrap();
        assert_eq!(
            commits,
            vec![
                Commit {
                    oid: c1.clone(),
                    subject: "change a".into()
                },
                Commit {
                    oid: c2.clone(),
                    subject: "add dir/b".into()
                },
            ]
        );

        let stats = numstat(repo.path(), &base, &c2).await.unwrap();
        let stat = |added, removed| FileStat {
            added,
            removed,
            old_path: None,
        };
        assert_eq!(stats.get("a.txt"), Some(&stat(2, 1)));
        assert_eq!(stats.get("dir/b.txt"), Some(&stat(1, 0)));

        assert_eq!(
            files_in_commit(repo.path(), &c1).await.unwrap(),
            vec!["a.txt"]
        );
        assert_eq!(
            files_in_commit(repo.path(), &c2).await.unwrap(),
            vec!["dir/b.txt"]
        );
    }

    #[tokio::test]
    async fn file_changed_between_commits() {
        let repo = TestRepo::new();
        let c1 = repo.commit("one", &[("a.txt", Some("1\n")), ("b.txt", Some("b\n"))]);
        let c2 = repo.commit("two", &[("a.txt", Some("2\n"))]);
        assert!(file_changed(repo.path(), &c1, &c2, "a.txt").await);
        assert!(!file_changed(repo.path(), &c1, &c2, "b.txt").await);
        assert!(
            file_changed(
                repo.path(),
                "0000000000000000000000000000000000000000",
                &c2,
                "b.txt"
            )
            .await
        );
        assert!(file_changed(repo.path(), "", &c2, "b.txt").await);
    }

    #[tokio::test]
    async fn file_text_and_diff() {
        let repo = TestRepo::new();
        let base = repo.commit("base", &[("a.txt", Some("1\n2\n"))]);
        let c1 = repo.commit(
            "edit",
            &[("a.txt", Some("1\n2\n3\n")), ("new.txt", Some("n\n"))],
        );

        assert_eq!(
            file_text(repo.path(), &base, "a.txt", None).await.unwrap(),
            "1\n2\n"
        );
        assert_eq!(
            file_text(repo.path(), &c1, "a.txt", None).await.unwrap(),
            "1\n2\n3\n"
        );
        assert_eq!(
            file_text(repo.path(), &base, "new.txt", None)
                .await
                .unwrap(),
            ""
        );

        let diff = file_diff(repo.path(), &base, &c1, "a.txt", None)
            .await
            .unwrap();
        assert!(diff.starts_with("diff --git a/a.txt b/a.txt"));
        assert!(diff.contains("\n+3\n"));
    }

    /// base: old.txt (10 lines). c1 renames it to new.txt and edits one line. c2 edits it again.
    fn renamed_repo() -> (TestRepo, String, String, String) {
        let repo = TestRepo::new();
        let text: String = (1..=10).map(|i| format!("line {i}\n")).collect();
        let base = repo.commit(
            "base",
            &[("old.txt", Some(&text)), ("keep.txt", Some("k\n"))],
        );
        let renamed = text.replace("line 5\n", "five\n");
        let c1 = repo.commit("rename", &[("old.txt", None), ("new.txt", Some(&renamed))]);
        let c2 = repo.commit(
            "edit",
            &[("new.txt", Some(&renamed.replace("line 9\n", "nine\n")))],
        );
        (repo, base, c1, c2)
    }

    #[tokio::test]
    async fn numstat_detects_a_rename_with_edits() {
        let (repo, base, _, c2) = renamed_repo();
        let stats = numstat(repo.path(), &base, &c2).await.unwrap();
        assert_eq!(stats.keys().collect::<Vec<_>>(), ["new.txt"]);
        assert_eq!(
            stats["new.txt"],
            FileStat {
                added: 2,
                removed: 2,
                old_path: Some("old.txt".into())
            }
        );
    }

    #[tokio::test]
    async fn a_renamed_file_is_read_under_its_old_name_before_the_rename() {
        let (repo, base, c1, _) = renamed_repo();
        let r = repo.path();
        let old = Some("old.txt");
        let at_base = file_text(r, &base, "new.txt", old).await.unwrap();
        assert!(at_base.contains("line 5\n"));
        let at_c1 = file_text(r, &c1, "new.txt", old).await.unwrap();
        assert!(at_c1.contains("five\n") && !at_c1.contains("line 5\n"));
        // Without the old name the file does not exist at the base.
        assert_eq!(file_text(r, &base, "new.txt", None).await.unwrap(), "");
    }

    #[tokio::test]
    async fn a_diff_across_a_rename_is_one_diff_of_old_against_new() {
        let (repo, base, c1, c2) = renamed_repo();
        let r = repo.path();
        let old = Some("old.txt");

        let across = file_diff(r, &base, &c2, "new.txt", old).await.unwrap();
        assert!(across.contains("-line 5\n") && across.contains("+five\n"));
        assert!(across.contains("-line 9\n") && across.contains("+nine\n"));
        assert!(
            !across.contains("-line 1\n"),
            "the whole file must not show as removed"
        );

        // Both ends after the rename: a plain diff of the new name.
        let after = file_diff(r, &c1, &c2, "new.txt", old).await.unwrap();
        assert!(after.starts_with("diff --git a/new.txt b/new.txt"));
        assert!(
            after.contains("+nine\n") && !after.contains("+five\n") && !after.contains("-five\n")
        );
    }

    #[tokio::test]
    async fn resolve_commit_and_merge_base() {
        let repo = TestRepo::new();
        let base = repo.commit("base", &[("a.txt", Some("1\n"))]);
        let tip = repo.commit("tip", &[("a.txt", Some("2\n"))]);

        assert_eq!(resolve_commit(repo.path(), &tip[..8]).await.unwrap(), tip);
        assert!(resolve_commit(repo.path(), "not-a-commit").await.is_err());
        assert_eq!(merge_base(repo.path(), &base, &tip).await.unwrap(), base);
    }

    #[test]
    fn parses_origin_urls() {
        let ok = |url| parse_github_owner_repo(url);
        let expected = Some(("cognitedata".to_string(), "pr-review".to_string()));
        assert_eq!(ok("git@github.com:cognitedata/pr-review.git"), expected);
        assert_eq!(ok("https://github.com/cognitedata/pr-review.git"), expected);
        assert_eq!(ok("https://github.com/cognitedata/pr-review"), expected);
        assert_eq!(ok("git@gitlab.com:cognitedata/pr-review.git"), None);
    }

    #[tokio::test]
    async fn toplevel_from_a_subdirectory_and_outside_a_repo() {
        let repo = TestRepo::new();
        repo.commit("x", &[("sub/a.txt", Some("1\n"))]);
        let top = toplevel(&repo.path().join("sub")).await.unwrap();
        assert_eq!(
            top.canonicalize().unwrap(),
            repo.path().canonicalize().unwrap()
        );

        let outside =
            std::env::temp_dir().join(format!("pr-review-core-nogit-{}", std::process::id()));
        std::fs::create_dir_all(&outside).unwrap();
        assert!(toplevel(&outside).await.is_err());
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[tokio::test]
    async fn fetch_pr_gets_head_and_merge_base() {
        let remote = TestRepo::new();
        remote.git(&["symbolic-ref", "HEAD", "refs/heads/main"]);
        let base = remote.commit("base", &[("a.txt", Some("1\n"))]);
        remote.git(&["checkout", "-q", "-b", "feature"]);
        let first = remote.commit("pr work", &[("a.txt", Some("2\n"))]);
        remote.git(&["update-ref", "refs/pull/7/head", &first]);
        remote.git(&["checkout", "-q", "main"]);
        remote.commit("main moves on", &[("b.txt", Some("b\n"))]);

        let local = TestRepo::clone_of(&remote);
        let range = fetch_pr(local.path(), 7, "main").await.unwrap();
        assert_eq!(
            range,
            PrRange {
                base: base.clone(),
                head: first
            }
        );
        assert_eq!(
            commits_in_range(local.path(), &range.base, &range.head)
                .await
                .unwrap()
                .len(),
            1
        );

        // A later push to the PR is picked up by the next fetch.
        remote.git(&["checkout", "-q", "feature"]);
        let second = remote.commit("more pr work", &[("a.txt", Some("3\n"))]);
        remote.git(&["update-ref", "refs/pull/7/head", &second]);
        let range = fetch_pr(local.path(), 7, "main").await.unwrap();
        assert_eq!(range, PrRange { base, head: second });

        assert!(fetch_pr(local.path(), 8, "main").await.is_err());
    }
}
