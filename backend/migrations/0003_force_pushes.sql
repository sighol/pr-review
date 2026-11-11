-- The PR head oid last seen, to detect a force-push (history rewrite) on the next open.
CREATE TABLE pr_heads (
    owner TEXT NOT NULL,
    repo TEXT NOT NULL,
    pr_number INTEGER NOT NULL,
    head_oid TEXT NOT NULL,
    PRIMARY KEY (owner, repo, pr_number)
) WITHOUT ROWID;

-- Permanent log of detected force-pushes: the previously known head was not an ancestor of the
-- newly fetched one. Never overwritten, so reopening across several force-pushes shows each one.
CREATE TABLE force_pushes (
    owner TEXT NOT NULL,
    repo TEXT NOT NULL,
    pr_number INTEGER NOT NULL,
    old_oid TEXT NOT NULL,
    new_oid TEXT NOT NULL,
    detected_at TEXT NOT NULL DEFAULT (STRFTIME('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (owner, repo, pr_number, new_oid)
) WITHOUT ROWID;
