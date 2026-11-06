use std::path::PathBuf;

use anyhow::{Context, Result};
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
    /// PR for this repo (owner/repo comes from the `origin` remote) if
    /// `pr_number` is omitted.
    Sync {
        #[arg(value_parser = parse_pr_number)]
        pr_number: Option<u64>,
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Refetch PR metadata from GitHub and `git fetch origin`, instead of
        /// resolving the branch from local git state.
        #[arg(long)]
        fetch: bool,
        /// Treat this commit as the PR's head: show the grid, diffs, and
        /// approvals only up through this commit, ignoring anything later.
        #[arg(long)]
        commit: Option<String>,
        /// Render diffs side-by-side. Only takes effect when `delta` is
        /// installed; ignored otherwise.
        #[arg(long)]
        side_by_side: bool,
    },
    /// Show the grid without entering interactive mode.
    Status {
        #[arg(value_parser = parse_pr_number)]
        pr_number: Option<u64>,
        #[arg(long)]
        repo: Option<PathBuf>,
        #[arg(long)]
        fetch: bool,
        #[arg(long)]
        commit: Option<String>,
    },
    /// Walk through PR discussions (review-comment threads and the top-level
    /// conversation) one at a time, `git add -p`-style, and locally track
    /// which ones you've resolved. Always refetches from GitHub.
    Threads {
        #[arg(value_parser = parse_pr_number)]
        pr_number: Option<u64>,
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Render each discussion's diff hunk side-by-side. Only takes effect
        /// when `delta` is installed; ignored otherwise.
        #[arg(long)]
        side_by_side: bool,
    },
}

/// Accepts a PR number with an optional leading `#`, e.g. `123` or `#123`.
fn parse_pr_number(input: &str) -> Result<u64> {
    input
        .strip_prefix('#')
        .unwrap_or(input)
        .parse()
        .with_context(|| format!("invalid PR number '{input}'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_number() {
        assert_eq!(parse_pr_number("123").unwrap(), 123);
    }

    #[test]
    fn parses_number_with_leading_hash() {
        assert_eq!(parse_pr_number("#123").unwrap(), 123);
    }

    #[test]
    fn rejects_non_numeric_input() {
        assert!(parse_pr_number("owner/repo#123").is_err());
    }
}
