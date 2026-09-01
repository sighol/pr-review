use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct Commit {
    pub oid: String,
    pub message: String,
    pub committed_date: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct PrFile {
    pub path: String,
    /// Indices into the `commits` array of the commits that changed this file.
    pub commits_that_changed_it: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalStatus {
    NoChange,
    Unapproved,
    Approved,
}

impl ApprovalStatus {
    pub fn as_char(self) -> char {
        match self {
            ApprovalStatus::NoChange => ' ',
            ApprovalStatus::Unapproved => '*',
            ApprovalStatus::Approved => 'A',
        }
    }
}

/// file_path -> commit oid the file is approved through
pub type Approvals = std::collections::HashMap<String, String>;

pub fn get_approval_status(
    file: &PrFile,
    commit_idx: usize,
    commits: &[Commit],
    approvals: &Approvals,
) -> ApprovalStatus {
    if !file.commits_that_changed_it.contains(&commit_idx) {
        return ApprovalStatus::NoChange;
    }

    let Some(approved_oid) = approvals.get(&file.path) else {
        return ApprovalStatus::Unapproved;
    };

    let approved_idx = commits.iter().position(|c| &c.oid == approved_oid);
    match approved_idx {
        Some(idx) if commit_idx <= idx => ApprovalStatus::Approved,
        _ => ApprovalStatus::Unapproved,
    }
}

/// A file is fully approved once none of its cells are `Unapproved` — since
/// approval is "through" a commit, this is equivalent to the latest commit
/// that changed the file being approved.
pub fn file_is_fully_approved(file: &PrFile, commits: &[Commit], approvals: &Approvals) -> bool {
    !(0..commits.len())
        .any(|idx| get_approval_status(file, idx, commits, approvals) == ApprovalStatus::Unapproved)
}

pub fn has_unapproved_changes(files: &[PrFile], commits: &[Commit], approvals: &Approvals) -> bool {
    files
        .iter()
        .any(|file| !file_is_fully_approved(file, commits, approvals))
}

/// Latest commit index (in `commits`) that changed this file.
pub fn latest_commit_idx(file: &PrFile) -> usize {
    *file
        .commits_that_changed_it
        .iter()
        .max()
        .expect("file must have at least one changed commit")
}
