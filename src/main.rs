mod cli;
mod db;
mod diff;
mod git;
mod github;
mod grid;
mod interactive;
mod pager;
mod threads;
mod types;

use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::Parser;
use sqlx::SqlitePool;

use cli::{Cli, Command};

/// Resolves the repo path, opens its local db, and figures out which PR is
/// active — either from `pr_number_arg` or (if omitted) the last-reviewed PR
/// for this repo. Shared by every subcommand.
async fn resolve_pr(
    repo_path: &Path,
    pr_number_arg: Option<u64>,
) -> Result<(String, String, SqlitePool, u64, Option<db::ActivePr>)> {
    let (owner, repo_name) = git::origin_owner_repo(&repo_path).await?;

    let pool = db::open(&owner, &repo_name).await?;
    let stored_active = db::load_active_pr(&pool).await?;

    let pr_number = pr_number_arg
        .or_else(|| stored_active.as_ref().map(|a| a.pr_number))
        .context(
            "no PR specified and no previously reviewed PR found for this repo; \
             run e.g. `pr-review sync 123`",
        )?;

    Ok((owner, repo_name, pool, pr_number, stored_active))
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let repo_path = match cli.repo {
        Some(p) => p.clone(),
        None => std::env::current_dir()?,
    };

    if let Command::Get = &cli.command {
        let (owner, repo_name) = git::origin_owner_repo(&repo_path).await?;
        let pool = db::open(&owner, &repo_name).await?;
        return match db::load_active_pr(&pool).await? {
            Some(active) => {
                println!("{}", active.pr_number);
                Ok(())
            }
            None => bail!("no active PR set for this repo; run e.g. `pr-review set 123`"),
        };
    }

    if let Command::Set { pr_number } = &cli.command {
        let (owner, repo_name) = git::origin_owner_repo(&repo_path).await?;
        let pool = db::open(&owner, &repo_name).await?;
        let pr_number = *pr_number;

        let pr_refs = github::fetch_pr_refs(&owner, &repo_name, pr_number).await?;
        eprintln!("Fetching origin (branch {})...", pr_refs.head_ref_name);
        git::fetch_origin(&repo_path).await?;
        git::ensure_commits_available(&repo_path, &[&pr_refs.base_ref_name, &pr_refs.head_oid])
            .await?;
        db::save_active_pr(
            &pool,
            pr_number,
            &pr_refs.base_ref_name,
            &pr_refs.head_ref_name,
            &pr_refs.title,
        )
        .await?;

        eprintln!("Set active PR to #{pr_number}");
        return Ok(());
    }

    if let Command::Threads {
        pr_number,
        side_by_side,
        fetch: _,
    } = &cli.command
    {
        let (owner, repo_name, pool, pr_number, _) = resolve_pr(&repo_path, *pr_number).await?;
        return threads::run(
            &repo_path,
            &pool,
            &owner,
            &repo_name,
            pr_number,
            *side_by_side,
        )
        .await;
    }

    let (pr_number_arg, force_fetch, commit_arg, interactive_mode, side_by_side) =
        match &cli.command {
            Command::Diff {
                pr_number,
                fetch,
                commit,
                side_by_side,
            } => (*pr_number, *fetch, commit, true, *side_by_side),
            Command::Status {
                pr_number,
                fetch,
                commit,
            } => (*pr_number, *fetch, commit, false, false),
            Command::Threads { .. } | Command::Get { .. } | Command::Set { .. } => {
                unreachable!("handled above")
            }
        };

    let (owner, repo_name, pool, pr_number, stored_active) =
        resolve_pr(&repo_path, pr_number_arg).await?;

    let switching_pr = stored_active
        .as_ref()
        .map(|a| a.pr_number != pr_number)
        .unwrap_or(true);
    let need_fetch = force_fetch || switching_pr;

    let (base_ref_name, mut head_oid, _head_ref_name, pr_title) = if need_fetch {
        eprintln!("Fetching PR #{pr_number} from GitHub...");
        let pr_refs = github::fetch_pr_refs(&owner, &repo_name, pr_number).await?;
        eprintln!("Fetching origin (branch {})...", pr_refs.head_ref_name);
        git::fetch_origin(&repo_path).await?;
        git::ensure_commits_available(&repo_path, &[&pr_refs.head_oid]).await?;
        db::save_active_pr(
            &pool,
            pr_number,
            &pr_refs.base_ref_name,
            &pr_refs.head_ref_name,
            &pr_refs.title,
        )
        .await?;
        (
            pr_refs.base_ref_name,
            pr_refs.head_oid,
            pr_refs.head_ref_name,
            pr_refs.title,
        )
    } else {
        let active = stored_active.expect("!need_fetch implies stored_active matched pr_number");
        let head_oid = git::resolve_branch_oid(&repo_path, &active.head_ref_name).await?;
        (
            active.base_ref_name,
            head_oid,
            active.head_ref_name,
            active.title,
        )
    };
    let pr_url = format!("https://github.com/{owner}/{repo_name}/pull/{pr_number}");

    if let Some(commit) = commit_arg {
        let resolved = git::resolve_commit(&repo_path, commit).await?;
        git::ensure_is_descendant(&repo_path, &base_ref_name, &resolved).await?;
        head_oid = resolved;
    }
    let base_oid = git::merge_base(&repo_path, &base_ref_name, &head_oid)
        .await?
        .to_string();

    let commits = git::commits_in_range(&repo_path, &base_oid, &head_oid).await?;
    let file_paths = git::changed_files_in_range(&repo_path, &base_oid, &head_oid).await?;
    let diffstat = git::diffstat_in_range(&repo_path, &base_oid, &head_oid).await?;
    let files = git::build_pr_files(&repo_path, &commits, &file_paths, &diffstat).await?;

    let approvals = db::load_approvals(&pool, pr_number).await?;

    if !types::has_unapproved_changes(&files, &commits, &approvals) {
        print!(
            "{}",
            grid::render_grid(
                pr_number, &pr_title, &pr_url, &files, &commits, &approvals, None
            )
        );
        println!("\n\u{2713} All files approved!");
        return Ok(());
    }

    if !interactive_mode {
        print!(
            "{}",
            grid::render_grid(
                pr_number, &pr_title, &pr_url, &files, &commits, &approvals, None
            )
        );
        return Ok(());
    }

    interactive::run(
        &repo_path,
        &pool,
        pr_number,
        &pr_title,
        &pr_url,
        &files,
        &commits,
        &base_oid,
        side_by_side,
    )
    .await
}
