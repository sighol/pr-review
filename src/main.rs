mod cli;
mod db;
mod diff;
mod git;
mod github;
mod grid;
mod interactive;
mod types;

use anyhow::{Context, Result};
use clap::Parser;

use cli::{Cli, Command};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let (pr_number_arg, repo_arg, force_fetch, interactive_mode) = match &cli.command {
        Command::Sync { pr_number, repo, fetch } => (*pr_number, repo, *fetch, true),
        Command::Status { pr_number, repo, fetch } => (*pr_number, repo, *fetch, false),
    };

    let repo_path = match repo_arg {
        Some(p) => p.clone(),
        None => std::env::current_dir()?,
    };

    let (owner, repo_name) = git::origin_owner_repo(&repo_path).await?;

    let pool = db::open(&owner, &repo_name).await?;
    let stored_active = db::load_active_pr(&pool).await?;

    let pr_number = pr_number_arg
        .or_else(|| stored_active.as_ref().map(|a| a.pr_number))
        .context(
            "no PR specified and no previously reviewed PR found for this repo; \
             run e.g. `pr-review sync 123`",
        )?;

    let switching_pr = stored_active
        .as_ref()
        .map(|a| a.pr_number != pr_number)
        .unwrap_or(true);
    let need_fetch = force_fetch || switching_pr;

    let (base_oid, head_oid, _head_ref_name) = if need_fetch {
        eprintln!("Fetching PR #{pr_number} from GitHub...");
        let pr_refs = github::fetch_pr_refs(&owner, &repo_name, pr_number).await?;
        eprintln!("Fetching origin (branch {})...", pr_refs.head_ref_name);
        git::fetch_origin(&repo_path).await?;
        git::ensure_commits_available(&repo_path, &pr_refs.base_oid, &pr_refs.head_oid).await?;
        db::save_active_pr(&pool, pr_number, &pr_refs.base_oid, &pr_refs.head_ref_name).await?;
        (pr_refs.base_oid, pr_refs.head_oid, pr_refs.head_ref_name)
    } else {
        let active = stored_active.expect("!need_fetch implies stored_active matched pr_number");
        let head_oid = git::resolve_branch_oid(&repo_path, &active.head_ref_name).await?;
        (active.base_oid, head_oid, active.head_ref_name)
    };

    let commits = git::commits_in_range(&repo_path, &base_oid, &head_oid).await?;
    let file_paths = git::changed_files_in_range(&repo_path, &base_oid, &head_oid).await?;
    let diffstat = git::diffstat_in_range(&repo_path, &base_oid, &head_oid).await?;
    let files = git::build_pr_files(&repo_path, &commits, &file_paths, &diffstat).await?;

    let approvals = db::load_approvals(&pool, pr_number).await?;

    if !types::has_unapproved_changes(&files, &commits, &approvals) {
        print!("{}", grid::render_grid(&files, &commits, &approvals, None));
        println!("\n\u{2713} All files approved!");
        return Ok(());
    }

    if !interactive_mode {
        print!("{}", grid::render_grid(&files, &commits, &approvals, None));
        return Ok(());
    }

    interactive::run(&repo_path, &pool, pr_number, &files, &commits, &base_oid).await
}
