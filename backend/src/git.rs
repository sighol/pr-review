use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use tokio::process::Command;

const FIELD_SEP: char = '\u{1f}';

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub oid: String,
    pub subject: String,
}

async fn run(repo: &Path, args: &[&str]) -> Result<std::process::Output> {
    Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .await
        .with_context(|| format!("failed to run git {}", args.join(" ")))
}

async fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let out = run(repo, args).await?;
    if !out.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Resolves `rev` (hash, tag, branch, ...) to a full commit oid.
pub async fn resolve_commit(repo: &Path, rev: &str) -> Result<String> {
    let out = run(
        repo,
        &["rev-parse", "--verify", "--quiet", &format!("{rev}^{{commit}}")],
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
    let out = git(repo, &["log", "--reverse", &format, &format!("{base}..{head}")]).await?;
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

/// Lines added/removed per file between `base` and `head`. Binary files count as 0/0.
pub async fn numstat(repo: &Path, base: &str, head: &str) -> Result<BTreeMap<String, (usize, usize)>> {
    let out = git(repo, &["diff", "--numstat", "-z", "--no-renames", base, head]).await?;
    let mut stats = BTreeMap::new();
    for record in out.split('\0').filter(|r| !r.is_empty()) {
        let mut parts = record.splitn(3, '\t');
        let added = parts.next().unwrap_or("0").parse().unwrap_or(0);
        let removed = parts.next().unwrap_or("0").parse().unwrap_or(0);
        let path = parts.next().unwrap_or_default().to_string();
        stats.insert(path, (added, removed));
    }
    Ok(stats)
}

/// Paths touched by a single commit.
pub async fn files_in_commit(repo: &Path, oid: &str) -> Result<Vec<String>> {
    let out = git(
        repo,
        &["diff-tree", "--no-commit-id", "--name-only", "-r", "-z", "--no-renames", oid],
    )
    .await?;
    Ok(out.split('\0').filter(|p| !p.is_empty()).map(String::from).collect())
}

/// The file's content at `oid`; empty if it does not exist there.
pub async fn file_text(repo: &Path, oid: &str, path: &str) -> Result<String> {
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

/// Plain `git diff` of one path between two commits.
pub async fn file_diff(repo: &Path, from: &str, to: &str, path: &str) -> Result<String> {
    git(
        repo,
        &["diff", "--no-color", "--no-renames", "-U3", from, to, "--", path],
    )
    .await
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
            let dir = std::env::temp_dir().join(format!("pr-review-core-{}-{n}", std::process::id()));
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
            let dir = std::env::temp_dir().join(format!("pr-review-core-{}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            let out = std::process::Command::new("git")
                .arg("clone")
                .arg("-q")
                .arg(&other.dir)
                .arg(&dir)
                .output()
                .unwrap();
            assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
            Self { dir }
        }

        pub fn git(&self, args: &[&str]) -> String {
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(&self.dir)
                .args(args)
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
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
                Commit { oid: c1.clone(), subject: "change a".into() },
                Commit { oid: c2.clone(), subject: "add dir/b".into() },
            ]
        );

        let stats = numstat(repo.path(), &base, &c2).await.unwrap();
        assert_eq!(stats.get("a.txt"), Some(&(2, 1)));
        assert_eq!(stats.get("dir/b.txt"), Some(&(1, 0)));

        assert_eq!(files_in_commit(repo.path(), &c1).await.unwrap(), vec!["a.txt"]);
        assert_eq!(files_in_commit(repo.path(), &c2).await.unwrap(), vec!["dir/b.txt"]);
    }

    #[tokio::test]
    async fn file_text_and_diff() {
        let repo = TestRepo::new();
        let base = repo.commit("base", &[("a.txt", Some("1\n2\n"))]);
        let c1 = repo.commit("edit", &[("a.txt", Some("1\n2\n3\n")), ("new.txt", Some("n\n"))]);

        assert_eq!(file_text(repo.path(), &base, "a.txt").await.unwrap(), "1\n2\n");
        assert_eq!(file_text(repo.path(), &c1, "a.txt").await.unwrap(), "1\n2\n3\n");
        assert_eq!(file_text(repo.path(), &base, "new.txt").await.unwrap(), "");

        let diff = file_diff(repo.path(), &base, &c1, "a.txt").await.unwrap();
        assert!(diff.starts_with("diff --git a/a.txt b/a.txt"));
        assert!(diff.contains("\n+3\n"));
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
        assert_eq!(top.canonicalize().unwrap(), repo.path().canonicalize().unwrap());

        let outside = std::env::temp_dir().join(format!("pr-review-core-nogit-{}", std::process::id()));
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
        assert_eq!(range, PrRange { base: base.clone(), head: first });
        assert_eq!(commits_in_range(local.path(), &range.base, &range.head).await.unwrap().len(), 1);

        // A later push to the PR is picked up by the next fetch.
        remote.git(&["checkout", "-q", "feature"]);
        let second = remote.commit("more pr work", &[("a.txt", Some("3\n"))]);
        remote.git(&["update-ref", "refs/pull/7/head", &second]);
        let range = fetch_pr(local.path(), 7, "main").await.unwrap();
        assert_eq!(range, PrRange { base, head: second });

        assert!(fetch_pr(local.path(), 8, "main").await.is_err());
    }
}
