use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::db::{Db, Draft, Resolution, SavedState};
use crate::directives::{Applied, Directives};
use crate::manifest::Manifest;
use crate::submit::{Event, Gh, NewComment, Posted, Submission, ThreadReply, submit};
use crate::threads::{CONVERSATION_ID, Thread, resolve_through};
use crate::{git, github};

/// A draft is stale when the version of its file it was written against (the PR head for the
/// right side, the PR base for the left) is not the current one and the file differs between them.
async fn is_stale(repo: &Path, base: &str, head: &str, d: &Draft) -> bool {
    let (was, now) = if d.side == "old" {
        (&d.base_oid, base)
    } else {
        (&d.head_oid, head)
    };
    if was == now {
        return false;
    }
    if was.is_empty() {
        return true;
    }
    git::file_changed(repo, was, now, &d.file_path).await
}

/// Everything saved for the PR, with out-of-date drafts flagged.
pub async fn load_saved(db: &Db, repo: &Path, base: &str, head: &str) -> Result<SavedState> {
    let mut saved = db.load().await?;
    for d in &mut saved.drafts {
        d.stale = is_stale(repo, base, head, d).await;
    }
    Ok(saved)
}

fn non_blank(s: &str) -> bool {
    !s.trim().is_empty()
}

/// Turns the saved review state into what gets sent, refusing what GitHub would refuse.
/// `body` is the review text with the directives already removed.
pub fn build_submission(
    saved: &SavedState,
    event: Event,
    head_oid: &str,
    body: &str,
    directives: Directives,
) -> Result<Submission> {
    let stale = saved.drafts.iter().filter(|d| d.stale).count();
    if stale > 0 {
        bail!(
            "{stale} draft comment{} may be on the wrong line because the file changed since you wrote {}. \
             Review {} on the Review tab first.",
            if stale == 1 { "" } else { "s" },
            if stale == 1 { "it" } else { "them" },
            if stale == 1 { "it" } else { "them" },
        );
    }

    let comments: Vec<NewComment> = saved
        .drafts
        .iter()
        .filter(|d| non_blank(&d.body))
        .map(|d| NewComment {
            path: d.file_path.clone(),
            body: d.body.clone(),
            side: d.side.clone(),
            start_line: d.start_line,
            end_line: d.end_line,
            resolve: d.resolve,
        })
        .collect();
    let replies: Vec<ThreadReply> = saved
        .replies
        .iter()
        .filter(|r| r.thread_id != CONVERSATION_ID && non_blank(&r.body))
        .map(|r| ThreadReply {
            thread_id: r.thread_id.clone(),
            body: r.body.clone(),
            resolve: r.resolve,
        })
        .collect();
    let conversation = saved
        .replies
        .iter()
        .find(|r| r.thread_id == CONVERSATION_ID && non_blank(&r.body));
    let conversation_reply = conversation.map(|r| r.body.clone());
    let conversation_reply_resolve = conversation.is_some_and(|r| r.resolve);

    let has_body = non_blank(body);
    let nothing_to_say =
        !has_body && comments.is_empty() && replies.is_empty() && conversation_reply.is_none();
    let mut post_review = true;
    match event {
        Event::RequestChanges if !has_body => {
            bail!("GitHub requires a comment to request changes.")
        }
        Event::Comment if nothing_to_say && directives.is_empty() => {
            bail!("There is nothing to send: write a comment first.")
        }
        // Only label and assignee changes: there is no review to post.
        Event::Comment if nothing_to_say => post_review = false,
        _ => {}
    }

    Ok(Submission {
        event,
        body: body.to_string(),
        directives,
        post_review,
        head_oid: head_oid.to_string(),
        comments,
        replies,
        conversation_reply,
        conversation_reply_resolve,
        resolve_conversation: event == Event::Approve && post_review,
    })
}

/// Which threads to resolve, and through which comment, once the review is on GitHub and the
/// discussion has been refetched. See `threads::resolve_through` for the rule.
fn resolutions_after_submit(
    posted: &Posted,
    known: &[Thread],
    refetched: &[Thread],
) -> Vec<Resolution> {
    let known: HashSet<&str> = known
        .iter()
        .flat_map(|t| &t.comments)
        .map(|c| c.id.as_str())
        .collect();
    let posted_ids: HashSet<&str> = posted.comment_ids.iter().map(String::as_str).collect();
    let mut seen = HashSet::new();
    posted
        .resolve_threads
        .iter()
        .filter(|id| seen.insert(id.as_str()))
        .filter_map(|id| {
            let thread = refetched.iter().find(|t| &t.id == id)?;
            let through = resolve_through(thread, &known, &posted_ids)?;
            Some(Resolution {
                thread_id: id.clone(),
                resolved_through: Some(through),
                reopened: false,
            })
        })
        .collect()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitResult {
    /// None when only label and assignee changes were made.
    pub review_url: Option<String>,
    /// Set when the review went through but the conversation comment did not.
    pub conversation_error: Option<String>,
    /// What the label and assignee changes did, and what is left to retry.
    pub directives: Applied,
    /// The PR's discussion after the review, so the new comments show up; None if refetching failed.
    pub threads: Option<Vec<Thread>>,
    pub refresh_error: Option<String>,
    /// Threads resolved locally by this review (already saved); empty if refetching failed.
    pub resolutions: Vec<Resolution>,
}

/// What the reviewer chose in the window: the verdict and the review text. The text arrives with
/// the directives already taken out of it.
#[derive(Debug, Clone)]
pub struct ReviewInput {
    pub event: String,
    pub body: String,
    pub directives: Directives,
}

/// Submits the saved review to GitHub, then clears what it used up and refetches the discussion.
pub async fn submit_saved(
    db: &Db,
    repo: &Path,
    manifest: &Manifest,
    input: ReviewInput,
    progress: &(dyn Fn(&str) + Send + Sync),
) -> Result<SubmitResult> {
    let event: Event = input.event.parse()?;
    let saved = load_saved(db, repo, &manifest.base, &manifest.head).await?;
    let submission =
        build_submission(&saved, event, &manifest.head, &input.body, input.directives)?;
    let key = db.key();

    let outcome = submit(
        &Gh,
        &key.owner,
        &key.repo,
        key.number,
        &submission,
        progress,
    )
    .await?;

    let keep = outcome.conversation_error.as_ref().map(|_| CONVERSATION_ID);
    db.clear_submitted(keep)
        .await
        .context("the review was submitted, but clearing your local drafts failed")?;

    let (threads, refresh_error) =
        match github::fetch_discussion(&key.owner, &key.repo, key.number).await {
            Ok((raw, conversation, reviews)) => (
                Some(manifest.threads_from(raw, conversation, reviews)),
                None,
            ),
            Err(e) => (None, Some(format!("{e:#}"))),
        };

    let resolutions = match (&threads, &manifest.threads) {
        (Some(refetched), Some(known)) => {
            resolutions_after_submit(&outcome.posted, known, refetched)
        }
        _ => vec![],
    };
    for r in &resolutions {
        db.set_resolution(&r.thread_id, r.resolved_through.as_deref(), r.reopened)
            .await
            .context("the review was submitted, but saving the resolved threads failed")?;
    }

    Ok(SubmitResult {
        review_url: outcome.review_url,
        conversation_error: outcome.conversation_error,
        directives: outcome.directives,
        threads,
        refresh_error,
        resolutions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Reply;
    use crate::git::testutil::TestRepo;

    fn draft(side: &str, head_oid: &str, base_oid: &str) -> Draft {
        Draft {
            draft_key: "k".into(),
            file_path: "a.txt".into(),
            side: side.into(),
            start_line: 1,
            end_line: 2,
            body: "text".into(),
            head_oid: head_oid.into(),
            base_oid: base_oid.into(),
            resolve: false,
            stale: false,
        }
    }

    #[tokio::test]
    async fn staleness_follows_the_file_not_the_pr() {
        let repo = TestRepo::new();
        let base = repo.commit(
            "base",
            &[("a.txt", Some("1\n")), ("other.txt", Some("o\n"))],
        );
        let c1 = repo.commit("c1", &[("a.txt", Some("2\n"))]);
        let c2 = repo.commit("c2", &[("other.txt", Some("o2\n"))]);
        let c3 = repo.commit("c3", &[("a.txt", Some("3\n"))]);
        let r = repo.path();

        // Written at the current head: fresh.
        assert!(!is_stale(r, &base, &c3, &draft("new", &c3, &base)).await);
        // a.txt did not change between c1 and c2 (only other.txt did).
        assert!(!is_stale(r, &base, &c2, &draft("new", &c1, &base)).await);
        // a.txt changed between c1 and c3.
        assert!(is_stale(r, &base, &c3, &draft("new", &c1, &base)).await);
        // A head that is gone cannot be compared.
        assert!(is_stale(r, &base, &c3, &draft("new", "deadbeef", &base)).await);
        // Left-side drafts follow the base: unchanged base is fresh; unknown base is stale.
        assert!(!is_stale(r, &base, &c3, &draft("old", "whatever", &base)).await);
        assert!(is_stale(r, &base, &c3, &draft("old", &c3, "")).await);
        // The base moved and the file differs between the old and new base.
        assert!(is_stale(r, &c1, &c3, &draft("old", &c3, &base)).await);
        // The base moved but the file is the same in both bases.
        assert!(!is_stale(r, &c2, &c3, &draft("old", &c3, &c1)).await);
    }

    fn saved() -> SavedState {
        SavedState {
            drafts: vec![draft("new", "h", "b")],
            replies: vec![
                Reply {
                    thread_id: "PRRT_1".into(),
                    body: "agreed".into(),
                    resolve: true,
                },
                Reply {
                    thread_id: CONVERSATION_ID.into(),
                    body: "thanks".into(),
                    resolve: false,
                },
            ],
            review_body: "LGTM".into(),
            ..SavedState::default()
        }
    }

    fn build(saved: &SavedState, event: Event, body: &str) -> Result<Submission> {
        build_submission(saved, event, "h", body, Directives::default())
    }

    fn label(name: &str) -> Directives {
        Directives {
            add_labels: vec![name.into()],
            ..Directives::default()
        }
    }

    #[test]
    fn splits_comments_thread_replies_and_the_conversation_reply() {
        let s = build(&saved(), Event::Approve, "LGTM").unwrap();
        assert_eq!(s.head_oid, "h");
        assert_eq!(s.body, "LGTM");
        assert!(s.post_review);
        assert_eq!(s.comments.len(), 1);
        assert_eq!(
            (
                s.comments[0].path.as_str(),
                s.comments[0].start_line,
                s.comments[0].end_line
            ),
            ("a.txt", 1, 2)
        );
        assert_eq!(
            s.replies,
            vec![ThreadReply {
                thread_id: "PRRT_1".into(),
                body: "agreed".into(),
                resolve: true
            }]
        );
        assert_eq!(s.conversation_reply.as_deref(), Some("thanks"));
    }

    #[test]
    fn stale_drafts_block_submitting() {
        let mut s = saved();
        s.drafts[0].stale = true;
        let err = build(&s, Event::Comment, "").unwrap_err().to_string();
        assert!(err.contains("1 draft comment may be on the wrong line"));
    }

    #[test]
    fn refuses_what_github_refuses() {
        let empty = SavedState::default();
        assert!(build(&empty, Event::Comment, "").is_err());
        assert!(build(&empty, Event::Approve, "").is_ok());

        let no_body = SavedState {
            drafts: vec![draft("new", "h", "b")],
            ..SavedState::default()
        };
        assert!(build(&no_body, Event::RequestChanges, "").is_err());
        assert!(build(&no_body, Event::Comment, "").is_ok());
    }

    #[test]
    fn a_conversation_reply_alone_is_enough_to_comment() {
        let only = SavedState {
            replies: vec![Reply {
                thread_id: CONVERSATION_ID.into(),
                body: "hi".into(),
                resolve: false,
            }],
            ..SavedState::default()
        };
        assert!(build(&only, Event::Comment, "").is_ok());
    }

    #[test]
    fn directives_alone_make_a_comment_that_posts_no_review() {
        let empty = SavedState::default();
        let s = build_submission(&empty, Event::Comment, "h", "", label("x")).unwrap();
        assert!(!s.post_review);
        assert_eq!(s.directives, label("x"));
    }

    #[test]
    fn directives_do_not_satisfy_request_changes_and_approve_still_posts() {
        let empty = SavedState::default();
        assert!(build_submission(&empty, Event::RequestChanges, "h", "", label("x")).is_err());
        let s = build_submission(&empty, Event::Approve, "h", "", label("x")).unwrap();
        assert!(s.post_review);
        let s = build_submission(&empty, Event::Comment, "h", "text", label("x")).unwrap();
        assert!(s.post_review);
    }

    #[test]
    fn the_resolve_flags_reach_the_submission() {
        let mut s = saved();
        s.drafts[0].resolve = true;
        s.replies[1].resolve = true;
        let sub = build(&s, Event::Comment, "").unwrap();
        assert!(sub.comments[0].resolve);
        assert!(sub.replies[0].resolve);
        assert!(sub.conversation_reply_resolve);
        assert!(
            !sub.resolve_conversation,
            "only an approval resolves the conversation"
        );
    }

    #[test]
    fn only_an_approval_that_posts_a_review_resolves_the_conversation() {
        let s = saved();
        assert!(build(&s, Event::Approve, "").unwrap().resolve_conversation);
        assert!(!build(&s, Event::Comment, "").unwrap().resolve_conversation);
        assert!(
            !build(&s, Event::RequestChanges, "x")
                .unwrap()
                .resolve_conversation
        );
    }

    fn comment_ids(thread: &str, ids: &[&str]) -> Thread {
        use crate::threads::Comment;
        Thread {
            id: thread.into(),
            file: None,
            side: "new",
            line: None,
            start_line: None,
            original_line: 0,
            commit: 0,
            github_resolved: false,
            comments: ids
                .iter()
                .map(|id| Comment {
                    id: id.to_string(),
                    author: "a".into(),
                    created_at: String::new(),
                    body: String::new(),
                    url: String::new(),
                    review_state: None,
                })
                .collect(),
        }
    }

    #[test]
    fn resolutions_cover_only_what_you_had_seen_or_posted() {
        let known = vec![
            comment_ids("t1", &["a", "b"]),
            comment_ids(CONVERSATION_ID, &["c1"]),
        ];
        let refetched = vec![
            // Someone replied to t1 between loading it and sending your reply.
            comment_ids("t1", &["a", "b", "theirs", "mine"]),
            // A new thread of yours.
            comment_ids("new", &["tc"]),
            // The conversation: your review came after a comment you had not seen.
            comment_ids(CONVERSATION_ID, &["c1", "c2", "review"]),
        ];
        let posted = Posted {
            comment_ids: vec!["mine".into(), "tc".into(), "review".into()],
            resolve_threads: vec![
                "t1".into(),
                "new".into(),
                CONVERSATION_ID.into(),
                "t1".into(),
                "gone".into(),
            ],
        };
        let r = resolutions_after_submit(&posted, &known, &refetched);
        let got: Vec<_> = r
            .iter()
            .map(|r| (r.thread_id.as_str(), r.resolved_through.as_deref()))
            .collect();
        assert_eq!(
            got,
            [
                ("t1", Some("b")),
                ("new", Some("tc")),
                (CONVERSATION_ID, Some("c1"))
            ],
            "duplicates are dropped, and a thread missing from the refetch is skipped"
        );
        assert!(r.iter().all(|r| !r.reopened));
    }
}
