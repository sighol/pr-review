-- Every row belongs to one PR, identified by (owner, repo, pr_number).

-- A commit is approved for a file; approving "through" a commit stores every commit
-- that touched the file up to it. Keyed by oid so approvals survive new pushes.
CREATE TABLE approvals (
    owner TEXT NOT NULL,
    repo TEXT NOT NULL,
    pr_number INTEGER NOT NULL,
    file_path TEXT NOT NULL,
    commit_oid TEXT NOT NULL,
    approved_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (owner, repo, pr_number, file_path, commit_oid)
) WITHOUT ROWID;

-- Local draft line comments. draft_key is the frontend's stable id for the line range.
-- head_oid is the PR head the draft was written against.
CREATE TABLE drafts (
    owner TEXT NOT NULL,
    repo TEXT NOT NULL,
    pr_number INTEGER NOT NULL,
    draft_key TEXT NOT NULL,
    file_path TEXT NOT NULL,
    side TEXT NOT NULL CHECK (side IN ('old', 'new')),
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    body TEXT NOT NULL,
    head_oid TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (owner, repo, pr_number, draft_key)
) WITHOUT ROWID;

-- Draft replies to existing threads, one per thread.
CREATE TABLE replies (
    owner TEXT NOT NULL,
    repo TEXT NOT NULL,
    pr_number INTEGER NOT NULL,
    thread_id TEXT NOT NULL,
    body TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (owner, repo, pr_number, thread_id)
) WITHOUT ROWID;

-- Local thread resolution: resolved only while resolved_through is still the thread's
-- last comment. reopened is a manual reopen that keeps the marker.
CREATE TABLE resolutions (
    owner TEXT NOT NULL,
    repo TEXT NOT NULL,
    pr_number INTEGER NOT NULL,
    thread_id TEXT NOT NULL,
    resolved_through TEXT,
    reopened INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (owner, repo, pr_number, thread_id)
) WITHOUT ROWID;

-- The text of the review being written.
CREATE TABLE review_bodies (
    owner TEXT NOT NULL,
    repo TEXT NOT NULL,
    pr_number INTEGER NOT NULL,
    body TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (owner, repo, pr_number)
) WITHOUT ROWID;
