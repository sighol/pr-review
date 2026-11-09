mod launch;

use std::future::Future;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use clap::Parser;
use pr_review_core::db::{Db, Draft, SavedState};
use pr_review_core::directives::Directives;
use pr_review_core::manifest::Manifest;
use pr_review_core::review::{ReviewInput, SubmitResult};
use pr_review_core::{git, github, pr, review};
use tauri::{Emitter, Manager, State};
use tracing::{debug, info, warn};
use tracing_subscriber::fmt::time::uptime;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{EnvFilter, Registry, reload};

/// Review a GitHub pull request side-by-side. Needs `git` and an authenticated `gh`.
///
/// By default the window opens in the background and the terminal is free again once it is up;
/// progress and errors from starting up are shown at warn level. Logs are off after that.
/// PR_REVIEW_LOG accepts a level or a full tracing filter and overrides --verbose.
#[derive(Parser)]
#[command(name = "pr-review")]
struct Args {
    /// PR number (a leading # is ignored)
    #[arg(value_parser = parse_pr_number)]
    number: u64,

    /// Repository to review, defaults to the current directory (like git's -C)
    #[arg(short = 'C', long = "repository", value_name = "PATH")]
    repository: Option<PathBuf>,

    /// Stay in the foreground until the window is closed
    #[arg(long, short)]
    wait: bool,

    /// Log at info level, including every git and gh call, to stderr (implies --wait)
    #[arg(long, short)]
    verbose: bool,
}

impl Args {
    fn foreground(&self) -> bool {
        self.wait || self.verbose
    }
}

fn parse_pr_number(s: &str) -> Result<u64, String> {
    s.trim_start_matches('#')
        .parse()
        .map_err(|_| format!("'{s}' is not a PR number"))
}

struct AppState {
    repo: PathBuf,
    manifest: Mutex<Manifest>,
    db: Db,
}

const STARTUP_FILTER: &str = "warn";

/// Set when logging is only on for starting up; see `init_logging`.
static STARTUP_ONLY: OnceLock<reload::Handle<EnvFilter, Registry>> = OnceLock::new();

/// The tracing filter for a level (`debug` turns on debug for this app's own crates and the
/// frontend, leaving dependencies quiet) or a full filter.
fn filter_for(spec: &str) -> String {
    match spec {
        "error" | "warn" | "info" | "debug" | "trace" => {
            format!("warn,pr_review_core={spec},pr_review_lib={spec},frontend={spec}")
        }
        custom => custom.to_string(),
    }
}

/// Sets up logging to stderr. With `PR_REVIEW_LOG` (a level or a full tracing filter) or
/// `--verbose` logging is on as asked. Otherwise it is on at warn level only while starting up,
/// so the terminal shows what is happening, and `logs_off_after_startup` turns it off once the
/// window is up.
fn init_logging(verbose: bool) {
    let asked = std::env::var("PR_REVIEW_LOG")
        .ok()
        .or_else(|| verbose.then(|| "info".to_string()));
    let directives = asked
        .as_deref()
        .map_or_else(|| STARTUP_FILTER.to_string(), filter_for);
    let filter = EnvFilter::try_new(&directives).unwrap_or_else(|e| {
        eprintln!("ignoring invalid PR_REVIEW_LOG '{directives}': {e}");
        EnvFilter::new(STARTUP_FILTER)
    });
    let (filter, handle) = reload::Layer::new(filter);
    tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_timer(uptime()),
        )
        .init();
    if asked.is_none() {
        let _ = STARTUP_ONLY.set(handle);
    }
}

/// Turns logging off unless it was asked for with `--verbose` or `PR_REVIEW_LOG`.
fn logs_off_after_startup() {
    if let Some(handle) = STARTUP_ONLY.get()
        && let Err(e) = handle.reload(EnvFilter::new("off"))
    {
        eprintln!("could not turn logging off: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn foreground(args: &[&str]) -> bool {
        Args::try_parse_from(std::iter::once("pr-review").chain(args.iter().copied()))
            .unwrap()
            .foreground()
    }

    #[test]
    fn runs_in_the_background_unless_asked_to_wait() {
        assert!(!foreground(&["13486"]));
        assert!(foreground(&["--wait", "13486"]));
        assert!(foreground(&["13486", "--wait"]));
    }

    #[test]
    fn verbose_implies_wait() {
        assert!(foreground(&["--verbose", "13486"]));
    }

    #[test]
    fn filters_for_levels_and_custom_specs() {
        assert_eq!(
            filter_for("debug"),
            "warn,pr_review_core=debug,pr_review_lib=debug,frontend=debug"
        );
        assert_eq!(filter_for("pr_review_core=trace"), "pr_review_core=trace");
    }
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

fn exit_with(message: &str, code: i32) -> ! {
    eprintln!("{message}");
    std::process::exit(code)
}

fn window_title(repo: &str, number: u64, pr_title: Option<&str>) -> String {
    match pr_title {
        Some(t) => format!("{repo} #{number} {t}"),
        None => format!("{repo} #{number}"),
    }
}

#[tauri::command]
fn open_pr(state: State<'_, AppState>) -> Manifest {
    let m = state.manifest.lock().unwrap();
    info!(
        "frontend asked for the PR: sending {} commits, {} files, {} threads",
        m.commits.len(),
        m.files.len(),
        m.threads.as_ref().map_or(0, Vec::len)
    );
    m.clone()
}

/// Re-fetches the PR's metadata and discussion (not its commits/files, so a new push still needs
/// a restart) and updates the stored manifest. See "Not built yet" in CLAUDE.md.
#[tauri::command]
async fn refresh(state: State<'_, AppState>, window: tauri::Window) -> Result<Manifest, String> {
    let key = state.db.key().clone();
    logged(&format!("refresh PR #{}", key.number), async {
        let (info, (raw_threads, conversation, reviews)) = tokio::try_join!(
            github::pr_view(&key.owner, &key.repo, key.number),
            github::fetch_discussion(&key.owner, &key.repo, key.number),
        )?;
        let mut manifest = state.manifest.lock().unwrap();
        manifest.set_threads(raw_threads, conversation, reviews);
        manifest.pr = Some(info.meta);
        info!(
            "refreshed PR #{}: {} threads",
            key.number,
            manifest.threads.as_ref().map_or(0, Vec::len)
        );
        let title = window_title(
            &key.repo,
            key.number,
            manifest.pr.as_ref().map(|p| p.title.as_str()),
        );
        if let Err(e) = window.set_title(&title) {
            warn!("could not set the window title: {e}");
        }
        Ok(manifest.clone())
    })
    .await
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
    old_path: Option<String>,
) -> Result<String, String> {
    logged(
        &format!("file_text {path}"),
        git::file_text(&state.repo, &oid, &path, old_path.as_deref()),
    )
    .await
}

#[tauri::command]
async fn file_diff(
    state: State<'_, AppState>,
    from_oid: String,
    to_oid: String,
    path: String,
    old_path: Option<String>,
) -> Result<String, String> {
    logged(
        &format!("file_diff {path}"),
        git::file_diff(&state.repo, &from_oid, &to_oid, &path, old_path.as_deref()),
    )
    .await
}

/// Opens `path` (relative to the repo) in `$PR_REVIEW_EDITOR`, or `code` if that isn't set.
#[tauri::command]
fn open_in_editor(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let editor = std::env::var("PR_REVIEW_EDITOR").unwrap_or_else(|_| "code".to_string());
    let mut parts = editor.split_whitespace();
    let program = parts
        .next()
        .ok_or_else(|| "PR_REVIEW_EDITOR is set but empty".to_string())?;
    let full_path = state.repo.join(&path);
    info!("opening {} in {editor}", full_path.display());
    std::process::Command::new(program)
        .args(parts)
        .arg(&full_path)
        .spawn()
        .map_err(|e| format!("could not run `{program}`: {e}"))?;
    Ok(())
}

#[tauri::command]
async fn saved_state(state: State<'_, AppState>) -> Result<SavedState, String> {
    let (base, head) = {
        let m = state.manifest.lock().unwrap();
        (m.base.clone(), m.head.clone())
    };
    logged(
        "saved_state",
        review::load_saved(&state.db, &state.repo, &base, &head),
    )
    .await
}

#[tauri::command]
async fn acknowledge_draft(state: State<'_, AppState>, draft_key: String) -> Result<(), String> {
    logged("acknowledge_draft", state.db.acknowledge_draft(&draft_key)).await
}

#[tauri::command]
async fn submit_review(
    state: State<'_, AppState>,
    window: tauri::Window,
    event: String,
    body: String,
    directives: Directives,
) -> Result<SubmitResult, String> {
    info!("submitting the review ({event})");
    let manifest = state.manifest.lock().unwrap().clone();
    let progress = |message: &str| {
        debug!("review progress: {message}");
        if let Err(e) = window.emit("review-progress", message) {
            warn!("could not emit review progress: {e}");
        }
    };
    let result = logged(
        "submit_review",
        review::submit_saved(
            &state.db,
            &state.repo,
            &manifest,
            ReviewInput {
                event: event.clone(),
                body,
                directives,
            },
            &progress,
        ),
    )
    .await;
    match &result {
        Ok(r) => info!(
            "review submitted: {}",
            r.review_url
                .as_deref()
                .unwrap_or("(no review, only label/assignee changes)")
        ),
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

/// WebKitGTK derives the page's devicePixelRatio from GDK's screen resolution, which is -1 until
/// something sets it. On KDE/Wayland it was observed to stay -1 while `gtk-xft-dpi` was a fine 96:
/// the page then got a ratio of -1/96 and a negative window size (tiny text, empty window). So,
/// before the webview exists, make sure the screen has a positive resolution, taken from
/// `gtk-xft-dpi` when that is set and 96 otherwise.
#[cfg(target_os = "linux")]
fn ensure_gtk_dpi() {
    use gtk::prelude::*;

    gtk::glib::set_prgname(Some("pr-review"));

    if let Err(e) = gtk::init() {
        warn!("could not initialize GTK to check the screen DPI: {e}");
        return;
    }

    // GTK derives each window's default WM_CLASS/Wayland app-id from the on-disk path of the
    // running executable when nothing overrides it, not from argv[0]. Nix's wrapGAppsHook renames
    // the real binary to `.<name>-wrapped` (the visible binary is a wrapper script that execs it),
    // so without this the Nix build's windows get that name instead of CARGO_CRATE_NAME and the
    // desktop environment can't match them to pr-review.desktop for the icon. Set the X11 program
    // class explicitly (requires GDK to already be initialized), to the same name cargo gives this
    // binary target (which is also what the .desktop file and its install are named after), so the
    // match works regardless of how the binary ends up being invoked.
    gtk::gdk::set_program_class("pr-review");

    let mut xft_dpi = -1;
    if let Some(settings) = gtk::Settings::default() {
        xft_dpi = settings.property::<i32>("gtk-xft-dpi");
        info!(
            "GTK gtk-xft-dpi: {xft_dpi} ({:.1} dpi)",
            f64::from(xft_dpi) / 1024.0
        );
        if xft_dpi <= 0 {
            warn!("gtk-xft-dpi is not set; setting it to 96 dpi");
            settings.set_property("gtk-xft-dpi", 96 * 1024_i32);
        }
    } else {
        warn!("GTK has no default settings");
    }

    let Some(screen) = gtk::gdk::Screen::default() else {
        warn!("GDK has no default screen; cannot check the screen resolution");
        return;
    };
    let before = screen.resolution();
    if before > 0.0 {
        info!("GDK screen resolution is {before}; leaving it");
        return;
    }
    let dpi = if xft_dpi > 0 {
        f64::from(xft_dpi) / 1024.0
    } else {
        96.0
    };
    warn!(
        "GDK screen resolution is {before}; setting it to {dpi} so the page gets a positive pixel ratio"
    );
    screen.set_resolution(dpi);
    info!("GDK screen resolution is now {}", screen.resolution());
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let args = Args::parse();
    if !args.foreground() && !launch::is_background_copy() {
        launch::start_in_background();
    }
    init_logging(args.verbose);
    let started = Instant::now();
    let number = args.number;
    let cwd = match args.repository {
        Some(repository) => repository,
        None => std::env::current_dir().unwrap_or_else(|e| exit_with(&format!("error: {e}"), 1)),
    };
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

    let title = window_title(
        &opened.key.repo,
        opened.key.number,
        opened.manifest.pr.as_ref().map(|p| p.title.as_str()),
    );

    info!("starting the window after {:?}", started.elapsed());
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            repo: opened.repo,
            manifest: Mutex::new(opened.manifest),
            db,
        })
        .setup(move |app| {
            if let Some(window) = app.get_webview_window("main") {
                if let Err(e) = window.set_title(&title) {
                    warn!("could not set the window title: {e}");
                }
            }
            launch::window_is_up();
            logs_off_after_startup();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            open_pr,
            refresh,
            open_in_editor,
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
