-- "Resolve this thread when the review is sent" flag on a draft comment (new thread) or a draft reply.
ALTER TABLE drafts ADD COLUMN resolve INTEGER NOT NULL DEFAULT 0;
ALTER TABLE replies ADD COLUMN resolve INTEGER NOT NULL DEFAULT 0;
