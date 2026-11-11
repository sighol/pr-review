use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;
use tracing::info;

use crate::db::{Db, PrKey};
use crate::manifest::{Manifest, build_manifest};
use crate::{git, github};

pub struct OpenedPr {
    pub repo: PathBuf,
    pub key: PrKey,
    pub manifest: Manifest,
}

/// Opens PR `number` of the repository containing `cwd`: asks GitHub for its metadata,
/// fetches its commits with git, and builds the manifest. Always fetches.
pub async fn open_pr(cwd: &Path, number: u64) -> Result<OpenedPr> {
    let started = Instant::now();
    let repo = git::toplevel(cwd).await?;
    let (owner, name) = git::origin_owner_repo(&repo).await?;
    info!("repository {} ({owner}/{name})", repo.display());

    info!("reading PR #{number} from {owner}/{name}");
    let info = github::pr_view(&owner, &name, number).await?;
    info!(
        "PR #{number}: \"{}\" (base branch {})",
        info.meta.title, info.base_ref_name
    );

    info!(
        "fetching PR #{number}, {} and its discussion",
        info.base_ref_name
    );
    let (range, (raw_threads, conversation, reviews)) = tokio::try_join!(
        git::fetch_pr(
            &repo,
            number,
            &info.base_ref_name,
            info.merge_commit.as_deref()
        ),
        github::fetch_discussion(&owner, &name, number)
    )?;
    info!(
        "fetched: base {} head {}, {} review threads, {} conversation comments, {} reviews",
        short(&range.base),
        short(&range.head),
        raw_threads.len(),
        conversation.len(),
        reviews.len()
    );

    info!("reading commits and diffs");
    let mut manifest = build_manifest(&repo, &range.base, &range.head).await?;
    manifest.set_threads(raw_threads, conversation, reviews);
    manifest.pr = Some(info.meta);
    info!(
        "PR #{number} ready: {} commits, {} files, {} threads in {:?}",
        manifest.commits.len(),
        manifest.files.len(),
        manifest.threads.as_ref().map_or(0, Vec::len),
        started.elapsed()
    );
    let key = PrKey {
        owner,
        repo: name,
        number,
    };
    Ok(OpenedPr {
        repo,
        key,
        manifest,
    })
}

fn short(oid: &str) -> &str {
    &oid[..oid.len().min(7)]
}

/// Compares the PR's newly fetched head against the one last recorded for it; if the old head
/// is no longer an ancestor of the new one, the history was rewritten (force-pushed), so it is
/// appended to the permanent force-push log. Always updates the recorded head, and is a no-op
/// the first time a PR is opened (nothing to compare against yet).
pub async fn record_head(db: &Db, repo: &Path, new_head: &str) -> Result<()> {
    if let Some(old_head) = db.known_head().await? {
        if old_head != new_head && !git::is_ancestor(repo, &old_head, new_head).await? {
            info!(
                "PR head {} is not an ancestor of {}: recording a force-push",
                short(&old_head),
                short(new_head)
            );
            db.record_force_push(&old_head, new_head).await?;
        }
    }
    db.set_known_head(new_head).await
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::git::testutil::TestRepo;

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    /// A fresh, uniquely-pathed and uniquely-keyed `Db`, so parallel tests never share state.
    async fn memory_db() -> Db {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        Db::open(
            &std::env::temp_dir().join(format!("pr-review-core-pr-rs-{}-{n}", std::process::id())),
            PrKey {
                owner: "acme".into(),
                repo: "widgets".into(),
                number: n as u64,
            },
            String::new(),
            String::new(),
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn first_open_just_records_the_head() {
        let db = memory_db().await;
        let repo = TestRepo::new();
        let h1 = repo.commit("c1", &[("a", Some("1"))]);

        record_head(&db, repo.path(), &h1).await.unwrap();

        assert_eq!(db.known_head().await.unwrap(), Some(h1));
        assert!(db.load().await.unwrap().force_pushes.is_empty());
    }

    #[tokio::test]
    async fn a_normal_push_is_not_recorded_as_a_force_push() {
        let db = memory_db().await;
        let repo = TestRepo::new();
        let h1 = repo.commit("c1", &[("a", Some("1"))]);
        record_head(&db, repo.path(), &h1).await.unwrap();

        let h2 = repo.commit("c2", &[("a", Some("2"))]);
        record_head(&db, repo.path(), &h2).await.unwrap();

        assert_eq!(db.known_head().await.unwrap(), Some(h2));
        assert!(db.load().await.unwrap().force_pushes.is_empty());
    }

    #[tokio::test]
    async fn a_rewrite_that_drops_the_known_head_is_recorded_as_a_force_push() {
        let db = memory_db().await;
        let repo = TestRepo::new();
        let h1 = repo.commit("c1", &[("a", Some("1"))]);
        record_head(&db, repo.path(), &h1).await.unwrap();
        let h2 = repo.commit("c2", &[("a", Some("2"))]);
        record_head(&db, repo.path(), &h2).await.unwrap();

        // Rewrite history from h1: h2 is no longer an ancestor of h3.
        repo.git(&["reset", "-q", "--hard", &h1]);
        let h3 = repo.commit("c2 rewritten", &[("a", Some("3"))]);
        record_head(&db, repo.path(), &h3).await.unwrap();

        assert_eq!(db.known_head().await.unwrap(), Some(h3.clone()));
        let pushes = db.load().await.unwrap().force_pushes;
        assert_eq!(pushes.len(), 1);
        assert_eq!(
            (pushes[0].old_oid.as_str(), pushes[0].new_oid.as_str()),
            (h2.as_str(), h3.as_str())
        );
    }
}
