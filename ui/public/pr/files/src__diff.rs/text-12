use std::io::{self, Write};
use std::path::Path;
use std::process::Stdio;

use anyhow::{Context, Result};
use crossterm::event::KeyCode;
use sqlx::SqlitePool;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::interactive::read_key;
use crate::pager::page;
use crate::types::{Approvals, Commit, PrFile, last_approved_idx, latest_commit_idx};
use crate::{db, git};

/// Shows the diff for `file` (from its last approved commit, or the PR base,
/// through the latest commit that changed it) and prompts for approval.
/// Returns true if the user approved.
pub async fn show_diff_and_prompt(
    repo_path: &Path,
    pool: &SqlitePool,
    pr_number: u64,
    file: &PrFile,
    commits: &[Commit],
    base_oid: &str,
    approvals: &Approvals,
    side_by_side: bool,
) -> Result<bool> {
    let latest_idx = latest_commit_idx(file);
    let to_oid = &commits[latest_idx].oid;
    let from_oid = last_approved_idx(file, commits, approvals)
        .map(|idx| commits[idx].oid.as_str())
        .unwrap_or(base_oid);

    let diff = render_diff(repo_path, from_oid, to_oid, &file.path, side_by_side).await?;
    page(&diff).await?;
    println!();
    print!(
        "Approve this file through commit #{} (of {})? (y/n): ",
        latest_idx + 1,
        commits.len()
    );
    io::stdout().flush()?;

    let input = read_key().unwrap_or(KeyCode::Enter);
    if input == KeyCode::Char('q') {
        println!();
        std::process::exit(0);
    }
    let approved = input == KeyCode::Char('y');
    if approved {
        // Every commit that ever changed this file (not just the latest) gets
        // recorded, since approving "through" a commit approves all of them.
        let commit_oids: Vec<String> = file
            .commits_that_changed_it
            .iter()
            .map(|&idx| commits[idx].oid.clone())
            .collect();
        db::record_approvals(pool, pr_number, &file.path, &commit_oids).await?;
    }
    Ok(approved)
}

/// Renders the diff for `file_path` between two oids, running it through
/// `delta` for real syntax highlighting if it's installed, else falling back
/// to plain `git diff --color=always`.
async fn render_diff(
    repo_path: &Path,
    from_oid: &str,
    to_oid: &str,
    file_path: &str,
    side_by_side: bool,
) -> Result<String> {
    if delta_available().await {
        let raw = git::plain_diff(repo_path, from_oid, to_oid, file_path).await?;
        render_with_delta(repo_path, &raw, side_by_side).await
    } else {
        git::colored_diff(repo_path, from_oid, to_oid, file_path).await
    }
}

pub(crate) async fn delta_available() -> bool {
    Command::new("delta")
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|status| status.success())
        .unwrap_or(false)
}

pub(crate) async fn render_with_delta(
    repo_path: &Path,
    raw_diff: &str,
    side_by_side: bool,
) -> Result<String> {
    let mut cmd = Command::new("delta");
    cmd.arg("--no-gitconfig");
    cmd.current_dir(repo_path);
    if side_by_side {
        cmd.arg("--side-by-side");
        // delta's stdout is a pipe (captured into a String below), not the
        // real terminal, so it can't detect the actual width itself and
        // falls back to a narrow default, squeezing both panes into
        // something like 40 columns each. Tell it the real width.
        if let Ok((cols, _rows)) = crossterm::terminal::size() {
            cmd.arg("--width").arg(cols.to_string());
        }
    } else {
        cmd.arg("--color-only");
        cmd.arg("-n");
    }
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("failed to launch delta")?;

    if let Some(mut stdin) = child.stdin.take() {
        match stdin.write_all(raw_diff.as_bytes()).await {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::BrokenPipe => {}
            Err(e) => return Err(e).context("failed to write diff to delta"),
        }
    }

    let output = child
        .wait_with_output()
        .await
        .context("delta exited with an error")?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Renders a single review-comment diff hunk (as GitHub recorded it at
/// comment time) through delta if available, else returns the raw hunk text
/// unmodified — it already reads fine without color.
pub async fn render_hunk(
    repo_path: &Path,
    file_path: &str,
    diff_hunk: &str,
    side_by_side: bool,
) -> Result<String> {
    let synthetic = format!(
        "diff --git a/{file_path} b/{file_path}\n--- a/{file_path}\n+++ b/{file_path}\n{diff_hunk}\n"
    );
    if delta_available().await {
        render_with_delta(repo_path, &synthetic, side_by_side).await
    } else {
        Ok(synthetic)
    }
}
