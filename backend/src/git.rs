use std::collections::BTreeMap;
use std::path::Path;

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
}
