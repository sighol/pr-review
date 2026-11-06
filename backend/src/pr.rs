use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;

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
pub async fn open_pr(cwd: &Path, number: u64, progress: impl Fn(&str)) -> Result<OpenedPr> {
    let repo = git::toplevel(cwd).await?;
    let (owner, name) = git::origin_owner_repo(&repo).await?;

    progress(&format!("Reading PR #{number} from {owner}/{name}"));
    let info = github::pr_view(&owner, &name, number).await?;

    progress(&format!(
        "Fetching PR #{number}, {} and its discussion",
        info.base_ref_name
    ));
    let (range, (raw_threads, conversation)) = tokio::try_join!(
        git::fetch_pr(&repo, number, &info.base_ref_name),
        github::fetch_discussion(&owner, &name, number)
    )?;

    progress("Reading commits and diffs");
    let mut manifest = build_manifest(&repo, &range.base, &range.head).await?;
    let commit_numbers: HashMap<&str, usize> = manifest
        .commits
        .iter()
        .map(|c| (c.oid.as_str(), c.n))
        .collect();
    manifest.threads = Some(build_threads(raw_threads, conversation, &commit_numbers));
    manifest.pr = Some(info.meta);
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
