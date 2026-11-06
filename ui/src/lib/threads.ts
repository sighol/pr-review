import type { Side } from "./diff";

export interface FakeComment {
  id: string;
  author: string;
  when: string;
  body: string;
}

export interface FakeThread {
  id: string;
  side: Side;
  line: number;
  /** GitHub's own resolved flag; independent of our local resolution. */
  githubResolved: boolean;
  /** Seed for local state: the comment id we last resolved this thread through. */
  resolvedThrough?: string;
  comments: FakeComment[];
}

/**
 * Local resolution state for a thread. `through` is the last comment we
 * resolved through; the thread is resolved only while that is still its last
 * comment, so any new comment reopens it automatically.
 */
export interface Resolution {
  through: string | null;
  reopened: boolean;
}

export const lastCommentId = (t: FakeThread) => t.comments[t.comments.length - 1].id;

export function isResolved(t: FakeThread, r: Resolution | undefined): boolean {
  return !!r && !r.reopened && r.through === lastCommentId(t);
}

/** Index of the first comment newer than the last resolution, or null. */
export function firstNewIndex(t: FakeThread, r: Resolution | undefined): number | null {
  if (!r?.through) return null;
  const i = t.comments.findIndex((c) => c.id === r.through);
  return i >= 0 && i < t.comments.length - 1 ? i + 1 : null;
}

export const FAKE_THREADS: Record<string, FakeThread[]> = {
  "small-main": [
    {
      id: "t1",
      side: "new",
      line: 12,
      githubResolved: false,
      comments: [
        { id: "c1", author: "alice", when: "3 days ago", body: "Do we really need `PathBuf` here, or can `traverse_up_to_git_repo` borrow?" },
        { id: "c2", author: "bob", when: "3 days ago", body: "It walks up parents and returns an owned path, so `PathBuf` is needed." },
      ],
    },
    {
      id: "t2",
      side: "new",
      line: 47,
      githubResolved: true,
      resolvedThrough: "c4",
      comments: [
        { id: "c3", author: "alice", when: "3 days ago", body: "`panic!` is harsh for a CLI. Maybe `bail!` instead?" },
        { id: "c4", author: "bob", when: "2 days ago", body: "Fixed in the follow-up." },
      ],
    },
    {
      id: "t4",
      side: "new",
      line: 58,
      githubResolved: false,
      resolvedThrough: "c5",
      comments: [
        { id: "c5", author: "carol", when: "2 days ago", body: "Nice, walking up to the repo root makes `--repo` optional in practice." },
        { id: "c6", author: "alice", when: "5 hours ago", body: "Does this also handle git worktrees, where `.git` is a file rather than a directory?" },
        { id: "c7", author: "bob", when: "1 hour ago", body: "Good catch, `is_dir()` misses that. I'll switch to `exists()`." },
      ],
    },
  ],
  "medium-grid": [
    {
      id: "t3",
      side: "new",
      line: 31,
      githubResolved: false,
      comments: [{ id: "c8", author: "carol", when: "1 day ago", body: "Could this header move into its own `render_header` function?" }],
    },
  ],
};
