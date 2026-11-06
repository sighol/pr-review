-- The PR base a draft was written against, so old-side (left) drafts can be checked for
-- staleness like new-side ones are with head_oid. '' means unknown (drafts saved before this).
ALTER TABLE drafts ADD COLUMN base_oid TEXT NOT NULL DEFAULT '';
