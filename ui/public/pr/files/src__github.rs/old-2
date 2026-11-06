use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
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

/// A single comment within a review thread, or within the top-level PR
/// conversation.
pub struct ReviewComment {
    pub id: String,
    pub author: String,
    pub created_at: DateTime<Utc>,
    pub body: String,
    /// The diff hunk this comment was made against, exactly as GitHub
    /// recorded it at the time — unset for top-level conversation comments.
    pub diff_hunk: String,
    /// The commit this comment's diff hunk is anchored to — unset for
    /// top-level conversation comments.
    pub original_commit_oid: Option<String>,
}

/// An inline code-review comment thread, with GitHub's own resolved flag
/// intentionally omitted: this tool tracks its own, separate resolution
/// state locally (see `db::thread_resolutions`).
pub struct ReviewThread {
    pub id: String,
    pub path: String,
    pub line: Option<i64>,
    /// Chronological order, oldest first. Always non-empty.
    pub comments: Vec<ReviewComment>,
}

#[derive(Deserialize)]
struct GraphQlEnvelope {
    data: GraphQlData,
}

#[derive(Deserialize)]
struct GraphQlData {
    repository: RepositoryData,
}

#[derive(Deserialize)]
struct RepositoryData {
    #[serde(rename = "pullRequest")]
    pull_request: PullRequestData,
}

#[derive(Deserialize)]
struct PullRequestData {
    #[serde(rename = "reviewThreads")]
    review_threads: ReviewThreadsConnection,
}

#[derive(Deserialize)]
struct ReviewThreadsConnection {
    #[serde(rename = "pageInfo")]
    page_info: PageInfo,
    nodes: Vec<ReviewThreadNode>,
}

#[derive(Deserialize)]
struct PageInfo {
    #[serde(rename = "hasNextPage")]
    has_next_page: bool,
    #[serde(rename = "endCursor")]
    end_cursor: Option<String>,
}

#[derive(Deserialize)]
struct ReviewThreadNode {
    id: String,
    path: String,
    line: Option<i64>,
    comments: CommentsConnection,
}

#[derive(Deserialize)]
struct CommentsConnection {
    nodes: Vec<CommentNode>,
}

#[derive(Deserialize)]
struct CommentNode {
    id: String,
    author: Option<Author>,
    #[serde(rename = "createdAt")]
    created_at: DateTime<Utc>,
    body: String,
    #[serde(rename = "diffHunk", default)]
    diff_hunk: String,
    #[serde(rename = "originalCommit")]
    original_commit: Option<OriginalCommit>,
}

#[derive(Deserialize)]
struct OriginalCommit {
    oid: String,
}

#[derive(Deserialize)]
struct Author {
    login: String,
}

const REVIEW_THREADS_QUERY: &str = r#"
query($owner: String!, $repo: String!, $number: Int!, $after: String) {
  repository(owner: $owner, name: $repo) {
    pullRequest(number: $number) {
      reviewThreads(first: 50, after: $after) {
        pageInfo { hasNextPage endCursor }
        nodes {
          id
          path
          line
          comments(first: 100) {
            nodes {
              id
              author { login }
              createdAt
              body
              diffHunk
              originalCommit { oid }
            }
          }
        }
      }
    }
  }
}
"#;

/// Fetches every inline code-review comment thread on the PR, oldest comment
/// first within each thread. Pages through all threads (100 comments per
/// thread is treated as a practical cap, not paginated further).
pub async fn fetch_review_threads(
    owner: &str,
    repo: &str,
    pr_number: u64,
) -> Result<Vec<ReviewThread>> {
    let mut threads = Vec::new();
    let mut cursor: Option<String> = None;

    loop {
        let mut args = vec![
            "api".to_string(),
            "graphql".to_string(),
            "-F".to_string(),
            format!("query={REVIEW_THREADS_QUERY}"),
            "-f".to_string(),
            format!("owner={owner}"),
            "-f".to_string(),
            format!("repo={repo}"),
            "-F".to_string(),
            format!("number={pr_number}"),
        ];
        if let Some(after) = &cursor {
            args.push("-f".to_string());
            args.push(format!("after={after}"));
        }

        let output = Command::new("gh")
            .args(&args)
            .output()
            .await
            .context("failed to run `gh api graphql` for review threads")?;

        if !output.status.success() {
            bail!(
                "gh api graphql (review threads) failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let parsed: GraphQlEnvelope = serde_json::from_slice(&output.stdout)
            .context("failed to parse review threads GraphQL response")?;
        let connection = parsed.data.repository.pull_request.review_threads;

        for node in connection.nodes {
            let mut comments: Vec<ReviewComment> = node
                .comments
                .nodes
                .into_iter()
                .map(|c| ReviewComment {
                    id: c.id,
                    author: c
                        .author
                        .map(|a| a.login)
                        .unwrap_or_else(|| "ghost".to_string()),
                    created_at: c.created_at,
                    body: c.body,
                    diff_hunk: c.diff_hunk,
                    original_commit_oid: c.original_commit.map(|oc| oc.oid),
                })
                .collect();
            comments.sort_by_key(|c| c.created_at);
            threads.push(ReviewThread {
                id: node.id,
                path: node.path,
                line: node.line,
                comments,
            });
        }

        if connection.page_info.has_next_page {
            cursor = connection.page_info.end_cursor;
        } else {
            break;
        }
    }

    Ok(threads)
}

#[derive(Deserialize)]
struct PrConversationJson {
    comments: Vec<ConversationCommentJson>,
}

#[derive(Deserialize)]
struct ConversationCommentJson {
    id: String,
    author: Option<Author>,
    #[serde(rename = "createdAt")]
    created_at: DateTime<Utc>,
    body: String,
}

/// Fetches the PR's top-level conversation comments (the ones with no
/// associated file/line and no resolved concept of their own), oldest first.
pub async fn fetch_pr_conversation_comments(
    owner: &str,
    repo: &str,
    pr_number: u64,
) -> Result<Vec<ReviewComment>> {
    let output = Command::new("gh")
        .args([
            "pr",
            "view",
            &pr_number.to_string(),
            "--repo",
            &format!("{owner}/{repo}"),
            "--json",
            "comments",
        ])
        .output()
        .await
        .context("failed to run `gh pr view` for conversation comments")?;

    if !output.status.success() {
        bail!(
            "gh pr view failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let parsed: PrConversationJson = serde_json::from_slice(&output.stdout)
        .context("failed to parse `gh pr view` conversation comments as JSON")?;

    let mut comments: Vec<ReviewComment> = parsed
        .comments
        .into_iter()
        .map(|c| ReviewComment {
            id: c.id,
            author: c
                .author
                .map(|a| a.login)
                .unwrap_or_else(|| "ghost".to_string()),
            created_at: c.created_at,
            body: c.body,
            diff_hunk: String::new(),
            original_commit_oid: None,
        })
        .collect();
    comments.sort_by_key(|c| c.created_at);
    Ok(comments)
}
