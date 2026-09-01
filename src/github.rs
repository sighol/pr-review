use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde_json::Value;
use tokio::process::Command;

use crate::types::Commit;

pub struct PrData {
    pub base_oid: String,
    pub head_oid: String,
    pub commits: Vec<Commit>,
    pub file_paths: Vec<String>,
}

pub async fn fetch_pr_data(owner: &str, repo: &str, pr_number: u64) -> Result<PrData> {
    let (base_oid, head_oid, commits) = fetch_pr_commits(owner, repo, pr_number).await?;
    let file_paths = fetch_pr_files(owner, repo, pr_number).await?;
    Ok(PrData {
        base_oid,
        head_oid,
        commits,
        file_paths,
    })
}

const COMMITS_QUERY: &str = r#"
query($owner: String!, $repo: String!, $number: Int!, $cursor: String) {
  repository(owner: $owner, name: $repo) {
    pullRequest(number: $number) {
      baseRefOid
      headRefOid
      commits(first: 100, after: $cursor) {
        pageInfo { hasNextPage endCursor }
        nodes { commit { oid message committedDate } }
      }
    }
  }
}"#;

const FILES_QUERY: &str = r#"
query($owner: String!, $repo: String!, $number: Int!, $cursor: String) {
  repository(owner: $owner, name: $repo) {
    pullRequest(number: $number) {
      files(first: 100, after: $cursor) {
        pageInfo { hasNextPage endCursor }
        nodes { path }
      }
    }
  }
}"#;

async fn fetch_pr_commits(
    owner: &str,
    repo: &str,
    pr_number: u64,
) -> Result<(String, String, Vec<Commit>)> {
    let mut commits = Vec::new();
    let mut base_oid: Option<String> = None;
    let mut head_oid: Option<String> = None;
    let mut cursor: Option<String> = None;

    loop {
        let mut vars = vec![
            ("owner", owner.to_string()),
            ("repo", repo.to_string()),
            ("number", pr_number.to_string()),
        ];
        if let Some(c) = &cursor {
            vars.push(("cursor", c.clone()));
        }
        let json = gh_graphql(COMMITS_QUERY, &vars).await?;
        let pr = &json["data"]["repository"]["pullRequest"];
        if pr.is_null() {
            bail!("PR #{pr_number} not found in {owner}/{repo}");
        }
        if base_oid.is_none() {
            base_oid = pr["baseRefOid"].as_str().map(|s| s.to_string());
        }
        if head_oid.is_none() {
            head_oid = pr["headRefOid"].as_str().map(|s| s.to_string());
        }

        let commits_obj = &pr["commits"];
        for node in commits_obj["nodes"]
            .as_array()
            .context("malformed gh response: missing commits.nodes")?
        {
            let c = &node["commit"];
            let committed_date = c["committedDate"]
                .as_str()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(Utc::now);
            commits.push(Commit {
                oid: c["oid"].as_str().unwrap_or_default().to_string(),
                message: c["message"].as_str().unwrap_or_default().to_string(),
                committed_date,
            });
        }

        let page_info = &commits_obj["pageInfo"];
        if !page_info["hasNextPage"].as_bool().unwrap_or(false) {
            break;
        }
        cursor = page_info["endCursor"].as_str().map(|s| s.to_string());
    }

    let base_oid = base_oid.context("PR response was missing baseRefOid")?;
    let head_oid = head_oid.context("PR response was missing headRefOid")?;
    Ok((base_oid, head_oid, commits))
}

async fn fetch_pr_files(owner: &str, repo: &str, pr_number: u64) -> Result<Vec<String>> {
    let mut files = Vec::new();
    let mut cursor: Option<String> = None;

    loop {
        let mut vars = vec![
            ("owner", owner.to_string()),
            ("repo", repo.to_string()),
            ("number", pr_number.to_string()),
        ];
        if let Some(c) = &cursor {
            vars.push(("cursor", c.clone()));
        }
        let json = gh_graphql(FILES_QUERY, &vars).await?;
        let files_obj = &json["data"]["repository"]["pullRequest"]["files"];
        for node in files_obj["nodes"]
            .as_array()
            .context("malformed gh response: missing files.nodes")?
        {
            if let Some(path) = node["path"].as_str() {
                files.push(path.to_string());
            }
        }

        let page_info = &files_obj["pageInfo"];
        if !page_info["hasNextPage"].as_bool().unwrap_or(false) {
            break;
        }
        cursor = page_info["endCursor"].as_str().map(|s| s.to_string());
    }

    Ok(files)
}

async fn gh_graphql(query: &str, vars: &[(&str, String)]) -> Result<Value> {
    let mut cmd = Command::new("gh");
    cmd.args(["api", "graphql", "-f", &format!("query={query}")]);
    for (key, value) in vars {
        cmd.args(["-F", &format!("{key}={value}")]);
    }

    let output = cmd
        .output()
        .await
        .context("failed to run `gh` (is the GitHub CLI installed?)")?;
    if !output.status.success() {
        bail!(
            "gh api graphql failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    serde_json::from_slice(&output.stdout).context("failed to parse gh response as JSON")
}
