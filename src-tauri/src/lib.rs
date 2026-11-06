use std::path::PathBuf;
use std::sync::Mutex;

use pr_review_core::{git, manifest};
use tauri::State;

#[derive(Default)]
struct AppState {
    repo: Mutex<Option<PathBuf>>,
}

fn repo_of(state: &AppState) -> Result<PathBuf, String> {
    state.repo.lock().unwrap().clone().ok_or_else(|| "no PR is open".to_string())
}

fn env(name: &str) -> Result<String, String> {
    std::env::var(name).map_err(|_| format!("set {name} (temporary: there is no PR picker yet)"))
}

/// Opens the PR named by PR_REVIEW_REPO (default: the current directory),
/// PR_REVIEW_BASE and PR_REVIEW_HEAD.
#[tauri::command]
async fn open_pr(state: State<'_, AppState>) -> Result<manifest::Manifest, String> {
    let repo = PathBuf::from(std::env::var("PR_REVIEW_REPO").unwrap_or_else(|_| ".".into()));
    let m = manifest::build_manifest(&repo, &env("PR_REVIEW_BASE")?, &env("PR_REVIEW_HEAD")?)
        .await
        .map_err(|e| format!("{e:#}"))?;
    *state.repo.lock().unwrap() = Some(repo);
    Ok(m)
}

#[tauri::command]
async fn file_text(state: State<'_, AppState>, oid: String, path: String) -> Result<String, String> {
    let repo = repo_of(&state)?;
    git::file_text(&repo, &oid, &path).await.map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn file_diff(
    state: State<'_, AppState>,
    from_oid: String,
    to_oid: String,
    path: String,
) -> Result<String, String> {
    let repo = repo_of(&state)?;
    git::file_diff(&repo, &from_oid, &to_oid, &path).await.map_err(|e| format!("{e:#}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![open_pr, file_text, file_diff])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
