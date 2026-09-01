use anyhow::{Context, Result, bail};
use serde::Deserialize;
use tokio::process::Command;

/// The minimal PR metadata we need from GitHub: enough to `git fetch` and
/// diff locally. Everything else (commits, changed files, diffs) comes from
/// local `git`, not the GitHub API.
pub struct PrRefs {
    pub base_oid: String,
    pub head_oid: String,
    pub head_ref_name: String,
}

#[derive(Deserialize)]
struct PrViewJson {
    #[serde(rename = "baseRefOid")]
    base_ref_oid: String,
    #[serde(rename = "headRefOid")]
    head_ref_oid: String,
    #[serde(rename = "headRefName")]
    head_ref_name: String,
}

pub async fn fetch_pr_refs(owner: &str, repo: &str, pr_number: u64) -> Result<PrRefs> {
    let output = Command::new("gh")
        .args([
            "pr",
            "view",
            &pr_number.to_string(),
            "--repo",
            &format!("{owner}/{repo}"),
            "--json",
            "baseRefOid,headRefOid,headRefName",
        ])
        .output()
        .await
        .context("failed to run `gh pr view` (is the GitHub CLI installed and authenticated?)")?;

    if !output.status.success() {
        bail!(
            "gh pr view failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let parsed: PrViewJson = serde_json::from_slice(&output.stdout)
        .context("failed to parse `gh pr view` output as JSON")?;

    Ok(PrRefs {
        base_oid: parsed.base_ref_oid,
        head_oid: parsed.head_ref_oid,
        head_ref_name: parsed.head_ref_name,
    })
}
