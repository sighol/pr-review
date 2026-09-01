DROP TABLE pr_review_state;

CREATE TABLE file_commit_approvals (
    id INTEGER PRIMARY KEY,
    pr_number INTEGER NOT NULL,
    file_path TEXT NOT NULL,
    commit_oid TEXT NOT NULL,
    approved_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(pr_number, file_path, commit_oid)
);
