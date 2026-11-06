use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use sqlx::Row;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

use crate::types::Approvals;

pub struct ActivePr {
    pub pr_number: u64,
    pub base_ref_name: String,
    pub head_ref_name: String,
    pub title: String,
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
    let rows =
        sqlx::query("SELECT file_path, commit_oid FROM file_commit_approvals WHERE pr_number = ?")
            .bind(pr_number as i64)
            .fetch_all(pool)
            .await
            .context("failed to load approvals")?;

    let mut approvals = Approvals::new();
    for row in rows {
        approvals
            .entry(row.get::<String, _>("file_path"))
            .or_default()
            .insert(row.get::<String, _>("commit_oid"));
    }
    Ok(approvals)
}

/// Records every commit in `commit_oids` as approved for `file_path` — i.e.
/// approving "through" a commit approves every change up to it, not just the
/// latest one.
pub async fn record_approvals(
    pool: &SqlitePool,
    pr_number: u64,
    file_path: &str,
    commit_oids: &[String],
) -> Result<()> {
    let mut tx = pool.begin().await.context("failed to start transaction")?;
    for commit_oid in commit_oids {
        sqlx::query(
            "INSERT INTO file_commit_approvals (pr_number, file_path, commit_oid)
             VALUES (?, ?, ?)
             ON CONFLICT(pr_number, file_path, commit_oid) DO NOTHING",
        )
        .bind(pr_number as i64)
        .bind(file_path)
        .bind(commit_oid)
        .execute(&mut *tx)
        .await
        .context("failed to record approval")?;
    }
    tx.commit().await.context("failed to commit approvals")?;
    Ok(())
}

pub async fn load_active_pr(pool: &SqlitePool) -> Result<Option<ActivePr>> {
    let row = sqlx::query(
        "SELECT pr_number, base_ref_name, head_ref_name, title FROM active_pr WHERE id = 1",
    )
    .fetch_optional(pool)
    .await
    .context("failed to load active PR")?;

    Ok(row.map(|row| ActivePr {
        pr_number: row.get::<i64, _>("pr_number") as u64,
        base_ref_name: row.get::<String, _>("base_ref_name"),
        head_ref_name: row.get::<String, _>("head_ref_name"),
        title: row.get::<String, _>("title"),
    }))
}

pub async fn save_active_pr(
    pool: &SqlitePool,
    pr_number: u64,
    base_ref_name: &str,
    head_ref_name: &str,
    title: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO active_pr (id, pr_number, base_ref_name, head_ref_name, title)
         VALUES (1, ?, ?, ?, ?)
         ON CONFLICT(id)
         DO UPDATE SET pr_number = excluded.pr_number, base_ref_name = excluded.base_ref_name, head_ref_name = excluded.head_ref_name, title = excluded.title",
    )
    .bind(pr_number as i64)
    .bind(base_ref_name)
    .bind(head_ref_name)
    .bind(title)
    .execute(pool)
    .await
    .context("failed to save active PR")?;
    Ok(())
}

/// Loads this PR's locally-tracked discussion resolutions: discussion id ->
/// the id of the comment it was last resolved through.
pub async fn load_thread_resolutions(
    pool: &SqlitePool,
    pr_number: u64,
) -> Result<HashMap<String, String>> {
    let rows = sqlx::query(
        "SELECT thread_id, resolved_comment_id FROM thread_resolutions WHERE pr_number = ?",
    )
    .bind(pr_number as i64)
    .fetch_all(pool)
    .await
    .context("failed to load thread resolutions")?;

    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("thread_id"),
                row.get::<String, _>("resolved_comment_id"),
            )
        })
        .collect())
}

/// Records that `thread_id` has been resolved (by us, locally) through
/// `resolved_comment_id`, overwriting whatever was previously stored.
pub async fn save_thread_resolution(
    pool: &SqlitePool,
    pr_number: u64,
    thread_id: &str,
    resolved_comment_id: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO thread_resolutions (pr_number, thread_id, resolved_comment_id)
         VALUES (?, ?, ?)
         ON CONFLICT(pr_number, thread_id)
         DO UPDATE SET resolved_comment_id = excluded.resolved_comment_id, resolved_at = CURRENT_TIMESTAMP",
    )
    .bind(pr_number as i64)
    .bind(thread_id)
    .bind(resolved_comment_id)
    .execute(pool)
    .await
    .context("failed to save thread resolution")?;
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

        record_approvals(&pool, 1, "src/main.rs", &["abc123".to_string()])
            .await
            .unwrap();
        let approvals = load_approvals(&pool, 1).await.unwrap();
        assert_eq!(
            approvals.get("src/main.rs"),
            Some(&std::collections::HashSet::from(["abc123".to_string()]))
        );

        // Approving through a later commit records every commit up to it,
        // accumulating with (not replacing) what was already recorded.
        record_approvals(
            &pool,
            1,
            "src/main.rs",
            &["abc123".to_string(), "def456".to_string()],
        )
        .await
        .unwrap();
        let approvals = load_approvals(&pool, 1).await.unwrap();
        assert_eq!(approvals.len(), 1);
        assert_eq!(
            approvals.get("src/main.rs"),
            Some(&std::collections::HashSet::from([
                "abc123".to_string(),
                "def456".to_string()
            ]))
        );

        // Approvals are scoped per PR number.
        assert!(load_approvals(&pool, 2).await.unwrap().is_empty());

        pool.close().await;
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn active_pr_round_trip_and_overwrite() {
        let path =
            std::env::temp_dir().join(format!("pr-review-test-active-{}.db", std::process::id()));
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

        save_active_pr(&pool, 42, "base1", "feature-a", "Title one")
            .await
            .unwrap();
        let active = load_active_pr(&pool).await.unwrap().unwrap();
        assert_eq!(active.pr_number, 42);
        assert_eq!(active.base_ref_name, "base1");
        assert_eq!(active.head_ref_name, "feature-a");
        assert_eq!(active.title, "Title one");

        // Only one active PR is tracked at a time; saving again overwrites it.
        save_active_pr(&pool, 43, "base2", "feature-b", "Title two")
            .await
            .unwrap();
        let active = load_active_pr(&pool).await.unwrap().unwrap();
        assert_eq!(active.pr_number, 43);
        assert_eq!(active.base_ref_name, "base2");
        assert_eq!(active.head_ref_name, "feature-b");
        assert_eq!(active.title, "Title two");

        pool.close().await;
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn thread_resolutions_round_trip_and_overwrite() {
        let path =
            std::env::temp_dir().join(format!("pr-review-test-threads-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);

        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .connect_with(options)
            .await
            .unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();

        assert!(load_thread_resolutions(&pool, 1).await.unwrap().is_empty());

        save_thread_resolution(&pool, 1, "thread-a", "comment-1")
            .await
            .unwrap();
        let resolutions = load_thread_resolutions(&pool, 1).await.unwrap();
        assert_eq!(resolutions.get("thread-a"), Some(&"comment-1".to_string()));

        // Re-resolving the same thread overwrites, not accumulates.
        save_thread_resolution(&pool, 1, "thread-a", "comment-2")
            .await
            .unwrap();
        let resolutions = load_thread_resolutions(&pool, 1).await.unwrap();
        assert_eq!(resolutions.len(), 1);
        assert_eq!(resolutions.get("thread-a"), Some(&"comment-2".to_string()));

        // Resolutions are scoped per PR number.
        assert!(load_thread_resolutions(&pool, 2).await.unwrap().is_empty());

        pool.close().await;
        let _ = std::fs::remove_file(&path);
    }
}
