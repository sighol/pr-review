use std::collections::HashMap;

use serde::Serialize;

use crate::github::{RawComment, RawThread};

/// Id of the thread holding the PR's top-level comments; the frontend's `CONVERSATION_ID`.
pub const CONVERSATION_ID: &str = "conversation";

/// A thread in the shape the frontend's `FakeThread` expects (before it formats times).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub id: String,
    /// None for the PR conversation.
    pub file: Option<String>,
    /// "old" (GitHub LEFT, the PR base) or "new" (RIGHT, the PR head).
    pub side: &'static str,
    /// Current line; None when the thread is outdated.
    pub line: Option<i64>,
    pub original_line: i64,
    /// Number of the commit the thread was made on, or 0 if it is not one of the PR's commits
    /// (force-pushed away) or the thread has no commit (the conversation).
    pub commit: usize,
    pub github_resolved: bool,
    pub comments: Vec<Comment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub id: String,
    pub author: String,
    pub created_at: String,
    pub body: String,
}

fn comment(c: RawComment) -> Comment {
    Comment { id: c.id, author: c.author, created_at: c.created_at, body: c.body }
}

/// `commit_numbers` maps a commit oid to its 1-based number in the PR.
pub fn build_threads(
    raw: Vec<RawThread>,
    conversation: Vec<RawComment>,
    commit_numbers: &HashMap<&str, usize>,
) -> Vec<Thread> {
    let mut threads: Vec<Thread> = raw
        .into_iter()
        .map(|t| {
            let commit = t
                .comments
                .first()
                .and_then(|c| c.original_commit_oid.as_deref())
                .and_then(|oid| commit_numbers.get(oid).copied())
                .unwrap_or(0);
            Thread {
                id: t.id,
                file: Some(t.path),
                side: if t.diff_side == "LEFT" { "old" } else { "new" },
                line: if t.is_outdated { None } else { t.line },
                original_line: t.original_line.or(t.line).unwrap_or(0),
                commit,
                github_resolved: t.is_resolved,
                comments: t.comments.into_iter().map(comment).collect(),
            }
        })
        .collect();

    if !conversation.is_empty() {
        threads.push(Thread {
            id: CONVERSATION_ID.to_string(),
            file: None,
            side: "new",
            line: Some(0),
            original_line: 0,
            commit: 0,
            github_resolved: false,
            comments: conversation.into_iter().map(comment).collect(),
        });
    }
    threads
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_comment(id: &str, commit: Option<&str>) -> RawComment {
        RawComment {
            id: id.into(),
            author: "alice".into(),
            created_at: "2026-01-02T03:04:05Z".into(),
            body: format!("body {id}"),
            original_commit_oid: commit.map(String::from),
        }
    }

    fn raw_thread(id: &str, side: &str, line: Option<i64>, outdated: bool, commit: Option<&str>) -> RawThread {
        RawThread {
            id: id.into(),
            path: "src/a.rs".into(),
            line,
            original_line: Some(7),
            diff_side: side.into(),
            is_outdated: outdated,
            is_resolved: false,
            comments: vec![raw_comment("c1", commit), raw_comment("c2", None)],
        }
    }

    #[test]
    fn maps_commit_side_and_lines() {
        let numbers = HashMap::from([("aaa", 1), ("bbb", 2)]);
        let threads = build_threads(
            vec![
                raw_thread("t1", "RIGHT", Some(9), false, Some("bbb")),
                raw_thread("t2", "LEFT", Some(4), false, Some("aaa")),
                raw_thread("t3", "RIGHT", None, true, Some("aaa")),
            ],
            vec![],
            &numbers,
        );
        assert_eq!(threads.len(), 3);
        assert_eq!((threads[0].side, threads[0].line, threads[0].original_line, threads[0].commit), ("new", Some(9), 7, 2));
        assert_eq!((threads[1].side, threads[1].commit), ("old", 1));
        assert_eq!((threads[2].line, threads[2].commit), (None, 1));
        assert_eq!(threads[0].file.as_deref(), Some("src/a.rs"));
        assert_eq!(threads[0].comments.len(), 2);
    }

    #[test]
    fn outdated_threads_have_no_current_line_even_if_github_sends_one() {
        let t = build_threads(vec![raw_thread("t", "RIGHT", Some(9), true, None)], vec![], &HashMap::new());
        assert_eq!(t[0].line, None);
    }

    #[test]
    fn a_commit_outside_the_pr_is_unknown() {
        let numbers = HashMap::from([("aaa", 1)]);
        let t = build_threads(
            vec![
                raw_thread("gone", "RIGHT", Some(9), false, Some("force-pushed-away")),
                raw_thread("none", "RIGHT", Some(9), false, None),
            ],
            vec![],
            &numbers,
        );
        assert_eq!((t[0].commit, t[1].commit), (0, 0));
    }

    #[test]
    fn conversation_becomes_one_thread_only_when_it_has_comments() {
        let none = build_threads(vec![], vec![], &HashMap::new());
        assert!(none.is_empty());

        let t = build_threads(vec![], vec![raw_comment("IC_1", None), raw_comment("IC_2", None)], &HashMap::new());
        assert_eq!(t.len(), 1);
        assert_eq!((t[0].id.as_str(), t[0].file.as_deref(), t[0].comments.len()), ("conversation", None, 2));
    }

    #[test]
    fn serializes_for_the_frontend() {
        let t = build_threads(vec![raw_thread("t", "LEFT", Some(1), false, None)], vec![], &HashMap::new());
        let v = serde_json::to_value(&t[0]).unwrap();
        assert_eq!(v["side"], "old");
        assert_eq!(v["originalLine"], 7);
        assert_eq!(v["githubResolved"], false);
        assert_eq!(v["comments"][0]["createdAt"], "2026-01-02T03:04:05Z");
    }
}
