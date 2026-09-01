use std::io::{self, Write};
use std::path::Path;

use anyhow::Result;
use chrono::{DateTime, Utc};
use crossterm::event::KeyCode;
use sqlx::SqlitePool;

use crate::db;
use crate::diff;
use crate::github::{self, ReviewComment};
use crate::interactive::read_key;
use crate::pager::page;

/// A single discussion to walk through: either an inline code-review thread
/// (`path`/`diff_hunk` set) or the PR's top-level conversation (both `None`).
struct Discussion {
    id: String,
    path: Option<String>,
    line: Option<i64>,
    diff_hunk: Option<String>,
    comments: Vec<ReviewComment>,
}

impl Discussion {
    fn first_comment_time(&self) -> DateTime<Utc> {
        self.comments
            .first()
            .expect("a discussion always has at least one comment")
            .created_at
    }

    fn last_comment_id(&self) -> &str {
        &self
            .comments
            .last()
            .expect("a discussion always has at least one comment")
            .id
    }
}

/// Fetches every discussion on the PR from GitHub, then walks through the
/// ones we haven't locally resolved through their current last comment,
/// oldest first — pausing after each to page its content and ask whether to
/// mark it resolved.
pub async fn run(
    repo_path: &Path,
    pool: &SqlitePool,
    owner: &str,
    repo: &str,
    pr_number: u64,
    side_by_side: bool,
) -> Result<()> {
    let review_threads = github::fetch_review_threads(owner, repo, pr_number).await?;
    let conversation_comments =
        github::fetch_pr_conversation_comments(owner, repo, pr_number).await?;

    let mut discussions: Vec<Discussion> = review_threads
        .into_iter()
        .map(|t| Discussion {
            id: t.id,
            path: Some(t.path),
            line: t.line,
            diff_hunk: t.comments.first().map(|c| c.diff_hunk.clone()),
            comments: t.comments,
        })
        .collect();
    if !conversation_comments.is_empty() {
        discussions.push(Discussion {
            id: "conversation".to_string(),
            path: None,
            line: None,
            diff_hunk: None,
            comments: conversation_comments,
        });
    }
    discussions.sort_by_key(|d| d.first_comment_time());

    let resolutions = db::load_thread_resolutions(pool, pr_number).await?;
    let unresolved: Vec<Discussion> = discussions
        .into_iter()
        .filter(|d| resolutions.get(&d.id).map(String::as_str) != Some(d.last_comment_id()))
        .collect();

    if unresolved.is_empty() {
        println!("\u{2713} All discussions resolved!");
        return Ok(());
    }

    let mut all_resolved = true;
    for discussion in &unresolved {
        let mut markdown = String::new();
        if let (Some(path), Some(diff_hunk)) = (&discussion.path, &discussion.diff_hunk) {
            let hunk = diff::render_hunk(repo_path, path, diff_hunk, side_by_side).await?;
            markdown.push_str(&hunk);
            markdown.push('\n');
        }

        let header = match (&discussion.path, discussion.line) {
            (Some(path), Some(line)) => format!("## {path}:{line}"),
            (Some(path), None) => format!("## {path}"),
            (None, _) => "## Conversation".to_string(),
        };
        markdown.push_str(&header);
        markdown.push_str("\n\n");
        for comment in &discussion.comments {
            markdown.push_str(&format!(
                "**{}** _{}_\n\n{}\n\n---\n\n",
                comment.author, comment.created_at, comment.body
            ));
        }

        page(&markdown).await?;
        println!();
        print!("Resolve this discussion? (y/n): ");
        io::stdout().flush()?;

        match read_key().unwrap_or(KeyCode::Enter) {
            KeyCode::Char('q') => return Ok(()),
            KeyCode::Char('y') => {
                db::save_thread_resolution(
                    pool,
                    pr_number,
                    &discussion.id,
                    discussion.last_comment_id(),
                )
                .await?;
            }
            _ => all_resolved = false,
        }
    }

    if all_resolved {
        println!("\u{2713} All discussions resolved!");
    }
    Ok(())
}
