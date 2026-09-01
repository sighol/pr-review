mod cli;
mod db;
mod diff;
mod git;
mod github;
mod grid;
mod interactive;
mod types;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Command, parse_pr_reference};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let (pr_ref_str, repo_arg, interactive_mode) = match &cli.command {
        Command::Sync { pr_ref, repo } => (pr_ref, repo, true),
        Command::Status { pr_ref, repo } => (pr_ref, repo, false),
    };

    let pr_ref = parse_pr_reference(pr_ref_str)?;
    let repo_path = match repo_arg {
        Some(p) => p.clone(),
        None => std::env::current_dir()?,
    };

    eprintln!(
        "Syncing {}/{}#{}...",
        pr_ref.owner, pr_ref.repo, pr_ref.pr_number
    );

    let pr_refs = github::fetch_pr_refs(&pr_ref.owner, &pr_ref.repo, pr_ref.pr_number).await?;
    eprintln!("Fetching origin (branch {})...", pr_refs.head_ref_name);
    git::fetch_origin(&repo_path).await?;
    git::ensure_commits_available(&repo_path, &pr_refs.base_oid, &pr_refs.head_oid).await?;

    let commits = git::commits_in_range(&repo_path, &pr_refs.base_oid, &pr_refs.head_oid).await?;
    let file_paths =
        git::changed_files_in_range(&repo_path, &pr_refs.base_oid, &pr_refs.head_oid).await?;
    let files = git::build_pr_files(&repo_path, &commits, &file_paths).await?;

    let pool = db::open(&pr_ref.owner, &pr_ref.repo).await?;
    let approvals = db::load_approvals(&pool, pr_ref.pr_number).await?;

    if !types::has_unapproved_changes(&files, &commits, &approvals) {
        print!("{}", grid::render_grid(&files, &commits, &approvals, None));
        println!("\n\u{2713} All files approved!");
        return Ok(());
    }

    if !interactive_mode {
        print!("{}", grid::render_grid(&files, &commits, &approvals, None));
        return Ok(());
    }

    interactive::run(
        &repo_path,
        &pool,
        pr_ref.pr_number,
        &files,
        &commits,
        &pr_refs.base_oid,
    )
    .await
}
