use std::time::Instant;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tracing::{debug, warn};

/// Logs a finished `gh` call: debug with timing when it worked, a warning with stderr when not.
fn log_gh(what: &str, started: Instant, out: &std::process::Output) {
    if out.status.success() {
        debug!("gh {what} -> {} in {:?}", out.status, started.elapsed());
    } else {
        warn!(
            "gh {what} failed ({}) after {:?}: {}",
            out.status,
            started.elapsed(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
}

/// The PR fields the UI shows (serialized into the manifest as `pr`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PrMeta {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub body: String,
    pub status: PrStatus,
    /// The PR's page on GitHub.
    pub url: String,
}

/// Where the PR stands on GitHub.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PrStatus {
    Draft,
    Open,
    Merged,
    Closed,
}

fn pr_status(state: &str, is_draft: bool) -> PrStatus {
    match state {
        "MERGED" => PrStatus::Merged,
        "CLOSED" => PrStatus::Closed,
        _ if is_draft => PrStatus::Draft,
        _ => PrStatus::Open,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct PrInfo {
    pub meta: PrMeta,
    pub base_ref_name: String,
    /// The commit GitHub merged this PR with, when it has been merged. Its first parent is the
    /// base branch's tip just before the merge, which `git::fetch_pr` falls back to when the
    /// base branch has since moved past the merge (so merge-base(base, head) degenerates to
    /// head).
    pub merge_commit: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PrViewJson {
    number: u64,
    title: String,
    body: String,
    author: Option<AuthorJson>,
    base_ref_name: String,
    state: String,
    is_draft: bool,
    #[serde(default)]
    url: String,
    #[serde(default)]
    merge_commit: Option<MergeCommitJson>,
}

#[derive(Deserialize)]
struct MergeCommitJson {
    oid: String,
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
    let v: PrViewJson =
        serde_json::from_slice(json).context("unexpected output from `gh pr view`")?;
    Ok(PrInfo {
        meta: PrMeta {
            number: v.number,
            title: v.title,
            body: v.body,
            author: ghost_if_none(v.author),
            status: pr_status(&v.state, v.is_draft),
            url: v.url,
        },
        base_ref_name: v.base_ref_name,
        merge_commit: v.merge_commit.map(|m| m.oid),
    })
}

/// PR metadata via the GitHub CLI, which brings its own authentication.
pub async fn pr_view(owner: &str, repo: &str, number: u64) -> Result<PrInfo> {
    let started = Instant::now();
    let out = Command::new("gh")
        .args([
            "pr",
            "view",
            &number.to_string(),
            "--repo",
            &format!("{owner}/{repo}"),
            "--json",
            "number,title,body,author,baseRefName,state,isDraft,url,mergeCommit",
        ])
        .output()
        .await
        .context("failed to run `gh` (is the GitHub CLI installed?)")?;
    log_gh(&format!("pr view {number}"), started, &out);
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

    let started = Instant::now();
    let body = serde_json::json!({ "query": query, "variables": variables });
    let mut child = Command::new("gh")
        .args([
            "api",
            "graphql",
            "-H",
            "Content-Type: application/json",
            "--input",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to run `gh` (is the GitHub CLI installed?)")?;
    let mut stdin = child.stdin.take().context("could not open gh's stdin")?;
    stdin.write_all(body.to_string().as_bytes()).await?;
    drop(stdin);
    let out = child.wait_with_output().await?;
    log_gh("api graphql", started, &out);
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
    response
        .get("data")
        .cloned()
        .context("GitHub's response had no data")
}

/// One comment, in a review thread or in the PR conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawComment {
    pub id: String,
    pub author: String,
    /// ISO 8601, as GitHub returns it.
    pub created_at: String,
    pub body: String,
    /// The comment's permalink on github.com.
    pub url: String,
    /// The commit the comment was made on; unset in the conversation.
    pub original_commit_oid: Option<String>,
}

/// A submitted review on the PR: its verdict and top-level summary text, separate from its inline
/// comments (see `RawThread`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawReview {
    pub id: String,
    pub author: String,
    /// ISO 8601, as GitHub returns it.
    pub submitted_at: String,
    pub body: String,
    /// The review's permalink on github.com.
    pub url: String,
    /// "APPROVED", "CHANGES_REQUESTED", "COMMENTED", or "DISMISSED".
    pub state: String,
}

/// An inline review thread as GitHub describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawThread {
    pub id: String,
    pub path: String,
    /// Current line; None when the thread is outdated.
    pub line: Option<i64>,
    pub original_line: Option<i64>,
    /// First line of a multi-line thread (it spans `start_line..=line`); None for a single line.
    pub start_line: Option<i64>,
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
          startLine
          diffSide
          isOutdated
          isResolved
          comments(first: 100) {
            nodes {
              id
              author { login }
              createdAt
              body
              url
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
    start_line: Option<i64>,
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
    url: String,
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
    let page: GqlEnvelope =
        serde_json::from_slice(json).context("unexpected output from `gh api graphql`")?;
    let threads = page.data.repository.pull_request.review_threads;
    let next = threads
        .page_info
        .has_next_page
        .then_some(threads.page_info.end_cursor)
        .flatten();
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
                    url: c.url,
                    original_commit_oid: c.original_commit.map(|o| o.oid),
                })
                .collect();
            (!comments.is_empty()).then_some(RawThread {
                id: t.id,
                path: t.path,
                line: t.line,
                original_line: t.original_line,
                start_line: t.start_line,
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
        let started = Instant::now();
        let out = Command::new("gh")
            .args(&args)
            .output()
            .await
            .context("failed to run `gh api graphql` for review threads")?;
        log_gh("api graphql (review threads)", started, &out);
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

const REVIEWS_QUERY: &str = r#"
query($owner: String!, $repo: String!, $number: Int!) {
  repository(owner: $owner, name: $repo) {
    pullRequest(number: $number) {
      reviews(first: 100) {
        nodes { id author { login } state body submittedAt url }
      }
    }
  }
}
"#;

#[derive(Deserialize)]
struct GqlReviewsEnvelope {
    data: GqlReviewsData,
}

#[derive(Deserialize)]
struct GqlReviewsData {
    repository: GqlReviewsRepository,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GqlReviewsRepository {
    pull_request: GqlReviewsPullRequest,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GqlReviewsPullRequest {
    reviews: GqlReviews,
}

#[derive(Deserialize)]
struct GqlReviews {
    nodes: Vec<GqlReview>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GqlReview {
    id: String,
    author: Option<AuthorJson>,
    state: String,
    body: String,
    submitted_at: Option<String>,
    url: String,
}

/// Drops the viewer's own pending (unsubmitted) review, like `parse_threads_page` drops pending
/// comments.
fn parse_reviews(json: &[u8]) -> Result<Vec<RawReview>> {
    let v: GqlReviewsEnvelope =
        serde_json::from_slice(json).context("unexpected output from `gh api graphql`")?;
    Ok(v.data
        .repository
        .pull_request
        .reviews
        .nodes
        .into_iter()
        .filter(|r| r.state != "PENDING")
        .map(|r| RawReview {
            id: r.id,
            author: ghost_if_none(r.author),
            submitted_at: r.submitted_at.unwrap_or_default(),
            body: r.body,
            url: r.url,
            state: r.state,
        })
        .collect())
}

/// Every submitted review on the PR (not your own pending one), via `gh api graphql`.
pub async fn fetch_reviews(owner: &str, repo: &str, number: u64) -> Result<Vec<RawReview>> {
    let started = Instant::now();
    let out = Command::new("gh")
        .args([
            "api",
            "graphql",
            "-f",
            &format!("query={REVIEWS_QUERY}"),
            "-f",
            &format!("owner={owner}"),
            "-f",
            &format!("repo={repo}"),
            "-F",
            &format!("number={number}"),
        ])
        .output()
        .await
        .context("failed to run `gh api graphql` for reviews")?;
    log_gh("api graphql (reviews)", started, &out);
    if !out.status.success() {
        bail!(
            "fetching reviews failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    parse_reviews(&out.stdout)
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
    url: String,
}

fn parse_conversation(json: &[u8]) -> Result<Vec<RawComment>> {
    let v: ConversationJson =
        serde_json::from_slice(json).context("unexpected output from `gh pr view`")?;
    Ok(v.comments
        .into_iter()
        .map(|c| RawComment {
            id: c.id,
            author: ghost_if_none(c.author),
            created_at: c.created_at,
            body: c.body,
            url: c.url,
            original_commit_oid: None,
        })
        .collect())
}

/// The PR's top-level comments (not attached to code), oldest first.
pub async fn fetch_conversation(owner: &str, repo: &str, number: u64) -> Result<Vec<RawComment>> {
    let started = Instant::now();
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
    log_gh("pr view (conversation)", started, &out);
    if !out.status.success() {
        bail!(
            "fetching conversation comments failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    parse_conversation(&out.stdout)
}

/// Review threads, conversation comments, and submitted reviews.
pub async fn fetch_discussion(
    owner: &str,
    repo: &str,
    number: u64,
) -> Result<(Vec<RawThread>, Vec<RawComment>, Vec<RawReview>)> {
    tokio::try_join!(
        fetch_review_threads(owner, repo, number),
        fetch_conversation(owner, repo, number),
        fetch_reviews(owner, repo, number),
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
              {"id":"PRRT_1","path":"src/a.rs","line":12,"originalLine":10,"startLine":10,"diffSide":"RIGHT","isOutdated":false,"isResolved":true,
               "comments":{"nodes":[
                 {"id":"C1","author":{"login":"alice"},"createdAt":"2026-01-02T03:04:05Z","body":"hm","url":"https://github.com/o/r/pull/1#discussion_r1","state":"SUBMITTED","originalCommit":{"oid":"abc"}},
                 {"id":"C2","author":null,"createdAt":"2026-01-03T03:04:05Z","body":"reply","url":"https://github.com/o/r/pull/1#discussion_r2","state":"SUBMITTED","originalCommit":null},
                 {"id":"C3","author":{"login":"me"},"createdAt":"2026-01-04T03:04:05Z","body":"unsent","url":"u3","state":"PENDING","originalCommit":null}
               ]}},
              {"id":"PRRT_2","path":"b.rs","line":null,"originalLine":3,"startLine":null,"diffSide":"LEFT","isOutdated":true,"isResolved":false,
               "comments":{"nodes":[
                 {"id":"C4","author":{"login":"me"},"createdAt":"2026-01-04T03:04:05Z","body":"only pending","url":"u4","state":"PENDING","originalCommit":{"oid":"def"}}
               ]}}
            ]}}}}}"#;
        let (threads, next) = parse_threads_page(json).unwrap();
        assert_eq!(next.as_deref(), Some("CUR"));
        assert_eq!(
            threads.len(),
            1,
            "a thread with only pending comments is dropped"
        );
        let t = &threads[0];
        assert_eq!(
            (t.id.as_str(), t.path.as_str(), t.line, t.original_line),
            ("PRRT_1", "src/a.rs", Some(12), Some(10))
        );
        assert_eq!(t.start_line, Some(10));
        assert_eq!(
            (t.diff_side.as_str(), t.is_outdated, t.is_resolved),
            ("RIGHT", false, true)
        );
        assert_eq!(t.comments.len(), 2);
        assert_eq!(t.comments[0].original_commit_oid.as_deref(), Some("abc"));
        assert_eq!(t.comments[1].author, "ghost");
        assert_eq!(
            t.comments[0].url,
            "https://github.com/o/r/pull/1#discussion_r1"
        );
    }

    #[test]
    fn last_page_has_no_cursor() {
        let json = br#"{"data":{"repository":{"pullRequest":{"reviewThreads":{
            "pageInfo":{"hasNextPage":false,"endCursor":"CUR"},"nodes":[]}}}}}"#;
        assert_eq!(parse_threads_page(json).unwrap(), (vec![], None));
    }

    #[test]
    fn parses_reviews_and_drops_the_pending_one() {
        let json = br#"{"data":{"repository":{"pullRequest":{"reviews":{"nodes":[
            {"id":"PRR_1","author":{"login":"alice"},"state":"APPROVED","body":"LGTM","submittedAt":"2026-01-05T00:00:00Z","url":"https://github.com/o/r/pull/1#pullrequestreview-1"},
            {"id":"PRR_2","author":null,"state":"CHANGES_REQUESTED","body":"please fix","submittedAt":"2026-01-06T00:00:00Z","url":"u2"},
            {"id":"PRR_3","author":{"login":"me"},"state":"PENDING","body":"","submittedAt":null,"url":"u3"}
        ]}}}}}"#;
        let reviews = parse_reviews(json).unwrap();
        assert_eq!(reviews.len(), 2, "the pending review is dropped");
        assert_eq!(
            reviews[0],
            RawReview {
                id: "PRR_1".into(),
                author: "alice".into(),
                submitted_at: "2026-01-05T00:00:00Z".into(),
                body: "LGTM".into(),
                url: "https://github.com/o/r/pull/1#pullrequestreview-1".into(),
                state: "APPROVED".into(),
            }
        );
        assert_eq!(reviews[1].author, "ghost");
        assert_eq!(reviews[1].state, "CHANGES_REQUESTED");
    }

    #[test]
    fn parses_conversation_comments() {
        let json = br#"{"comments":[{"id":"IC_1","author":{"login":"bob"},"body":"hi","createdAt":"2026-02-01T00:00:00Z","url":"https://github.com/o/r/pull/1#issuecomment-1"}]}"#;
        assert_eq!(
            parse_conversation(json).unwrap(),
            vec![RawComment {
                id: "IC_1".into(),
                author: "bob".into(),
                created_at: "2026-02-01T00:00:00Z".into(),
                body: "hi".into(),
                url: "https://github.com/o/r/pull/1#issuecomment-1".into(),
                original_commit_oid: None,
            }]
        );
    }

    #[test]
    fn parses_gh_pr_view_output() {
        let json = br#"{"number":13486,"title":"Fix it","body":"Some **markdown**","author":{"login":"sigurd","name":"S"},"baseRefName":"master","state":"OPEN","isDraft":false,"url":"https://github.com/o/r/pull/13486"}"#;
        assert_eq!(
            parse_pr_view(json).unwrap(),
            PrInfo {
                meta: PrMeta {
                    number: 13486,
                    title: "Fix it".into(),
                    author: "sigurd".into(),
                    body: "Some **markdown**".into(),
                    status: PrStatus::Open,
                    url: "https://github.com/o/r/pull/13486".into(),
                },
                base_ref_name: "master".into(),
                merge_commit: None,
            }
        );
    }

    #[test]
    fn deleted_author_becomes_ghost() {
        let json = br#"{"number":1,"title":"t","body":"","author":null,"baseRefName":"main","state":"OPEN","isDraft":false}"#;
        assert_eq!(parse_pr_view(json).unwrap().meta.author, "ghost");
    }

    #[test]
    fn draft_pr_is_draft_even_though_open() {
        let json = br#"{"number":1,"title":"t","body":"","author":null,"baseRefName":"main","state":"OPEN","isDraft":true}"#;
        assert_eq!(parse_pr_view(json).unwrap().meta.status, PrStatus::Draft);
    }

    #[test]
    fn merged_pr_status_wins_over_draft_flag() {
        let json = br#"{"number":1,"title":"t","body":"","author":null,"baseRefName":"main","state":"MERGED","isDraft":false}"#;
        assert_eq!(parse_pr_view(json).unwrap().meta.status, PrStatus::Merged);
    }

    #[test]
    fn closed_pr_status() {
        let json = br#"{"number":1,"title":"t","body":"","author":null,"baseRefName":"main","state":"CLOSED","isDraft":false}"#;
        assert_eq!(parse_pr_view(json).unwrap().meta.status, PrStatus::Closed);
    }

    #[test]
    fn merged_pr_carries_its_merge_commit() {
        let json = br#"{"number":1,"title":"t","body":"","author":null,"baseRefName":"main","state":"MERGED","isDraft":false,"mergeCommit":{"oid":"abc123"}}"#;
        assert_eq!(
            parse_pr_view(json).unwrap().merge_commit,
            Some("abc123".into())
        );
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_pr_view(b"not json").is_err());
    }
}
