use std::future::Future;
use std::path::PathBuf;
use std::time::Instant;

use pr_review_core::db::{Db, Draft, SavedState};
use pr_review_core::manifest::Manifest;
use pr_review_core::review::SubmitResult;
use pr_review_core::{git, pr, review};
use tauri::State;
use tracing::{debug, info, warn};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::time::uptime;

const USAGE: &str = "usage: pr-review <PR number>\n\nRun it inside a clone of the GitHub repository. Needs `git` and an authenticated `gh`.\n\nLogs go to stderr. PR_REVIEW_LOG=debug shows every git/gh call and command; PR_REVIEW_LOG also accepts full tracing filters.";

struct AppState {
    repo: PathBuf,
    manifest: Manifest,
    db: Db,
}

/// Sets up logging to stderr. `PR_REVIEW_LOG` is either a level (`debug` turns on debug for this
/// app's own crates and the frontend, leaving dependencies quiet) or a full tracing filter.
fn init_logging() {
    let spec = std::env::var("PR_REVIEW_LOG").unwrap_or_else(|_| "info".to_string());
    let directives = match spec.as_str() {
        "error" | "warn" | "info" | "debug" | "trace" => {
            format!("warn,pr_review_core={spec},pr_review_lib={spec},frontend={spec}")
        }
        custom => custom.to_string(),
    };
    let filter = EnvFilter::try_new(&directives).unwrap_or_else(|e| {
        eprintln!("ignoring invalid PR_REVIEW_LOG '{spec}': {e}");
        EnvFilter::new("warn,pr_review_core=info,pr_review_lib=info,frontend=info")
    });
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_timer(uptime())
        .init();
}

/// Runs a command's work and logs how it went, so failures always reach the terminal.
async fn logged<T>(name: &str, work: impl Future<Output = anyhow::Result<T>>) -> Result<T, String> {
    let started = Instant::now();
    match work.await {
        Ok(value) => {
            debug!("{name} ok in {:?}", started.elapsed());
            Ok(value)
        }
        Err(e) => {
            warn!("{name} failed after {:?}: {e:#}", started.elapsed());
            Err(format!("{e:#}"))
        }
    }
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
    let m = &state.manifest;
    info!(
        "frontend asked for the PR: sending {} commits, {} files, {} threads",
        m.commits.len(),
        m.files.len(),
        m.threads.as_ref().map_or(0, Vec::len)
    );
    m.clone()
}

/// Lets the frontend write to the same log (its errors and startup trace).
#[tauri::command]
fn frontend_log(level: String, message: String) {
    match level.as_str() {
        "error" => tracing::error!(target: "frontend", "{message}"),
        "warn" => tracing::warn!(target: "frontend", "{message}"),
        "debug" => tracing::debug!(target: "frontend", "{message}"),
        _ => tracing::info!(target: "frontend", "{message}"),
    }
}

#[tauri::command]
async fn file_text(
    state: State<'_, AppState>,
    oid: String,
    path: String,
) -> Result<String, String> {
    logged(
        &format!("file_text {path}"),
        git::file_text(&state.repo, &oid, &path),
    )
    .await
}

#[tauri::command]
async fn file_diff(
    state: State<'_, AppState>,
    from_oid: String,
    to_oid: String,
    path: String,
) -> Result<String, String> {
    logged(
        &format!("file_diff {path}"),
        git::file_diff(&state.repo, &from_oid, &to_oid, &path),
    )
    .await
}

#[tauri::command]
async fn saved_state(state: State<'_, AppState>) -> Result<SavedState, String> {
    logged(
        "saved_state",
        review::load_saved(
            &state.db,
            &state.repo,
            &state.manifest.base,
            &state.manifest.head,
        ),
    )
    .await
}

#[tauri::command]
async fn acknowledge_draft(state: State<'_, AppState>, draft_key: String) -> Result<(), String> {
    logged("acknowledge_draft", state.db.acknowledge_draft(&draft_key)).await
}

#[tauri::command]
async fn submit_review(state: State<'_, AppState>, event: String) -> Result<SubmitResult, String> {
    info!("submitting the review ({event})");
    let m = &state.manifest;
    let result = logged(
        "submit_review",
        review::submit_saved(&state.db, &state.repo, &m.base, &m.head, &m.commits, &event),
    )
    .await;
    match &result {
        Ok(r) => info!("review submitted: {}", r.review_url),
        Err(e) => warn!("review not submitted: {e}"),
    }
    result
}

#[tauri::command]
async fn set_file_approvals(
    state: State<'_, AppState>,
    file_path: String,
    commit_oids: Vec<String>,
) -> Result<(), String> {
    logged(
        "set_file_approvals",
        state.db.set_file_approvals(&file_path, &commit_oids),
    )
    .await
}

#[tauri::command]
async fn save_draft(state: State<'_, AppState>, draft: Draft) -> Result<(), String> {
    logged("save_draft", state.db.save_draft(&draft)).await
}

#[tauri::command]
async fn delete_draft(state: State<'_, AppState>, draft_key: String) -> Result<(), String> {
    logged("delete_draft", state.db.delete_draft(&draft_key)).await
}

#[tauri::command]
async fn save_reply(
    state: State<'_, AppState>,
    thread_id: String,
    body: String,
) -> Result<(), String> {
    logged("save_reply", state.db.save_reply(&thread_id, &body)).await
}

#[tauri::command]
async fn set_resolution(
    state: State<'_, AppState>,
    thread_id: String,
    resolved_through: Option<String>,
    reopened: bool,
) -> Result<(), String> {
    logged(
        "set_resolution",
        state
            .db
            .set_resolution(&thread_id, resolved_through.as_deref(), reopened),
    )
    .await
}

#[tauri::command]
async fn set_review_body(state: State<'_, AppState>, body: String) -> Result<(), String> {
    logged("set_review_body", state.db.set_review_body(&body)).await
}

/// WebKitGTK takes its pixel ratio from GTK's `gtk-xft-dpi`, whose default is -1. When the desktop
/// has not supplied it yet (it can arrive late, or never, outside a full desktop session) the page
/// gets a negative devicePixelRatio and a nonsense viewport: tiny text and an empty window. This
/// must run before the webview is created, so it sets 96 dpi when the value is not positive.
#[cfg(target_os = "linux")]
fn ensure_gtk_dpi() {
    use gtk::prelude::*;

    if let Err(e) = gtk::init() {
        warn!("could not initialize GTK to check the screen DPI: {e}");
        return;
    }
    let Some(settings) = gtk::Settings::default() else {
        warn!("GTK has no default settings; cannot check the screen DPI");
        return;
    };
    let xft_dpi = settings.property::<i32>("gtk-xft-dpi");
    if xft_dpi > 0 {
        info!("GTK screen DPI: {:.1}", f64::from(xft_dpi) / 1024.0);
    } else {
        warn!(
            "GTK has no screen DPI (gtk-xft-dpi is {xft_dpi}); using 96 so the page is not laid out with a negative pixel ratio"
        );
        settings.set_property("gtk-xft-dpi", 96 * 1024_i32);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logging();
    let started = Instant::now();
    let number = parse_args(std::env::args().skip(1)).unwrap_or_else(|msg| exit_with(&msg, 2));
    let cwd = std::env::current_dir().unwrap_or_else(|e| exit_with(&format!("error: {e}"), 1));
    info!(
        "pr-review {} opening PR #{number} from {}",
        env!("CARGO_PKG_VERSION"),
        cwd.display()
    );

    let (opened, db) = tauri::async_runtime::block_on(async {
        let opened = pr::open_pr(&cwd, number).await?;
        let db_path = pr_review_core::db::default_path()?;
        info!("opening the database at {}", db_path.display());
        let db = Db::open(
            &db_path,
            opened.key.clone(),
            opened.manifest.head.clone(),
            opened.manifest.base.clone(),
        )
        .await?;
        anyhow::Ok((opened, db))
    })
    .unwrap_or_else(|e| exit_with(&format!("error: {e:#}"), 1));

    #[cfg(target_os = "linux")]
    ensure_gtk_dpi();

    info!("starting the window after {:?}", started.elapsed());
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            repo: opened.repo,
            manifest: opened.manifest,
            db,
        })
        .invoke_handler(tauri::generate_handler![
            open_pr,
            frontend_log,
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
