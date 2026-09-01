use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "pr-review")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Display the grid and enter interactive mode. Reuses the last-reviewed
    /// PR for this repo if `pr_ref` is omitted.
    Sync {
        pr_ref: Option<String>,
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Refetch PR metadata from GitHub and `git fetch origin`, instead of
        /// resolving the branch from local git state.
        #[arg(long)]
        fetch: bool,
    },
    /// Show the grid without entering interactive mode.
    Status {
        pr_ref: Option<String>,
        #[arg(long)]
        repo: Option<PathBuf>,
        #[arg(long)]
        fetch: bool,
    },
}

pub struct PrRef {
    pub owner: String,
    pub repo: String,
    pub pr_number: u64,
}

/// Parses a PR reference of the form `owner/repo#123`.
pub fn parse_pr_reference(input: &str) -> Result<PrRef> {
    let (repo_part, pr_part) = input
        .split_once('#')
        .with_context(|| format!("invalid PR reference '{input}', expected owner/repo#123"))?;
    let (owner, repo) = repo_part
        .split_once('/')
        .with_context(|| format!("invalid PR reference '{input}', expected owner/repo#123"))?;
    if owner.is_empty() || repo.is_empty() {
        bail!("invalid PR reference '{input}', expected owner/repo#123");
    }
    let pr_number: u64 = pr_part
        .parse()
        .with_context(|| format!("invalid PR number '{pr_part}' in '{input}'"))?;
    Ok(PrRef {
        owner: owner.to_string(),
        repo: repo.to_string(),
        pr_number,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_reference() {
        let pr_ref = parse_pr_reference("anthropic/anthropic-sdk-python#123").unwrap();
        assert_eq!(pr_ref.owner, "anthropic");
        assert_eq!(pr_ref.repo, "anthropic-sdk-python");
        assert_eq!(pr_ref.pr_number, 123);
    }

    #[test]
    fn rejects_missing_hash() {
        assert!(parse_pr_reference("anthropic/anthropic-sdk-python").is_err());
    }

    #[test]
    fn rejects_missing_slash() {
        assert!(parse_pr_reference("anthropic-sdk-python#123").is_err());
    }
}
