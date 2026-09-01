use std::io::stdout;
use std::path::Path;

use anyhow::Result;
use crossterm::cursor::MoveTo;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode};
use crossterm::execute;
use sqlx::SqlitePool;

use crate::db;
use crate::diff;
use crate::grid::render_grid;
use crate::types::{Commit, PrFile, has_unapproved_changes};

pub async fn run(
    repo_path: &Path,
    pool: &SqlitePool,
    pr_number: u64,
    files: &[PrFile],
    commits: &[Commit],
    base_oid: &str,
) -> Result<()> {
    let mut approvals = db::load_approvals(pool, pr_number).await?;
    let mut selected_idx = 0usize;

    loop {
        redraw(files, commits, &approvals, Some(selected_idx));
        println!();
        println!("Use arrow keys (\u{2191}\u{2193}) or j/k to navigate");
        println!("Press ENTER to review file");
        println!("Press 'q' to quit");

        let key = read_key()?;

        match key {
            KeyCode::Up | KeyCode::Char('k') => {
                selected_idx = (selected_idx + files.len() - 1) % files.len();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                selected_idx = (selected_idx + 1) % files.len();
            }
            KeyCode::Enter => {
                redraw(files, commits, &approvals, Some(selected_idx));
                diff::show_diff_and_prompt(
                    repo_path,
                    pool,
                    pr_number,
                    &files[selected_idx],
                    commits,
                    base_oid,
                    &approvals,
                )
                .await?;
                approvals = db::load_approvals(pool, pr_number).await?;
            }
            KeyCode::Char('q') => break,
            _ => {}
        }

        if !has_unapproved_changes(files, commits, &approvals) {
            redraw(files, commits, &approvals, None);
            println!("\n\u{2713} All files approved!");
            break;
        }
    }

    Ok(())
}

fn redraw(files: &[PrFile], commits: &[Commit], approvals: &crate::types::Approvals, selected_idx: Option<usize>) {
    let _ = execute!(stdout(), Clear(ClearType::All), MoveTo(0, 0));
    print!("{}", render_grid(files, commits, approvals, selected_idx));
}

/// Reads a single keypress, entering raw mode only for the duration of the read
/// so that the diff view's line-based `y/n` prompt keeps working normally.
fn read_key() -> Result<KeyCode> {
    enable_raw_mode()?;
    let result = loop {
        match event::read()? {
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => break Ok(key_event.code),
            _ => continue,
        }
    };
    disable_raw_mode()?;
    result
}
