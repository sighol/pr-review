use std::future::Future;
use std::str::FromStr;

use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;
use serde_json::{Value, json};

use crate::directives::{self, Applied, Directives};
use crate::github;
use crate::threads::CONVERSATION_ID;

/// What the review says about the PR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Comment,
    Approve,
    RequestChanges,
}

impl Event {
    fn graphql(self) -> &'static str {
        match self {
            Event::Comment => "COMMENT",
            Event::Approve => "APPROVE",
            Event::RequestChanges => "REQUEST_CHANGES",
        }
    }
}

impl FromStr for Event {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "COMMENT" => Ok(Event::Comment),
            "APPROVE" => Ok(Event::Approve),
            "REQUEST_CHANGES" => Ok(Event::RequestChanges),
            other => bail!("unknown review event '{other}'"),
        }
    }
}

/// A new thread on a line (or line range) of the diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewComment {
    pub path: String,
    pub body: String,
    /// "old" (LEFT, the PR base) or "new" (RIGHT, the PR head).
    pub side: String,
    pub start_line: i64,
    pub end_line: i64,
    /// Resolve the new thread locally once it is posted.
    pub resolve: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadReply {
    pub thread_id: String,
    pub body: String,
    /// Resolve the thread locally through this reply once it is posted.
    pub resolve: bool,
}

#[derive(Debug, Clone)]
pub struct Submission {
    pub event: Event,
    pub body: String,
    /// The PR head the app fetched; every comment is anchored to it.
    pub head_oid: String,
    pub comments: Vec<NewComment>,
    pub replies: Vec<ThreadReply>,
    /// A reply to the PR conversation; posted after the review, as a plain comment.
    pub conversation_reply: Option<String>,
    /// Resolve the conversation locally through that reply once it is posted.
    pub conversation_reply_resolve: bool,
    /// Resolve the conversation locally through the review itself (an approval).
    pub resolve_conversation: bool,
    /// Label and assignee changes, applied after the review.
    pub directives: Directives,
    /// False when there is only label/assignee changes and nothing to say to GitHub as a review.
    pub post_review: bool,
}

/// What a submission created on GitHub, for resolving threads afterwards.
#[derive(Debug, Default)]
pub struct Posted {
    /// Node ids of the comments and the review that were posted.
    pub comment_ids: Vec<String>,
    /// Ids of the threads to resolve through what was posted (may repeat).
    pub resolve_threads: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    /// None when no review was posted (only directives).
    pub review_url: Option<String>,
    /// Set when the review went through but the conversation comment did not.
    pub conversation_error: Option<String>,
    pub directives: Applied,
    #[serde(skip)]
    pub posted: Posted,
}

/// Anything that can run a GraphQL query; `gh` in production, a fake in tests.
pub trait GraphQl {
    fn call(&self, query: &str, variables: Value) -> impl Future<Output = Result<Value>>;
}

pub struct Gh;

impl GraphQl for Gh {
    async fn call(&self, query: &str, variables: Value) -> Result<Value> {
        github::graphql(query, variables).await
    }
}

const PR_QUERY: &str = "query($owner: String!, $repo: String!, $number: Int!) {
  repository(owner: $owner, name: $repo) {
    pullRequest(number: $number) { id headRefOid reviews(states: PENDING, first: 1) { totalCount } }
  }
}";
const ADD_REVIEW: &str = "mutation($pr: ID!, $commit: GitObjectID!) {
  addPullRequestReview(input: {pullRequestId: $pr, commitOID: $commit}) { pullRequestReview { id } }
}";
const ADD_THREAD: &str = "mutation($input: AddPullRequestReviewThreadInput!) {
  addPullRequestReviewThread(input: $input) { thread { id comments(first: 1) { nodes { id } } } }
}";
const ADD_REPLY: &str = "mutation($input: AddPullRequestReviewThreadReplyInput!) {
  addPullRequestReviewThreadReply(input: $input) { comment { id } }
}";
const SUBMIT: &str = "mutation($input: SubmitPullRequestReviewInput!) {
  submitPullRequestReview(input: $input) { pullRequestReview { id url } }
}";
const DELETE: &str = "mutation($id: ID!) {
  deletePullRequestReview(input: {pullRequestReviewId: $id}) { pullRequestReview { id } }
}";
const ADD_COMMENT: &str = "mutation($input: AddCommentInput!) {
  addComment(input: $input) { commentEdge { node { id url } } }
}";

fn diff_side(side: &str) -> &'static str {
    if side == "old" { "LEFT" } else { "RIGHT" }
}

fn thread_input(review_id: &str, c: &NewComment) -> Value {
    let mut input = json!({
        "pullRequestReviewId": review_id,
        "path": c.path,
        "body": c.body,
        "line": c.end_line,
        "side": diff_side(&c.side),
    });
    if c.start_line != c.end_line {
        input["startLine"] = json!(c.start_line);
        input["startSide"] = json!(diff_side(&c.side));
    }
    input
}

fn string_at<'a>(v: &'a Value, pointer: &str) -> Result<&'a str> {
    v.pointer(pointer)
        .and_then(Value::as_str)
        .with_context(|| format!("GitHub's response had no {pointer}"))
}

/// Creates a pending review, fills it, and submits it. If anything goes wrong while the
/// review is pending it is deleted, so a failure never leaves a half-posted review.
pub async fn submit(
    gh: &impl GraphQl,
    owner: &str,
    repo: &str,
    number: u64,
    sub: &Submission,
    progress: &(dyn Fn(&str) + Send + Sync),
) -> Result<Outcome> {
    progress("Checking the PR on GitHub...");
    let pr = gh
        .call(
            PR_QUERY,
            json!({ "owner": owner, "repo": repo, "number": number }),
        )
        .await?;
    let pr_id = string_at(&pr, "/repository/pullRequest/id")?.to_string();
    if sub.post_review && string_at(&pr, "/repository/pullRequest/headRefOid")? != sub.head_oid {
        bail!(
            "The PR has new commits on GitHub since pr-review fetched it. \
             Restart pr-review to pick them up, then submit again."
        );
    }
    if sub.post_review
        && pr
            .pointer("/repository/pullRequest/reviews/totalCount")
            .and_then(Value::as_u64)
            != Some(0)
    {
        bail!(
            "You already have a pending review on this PR on GitHub. Submit or discard it there first."
        );
    }

    // Checked before anything is posted, so a typo in a label never leaves a half-done result.
    let plan = if sub.directives.is_empty() {
        None
    } else {
        progress("Looking up labels and assignees...");
        Some(directives::plan(gh, owner, repo, number, &pr_id, &sub.directives).await?)
    };

    let mut posted = Posted::default();
    let review_url = if sub.post_review {
        progress("Creating the pending review...");
        let review = gh
            .call(ADD_REVIEW, json!({ "pr": pr_id, "commit": sub.head_oid }))
            .await?;
        let review_id =
            string_at(&review, "/addPullRequestReview/pullRequestReview/id")?.to_string();
        match fill_and_submit(gh, &review_id, sub, progress).await {
            Ok((url, filled)) => {
                posted = filled;
                if sub.resolve_conversation {
                    posted.resolve_threads.push(CONVERSATION_ID.to_string());
                }
                Some(url)
            }
            Err(e) => {
                progress("Rolling back the pending review...");
                return Err(match gh.call(DELETE, json!({ "id": review_id })).await {
                    Ok(_) => e,
                    Err(cleanup) => anyhow!(
                        "{e:#}. Also could not delete the pending review on GitHub, discard it there: {cleanup:#}"
                    ),
                });
            }
        }
    } else {
        None
    };

    let conversation_error = match sub.conversation_reply.as_deref() {
        Some(body) => {
            progress("Posting your reply to the conversation...");
            match gh
                .call(
                    ADD_COMMENT,
                    json!({ "input": { "subjectId": pr_id, "body": body } }),
                )
                .await
            {
                Ok(added) => {
                    if let Some(id) = added
                        .pointer("/addComment/commentEdge/node/id")
                        .and_then(Value::as_str)
                    {
                        posted.comment_ids.push(id.to_string());
                    }
                    if sub.conversation_reply_resolve {
                        posted.resolve_threads.push(CONVERSATION_ID.to_string());
                    }
                    None
                }
                Err(e) => Some(format!("{e:#}")),
            }
        }
        None => None,
    };
    let directives = match &plan {
        Some(plan) => {
            progress("Applying label and assignee changes...");
            directives::apply(gh, plan).await
        }
        None => Applied::default(),
    };
    Ok(Outcome {
        review_url,
        conversation_error,
        directives,
        posted,
    })
}

async fn fill_and_submit(
    gh: &impl GraphQl,
    review_id: &str,
    sub: &Submission,
    progress: &(dyn Fn(&str) + Send + Sync),
) -> Result<(String, Posted)> {
    let mut posted = Posted::default();
    for c in &sub.comments {
        progress(&format!("Posting comment on {}:{}...", c.path, c.end_line));
        let added = gh
            .call(ADD_THREAD, json!({ "input": thread_input(review_id, c) }))
            .await
            .with_context(|| format!("posting the comment on {}:{}", c.path, c.end_line))?;
        if let Some(id) = added
            .pointer("/addPullRequestReviewThread/thread/comments/nodes/0/id")
            .and_then(Value::as_str)
        {
            posted.comment_ids.push(id.to_string());
        }
        if c.resolve {
            let thread = string_at(&added, "/addPullRequestReviewThread/thread/id")?;
            posted.resolve_threads.push(thread.to_string());
        }
    }
    for r in &sub.replies {
        progress("Posting reply...");
        let added = gh
            .call(
                ADD_REPLY,
                json!({ "input": {
                    "pullRequestReviewId": review_id,
                    "pullRequestReviewThreadId": r.thread_id,
                    "body": r.body,
                } }),
            )
            .await
            .context("posting a reply")?;
        if let Some(id) = added
            .pointer("/addPullRequestReviewThreadReply/comment/id")
            .and_then(Value::as_str)
        {
            posted.comment_ids.push(id.to_string());
        }
        if r.resolve {
            posted.resolve_threads.push(r.thread_id.clone());
        }
    }
    progress("Submitting the review...");
    let submitted = gh
        .call(
            SUBMIT,
            json!({ "input": {
                "pullRequestReviewId": review_id,
                "event": sub.event.graphql(),
                "body": sub.body,
            } }),
        )
        .await
        .map_err(|e| anyhow!("submitting the review: {e:#}"))?;
    posted
        .comment_ids
        .push(string_at(&submitted, "/submitPullRequestReview/pullRequestReview/id")?.to_string());
    let url = string_at(&submitted, "/submitPullRequestReview/pullRequestReview/url")?;
    Ok((url.to_string(), posted))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// Records every call and answers with canned data; `fail` names the calls that should error.
    struct Fake {
        calls: Mutex<Vec<(&'static str, Value)>>,
        fail: &'static [&'static str],
        head: &'static str,
        pending: u64,
    }

    impl Fake {
        fn new() -> Self {
            Fake {
                calls: Mutex::new(vec![]),
                fail: &[],
                head: "HEAD",
                pending: 0,
            }
        }

        fn names(&self) -> Vec<&'static str> {
            self.calls.lock().unwrap().iter().map(|(n, _)| *n).collect()
        }
    }

    impl GraphQl for Fake {
        async fn call(&self, query: &str, variables: Value) -> Result<Value> {
            let (name, response) = match query {
                PR_QUERY => (
                    "pr",
                    json!({"repository": {"pullRequest": {"id": "PR1", "headRefOid": self.head, "reviews": {"totalCount": self.pending}}}}),
                ),
                ADD_REVIEW => (
                    "review",
                    json!({"addPullRequestReview": {"pullRequestReview": {"id": "R1"}}}),
                ),
                ADD_THREAD => (
                    "thread",
                    json!({"addPullRequestReviewThread": {"thread": {"id": "T", "comments": {"nodes": [{"id": "TC"}]}}}}),
                ),
                ADD_REPLY => (
                    "reply",
                    json!({"addPullRequestReviewThreadReply": {"comment": {"id": "C"}}}),
                ),
                SUBMIT => (
                    "submit",
                    json!({"submitPullRequestReview": {"pullRequestReview": {"id": "R1", "url": "https://example/r"}}}),
                ),
                DELETE => (
                    "delete",
                    json!({"deletePullRequestReview": {"pullRequestReview": {"id": "R1"}}}),
                ),
                ADD_COMMENT => (
                    "comment",
                    json!({"addComment": {"commentEdge": {"node": {"id": "IC", "url": "https://example/c"}}}}),
                ),
                directives::LOOKUP => (
                    "lookup",
                    json!({"viewer": {"login": "me"}, "repository": {
                        "labels": {"nodes": [{"id": "L1", "name": "ready"}], "pageInfo": {"hasNextPage": false, "endCursor": null}},
                        "pullRequest": {"labels": {"nodes": []}, "assignees": {"nodes": []}},
                    }}),
                ),
                directives::ADD_LABELS => ("add_labels", json!({})),
                other => panic!("unexpected query {other}"),
            };
            self.calls.lock().unwrap().push((name, variables));
            if self.fail.contains(&name) {
                bail!("boom in {name}");
            }
            Ok(response)
        }
    }

    fn submission() -> Submission {
        Submission {
            event: Event::RequestChanges,
            body: "needs work".into(),
            head_oid: "HEAD".into(),
            comments: vec![
                NewComment {
                    path: "a.rs".into(),
                    body: "one line".into(),
                    side: "new".into(),
                    start_line: 3,
                    end_line: 3,
                    resolve: true,
                },
                NewComment {
                    path: "b.rs".into(),
                    body: "range".into(),
                    side: "old".into(),
                    start_line: 5,
                    end_line: 8,
                    resolve: false,
                },
            ],
            replies: vec![ThreadReply {
                thread_id: "PRRT_1".into(),
                body: "agreed".into(),
                resolve: true,
            }],
            conversation_reply: Some("thanks".into()),
            conversation_reply_resolve: false,
            resolve_conversation: false,
            directives: Directives::default(),
            post_review: true,
        }
    }

    #[tokio::test]
    async fn posts_everything_in_one_review_in_order() {
        let gh = Fake::new();
        let out = submit(&gh, "o", "r", 7, &submission(), &|_| {})
            .await
            .unwrap();
        assert_eq!(
            gh.names(),
            [
                "pr", "review", "thread", "thread", "reply", "submit", "comment"
            ]
        );
        assert_eq!(out.review_url.as_deref(), Some("https://example/r"));
        assert_eq!(out.conversation_error, None);

        let calls = gh.calls.lock().unwrap();
        assert_eq!(calls[1].1, json!({"pr": "PR1", "commit": "HEAD"}));
        assert_eq!(calls[4].1["input"]["pullRequestReviewThreadId"], "PRRT_1");
        assert_eq!(
            calls[5].1["input"],
            json!({"pullRequestReviewId": "R1", "event": "REQUEST_CHANGES", "body": "needs work"})
        );
        assert_eq!(
            calls[6].1["input"],
            json!({"subjectId": "PR1", "body": "thanks"})
        );
    }

    #[tokio::test]
    async fn reports_what_was_posted_and_which_threads_to_resolve() {
        let gh = Fake::new();
        let out = submit(&gh, "o", "r", 7, &submission(), &|_| {})
            .await
            .unwrap();
        // Two new threads (TC each), the reply, the review, and the conversation comment.
        assert_eq!(out.posted.comment_ids, ["TC", "TC", "C", "R1", "IC"]);
        // Only the first new thread and the reply asked to be resolved.
        assert_eq!(out.posted.resolve_threads, ["T", "PRRT_1"]);
    }

    #[tokio::test]
    async fn an_approval_and_a_conversation_reply_resolve_the_conversation() {
        let gh = Fake::new();
        let sub = Submission {
            event: Event::Approve,
            resolve_conversation: true,
            ..submission()
        };
        let out = submit(&gh, "o", "r", 7, &sub, &|_| {}).await.unwrap();
        assert_eq!(out.posted.resolve_threads, ["T", "PRRT_1", CONVERSATION_ID]);

        let sub = Submission {
            conversation_reply_resolve: true,
            ..submission()
        };
        let out = submit(&gh, "o", "r", 7, &sub, &|_| {}).await.unwrap();
        assert_eq!(out.posted.resolve_threads, ["T", "PRRT_1", CONVERSATION_ID]);
    }

    #[tokio::test]
    async fn a_conversation_reply_that_failed_resolves_nothing() {
        let gh = Fake {
            fail: &["comment"],
            ..Fake::new()
        };
        let sub = Submission {
            conversation_reply_resolve: true,
            ..submission()
        };
        let out = submit(&gh, "o", "r", 7, &sub, &|_| {}).await.unwrap();
        assert_eq!(out.posted.resolve_threads, ["T", "PRRT_1"]);
        assert!(!out.posted.comment_ids.contains(&"IC".to_string()));
    }

    #[tokio::test]
    async fn a_failure_while_pending_deletes_the_review_and_never_submits() {
        let gh = Fake {
            fail: &["reply"],
            ..Fake::new()
        };
        let err = submit(&gh, "o", "r", 7, &submission(), &|_| {})
            .await
            .unwrap_err();
        assert!(format!("{err:#}").contains("boom in reply"));
        assert_eq!(
            gh.names(),
            ["pr", "review", "thread", "thread", "reply", "delete"]
        );
    }

    #[tokio::test]
    async fn a_failed_cleanup_is_reported_too() {
        let gh = Fake {
            fail: &["thread", "delete"],
            ..Fake::new()
        };
        let err = format!(
            "{:#}",
            submit(&gh, "o", "r", 7, &submission(), &|_| {})
                .await
                .unwrap_err()
        );
        assert!(err.contains("boom in thread"));
        assert!(err.contains("could not delete the pending review"));
    }

    #[tokio::test]
    async fn an_existing_pending_review_stops_everything() {
        let gh = Fake {
            pending: 1,
            ..Fake::new()
        };
        let err = submit(&gh, "o", "r", 7, &submission(), &|_| {})
            .await
            .unwrap_err();
        assert!(err.to_string().contains("pending review"));
        assert_eq!(gh.names(), ["pr"]);
    }

    #[tokio::test]
    async fn a_moved_head_stops_everything() {
        let gh = Fake {
            head: "NEWER",
            ..Fake::new()
        };
        let err = submit(&gh, "o", "r", 7, &submission(), &|_| {})
            .await
            .unwrap_err();
        assert!(err.to_string().contains("new commits"));
        assert_eq!(gh.names(), ["pr"]);
    }

    #[tokio::test]
    async fn a_failed_conversation_comment_does_not_undo_the_review() {
        let gh = Fake {
            fail: &["comment"],
            ..Fake::new()
        };
        let out = submit(&gh, "o", "r", 7, &submission(), &|_| {})
            .await
            .unwrap();
        assert_eq!(out.review_url.as_deref(), Some("https://example/r"));
        assert!(out.conversation_error.unwrap().contains("boom in comment"));
        assert!(!gh.names().contains(&"delete"));
    }

    #[tokio::test]
    async fn no_conversation_reply_means_no_comment_call() {
        let gh = Fake::new();
        let sub = Submission {
            conversation_reply: None,
            ..submission()
        };
        submit(&gh, "o", "r", 7, &sub, &|_| {}).await.unwrap();
        assert!(!gh.names().contains(&"comment"));
    }

    fn add_label(name: &str) -> Directives {
        Directives {
            add_labels: vec![name.into()],
            ..Directives::default()
        }
    }

    #[tokio::test]
    async fn directives_are_checked_before_the_review_and_applied_after_it() {
        let gh = Fake::new();
        let sub = Submission {
            directives: add_label("Ready"),
            ..submission()
        };
        let out = submit(&gh, "o", "r", 7, &sub, &|_| {}).await.unwrap();
        assert_eq!(
            gh.names(),
            [
                "pr",
                "lookup",
                "review",
                "thread",
                "thread",
                "reply",
                "submit",
                "comment",
                "add_labels"
            ]
        );
        assert_eq!(out.directives.changes, ["Added label ready"]);
        assert_eq!(out.directives.error, None);
    }

    #[tokio::test]
    async fn an_unknown_label_stops_everything_before_the_review() {
        let gh = Fake::new();
        let sub = Submission {
            directives: add_label("reayd"),
            ..submission()
        };
        let err = submit(&gh, "o", "r", 7, &sub, &|_| {}).await.unwrap_err();
        assert!(err.to_string().contains("no such label"));
        assert_eq!(gh.names(), ["pr", "lookup"]);
    }

    #[tokio::test]
    async fn only_directives_post_no_review() {
        let gh = Fake {
            head: "NEWER",
            pending: 1,
            ..Fake::new()
        };
        let sub = Submission {
            event: Event::Comment,
            body: String::new(),
            comments: vec![],
            replies: vec![],
            conversation_reply: None,
            directives: add_label("ready"),
            post_review: false,
            ..submission()
        };
        let out = submit(&gh, "o", "r", 7, &sub, &|_| {}).await.unwrap();
        assert_eq!(out.review_url, None);
        assert_eq!(gh.names(), ["pr", "lookup", "add_labels"]);
    }

    #[tokio::test]
    async fn a_failed_label_change_keeps_the_review() {
        let gh = Fake {
            fail: &["add_labels"],
            ..Fake::new()
        };
        let sub = Submission {
            directives: add_label("ready"),
            ..submission()
        };
        let out = submit(&gh, "o", "r", 7, &sub, &|_| {}).await.unwrap();
        assert_eq!(out.review_url.as_deref(), Some("https://example/r"));
        assert!(out.directives.error.unwrap().contains("boom in add_labels"));
        assert_eq!(out.directives.unapplied, add_label("ready"));
    }

    #[test]
    fn thread_input_anchors_lines_and_sides() {
        let single = NewComment {
            path: "a.rs".into(),
            body: "b".into(),
            side: "new".into(),
            start_line: 3,
            end_line: 3,
            resolve: false,
        };
        assert_eq!(
            thread_input("R", &single),
            json!({"pullRequestReviewId": "R", "path": "a.rs", "body": "b", "line": 3, "side": "RIGHT"})
        );
        let range = NewComment {
            side: "old".into(),
            start_line: 5,
            end_line: 8,
            ..single
        };
        assert_eq!(
            thread_input("R", &range),
            json!({"pullRequestReviewId": "R", "path": "a.rs", "body": "b", "line": 8, "side": "LEFT",
                   "startLine": 5, "startSide": "LEFT"})
        );
    }

    #[test]
    fn parses_events() {
        assert_eq!("APPROVE".parse::<Event>().unwrap(), Event::Approve);
        assert_eq!(
            "REQUEST_CHANGES".parse::<Event>().unwrap(),
            Event::RequestChanges
        );
        assert!("MERGE".parse::<Event>().is_err());
    }
}
