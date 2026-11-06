use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tokio::process::Command;

/// The PR fields the UI shows (serialized into the manifest as `pr`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PrMeta {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub body: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PrInfo {
    pub meta: PrMeta,
    pub base_ref_name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PrViewJson {
    number: u64,
    title: String,
    body: String,
    author: Option<AuthorJson>,
    base_ref_name: String,
}

#[derive(Deserialize)]
struct AuthorJson {
    login: String,
}

fn parse_pr_view(json: &[u8]) -> Result<PrInfo> {
    let v: PrViewJson = serde_json::from_slice(json).context("unexpected output from `gh pr view`")?;
    Ok(PrInfo {
        meta: PrMeta {
            number: v.number,
            title: v.title,
            body: v.body,
            // GitHub returns no author for deleted accounts.
            author: v.author.map_or_else(|| "ghost".to_string(), |a| a.login),
        },
        base_ref_name: v.base_ref_name,
    })
}

/// PR metadata via the GitHub CLI, which brings its own authentication.
pub async fn pr_view(owner: &str, repo: &str, number: u64) -> Result<PrInfo> {
    let out = Command::new("gh")
        .args([
            "pr",
            "view",
            &number.to_string(),
            "--repo",
            &format!("{owner}/{repo}"),
            "--json",
            "number,title,body,author,baseRefName",
        ])
        .output()
        .await
        .context("failed to run `gh` (is the GitHub CLI installed?)")?;
    if !out.status.success() {
        bail!(
            "`gh pr view {number}` failed for {owner}/{repo} (try `gh auth status`): {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    parse_pr_view(&out.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gh_pr_view_output() {
        let json = br#"{"number":13486,"title":"Fix it","body":"Some **markdown**","author":{"login":"sigurd","name":"S"},"baseRefName":"master"}"#;
        assert_eq!(
            parse_pr_view(json).unwrap(),
            PrInfo {
                meta: PrMeta {
                    number: 13486,
                    title: "Fix it".into(),
                    author: "sigurd".into(),
                    body: "Some **markdown**".into(),
                },
                base_ref_name: "master".into(),
            }
        );
    }

    #[test]
    fn deleted_author_becomes_ghost() {
        let json = br#"{"number":1,"title":"t","body":"","author":null,"baseRefName":"main"}"#;
        assert_eq!(parse_pr_view(json).unwrap().meta.author, "ghost");
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_pr_view(b"not json").is_err());
    }
}
