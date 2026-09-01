use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use chrono::Utc;
use tokio::process::Command;

use crate::types::{Commit, PrFile};

const FIELD_SEP: char = '\u{1f}';

pub async fn fetch_origin(repo_path: &Path) -> Result<()> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["fetch", "origin"])
        .output()
        .await
        .context("failed to run git fetch origin")?;
    if !output.status.success() {
        bail!(
            "git fetch origin failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

/// Errors out with a helpful message if `base_oid`/`head_oid` aren't present
/// locally after `fetch_origin` (e.g. the PR's branch lives on a fork).
pub async fn ensure_commits_available(repo_path: &Path, base_oid: &str, head_oid: &str) -> Result<()> {
    for oid in [base_oid, head_oid] {
        if !commit_exists(repo_path, oid).await? {
            bail!(
                "commit {oid} was not found locally after `git fetch origin`. \
                 If this PR's branch lives on a fork, add that fork as a remote, \
                 fetch it, then re-run."
            );
        }
    }
    Ok(())
}

async fn commit_exists(repo_path: &Path, oid: &str) -> Result<bool> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["cat-file", "-e", oid])
        .output()
        .await
        .context("failed to run git cat-file")?;
    Ok(output.status.success())
}

/// Resolves `branch` to the commit it currently points to locally, without
/// any network access: tries the local branch first, then `origin/<branch>`.
pub async fn resolve_branch_oid(repo_path: &Path, branch: &str) -> Result<String> {
    for candidate in [format!("refs/heads/{branch}"), format!("refs/remotes/origin/{branch}")] {
        if let Some(oid) = rev_parse(repo_path, &candidate).await? {
            return Ok(oid);
        }
    }
    bail!(
        "could not resolve branch '{branch}' locally (checked refs/heads/{branch} and \
         refs/remotes/origin/{branch}). Run with --fetch, or check out the branch first."
    );
}

async fn rev_parse(repo_path: &Path, rev: &str) -> Result<Option<String>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["rev-parse", "--verify", "--quiet", rev])
        .output()
        .await
        .context("failed to run git rev-parse")?;
    if !output.status.success() {
        return Ok(None);
    }
    Ok(Some(String::from_utf8_lossy(&output.stdout).trim().to_string()))
}

/// Resolves `rev` (a hash, tag, branch, etc.) to a full commit oid, for use
/// with `--commit`.
pub async fn resolve_commit(repo_path: &Path, rev: &str) -> Result<String> {
    rev_parse(repo_path, &format!("{rev}^{{commit}}"))
        .await?
        .with_context(|| format!("'{rev}' is not a valid commit in this repo"))
}

/// Errors out unless `oid` is `ancestor` or a descendant of it — used to
/// sanity-check that `--commit` actually falls within the PR's range.
pub async fn ensure_is_descendant(repo_path: &Path, ancestor: &str, oid: &str) -> Result<()> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["merge-base", "--is-ancestor", ancestor, oid])
        .output()
        .await
        .context("failed to run git merge-base --is-ancestor")?;
    if !output.status.success() {
        bail!("commit {oid} is not a descendant of the PR base ({ancestor}) — is it really part of this PR?");
    }
    Ok(())
}

/// Derives the GitHub `owner/repo` from the `origin` remote, so a PR
/// reference can be omitted once a repo has an active PR stored.
pub async fn origin_owner_repo(repo_path: &Path) -> Result<(String, String)> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["remote", "get-url", "origin"])
        .output()
        .await
        .context("failed to run git remote get-url origin")?;
    if !output.status.success() {
        bail!(
            "no 'origin' remote configured for this repo: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    parse_github_owner_repo(&url).with_context(|| format!("could not parse owner/repo out of origin URL '{url}'"))
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

/// All commits between `base_oid` (exclusive) and `head_oid` (inclusive), oldest first.
pub async fn commits_in_range(repo_path: &Path, base_oid: &str, head_oid: &str) -> Result<Vec<Commit>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args([
            "log",
            "--reverse",
            &format!("--format=%H{FIELD_SEP}%s{FIELD_SEP}%cI"),
            &format!("{base_oid}..{head_oid}"),
        ])
        .output()
        .await
        .context("failed to run git log")?;
    if !output.status.success() {
        bail!("git log failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut commits = Vec::new();
    for line in stdout.lines() {
        let mut parts = line.split(FIELD_SEP);
        let oid = parts.next().unwrap_or_default().to_string();
        let message = parts.next().unwrap_or_default().to_string();
        let committed_date = parts
            .next()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);
        commits.push(Commit {
            oid,
            message,
            committed_date,
        });
    }
    Ok(commits)
}

/// All file paths changed between `base_oid` and `head_oid`.
pub async fn changed_files_in_range(
    repo_path: &Path,
    base_oid: &str,
    head_oid: &str,
) -> Result<Vec<String>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["diff", "--name-only", &format!("{base_oid}..{head_oid}")])
        .output()
        .await
        .context("failed to run git diff --name-only")?;
    if !output.status.success() {
        bail!(
            "git diff --name-only failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|s| s.to_string())
        .collect())
}

/// Lines added/removed per file across `base_oid..head_oid`. Binary files
/// (reported by git as `-\t-\tpath`) are counted as 0/0.
pub async fn diffstat_in_range(
    repo_path: &Path,
    base_oid: &str,
    head_oid: &str,
) -> Result<HashMap<String, (usize, usize)>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["diff", "--numstat", &format!("{base_oid}..{head_oid}")])
        .output()
        .await
        .context("failed to run git diff --numstat")?;
    if !output.status.success() {
        bail!(
            "git diff --numstat failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let mut stats = HashMap::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let mut parts = line.splitn(3, '\t');
        let added: usize = parts.next().unwrap_or("0").parse().unwrap_or(0);
        let removed: usize = parts.next().unwrap_or("0").parse().unwrap_or(0);
        let path = parts.next().unwrap_or_default().to_string();
        stats.insert(path, (added, removed));
    }
    Ok(stats)
}

/// For each commit (in order), find which files it changed, then build a
/// `PrFile` per known file path with the indices of commits that touched it.
pub async fn build_pr_files(
    repo_path: &Path,
    commits: &[Commit],
    file_paths: &[String],
    diffstat: &HashMap<String, (usize, usize)>,
) -> Result<Vec<PrFile>> {
    let mut changed_by_path: HashMap<&str, Vec<usize>> = HashMap::new();

    for (idx, commit) in commits.iter().enumerate() {
        let changed = diff_tree_names(repo_path, &commit.oid).await?;
        for path in &changed {
            if let Some(known) = file_paths.iter().find(|p| p.as_str() == path.as_str()) {
                changed_by_path.entry(known.as_str()).or_default().push(idx);
            }
        }
    }

    Ok(file_paths
        .iter()
        .map(|path| {
            let (added, removed) = diffstat.get(path.as_str()).copied().unwrap_or((0, 0));
            PrFile {
                path: path.clone(),
                commits_that_changed_it: changed_by_path
                    .get(path.as_str())
                    .cloned()
                    .unwrap_or_default(),
                added,
                removed,
            }
        })
        .collect())
}

async fn diff_tree_names(repo_path: &Path, oid: &str) -> Result<Vec<String>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["diff-tree", "--no-commit-id", "--name-only", "-r", oid])
        .output()
        .await
        .context("failed to run git diff-tree")?;
    if !output.status.success() {
        bail!(
            "git diff-tree failed for {oid}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|s| s.to_string())
        .collect())
}

/// Colored `git diff` between two commit oids, restricted to a single path.
pub async fn colored_diff(
    repo_path: &Path,
    from_oid: &str,
    to_oid: &str,
    file_path: &str,
) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["diff", "--color=always", from_oid, to_oid, "--", file_path])
        .output()
        .await
        .context("failed to run git diff")?;
    if !output.status.success() {
        bail!("git diff failed: {}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ssh_origin_url() {
        assert_eq!(
            parse_github_owner_repo("git@github.com:anthropic/anthropic-sdk-python.git"),
            Some(("anthropic".to_string(), "anthropic-sdk-python".to_string()))
        );
    }

    #[test]
    fn parses_https_origin_url() {
        assert_eq!(
            parse_github_owner_repo("https://github.com/kubernetes/kubernetes.git"),
            Some(("kubernetes".to_string(), "kubernetes".to_string()))
        );
        assert_eq!(
            parse_github_owner_repo("https://github.com/kubernetes/kubernetes"),
            Some(("kubernetes".to_string(), "kubernetes".to_string()))
        );
    }

    #[test]
    fn rejects_non_github_url() {
        assert_eq!(parse_github_owner_repo("git@gitlab.com:owner/repo.git"), None);
    }

    #[tokio::test]
    async fn diffstat_in_range_parses_numstat_output() {
        let dir = std::env::temp_dir().join(format!("pr-review-git-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let run = |args: &[&str]| {
            let status = std::process::Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(args)
                .status()
                .unwrap();
            assert!(status.success());
        };
        let rev_parse_head = || {
            let output = std::process::Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(["rev-parse", "HEAD"])
                .output()
                .unwrap();
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        };

        run(&["init", "-q"]);
        run(&["config", "user.email", "test@example.com"]);
        run(&["config", "user.name", "Test"]);

        std::fs::write(dir.join("a.txt"), "line1\nline2\nline3\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "base"]);
        let base_oid = rev_parse_head();

        std::fs::write(dir.join("a.txt"), "line1\nchanged\nline3\nline4\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "change"]);
        let head_oid = rev_parse_head();

        let stats = diffstat_in_range(&dir, &base_oid, &head_oid).await.unwrap();
        assert_eq!(stats.get("a.txt").copied(), Some((2, 1)));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn resolve_commit_and_ancestry_check() {
        let dir = std::env::temp_dir().join(format!("pr-review-git-test-ancestry-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let run = |args: &[&str]| {
            let status = std::process::Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(args)
                .status()
                .unwrap();
            assert!(status.success());
        };
        let rev_parse_head = || {
            let output = std::process::Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(["rev-parse", "HEAD"])
                .output()
                .unwrap();
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        };

        run(&["init", "-q"]);
        run(&["config", "user.email", "test@example.com"]);
        run(&["config", "user.name", "Test"]);

        std::fs::write(dir.join("a.txt"), "base\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "base"]);
        let base_oid = rev_parse_head();

        std::fs::write(dir.join("a.txt"), "mid\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "mid"]);
        let mid_oid = rev_parse_head();

        let resolved = resolve_commit(&dir, &mid_oid[..8]).await.unwrap();
        assert_eq!(resolved, mid_oid);

        assert!(resolve_commit(&dir, "not-a-commit").await.is_err());

        ensure_is_descendant(&dir, &base_oid, &mid_oid).await.unwrap();
        assert!(ensure_is_descendant(&dir, &mid_oid, &base_oid).await.is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
