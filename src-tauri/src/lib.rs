use std::path::PathBuf;

use pr_review_core::manifest::Manifest;
use pr_review_core::{git, pr};
use tauri::State;

const USAGE: &str = "usage: pr-review <PR number>\n\nRun it inside a clone of the GitHub repository. Needs `git` and an authenticated `gh`.";

struct AppState {
    repo: PathBuf,
    manifest: Manifest,
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<u64, String> {
    let (Some(arg), None) = (args.next(), args.next()) else {
        return Err(USAGE.to_string());
    };
    if arg == "-h" || arg == "--help" {
        return Err(USAGE.to_string());
    }
    arg.trim_start_matches('#')
        .parse()
        .map_err(|_| format!("'{arg}' is not a PR number\n\n{USAGE}"))
}

fn exit_with(message: &str, code: i32) -> ! {
    eprintln!("{message}");
    std::process::exit(code)
}

#[tauri::command]
fn open_pr(state: State<'_, AppState>) -> Manifest {
    state.manifest.clone()
}

#[tauri::command]
async fn file_text(state: State<'_, AppState>, oid: String, path: String) -> Result<String, String> {
    git::file_text(&state.repo, &oid, &path).await.map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn file_diff(
    state: State<'_, AppState>,
    from_oid: String,
    to_oid: String,
    path: String,
) -> Result<String, String> {
    git::file_diff(&state.repo, &from_oid, &to_oid, &path)
        .await
        .map_err(|e| format!("{e:#}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let number = parse_args(std::env::args().skip(1)).unwrap_or_else(|msg| exit_with(&msg, 2));
    let cwd = std::env::current_dir().unwrap_or_else(|e| exit_with(&format!("error: {e}"), 1));

    let opened = tauri::async_runtime::block_on(pr::open_pr(&cwd, number, |step| eprintln!("{step}...")))
        .unwrap_or_else(|e| exit_with(&format!("error: {e:#}"), 1));

    tauri::Builder::default()
        .manage(AppState {
            repo: opened.repo,
            manifest: opened.manifest,
        })
        .invoke_handler(tauri::generate_handler![open_pr, file_text, file_diff])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
