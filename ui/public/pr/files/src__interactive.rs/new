use std::io::stdout;
use std::path::Path;

use anyhow::Result;
use crossterm::cursor::MoveTo;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode};
use sqlx::SqlitePool;

use crate::db;
use crate::diff;
use crate::grid::render_grid;
use crate::types::{Approvals, Commit, PrFile, file_is_fully_approved, has_unapproved_changes};

pub async fn run(
    repo_path: &Path,
    pool: &SqlitePool,
    pr_number: u64,
    pr_title: &str,
    pr_url: &str,
    files: &[PrFile],
    commits: &[Commit],
    base_oid: &str,
    side_by_side: bool,
) -> Result<()> {
    let mut approvals = db::load_approvals(pool, pr_number).await?;
    // `run` is only entered while `has_unapproved_changes` is true, so there's
    // always at least one selectable (not fully approved) file.
    let mut selected_idx = first_unapproved(files, commits, &approvals);

    loop {
        redraw(
            pr_number,
            pr_title,
            pr_url,
            files,
            commits,
            &approvals,
            Some(selected_idx),
        );
        println!();
        println!("Use arrow keys (\u{2191}\u{2193}) or j/k to navigate");
        println!("Press ENTER to review file");
        println!("Press 'q' to quit");

        let key = read_key()?;

        match key {
            KeyCode::Up | KeyCode::Char('k') => {
                selected_idx = step(files, commits, &approvals, selected_idx, false);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                selected_idx = step(files, commits, &approvals, selected_idx, true);
            }
            KeyCode::Enter => {
                redraw(
                    pr_number,
                    pr_title,
                    pr_url,
                    files,
                    commits,
                    &approvals,
                    Some(selected_idx),
                );
                println!();
                diff::show_diff_and_prompt(
                    repo_path,
                    pool,
                    pr_number,
                    &files[selected_idx],
                    commits,
                    base_oid,
                    &approvals,
                    side_by_side,
                )
                .await?;
                approvals = db::load_approvals(pool, pr_number).await?;
                // The file just approved is no longer selectable; move on.
                if file_is_fully_approved(&files[selected_idx], commits, &approvals) {
                    selected_idx = step(files, commits, &approvals, selected_idx, true);
                }
            }
            KeyCode::Char('q') => break,
            _ => {}
        }

        if !has_unapproved_changes(files, commits, &approvals) {
            redraw(
                pr_number, pr_title, pr_url, files, commits, &approvals, None,
            );
            println!("\n\u{2713} All files approved!");
            break;
        }
    }

    Ok(())
}

fn first_unapproved(files: &[PrFile], commits: &[Commit], approvals: &Approvals) -> usize {
    files
        .iter()
        .position(|f| !file_is_fully_approved(f, commits, approvals))
        .expect("run() is only entered when at least one file has unapproved changes")
}

/// Moves the selection one step in the given direction, skipping over fully
/// approved files (which can't be selected). Wraps around, and is a no-op if
/// every file happens to be fully approved (defensive; shouldn't occur).
fn step(
    files: &[PrFile],
    commits: &[Commit],
    approvals: &Approvals,
    from: usize,
    forward: bool,
) -> usize {
    let len = files.len();
    let mut idx = from;
    loop {
        idx = if forward {
            (idx + 1) % len
        } else {
            (idx + len - 1) % len
        };
        if !file_is_fully_approved(&files[idx], commits, approvals) {
            return idx;
        }
        if idx == from {
            return idx;
        }
    }
}

fn redraw(
    pr_number: u64,
    pr_title: &str,
    pr_url: &str,
    files: &[PrFile],
    commits: &[Commit],
    approvals: &crate::types::Approvals,
    selected_idx: Option<usize>,
) {
    let _ = execute!(stdout(), Clear(ClearType::All), MoveTo(0, 0));
    print!(
        "{}",
        render_grid(
            pr_number,
            pr_title,
            pr_url,
            files,
            commits,
            approvals,
            selected_idx
        )
    );
}

/// Reads a single keypress, entering raw mode only for the duration of the read
/// so that the diff view's line-based `y/n` prompt keeps working normally.
pub fn read_key() -> Result<KeyCode> {
    enable_raw_mode()?;
    let result = loop {
        match event::read()? {
            Event::Key(key_event)
                if key_event.is_press()
                    && key_event.modifiers.contains(KeyModifiers::CONTROL)
                    && key_event.code == KeyCode::Char('c') =>
            {
                disable_raw_mode()?;
                eprintln!("Keyboard interrupted");
                std::process::exit(130);
            }
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                break Ok(key_event.code);
            }
            _ => continue,
        }
    };
    disable_raw_mode()?;
    result
}
