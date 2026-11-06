import type { Side } from "./diff";
import { relativeTime } from "./time";

export interface FakeComment {
  id: string;
  author: string;
  when: string;
  body: string;
  /** The comment on github.com; the fake comments have none. */
  url?: string;
}

export interface FakeThread {
  id: string;
  /** Path of the file this thread is on; null for the PR conversation. */
  file: string | null;
  side: Side;
  /** Current line (old-side threads: line in the PR base); null when the thread is outdated. */
  line: number | null;
  /** The line in the version the reviewer saw, i.e. at `commit`. */
  originalLine: number;
  /** The commit the thread was made on (GitHub's `originalCommit`); 0 when that commit is not in the PR. */
  commit: number;
  /** GitHub's own resolved flag; independent of our local resolution. */
  githubResolved: boolean;
  /** Seed for local state: the comment id we last resolved this thread through. */
  resolvedThrough?: string;
  comments: FakeComment[];
}

/** A thread as the backend sends it. */
export interface ThreadJson {
  id: string;
  file: string | null;
  side: Side;
  line: number | null;
  originalLine: number;
  /** Commit number; 0 when unknown (force-pushed away) or for the conversation. */
  commit: number;
  githubResolved: boolean;
  comments: { id: string; author: string; createdAt: string; body: string; url: string }[];
}

export function threadFromJson(j: ThreadJson, now: number): FakeThread {
  return {
    ...j,
    comments: j.comments.map((c) => ({
      id: c.id,
      author: c.author,
      when: relativeTime(c.createdAt, now),
      body: c.body,
      url: c.url,
    })),
  };
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

export interface PrInfo {
  number: number;
  title: string;
  author: string;
  body: string;
}

/** The fake PR shown when running in a browser against fixtures. */
export const PR: PrInfo = {
  number: 128,
  title: "Thread review improvements, PR title in the grid, and repo discovery",
  author: "sigurd",
  body: `## What

A batch of quality-of-life changes to \`pr-review\`, mostly around the \`threads\` flow.

- **Threads:** show the commit each comment was made on, mark where you left off, surface GitHub's own resolved flag, and link every comment to its permalink.
- **Active PR:** \`get\` and \`set\` subcommands to change the selected PR, and \`--fetch\` on \`threads\` as well as \`diff\`.
- **Grid:** show the PR number (as a clickable terminal hyperlink) and title above the file matrix.
- **Repo discovery:** walk up from the current directory to the repo root (\`.git\` or \`.jj\`) when \`--repo\` is not given.
- **Base:** use the merge-base with the base branch instead of the branch tip (migration \`0005\`), and store the PR title (migration \`0006\`).

## Notes for reviewers

The hyperlink uses an OSC 8 escape sequence, so terminals without support just show the plain \`#128\`.

| Area | Files |
| --- | --- |
| Threads | \`threads.rs\`, \`github.rs\` |
| CLI | \`cli.rs\`, \`main.rs\` |
| Grid and prompts | \`grid.rs\`, \`interactive.rs\` |
| Storage | \`db.rs\`, \`migrations/\` |

\`\`\`rust
fn traverse_up_to_git_repo(path: &Path) -> PathBuf
\`\`\`
`,
};

export const ALL_THREADS: FakeThread[] = [
  {
    id: "t1",
    file: "src/main.rs",
    side: "new",
    line: 42,
    originalLine: 42,
    commit: 18,
    githubResolved: false,
    comments: [
      { id: "c1", author: "alice", when: "3 days ago", body: "Do we really need `PathBuf` here, or can `traverse_up_to_git_repo` borrow?" },
      { id: "c2", author: "bob", when: "3 days ago", body: "It walks up parents and returns an owned path, so `PathBuf` is needed." },
    ],
  },
  {
    id: "t2",
    file: "src/main.rs",
    side: "new",
    line: 47,
    originalLine: 47,
    commit: 18,
    githubResolved: true,
    resolvedThrough: "c4",
    comments: [
      { id: "c3", author: "alice", when: "3 days ago", body: "`panic!` is harsh for a CLI. Maybe `bail!` instead?" },
      { id: "c4", author: "bob", when: "2 days ago", body: "Fixed in the follow-up." },
    ],
  },
  {
    id: "t3",
    file: "src/main.rs",
    side: "new",
    line: 73,
    originalLine: 63,
    commit: 14,
    githubResolved: false,
    resolvedThrough: "c5",
    comments: [
      { id: "c5", author: "carol", when: "2 days ago", body: "Nice, `set` fetching the PR up front avoids the confusing 'no PR found' error later." },
      { id: "c6", author: "alice", when: "5 hours ago", body: "Does `set` also reset the stored approvals when you switch PRs?" },
      { id: "c7", author: "bob", when: "1 hour ago", body: "Good question. Approvals are keyed by PR number, so no, they stay." },
    ],
  },
  {
    id: "t4",
    file: "src/threads.rs",
    side: "new",
    line: 170,
    originalLine: 162,
    commit: 10,
    githubResolved: false,
    comments: [{ id: "c8", author: "carol", when: "1 day ago", body: "Could the 'NEW COMMENTS' divider use the same colour as the rest of the headings?" }],
  },
  {
    id: "t5",
    file: "src/threads.rs",
    side: "new",
    line: 95,
    originalLine: 95,
    commit: 16,
    githubResolved: true,
    resolvedThrough: "c10",
    comments: [
      { id: "c9", author: "alice", when: "2 days ago", body: "Is `Some(t.is_resolved)` right here? The conversation case below uses `None`." },
      { id: "c10", author: "sigurd", when: "2 days ago", body: "Yes. The top-level conversation has no resolved concept on GitHub." },
    ],
  },
  {
    id: "t6",
    file: "Cargo.toml",
    side: "old",
    line: 13,
    originalLine: 13,
    commit: 1,
    githubResolved: false,
    comments: [{ id: "c11", author: "bob", when: "4 hours ago", body: "What was the reason for staying on `crossterm` 0.28 before?" }],
  },
  {
    id: "t7",
    file: "Cargo.toml",
    side: "new",
    line: 13,
    originalLine: 13,
    commit: 1,
    githubResolved: false,
    comments: [
      {
        id: "c12",
        author: "bob",
        when: "4 hours ago",
        body: "Is the `crossterm` bump needed for this PR?\n\n```suggestion\ncrossterm = \"0.28\"\n```",
      },
    ],
  },
  {
    id: "t8",
    file: "src/grid.rs",
    side: "new",
    line: 32,
    originalLine: 32,
    commit: 13,
    githubResolved: false,
    comments: [{ id: "c13", author: "carol", when: "1 day ago", body: "Could this header move into its own `render_header` function?" }],
  },
  {
    id: "t9",
    file: "src/github.rs",
    side: "new",
    line: 131,
    originalLine: 131,
    commit: 16,
    githubResolved: true,
    resolvedThrough: "c15",
    comments: [
      { id: "c14", author: "alice", when: "3 days ago", body: "Why the `serde(rename)` instead of `rename_all = \"camelCase\"` on the struct?" },
      { id: "c15", author: "sigurd", when: "3 days ago", body: "Only this one field is camelCase in our query; the rest are fine as-is." },
    ],
  },
  {
    id: "t10",
    file: "src/db.rs",
    side: "new",
    line: 119,
    originalLine: 119,
    commit: 13,
    githubResolved: false,
    comments: [{ id: "c16", author: "bob", when: "6 hours ago", body: "Please add a test for upserting a changed base ref name." }],
  },
  {
    id: "t11",
    file: "src/db.rs",
    side: "new",
    line: null,
    originalLine: 96,
    commit: 7,
    githubResolved: false,
    comments: [{ id: "c17", author: "alice", when: "5 days ago", body: "This query will get long. Maybe build it from a constant column list?" }],
  },
  {
    id: "t12",
    file: "src/interactive.rs",
    side: "new",
    line: 166,
    originalLine: 132,
    commit: 9,
    githubResolved: false,
    comments: [{ id: "c18", author: "carol", when: "4 days ago", body: "Should Ctrl-C also restore the cursor before exiting?" }],
  },
  {
    id: "t13",
    file: "src/main.rs",
    side: "new",
    line: 178,
    originalLine: 133,
    commit: 7,
    githubResolved: false,
    comments: [
      { id: "c19", author: "bob", when: "5 days ago", body: "Does `merge_base` fail loudly when the base branch isn't fetched yet?" },
      { id: "c20", author: "sigurd", when: "5 days ago", body: "Yes, it bubbles up the git error with the branch name in it." },
    ],
  },
  {
    id: CONVERSATION_ID,
    file: null,
    side: "new",
    line: 0,
    originalLine: 0,
    commit: 18,
    githubResolved: false,
    resolvedThrough: "cv1",
    comments: [
      { id: "cv1", author: "alice", when: "3 days ago", body: "Thanks for picking this up. Will review properly tomorrow." },
      { id: "cv2", author: "bob", when: "1 day ago", body: "FWIW I tried this from a `jj` workspace and it works there too." },
      { id: "cv3", author: "carol", when: "2 hours ago", body: "Can you add a note to the README about running from subdirectories?" },
    ],
  },
];
