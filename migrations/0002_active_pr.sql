CREATE TABLE active_pr (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    pr_number INTEGER NOT NULL,
    base_oid TEXT NOT NULL,
    head_ref_name TEXT NOT NULL
);
