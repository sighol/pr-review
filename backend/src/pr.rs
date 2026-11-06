use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::manifest::{Manifest, build_manifest};
use crate::{git, github};

pub struct OpenedPr {
    pub repo: PathBuf,
    pub manifest: Manifest,
}

/// Opens PR `number` of the repository containing `cwd`: asks GitHub for its metadata,
/// fetches its commits with git, and builds the manifest. Always fetches.
pub async fn open_pr(cwd: &Path, number: u64, progress: impl Fn(&str)) -> Result<OpenedPr> {
    let repo = git::toplevel(cwd).await?;
    let (owner, name) = git::origin_owner_repo(&repo).await?;

    progress(&format!("Reading PR #{number} from {owner}/{name}"));
    let info = github::pr_view(&owner, &name, number).await?;

    progress(&format!("Fetching PR #{number} and {}", info.base_ref_name));
    let range = git::fetch_pr(&repo, number, &info.base_ref_name).await?;

    progress("Reading commits and diffs");
    let mut manifest = build_manifest(&repo, &range.base, &range.head).await?;
    manifest.pr = Some(info.meta);
    Ok(OpenedPr { repo, manifest })
}
