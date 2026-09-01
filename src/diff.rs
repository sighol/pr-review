use std::io::{self, Write};
use std::path::Path;

use anyhow::Result;
use sqlx::SqlitePool;

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
    println!("{diff}");
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
