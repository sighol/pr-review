use std::path::PathBuf;

use anyhow::{Context, Result};
use sqlx::Row;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

use crate::types::Approvals;

pub async fn open(owner: &str, repo: &str) -> Result<SqlitePool> {
    let db_path = db_path(owner, repo)?;
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }

    let options = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .connect_with(options)
        .await
        .with_context(|| format!("failed to open database at {}", db_path.display()))?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS pr_review_state (
            id INTEGER PRIMARY KEY,
            pr_number INTEGER NOT NULL,
            file_path TEXT NOT NULL,
            approved_up_to_commit TEXT NOT NULL,
            approved_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            UNIQUE(pr_number, file_path)
        )",
    )
    .execute(&pool)
    .await
    .context("failed to create pr_review_state table")?;

    Ok(pool)
}

fn db_path(owner: &str, repo: &str) -> Result<PathBuf> {
    let home = dirs::home_dir().context("could not determine home directory")?;
    Ok(home
        .join(".cache")
        .join("pr-review")
        .join(owner)
        .join(repo)
        .join("reviews.db"))
}

pub async fn load_approvals(pool: &SqlitePool, pr_number: u64) -> Result<Approvals> {
    let rows = sqlx::query(
        "SELECT file_path, approved_up_to_commit FROM pr_review_state WHERE pr_number = ?",
    )
    .bind(pr_number as i64)
    .fetch_all(pool)
    .await
    .context("failed to load approvals")?;

    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("file_path"),
                row.get::<String, _>("approved_up_to_commit"),
            )
        })
        .collect())
}

pub async fn upsert_approval(
    pool: &SqlitePool,
    pr_number: u64,
    file_path: &str,
    commit_oid: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO pr_review_state (pr_number, file_path, approved_up_to_commit, approved_at)
         VALUES (?, ?, ?, CURRENT_TIMESTAMP)
         ON CONFLICT(pr_number, file_path)
         DO UPDATE SET approved_up_to_commit = excluded.approved_up_to_commit, approved_at = CURRENT_TIMESTAMP",
    )
    .bind(pr_number as i64)
    .bind(file_path)
    .bind(commit_oid)
    .execute(pool)
    .await
    .context("failed to upsert approval")?;
    Ok(())
}
