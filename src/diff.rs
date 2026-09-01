use std::io::{self, Write};
use std::path::Path;
use std::process::Stdio;

use anyhow::{Context, Result};
use sqlx::SqlitePool;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::types::{Approvals, Commit, PrFile, latest_commit_idx};
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
    let from_oid = approvals
        .get(&file.path)
        .map(String::as_str)
        .unwrap_or(base_oid);

    let diff = git::colored_diff(repo_path, from_oid, to_oid, &file.path).await?;
    page(&diff).await?;
    print!(
        "Approve this file through commit #{} (of {})? (y/n): ",
        latest_idx + 1,
        commits.len()
    );
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let approved = input.trim() == "y";
    if approved {
        db::upsert_approval(pool, pr_number, &file.path, to_oid).await?;
    }
    Ok(approved)
}

/// Pipes `content` through `$PAGER` (falling back to `less -R -F -X`, which
/// renders ANSI colors and skips paging when the content fits on one screen).
async fn page(content: &str) -> Result<()> {
    let pager_cmd = std::env::var("PAGER").unwrap_or_else(|_| "less -R -F -X".to_string());
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
