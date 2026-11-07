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
    /// Current line (the last one of a range); None when the thread is outdated.
    pub line: Option<i64>,
    /// First line of a multi-line thread, so it covers `start_line..=line`; None for a single line
    /// or an outdated thread.
    pub start_line: Option<i64>,
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
    /// The comment's permalink on github.com.
    pub url: String,
}

fn comment(c: RawComment) -> Comment {
    Comment {
        id: c.id,
        author: c.author,
        created_at: c.created_at,
        body: c.body,
        url: c.url,
    }
}

/// Moves threads written on a file's old name to its new name. `renames` maps a path at the PR
/// base to the same file's path at the head (see `Manifest::renames`).
pub fn follow_renames(threads: &mut [Thread], renames: &HashMap<&str, &str>) {
    for t in threads {
        if let Some(new) = t.file.as_deref().and_then(|f| renames.get(f)) {
            t.file = Some(new.to_string());
        }
    }
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
                start_line: if t.is_outdated {
                    None
                } else {
                    t.start_line.filter(|s| t.line.is_some_and(|l| *s < l))
                },
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
            start_line: None,
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
            url: format!("https://github.com/o/r/pull/1#discussion_{id}"),
            original_commit_oid: commit.map(String::from),
        }
    }

    fn raw_thread(
        id: &str,
        side: &str,
        line: Option<i64>,
        outdated: bool,
        commit: Option<&str>,
    ) -> RawThread {
        RawThread {
            id: id.into(),
            path: "src/a.rs".into(),
            line,
            original_line: Some(7),
            start_line: None,
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
        assert_eq!(
            (
                threads[0].side,
                threads[0].line,
                threads[0].original_line,
                threads[0].commit
            ),
            ("new", Some(9), 7, 2)
        );
        assert_eq!((threads[1].side, threads[1].commit), ("old", 1));
        assert_eq!((threads[2].line, threads[2].commit), (None, 1));
        assert_eq!(threads[0].file.as_deref(), Some("src/a.rs"));
        assert_eq!(threads[0].comments.len(), 2);
    }

    #[test]
    fn a_multi_line_thread_keeps_its_first_line() {
        let multi = RawThread {
            line: Some(51),
            start_line: Some(49),
            ..raw_thread("multi", "RIGHT", Some(51), false, None)
        };
        let single_with_equal_start = RawThread {
            start_line: Some(9),
            ..raw_thread("single", "RIGHT", Some(9), false, None)
        };
        let outdated = RawThread {
            line: None,
            start_line: Some(3),
            is_outdated: true,
            ..raw_thread("old", "RIGHT", None, true, None)
        };
        let t = build_threads(
            vec![multi, single_with_equal_start, outdated],
            vec![],
            &HashMap::new(),
        );
        assert_eq!((t[0].start_line, t[0].line), (Some(49), Some(51)));
        assert_eq!(
            t[1].start_line, None,
            "a range of one line is a single line"
        );
        assert_eq!(t[2].start_line, None);
        assert_eq!(serde_json::to_value(&t[0]).unwrap()["startLine"], 49);
    }

    #[test]
    fn outdated_threads_have_no_current_line_even_if_github_sends_one() {
        let t = build_threads(
            vec![raw_thread("t", "RIGHT", Some(9), true, None)],
            vec![],
            &HashMap::new(),
        );
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

        let t = build_threads(
            vec![],
            vec![raw_comment("IC_1", None), raw_comment("IC_2", None)],
            &HashMap::new(),
        );
        assert_eq!(t.len(), 1);
        assert_eq!(
            (t[0].id.as_str(), t[0].file.as_deref(), t[0].comments.len()),
            ("conversation", None, 2)
        );
    }

    #[test]
    fn threads_on_an_old_name_follow_the_rename() {
        let mut t = build_threads(
            vec![raw_thread("t", "RIGHT", Some(1), false, None)],
            vec![raw_comment("IC_1", None)],
            &HashMap::new(),
        );
        let old = t[0].file.clone().unwrap();
        follow_renames(&mut t, &HashMap::from([(old.as_str(), "new/name.rs")]));
        assert_eq!(t[0].file.as_deref(), Some("new/name.rs"));
        assert_eq!(t[1].file, None, "the conversation has no file");
    }

    #[test]
    fn serializes_for_the_frontend() {
        let t = build_threads(
            vec![raw_thread("t", "LEFT", Some(1), false, None)],
            vec![],
            &HashMap::new(),
        );
        let v = serde_json::to_value(&t[0]).unwrap();
        assert_eq!(v["side"], "old");
        assert_eq!(v["originalLine"], 7);
        assert_eq!(v["githubResolved"], false);
        assert_eq!(v["comments"][0]["createdAt"], "2026-01-02T03:04:05Z");
        assert_eq!(
            v["comments"][0]["url"],
            "https://github.com/o/r/pull/1#discussion_c1"
        );
    }
}
