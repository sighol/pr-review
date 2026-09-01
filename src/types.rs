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
    /// Lines added/removed across the whole PR range (base..head).
    pub added: usize,
    pub removed: usize,
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

/// file_path -> set of commit oids that have been individually approved for
/// that file. Approving "through" a commit records every commit oid that
/// changed the file up to that point, rather than a single pointer — so
/// status lookups are exact-oid membership checks, robust to a `commits`
/// list that's been truncated (e.g. by `--commit`) relative to when the
/// approval was recorded.
pub type Approvals = std::collections::HashMap<String, std::collections::HashSet<String>>;

pub fn get_approval_status(
    file: &PrFile,
    commit_idx: usize,
    commits: &[Commit],
    approvals: &Approvals,
) -> ApprovalStatus {
    if !file.commits_that_changed_it.contains(&commit_idx) {
        return ApprovalStatus::NoChange;
    }

    let approved = approvals
        .get(&file.path)
        .is_some_and(|oids| oids.contains(&commits[commit_idx].oid));
    if approved {
        ApprovalStatus::Approved
    } else {
        ApprovalStatus::Unapproved
    }
}

/// The highest commit index (within `commits`) that's been approved for this
/// file, if any. Used to know where to resume diffing/rendering from.
pub fn last_approved_idx(file: &PrFile, commits: &[Commit], approvals: &Approvals) -> Option<usize> {
    let approved_oids = approvals.get(&file.path)?;
    (0..commits.len())
        .filter(|&idx| approved_oids.contains(&commits[idx].oid))
        .max()
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
