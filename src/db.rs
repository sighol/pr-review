use std::path::PathBuf;

use anyhow::{Context, Result};
use sqlx::Row;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

use crate::types::Approvals;

pub struct ActivePr {
    pub pr_number: u64,
    pub base_oid: String,
    pub head_ref_name: String,
}

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

    sqlx::migrate!()
        .run(&pool)
        .await
        .context("failed to run database migrations")?;

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

pub async fn load_active_pr(pool: &SqlitePool) -> Result<Option<ActivePr>> {
    let row = sqlx::query("SELECT pr_number, base_oid, head_ref_name FROM active_pr WHERE id = 1")
        .fetch_optional(pool)
        .await
        .context("failed to load active PR")?;

    Ok(row.map(|row| ActivePr {
        pr_number: row.get::<i64, _>("pr_number") as u64,
        base_oid: row.get::<String, _>("base_oid"),
        head_ref_name: row.get::<String, _>("head_ref_name"),
    }))
}

pub async fn save_active_pr(
    pool: &SqlitePool,
    pr_number: u64,
    base_oid: &str,
    head_ref_name: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO active_pr (id, pr_number, base_oid, head_ref_name)
         VALUES (1, ?, ?, ?)
         ON CONFLICT(id)
         DO UPDATE SET pr_number = excluded.pr_number, base_oid = excluded.base_oid, head_ref_name = excluded.head_ref_name",
    )
    .bind(pr_number as i64)
    .bind(base_oid)
    .bind(head_ref_name)
    .execute(pool)
    .await
    .context("failed to save active PR")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn migrations_run_and_approvals_round_trip() {
        let path = std::env::temp_dir().join(format!("pr-review-test-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);

        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .connect_with(options)
            .await
            .unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();

        assert!(load_approvals(&pool, 1).await.unwrap().is_empty());

        upsert_approval(&pool, 1, "src/main.rs", "abc123").await.unwrap();
        let approvals = load_approvals(&pool, 1).await.unwrap();
        assert_eq!(approvals.get("src/main.rs"), Some(&"abc123".to_string()));

        // Re-approving the same file through a later commit updates in place.
        upsert_approval(&pool, 1, "src/main.rs", "def456").await.unwrap();
        let approvals = load_approvals(&pool, 1).await.unwrap();
        assert_eq!(approvals.len(), 1);
        assert_eq!(approvals.get("src/main.rs"), Some(&"def456".to_string()));

        // Approvals are scoped per PR number.
        assert!(load_approvals(&pool, 2).await.unwrap().is_empty());

        pool.close().await;
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn active_pr_round_trip_and_overwrite() {
        let path = std::env::temp_dir().join(format!("pr-review-test-active-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);

        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .connect_with(options)
            .await
            .unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();

        assert!(load_active_pr(&pool).await.unwrap().is_none());

        save_active_pr(&pool, 42, "base1", "feature-a").await.unwrap();
        let active = load_active_pr(&pool).await.unwrap().unwrap();
        assert_eq!(active.pr_number, 42);
        assert_eq!(active.base_oid, "base1");
        assert_eq!(active.head_ref_name, "feature-a");

        // Only one active PR is tracked at a time; saving again overwrites it.
        save_active_pr(&pool, 43, "base2", "feature-b").await.unwrap();
        let active = load_active_pr(&pool).await.unwrap().unwrap();
        assert_eq!(active.pr_number, 43);
        assert_eq!(active.base_oid, "base2");
        assert_eq!(active.head_ref_name, "feature-b");

        pool.close().await;
        let _ = std::fs::remove_file(&path);
    }
}
