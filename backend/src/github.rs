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

/// GitHub returns no author for deleted accounts; it calls them "ghost".
fn ghost_if_none(author: Option<AuthorJson>) -> String {
    author.map_or_else(|| "ghost".to_string(), |a| a.login)
}

fn parse_pr_view(json: &[u8]) -> Result<PrInfo> {
    let v: PrViewJson = serde_json::from_slice(json).context("unexpected output from `gh pr view`")?;
    Ok(PrInfo {
        meta: PrMeta {
            number: v.number,
            title: v.title,
            body: v.body,
            author: ghost_if_none(v.author),
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

/// Runs a GraphQL query or mutation through `gh api graphql`, sending the variables as JSON on
/// stdin so nested inputs work. Returns the response's `data`.
pub async fn graphql(query: &str, variables: serde_json::Value) -> Result<serde_json::Value> {
    use std::process::Stdio;
    use tokio::io::AsyncWriteExt;

    let body = serde_json::json!({ "query": query, "variables": variables });
    let mut child = Command::new("gh")
        .args(["api", "graphql", "-H", "Content-Type: application/json", "--input", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to run `gh` (is the GitHub CLI installed?)")?;
    let mut stdin = child.stdin.take().context("could not open gh's stdin")?;
    stdin.write_all(body.to_string().as_bytes()).await?;
    drop(stdin);
    let out = child.wait_with_output().await?;
    if !out.status.success() {
        bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    let response: serde_json::Value =
        serde_json::from_slice(&out.stdout).context("unexpected output from `gh api graphql`")?;
    if let Some(errors) = response.get("errors") {
        let messages: Vec<&str> = errors
            .as_array()
            .map(|a| a.iter().filter_map(|e| e["message"].as_str()).collect())
            .unwrap_or_default();
        bail!("GitHub rejected the request: {}", messages.join("; "));
    }
    response.get("data").cloned().context("GitHub's response had no data")
}

/// One comment, in a review thread or in the PR conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawComment {
    pub id: String,
    pub author: String,
    /// ISO 8601, as GitHub returns it.
    pub created_at: String,
    pub body: String,
    /// The commit the comment was made on; unset in the conversation.
    pub original_commit_oid: Option<String>,
}

/// An inline review thread as GitHub describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawThread {
    pub id: String,
    pub path: String,
    /// Current line; None when the thread is outdated.
    pub line: Option<i64>,
    pub original_line: Option<i64>,
    /// "LEFT" (the PR base) or "RIGHT" (the PR head).
    pub diff_side: String,
    pub is_outdated: bool,
    pub is_resolved: bool,
    /// Oldest first. Never empty.
    pub comments: Vec<RawComment>,
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
          originalLine
          diffSide
          isOutdated
          isResolved
          comments(first: 100) {
            nodes {
              id
              author { login }
              createdAt
              body
              state
              originalCommit { oid }
            }
          }
        }
      }
    }
  }
}
"#;

#[derive(Deserialize)]
struct GqlEnvelope {
    data: GqlData,
}

#[derive(Deserialize)]
struct GqlData {
    repository: GqlRepository,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GqlRepository {
    pull_request: GqlPullRequest,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GqlPullRequest {
    review_threads: GqlThreads,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GqlThreads {
    page_info: GqlPageInfo,
    nodes: Vec<GqlThread>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GqlPageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GqlThread {
    id: String,
    path: String,
    line: Option<i64>,
    original_line: Option<i64>,
    diff_side: String,
    is_outdated: bool,
    is_resolved: bool,
    comments: GqlComments,
}

#[derive(Deserialize)]
struct GqlComments {
    nodes: Vec<GqlComment>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GqlComment {
    id: String,
    author: Option<AuthorJson>,
    created_at: String,
    body: String,
    state: String,
    original_commit: Option<GqlCommit>,
}

#[derive(Deserialize)]
struct GqlCommit {
    oid: String,
}

/// Parses one page of `reviewThreads`; returns the threads and the cursor of the next page.
/// Comments in the viewer's own unsubmitted review are dropped, and so are threads left empty.
fn parse_threads_page(json: &[u8]) -> Result<(Vec<RawThread>, Option<String>)> {
    let page: GqlEnvelope = serde_json::from_slice(json).context("unexpected output from `gh api graphql`")?;
    let threads = page.data.repository.pull_request.review_threads;
    let next = threads.page_info.has_next_page.then_some(threads.page_info.end_cursor).flatten();
    let parsed = threads
        .nodes
        .into_iter()
        .filter_map(|t| {
            let comments: Vec<RawComment> = t
                .comments
                .nodes
                .into_iter()
                .filter(|c| c.state != "PENDING")
                .map(|c| RawComment {
                    id: c.id,
                    author: ghost_if_none(c.author),
                    created_at: c.created_at,
                    body: c.body,
                    original_commit_oid: c.original_commit.map(|o| o.oid),
                })
                .collect();
            (!comments.is_empty()).then_some(RawThread {
                id: t.id,
                path: t.path,
                line: t.line,
                original_line: t.original_line,
                diff_side: t.diff_side,
                is_outdated: t.is_outdated,
                is_resolved: t.is_resolved,
                comments,
            })
        })
        .collect();
    Ok((parsed, next))
}

/// Every review thread on the PR, via `gh api graphql`.
pub async fn fetch_review_threads(owner: &str, repo: &str, number: u64) -> Result<Vec<RawThread>> {
    let mut threads = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let mut args = vec![
            "api".to_string(),
            "graphql".to_string(),
            "-f".to_string(),
            format!("query={REVIEW_THREADS_QUERY}"),
            "-f".to_string(),
            format!("owner={owner}"),
            "-f".to_string(),
            format!("repo={repo}"),
            "-F".to_string(),
            format!("number={number}"),
        ];
        if let Some(after) = &cursor {
            args.push("-f".to_string());
            args.push(format!("after={after}"));
        }
        let out = Command::new("gh")
            .args(&args)
            .output()
            .await
            .context("failed to run `gh api graphql` for review threads")?;
        if !out.status.success() {
            bail!(
                "fetching review threads failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        let (page, next) = parse_threads_page(&out.stdout)?;
        threads.extend(page);
        match next {
            Some(after) => cursor = Some(after),
            None => return Ok(threads),
        }
    }
}

#[derive(Deserialize)]
struct ConversationJson {
    comments: Vec<ConversationCommentJson>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConversationCommentJson {
    id: String,
    author: Option<AuthorJson>,
    body: String,
    created_at: String,
}

fn parse_conversation(json: &[u8]) -> Result<Vec<RawComment>> {
    let v: ConversationJson = serde_json::from_slice(json).context("unexpected output from `gh pr view`")?;
    Ok(v
        .comments
        .into_iter()
        .map(|c| RawComment {
            id: c.id,
            author: ghost_if_none(c.author),
            created_at: c.created_at,
            body: c.body,
            original_commit_oid: None,
        })
        .collect())
}

/// The PR's top-level comments (not attached to code), oldest first.
pub async fn fetch_conversation(owner: &str, repo: &str, number: u64) -> Result<Vec<RawComment>> {
    let out = Command::new("gh")
        .args([
            "pr",
            "view",
            &number.to_string(),
            "--repo",
            &format!("{owner}/{repo}"),
            "--json",
            "comments",
        ])
        .output()
        .await
        .context("failed to run `gh pr view` for conversation comments")?;
    if !out.status.success() {
        bail!(
            "fetching conversation comments failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    parse_conversation(&out.stdout)
}

/// Review threads and conversation comments.
pub async fn fetch_discussion(owner: &str, repo: &str, number: u64) -> Result<(Vec<RawThread>, Vec<RawComment>)> {
    tokio::try_join!(
        fetch_review_threads(owner, repo, number),
        fetch_conversation(owner, repo, number)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_threads_page_and_cursor() {
        let json = br#"{"data":{"repository":{"pullRequest":{"reviewThreads":{
            "pageInfo":{"hasNextPage":true,"endCursor":"CUR"},
            "nodes":[
              {"id":"PRRT_1","path":"src/a.rs","line":12,"originalLine":10,"diffSide":"RIGHT","isOutdated":false,"isResolved":true,
               "comments":{"nodes":[
                 {"id":"C1","author":{"login":"alice"},"createdAt":"2026-01-02T03:04:05Z","body":"hm","state":"SUBMITTED","originalCommit":{"oid":"abc"}},
                 {"id":"C2","author":null,"createdAt":"2026-01-03T03:04:05Z","body":"reply","state":"SUBMITTED","originalCommit":null},
                 {"id":"C3","author":{"login":"me"},"createdAt":"2026-01-04T03:04:05Z","body":"unsent","state":"PENDING","originalCommit":null}
               ]}},
              {"id":"PRRT_2","path":"b.rs","line":null,"originalLine":3,"diffSide":"LEFT","isOutdated":true,"isResolved":false,
               "comments":{"nodes":[
                 {"id":"C4","author":{"login":"me"},"createdAt":"2026-01-04T03:04:05Z","body":"only pending","state":"PENDING","originalCommit":{"oid":"def"}}
               ]}}
            ]}}}}}"#;
        let (threads, next) = parse_threads_page(json).unwrap();
        assert_eq!(next.as_deref(), Some("CUR"));
        assert_eq!(threads.len(), 1, "a thread with only pending comments is dropped");
        let t = &threads[0];
        assert_eq!((t.id.as_str(), t.path.as_str(), t.line, t.original_line), ("PRRT_1", "src/a.rs", Some(12), Some(10)));
        assert_eq!((t.diff_side.as_str(), t.is_outdated, t.is_resolved), ("RIGHT", false, true));
        assert_eq!(t.comments.len(), 2);
        assert_eq!(t.comments[0].original_commit_oid.as_deref(), Some("abc"));
        assert_eq!(t.comments[1].author, "ghost");
    }

    #[test]
    fn last_page_has_no_cursor() {
        let json = br#"{"data":{"repository":{"pullRequest":{"reviewThreads":{
            "pageInfo":{"hasNextPage":false,"endCursor":"CUR"},"nodes":[]}}}}}"#;
        assert_eq!(parse_threads_page(json).unwrap(), (vec![], None));
    }

    #[test]
    fn parses_conversation_comments() {
        let json = br#"{"comments":[{"id":"IC_1","author":{"login":"bob"},"body":"hi","createdAt":"2026-02-01T00:00:00Z","url":"u"}]}"#;
        assert_eq!(
            parse_conversation(json).unwrap(),
            vec![RawComment {
                id: "IC_1".into(),
                author: "bob".into(),
                created_at: "2026-02-01T00:00:00Z".into(),
                body: "hi".into(),
                original_commit_oid: None,
            }]
        );
    }

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
