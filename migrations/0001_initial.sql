CREATE TABLE pr_review_state (
    id INTEGER PRIMARY KEY,
    pr_number INTEGER NOT NULL,
    file_path TEXT NOT NULL,
    approved_up_to_commit TEXT NOT NULL,
    approved_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(pr_number, file_path)
);
