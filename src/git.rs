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

/// For each commit (in order), find which files it changed, then build a
/// `PrFile` per known file path with the indices of commits that touched it.
pub async fn build_pr_files(
    repo_path: &Path,
    commits: &[Commit],
    file_paths: &[String],
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
        .map(|path| PrFile {
            path: path.clone(),
            commits_that_changed_it: changed_by_path
                .get(path.as_str())
                .cloned()
                .unwrap_or_default(),
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
