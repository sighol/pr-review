CREATE TABLE thread_resolutions (
    id INTEGER PRIMARY KEY,
    pr_number INTEGER NOT NULL,
    thread_id TEXT NOT NULL,
    resolved_comment_id TEXT NOT NULL,
    resolved_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(pr_number, thread_id)
);
