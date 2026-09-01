use std::collections::HashMap;
use std::io::{self, Write};
use std::path::Path;

use anyhow::Result;
use chrono::{DateTime, Utc};
use crossterm::cursor::MoveTo;
use crossterm::event::KeyCode;
use crossterm::execute;
use crossterm::terminal::{Clear, ClearType};
use sqlx::SqlitePool;
use termimad::MadSkin;

use crate::db;
use crate::diff;
use crate::git;
use crate::github::{self, ReviewComment};
use crate::interactive::read_key;
use crate::pager::page;

/// A single discussion to walk through: either an inline code-review thread
/// (`path`/`diff_hunk`/`commit_oid` set) or the PR's top-level conversation
/// (all three `None`).
struct Discussion {
    id: String,
    path: Option<String>,
    line: Option<i64>,
    diff_hunk: Option<String>,
    /// The commit the discussion's diff hunk is anchored to.
    commit_oid: Option<String>,
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
    eprintln!("Fetching PR #{pr_number} from GitHub...");
    let pr_refs = github::fetch_pr_refs(owner, repo, pr_number).await?;
    let review_threads = github::fetch_review_threads(owner, repo, pr_number).await?;
    let conversation_comments =
        github::fetch_pr_conversation_comments(owner, repo, pr_number).await?;

    eprintln!("Fetching origin (for commit numbers)...");
    git::fetch_origin(repo_path).await?;
    git::ensure_commits_available(repo_path, &[&pr_refs.base_ref_name, &pr_refs.head_oid]).await?;
    let commits =
        git::commits_in_range(repo_path, &pr_refs.base_ref_name, &pr_refs.head_oid).await?;
    // 1-based, matching the numbering `sync`/`status` already show elsewhere.
    let commit_numbers: HashMap<&str, usize> = commits
        .iter()
        .enumerate()
        .map(|(i, c)| (c.oid.as_str(), i + 1))
        .collect();

    let mut discussions: Vec<Discussion> = review_threads
        .into_iter()
        .map(|t| Discussion {
            id: t.id,
            path: Some(t.path),
            line: t.line,
            diff_hunk: t.comments.first().map(|c| c.diff_hunk.clone()),
            commit_oid: t
                .comments
                .first()
                .and_then(|c| c.original_commit_oid.clone()),
            comments: t.comments,
        })
        .collect();
    if !conversation_comments.is_empty() {
        discussions.push(Discussion {
            id: "conversation".to_string(),
            path: None,
            line: None,
            diff_hunk: None,
            commit_oid: None,
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

    let skin = MadSkin::default();
    let mut discussion_index = 0;
    let mut all_resolved = true;
    for discussion in &unresolved {
        discussion_index += 1;
        let mut rendered = String::new();
        if let (Some(path), Some(diff_hunk)) = (&discussion.path, &discussion.diff_hunk) {
            let hunk = diff::render_hunk(repo_path, path, diff_hunk, side_by_side).await?;
            rendered.push_str(&hunk);
            rendered.push('\n');
        }

        match &discussion.path {
            Some(path) => {
                let location = match discussion.line {
                    Some(line) => format!("{path}:{line}"),
                    None => path.clone(),
                };
                let mut header_line = location;
                if let Some(oid) = &discussion.commit_oid {
                    let short = &oid[..oid.len().min(8)];
                    let url = format!("https://github.com/{owner}/{repo}/commit/{oid}");
                    header_line.push_str("  ");
                    header_line.push_str(&hyperlink(&url, short));
                    if let Some(n) = commit_numbers.get(oid.as_str()) {
                        header_line.push_str(&format!(" #{n}"));
                    }
                }
                rendered.push_str(&format!("\x1b[1m{header_line}\x1b[0m\n\n"));
            }
            None => rendered.push_str(&skin.term_text("## Conversation\n").to_string()),
        }

        let mut markdown = String::new();
        for comment in &discussion.comments {
            markdown.push_str(&format!(
                "**{}** *{}*\n\n{}\n\n---\n\n",
                comment.author, comment.created_at, comment.body
            ));
        }
        rendered.push_str(&skin.term_text(&markdown).to_string());

        let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0));
        page(&rendered).await?;
        println!();
        print!(
            "Resolve discussion {discussion_index}/{}? (y/n): ",
            unresolved.len()
        );
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

/// Wraps `text` in an OSC 8 terminal hyperlink escape sequence pointing at
/// `url`. Markdown itself has no link support in the flavor `termimad`
/// renders, so this is applied outside of markdown rendering.
fn hyperlink(url: &str, text: &str) -> String {
    format!("\x1b]8;;{url}\x1b\\{text}\x1b]8;;\x1b\\")
}
