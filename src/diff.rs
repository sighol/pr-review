use std::io::{self, Write};
use std::path::Path;
use std::process::Stdio;

use anyhow::{Context, Result};
use sqlx::SqlitePool;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

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
) -> Result<bool> {
    let latest_idx = latest_commit_idx(file);
    let to_oid = &commits[latest_idx].oid;
    let from_oid = last_approved_idx(file, commits, approvals)
        .map(|idx| commits[idx].oid.as_str())
        .unwrap_or(base_oid);

    let diff = render_diff(repo_path, from_oid, to_oid, &file.path).await?;
    page(&diff).await?;
    print!(
        "Approve this file through commit #{} (of {})? (y/n): ",
        latest_idx + 1,
        commits.len()
    );
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let approved = input.trim().eq_ignore_ascii_case("y");
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
) -> Result<String> {
    if delta_available().await {
        let raw = git::plain_diff(repo_path, from_oid, to_oid, file_path).await?;
        render_with_delta(repo_path, &raw).await
    } else {
        git::colored_diff(repo_path, from_oid, to_oid, file_path).await
    }
}

async fn delta_available() -> bool {
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

async fn render_with_delta(repo_path: &Path, raw_diff: &str) -> Result<String> {
    let mut child = Command::new("delta")
        .current_dir(repo_path)
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

/// Pipes `content` through `$PAGER` (falling back to `less -R -F`, which
/// renders ANSI colors and skips paging when the content fits on one screen).
/// Deliberately omits `-X`: it skips entering the terminal's alternate
/// screen, which is what most terminal emulators key off of to translate
/// mouse-wheel scrolling into arrow keys for the foreground app.
async fn page(content: &str) -> Result<()> {
    let pager_cmd = std::env::var("PAGER").unwrap_or_else(|_| "less -R -F".to_string());
    let mut parts = pager_cmd.split_whitespace();
    let program = parts.next().unwrap_or("less");
    let args: Vec<&str> = parts.collect();

    let mut child = Command::new(program)
        .args(&args)
        .stdin(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to launch pager '{pager_cmd}'"))?;

    if let Some(mut stdin) = child.stdin.take() {
        match stdin.write_all(content.as_bytes()).await {
            Ok(()) => {}
            // The pager (e.g. `less -F`) may exit as soon as it has enough to
            // fill the screen, closing its end of the pipe before we're done.
            Err(e) if e.kind() == io::ErrorKind::BrokenPipe => {}
            Err(e) => return Err(e).context("failed to write diff to pager"),
        }
    }

    child.wait().await.context("pager exited with an error")?;
    Ok(())
}
