import type { Side } from "./diff";

export interface FakeComment {
  id: string;
  author: string;
  when: string;
  body: string;
}

export interface FakeThread {
  id: string;
  /** Fixture name of the file this thread is on; null for the PR conversation. */
  file: string | null;
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

export const CONVERSATION_ID = "conversation";

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

export function newCount(t: FakeThread, r: Resolution | undefined): number {
  const i = firstNewIndex(t, r);
  return i === null ? 0 : t.comments.length - i;
}

/** Open-with-new first, then open, then resolved; stable within each group. */
export function sortThreads(threads: FakeThread[], resolutions: ReadonlyMap<string, Resolution>): FakeThread[] {
  const rank = (t: FakeThread) => {
    const r = resolutions.get(t.id);
    if (isResolved(t, r)) return 2;
    return newCount(t, r) > 0 ? 0 : 1;
  };
  return [...threads].sort((a, b) => rank(a) - rank(b));
}

export const PR = {
  number: 128,
  title: "Walk up to the git root and show the PR title in the grid",
  author: "sigurd",
  body: `## What

\`pr-review\` now works from any subdirectory of a repo, and the grid view shows which PR you are looking at.

- Walk up from the current directory to find the repo root (\`.git\` or \`.jj\`) when \`--repo\` is not given.
- Thread \`pr_number\`, \`pr_title\` and \`pr_url\` through \`render_grid\` and \`redraw\`, and render the number as a clickable terminal hyperlink.
- Fetch the PR title from GitHub and store it (migration \`0006\`).
- Bump a few dependencies.

## Notes for reviewers

The hyperlink uses an OSC 8 escape sequence, so terminals without support just show the plain \`#128\`.

| Area | Files |
| --- | --- |
| Repo discovery | \`main.rs\` |
| Grid header | \`grid.rs\`, \`interactive.rs\` |
| Plumbing | \`github.rs\`, \`Cargo.toml\` |

\`\`\`rust
fn traverse_up_to_git_repo(path: &Path) -> PathBuf
\`\`\`
`,
};

export const ALL_THREADS: FakeThread[] = [
  {
    id: "t1",
    file: "small-main",
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
    file: "small-main",
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
    file: "small-main",
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
  {
    id: "t3",
    file: "medium-grid",
    side: "new",
    line: 31,
    githubResolved: false,
    comments: [{ id: "c8", author: "carol", when: "1 day ago", body: "Could this header move into its own `render_header` function?" }],
  },
  {
    id: "t5",
    file: "medium-interactive",
    side: "new",
    line: 33,
    githubResolved: false,
    resolvedThrough: "c9",
    comments: [
      { id: "c9", author: "alice", when: "2 days ago", body: "These `redraw` call sites keep growing arguments. Worth a small struct?" },
      { id: "c10", author: "sigurd", when: "2 days ago", body: "Agreed, but I'd rather do that as a follow-up." },
    ],
  },
  {
    id: "t6",
    file: "toml-cargo",
    side: "new",
    line: 13,
    githubResolved: false,
    comments: [
      {
        id: "c11",
        author: "bob",
        when: "4 hours ago",
        body: "Is the `crossterm` bump needed for this PR?\n\n```suggestion\ncrossterm = \"0.28\"\n```",
      },
    ],
  },
  {
    id: CONVERSATION_ID,
    file: null,
    side: "new",
    line: 0,
    githubResolved: false,
    resolvedThrough: "cv1",
    comments: [
      { id: "cv1", author: "alice", when: "3 days ago", body: "Thanks for picking this up. Will review properly tomorrow." },
      { id: "cv2", author: "bob", when: "1 day ago", body: "FWIW I tried this from a `jj` workspace and it works there too." },
      { id: "cv3", author: "carol", when: "2 hours ago", body: "Can you add a note to the README about running from subdirectories?" },
    ],
  },
];
