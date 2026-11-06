use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrKey {
    pub owner: String,
    pub repo: String,
    pub number: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Approval {
    pub file_path: String,
    pub commit_oid: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub draft_key: String,
    pub file_path: String,
    /// "old" or "new".
    pub side: String,
    pub start_line: i64,
    pub end_line: i64,
    pub body: String,
    /// Ignored on save (the database stamps its own PR head); set on load.
    #[serde(default)]
    pub head_oid: String,
    /// Ignored on save like `head_oid`: the PR base the draft was written against.
    #[serde(default)]
    pub base_oid: String,
    /// Not stored; filled in on load: the file changed since the draft was written.
    #[serde(default)]
    #[sqlx(default)]
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Reply {
    pub thread_id: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Resolution {
    pub thread_id: String,
    pub resolved_through: Option<String>,
    pub reopened: bool,
}

/// Everything saved for one PR.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedState {
    pub approvals: Vec<Approval>,
    pub drafts: Vec<Draft>,
    pub replies: Vec<Reply>,
    pub resolutions: Vec<Resolution>,
    pub review_body: String,
}

/// The review state database, scoped to one PR. All PRs share one sqlite file.
#[derive(Clone)]
pub struct Db {
    pool: SqlitePool,
    key: PrKey,
    head_oid: String,
    base_oid: String,
}

pub fn default_path() -> Result<PathBuf> {
    let dir = dirs::data_dir().context("could not find a data directory")?;
    Ok(dir.join("pr-review").join("pr-review.db"))
}

impl Db {
    /// Opens (creating and migrating if needed) the database at `path`. `head_oid` and
    /// `base_oid` are the PR's current head and base, stamped on every draft saved.
    pub async fn open(path: &Path, key: PrKey, head_oid: String, base_oid: String) -> Result<Db> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| format!("could not create {}", parent.display()))?;
        }
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true);
        Self::connect(options, key, head_oid, base_oid)
            .await
            .with_context(|| format!("could not open the database at {}", path.display()))
    }

    pub async fn open_default(key: PrKey, head_oid: String, base_oid: String) -> Result<Db> {
        Self::open(&default_path()?, key, head_oid, base_oid).await
    }

    async fn connect(options: SqliteConnectOptions, key: PrKey, head_oid: String, base_oid: String) -> Result<Db> {
        let pool = SqlitePoolOptions::new().max_connections(4).connect_with(options).await?;
        MIGRATOR.run(&pool).await?;
        Ok(Db { pool, key, head_oid, base_oid })
    }

    pub fn key(&self) -> &PrKey {
        &self.key
    }

    pub async fn load(&self) -> Result<SavedState> {
        let (owner, repo, number) = self.pr();
        let approvals = sqlx::query_as(
            "SELECT file_path, commit_oid FROM approvals
             WHERE owner = ?1 AND repo = ?2 AND pr_number = ?3 ORDER BY file_path, commit_oid",
        )
        .bind(owner)
        .bind(repo)
        .bind(number)
        .fetch_all(&self.pool)
        .await?;
        let drafts = sqlx::query_as(
            "SELECT draft_key, file_path, side, start_line, end_line, body, head_oid, base_oid FROM drafts
             WHERE owner = ?1 AND repo = ?2 AND pr_number = ?3 ORDER BY file_path, start_line, draft_key",
        )
        .bind(owner)
        .bind(repo)
        .bind(number)
        .fetch_all(&self.pool)
        .await?;
        let replies = sqlx::query_as(
            "SELECT thread_id, body FROM replies
             WHERE owner = ?1 AND repo = ?2 AND pr_number = ?3 ORDER BY thread_id",
        )
        .bind(owner)
        .bind(repo)
        .bind(number)
        .fetch_all(&self.pool)
        .await?;
        let resolutions = sqlx::query_as(
            "SELECT thread_id, resolved_through, reopened FROM resolutions
             WHERE owner = ?1 AND repo = ?2 AND pr_number = ?3 ORDER BY thread_id",
        )
        .bind(owner)
        .bind(repo)
        .bind(number)
        .fetch_all(&self.pool)
        .await?;
        let review_body = sqlx::query("SELECT body FROM review_bodies WHERE owner = ?1 AND repo = ?2 AND pr_number = ?3")
            .bind(owner)
            .bind(repo)
            .bind(number)
            .fetch_optional(&self.pool)
            .await?
            .map(|row| row.get("body"))
            .unwrap_or_default();
        Ok(SavedState { approvals, drafts, replies, resolutions, review_body })
    }

    /// Replaces the approved commits of one file (an empty list unapproves it).
    pub async fn set_file_approvals(&self, file_path: &str, commit_oids: &[String]) -> Result<()> {
        let (owner, repo, number) = self.pr();
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM approvals WHERE owner = ?1 AND repo = ?2 AND pr_number = ?3 AND file_path = ?4")
            .bind(owner)
            .bind(repo)
            .bind(number)
            .bind(file_path)
            .execute(&mut *tx)
            .await?;
        for oid in commit_oids {
            sqlx::query("INSERT INTO approvals (owner, repo, pr_number, file_path, commit_oid) VALUES (?1, ?2, ?3, ?4, ?5)")
                .bind(owner)
                .bind(repo)
                .bind(number)
                .bind(file_path)
                .bind(oid)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Creates or updates a draft; a blank body deletes it, since empty drafts are discarded.
    pub async fn save_draft(&self, draft: &Draft) -> Result<()> {
        if draft.body.trim().is_empty() {
            return self.delete_draft(&draft.draft_key).await;
        }
        let (owner, repo, number) = self.pr();
        sqlx::query(
            "INSERT INTO drafts (owner, repo, pr_number, draft_key, file_path, side, start_line, end_line, body, head_oid, base_oid)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT (owner, repo, pr_number, draft_key) DO UPDATE SET
                 body = excluded.body, head_oid = excluded.head_oid, base_oid = excluded.base_oid,
                 updated_at = CURRENT_TIMESTAMP",
        )
        .bind(owner)
        .bind(repo)
        .bind(number)
        .bind(&draft.draft_key)
        .bind(&draft.file_path)
        .bind(&draft.side)
        .bind(draft.start_line)
        .bind(draft.end_line)
        .bind(&draft.body)
        .bind(&self.head_oid)
        .bind(&self.base_oid)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Re-stamps a draft with the current head and base: "this line is still right".
    pub async fn acknowledge_draft(&self, draft_key: &str) -> Result<()> {
        let (owner, repo, number) = self.pr();
        sqlx::query(
            "UPDATE drafts SET head_oid = ?1, base_oid = ?2, updated_at = CURRENT_TIMESTAMP
             WHERE owner = ?3 AND repo = ?4 AND pr_number = ?5 AND draft_key = ?6",
        )
        .bind(&self.head_oid)
        .bind(&self.base_oid)
        .bind(owner)
        .bind(repo)
        .bind(number)
        .bind(draft_key)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Clears what a submitted review used up: every draft, the review body, and every reply
    /// except the one for `keep_reply_thread` (a reply that failed to post).
    pub async fn clear_submitted(&self, keep_reply_thread: Option<&str>) -> Result<()> {
        let (owner, repo, number) = self.pr();
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM drafts WHERE owner = ?1 AND repo = ?2 AND pr_number = ?3")
            .bind(owner)
            .bind(repo)
            .bind(number)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM review_bodies WHERE owner = ?1 AND repo = ?2 AND pr_number = ?3")
            .bind(owner)
            .bind(repo)
            .bind(number)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM replies WHERE owner = ?1 AND repo = ?2 AND pr_number = ?3 AND thread_id IS NOT ?4")
            .bind(owner)
            .bind(repo)
            .bind(number)
            .bind(keep_reply_thread)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn delete_draft(&self, draft_key: &str) -> Result<()> {
        let (owner, repo, number) = self.pr();
        sqlx::query("DELETE FROM drafts WHERE owner = ?1 AND repo = ?2 AND pr_number = ?3 AND draft_key = ?4")
            .bind(owner)
            .bind(repo)
            .bind(number)
            .bind(draft_key)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Creates or updates the reply draft for a thread; a blank body deletes it.
    pub async fn save_reply(&self, thread_id: &str, body: &str) -> Result<()> {
        let (owner, repo, number) = self.pr();
        if body.trim().is_empty() {
            sqlx::query("DELETE FROM replies WHERE owner = ?1 AND repo = ?2 AND pr_number = ?3 AND thread_id = ?4")
                .bind(owner)
                .bind(repo)
                .bind(number)
                .bind(thread_id)
                .execute(&self.pool)
                .await?;
            return Ok(());
        }
        sqlx::query(
            "INSERT INTO replies (owner, repo, pr_number, thread_id, body) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (owner, repo, pr_number, thread_id) DO UPDATE SET
                 body = excluded.body, updated_at = CURRENT_TIMESTAMP",
        )
        .bind(owner)
        .bind(repo)
        .bind(number)
        .bind(thread_id)
        .bind(body)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_resolution(&self, thread_id: &str, resolved_through: Option<&str>, reopened: bool) -> Result<()> {
        let (owner, repo, number) = self.pr();
        sqlx::query(
            "INSERT INTO resolutions (owner, repo, pr_number, thread_id, resolved_through, reopened)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT (owner, repo, pr_number, thread_id) DO UPDATE SET
                 resolved_through = excluded.resolved_through, reopened = excluded.reopened,
                 updated_at = CURRENT_TIMESTAMP",
        )
        .bind(owner)
        .bind(repo)
        .bind(number)
        .bind(thread_id)
        .bind(resolved_through)
        .bind(reopened)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Saves the review text; a blank body deletes it.
    pub async fn set_review_body(&self, body: &str) -> Result<()> {
        let (owner, repo, number) = self.pr();
        if body.trim().is_empty() {
            sqlx::query("DELETE FROM review_bodies WHERE owner = ?1 AND repo = ?2 AND pr_number = ?3")
                .bind(owner)
                .bind(repo)
                .bind(number)
                .execute(&self.pool)
                .await?;
            return Ok(());
        }
        sqlx::query(
            "INSERT INTO review_bodies (owner, repo, pr_number, body) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (owner, repo, pr_number) DO UPDATE SET
                 body = excluded.body, updated_at = CURRENT_TIMESTAMP",
        )
        .bind(owner)
        .bind(repo)
        .bind(number)
        .bind(body)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    fn pr(&self) -> (&str, &str, i64) {
        (&self.key.owner, &self.key.repo, self.key.number as i64)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn key(number: u64) -> PrKey {
        PrKey { owner: "acme".into(), repo: "widgets".into(), number }
    }

    async fn memory_db() -> Db {
        let options = SqliteConnectOptions::from_str("sqlite::memory:").unwrap();
        // One connection: every connection to `:memory:` would be its own database.
        let pool = SqlitePoolOptions::new().max_connections(1).connect_with(options).await.unwrap();
        MIGRATOR.run(&pool).await.unwrap();
        Db { pool, key: key(1), head_oid: "head1".into(), base_oid: "base1".into() }
    }

    fn draft(key: &str, body: &str) -> Draft {
        Draft {
            draft_key: key.into(),
            file_path: "src/a.rs".into(),
            side: "new".into(),
            start_line: 3,
            end_line: 5,
            body: body.into(),
            head_oid: String::new(),
            base_oid: String::new(),
            stale: false,
        }
    }

    #[tokio::test]
    async fn empty_state_loads_empty() {
        assert_eq!(memory_db().await.load().await.unwrap(), SavedState::default());
    }

    #[tokio::test]
    async fn approvals_are_replaced_per_file() {
        let db = memory_db().await;
        db.set_file_approvals("a.rs", &["c1".into(), "c3".into()]).await.unwrap();
        db.set_file_approvals("b.rs", &["c2".into()]).await.unwrap();
        db.set_file_approvals("a.rs", &["c1".into()]).await.unwrap();
        let pairs: Vec<_> = db.load().await.unwrap().approvals.into_iter().map(|a| (a.file_path, a.commit_oid)).collect();
        assert_eq!(pairs, vec![("a.rs".into(), "c1".into()), ("b.rs".into(), "c2".into())]);

        db.set_file_approvals("a.rs", &[]).await.unwrap();
        assert_eq!(db.load().await.unwrap().approvals.len(), 1);
    }

    #[tokio::test]
    async fn drafts_upsert_stamp_the_head_and_blank_deletes() {
        let db = memory_db().await;
        db.save_draft(&draft("k1", "first")).await.unwrap();
        db.save_draft(&draft("k1", "edited")).await.unwrap();
        db.save_draft(&draft("k2", "other")).await.unwrap();

        let drafts = db.load().await.unwrap().drafts;
        assert_eq!(drafts.len(), 2);
        assert_eq!(drafts[0].body, "edited");
        assert_eq!((drafts[0].head_oid.as_str(), drafts[0].base_oid.as_str()), ("head1", "base1"));
        assert_eq!((drafts[0].side.as_str(), drafts[0].start_line, drafts[0].end_line), ("new", 3, 5));

        db.save_draft(&draft("k1", "  \n")).await.unwrap();
        db.delete_draft("k2").await.unwrap();
        assert!(db.load().await.unwrap().drafts.is_empty());
    }

    #[tokio::test]
    async fn acknowledging_a_draft_restamps_it_with_the_current_head_and_base() {
        let mut db = memory_db().await;
        db.save_draft(&draft("k", "text")).await.unwrap();
        db.head_oid = "head2".into();
        db.base_oid = "base2".into();
        db.acknowledge_draft("k").await.unwrap();
        let d = &db.load().await.unwrap().drafts[0];
        assert_eq!((d.head_oid.as_str(), d.base_oid.as_str(), d.body.as_str()), ("head2", "base2", "text"));
    }

    #[tokio::test]
    async fn clear_submitted_keeps_only_the_named_reply_and_the_rest_of_the_state() {
        let db = memory_db().await;
        db.save_draft(&draft("k", "text")).await.unwrap();
        db.save_reply("t1", "reply 1").await.unwrap();
        db.save_reply("conversation", "reply 2").await.unwrap();
        db.set_review_body("LGTM").await.unwrap();
        db.set_file_approvals("a.rs", &["c1".into()]).await.unwrap();
        db.set_resolution("t1", Some("c9"), false).await.unwrap();

        db.clear_submitted(Some("conversation")).await.unwrap();
        let s = db.load().await.unwrap();
        assert!(s.drafts.is_empty());
        assert_eq!(s.review_body, "");
        assert_eq!(s.replies, vec![Reply { thread_id: "conversation".into(), body: "reply 2".into() }]);
        assert_eq!((s.approvals.len(), s.resolutions.len()), (1, 1));

        db.clear_submitted(None).await.unwrap();
        assert!(db.load().await.unwrap().replies.is_empty());
    }

    #[tokio::test]
    async fn invalid_side_is_rejected() {
        let db = memory_db().await;
        let mut d = draft("k", "text");
        d.side = "middle".into();
        assert!(db.save_draft(&d).await.is_err());
    }

    #[tokio::test]
    async fn replies_resolutions_and_review_body() {
        let db = memory_db().await;
        db.save_reply("t1", "reply").await.unwrap();
        db.save_reply("t1", "reply 2").await.unwrap();
        db.save_reply("t2", "x").await.unwrap();
        db.save_reply("t2", "").await.unwrap();

        db.set_resolution("t1", Some("c9"), false).await.unwrap();
        db.set_resolution("t1", Some("c9"), true).await.unwrap();
        db.set_resolution("t3", None, true).await.unwrap();

        db.set_review_body("LGTM").await.unwrap();
        db.set_review_body("LGTM with nits").await.unwrap();

        let s = db.load().await.unwrap();
        assert_eq!(s.replies, vec![Reply { thread_id: "t1".into(), body: "reply 2".into() }]);
        assert_eq!(
            s.resolutions,
            vec![
                Resolution { thread_id: "t1".into(), resolved_through: Some("c9".into()), reopened: true },
                Resolution { thread_id: "t3".into(), resolved_through: None, reopened: true },
            ]
        );
        assert_eq!(s.review_body, "LGTM with nits");

        db.set_review_body("").await.unwrap();
        assert_eq!(db.load().await.unwrap().review_body, "");
    }

    #[tokio::test]
    async fn state_is_per_pr_and_survives_reopening_the_file() {
        let dir = std::env::temp_dir().join(format!("pr-review-core-db-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("nested").join("test.db");

        let one = Db::open(&path, key(1), "h".into(), "b".into()).await.unwrap();
        one.set_review_body("for PR 1").await.unwrap();
        one.set_file_approvals("a.rs", &["c1".into()]).await.unwrap();

        let two = Db::open(&path, key(2), "h".into(), "b".into()).await.unwrap();
        assert_eq!(two.load().await.unwrap(), SavedState::default());
        two.set_review_body("for PR 2").await.unwrap();

        let other_repo = Db::open(&path, PrKey { repo: "gadgets".into(), ..key(1) }, "h".into(), "b".into())
            .await
            .unwrap();
        assert_eq!(other_repo.load().await.unwrap(), SavedState::default());

        let again = Db::open(&path, key(1), "h".into(), "b".into()).await.unwrap();
        let s = again.load().await.unwrap();
        assert_eq!(s.review_body, "for PR 1");
        assert_eq!(s.approvals.len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
