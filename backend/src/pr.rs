use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;
use tracing::info;

use crate::db::PrKey;
use crate::manifest::{Manifest, build_manifest};
use crate::threads::build_threads;
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
    let (range, (raw_threads, conversation)) = tokio::try_join!(
        git::fetch_pr(&repo, number, &info.base_ref_name),
        github::fetch_discussion(&owner, &name, number)
    )?;
    info!(
        "fetched: base {} head {}, {} review threads, {} conversation comments",
        short(&range.base),
        short(&range.head),
        raw_threads.len(),
        conversation.len()
    );

    info!("reading commits and diffs");
    let mut manifest = build_manifest(&repo, &range.base, &range.head).await?;
    let commit_numbers: HashMap<&str, usize> = manifest
        .commits
        .iter()
        .map(|c| (c.oid.as_str(), c.n))
        .collect();
    manifest.threads = Some(build_threads(raw_threads, conversation, &commit_numbers));
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
