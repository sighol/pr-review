use std::path::PathBuf;

use pr_review_core::db::{Db, Draft, SavedState};
use pr_review_core::manifest::Manifest;
use pr_review_core::review::SubmitResult;
use pr_review_core::{git, pr, review};
use tauri::State;

const USAGE: &str = "usage: pr-review <PR number>\n\nRun it inside a clone of the GitHub repository. Needs `git` and an authenticated `gh`.";

struct AppState {
    repo: PathBuf,
    manifest: Manifest,
    db: Db,
}

fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
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
    git::file_text(&state.repo, &oid, &path).await.map_err(err)
}

#[tauri::command]
async fn saved_state(state: State<'_, AppState>) -> Result<SavedState, String> {
    review::load_saved(&state.db, &state.repo, &state.manifest.base, &state.manifest.head)
        .await
        .map_err(err)
}

#[tauri::command]
async fn acknowledge_draft(state: State<'_, AppState>, draft_key: String) -> Result<(), String> {
    state.db.acknowledge_draft(&draft_key).await.map_err(err)
}

#[tauri::command]
async fn submit_review(state: State<'_, AppState>, event: String) -> Result<SubmitResult, String> {
    let m = &state.manifest;
    review::submit_saved(&state.db, &state.repo, &m.base, &m.head, &m.commits, &event)
        .await
        .map_err(err)
}

#[tauri::command]
async fn set_file_approvals(
    state: State<'_, AppState>,
    file_path: String,
    commit_oids: Vec<String>,
) -> Result<(), String> {
    state.db.set_file_approvals(&file_path, &commit_oids).await.map_err(err)
}

#[tauri::command]
async fn save_draft(state: State<'_, AppState>, draft: Draft) -> Result<(), String> {
    state.db.save_draft(&draft).await.map_err(err)
}

#[tauri::command]
async fn delete_draft(state: State<'_, AppState>, draft_key: String) -> Result<(), String> {
    state.db.delete_draft(&draft_key).await.map_err(err)
}

#[tauri::command]
async fn save_reply(state: State<'_, AppState>, thread_id: String, body: String) -> Result<(), String> {
    state.db.save_reply(&thread_id, &body).await.map_err(err)
}

#[tauri::command]
async fn set_resolution(
    state: State<'_, AppState>,
    thread_id: String,
    resolved_through: Option<String>,
    reopened: bool,
) -> Result<(), String> {
    state
        .db
        .set_resolution(&thread_id, resolved_through.as_deref(), reopened)
        .await
        .map_err(err)
}

#[tauri::command]
async fn set_review_body(state: State<'_, AppState>, body: String) -> Result<(), String> {
    state.db.set_review_body(&body).await.map_err(err)
}

#[tauri::command]
async fn file_diff(
    state: State<'_, AppState>,
    from_oid: String,
    to_oid: String,
    path: String,
) -> Result<String, String> {
    git::file_diff(&state.repo, &from_oid, &to_oid, &path).await.map_err(err)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let number = parse_args(std::env::args().skip(1)).unwrap_or_else(|msg| exit_with(&msg, 2));
    let cwd = std::env::current_dir().unwrap_or_else(|e| exit_with(&format!("error: {e}"), 1));

    let (opened, db) = tauri::async_runtime::block_on(async {
        let opened = pr::open_pr(&cwd, number, |step| eprintln!("{step}...")).await?;
        let db = Db::open_default(opened.key.clone(), opened.manifest.head.clone(), opened.manifest.base.clone()).await?;
        anyhow::Ok((opened, db))
    })
    .unwrap_or_else(|e| exit_with(&format!("error: {e:#}"), 1));

    tauri::Builder::default()
        .manage(AppState {
            repo: opened.repo,
            manifest: opened.manifest,
            db,
        })
        .invoke_handler(tauri::generate_handler![
            open_pr,
            file_text,
            file_diff,
            saved_state,
            acknowledge_draft,
            submit_review,
            set_file_approvals,
            save_draft,
            delete_draft,
            save_reply,
            set_resolution,
            set_review_body,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
