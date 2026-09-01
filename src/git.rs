use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use tokio::process::Command;

use crate::types::{Commit, PrFile};

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

/// Fetches the PR head from `origin` if it isn't already present locally.
pub async fn ensure_commits_available(repo_path: &Path, pr_number: u64, head_oid: &str) -> Result<()> {
    if commit_exists(repo_path, head_oid).await? {
        return Ok(());
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["fetch", "origin", &format!("pull/{pr_number}/head")])
        .output()
        .await
        .context("failed to run git fetch")?;
    if !output.status.success() {
        bail!(
            "git fetch failed while fetching PR #{pr_number}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
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
