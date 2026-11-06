# pr-review

A desktop app for reviewing GitHub PRs: side-by-side diffs, per-file/per-commit approval tracking, local draft comments, and a final review submit. Planned stack is Tauri + Rust backend with a React frontend.

## State of the repo

- **`src/`, `Cargo.toml`, `migrations/` — the old TUI/CLI. It is going to be deleted.** Do not extend it, refactor it, or keep it compiling for its own sake. It is only useful as a reference for behaviour and for logic worth porting (see "Porting from the TUI"). When the Tauri app exists, this code and the root crate go away, and the project may move to a new repo.
- **`ui/` — the new frontend, currently a browser-only prototype.** No Tauri, no Rust, no backend, no sqlite yet. All PR data is fake, derived from this repo's own git history. This is where all current work happens.
- `UI_SPEC.md` — the original (rough) product brief. The decisions below supersede it where they differ.

## Running the prototype

Use **bun** (there is no node/npm on this machine; it's NixOS).

```
cd ui
bun install
bun run dev        # http://localhost:5173
bun test           # unit tests + a render-time smoke test
bun run typecheck  # tsc --noEmit
bun run fixtures   # regenerate public/pr from git history
```

Routes: `/` summary (default), `/diff?file=<path>[&thread=<id> | &line=<side>:<n>]`, `/review`. Browser back/forward work (History API, no router library). The selected commit range is **not** in the URL yet.

## Product decisions already made

Don't re-litigate these without the user asking.

- **Layout:** header with PR title, three tabs (Summary, Diff, Review), a pending-count badge on Review, and a "Finish review" button. All tabs stay mounted so scroll positions survive. A file picker (a `<select>`) sits in the Diff toolbar; there is no file sidebar.
- **Diff view:** custom CSS-grid row renderer, not CodeMirror. Input is `git diff` text plus old/new file text; the frontend parses hunks. Shiki for syntax highlighting, `diff`'s `diffWordsWithSpace` for the stronger changed-word highlight. Soft wrap, `content-visibility: auto` instead of virtualization. Dark mode only. Deleted/added lines pair positionally within a hunk (as GitHub does), with word-level highlights on every pair.
- **Comments:** drag in a gutter on a changed line; the gutter you drag in sets the comment side (left = old/LEFT, right = new/RIGHT). A drag starts and ends on changed lines, may include unchanged lines in between, and can't cross a hunk or collapsed gap. No submit step: a box is a draft the moment it exists, an empty draft is discarded on blur. A **Suggest** button (right side only, single or multi-line) inserts a ` ```suggestion ` block with the current code. Suggestions render as a GitHub-style red/green diff.
- **Threads:** resolution is *local* and modelled on the TUI: a thread is resolved only while the comment you resolved through is still its last comment, so any new comment reopens it. Comments after that are "new since you resolved". GitHub's own `isResolved` is shown separately. Replies are drafts too, shown in an indented card with a vertical DRAFT tab.
- **Review tab:** pending drafts grouped by file, each with a code snippet, the thread above for replies, editable text with a rendered preview, jump and delete. Then the review body and three direct-submit buttons in this order: **Comment, Request changes, Approve**. Buttons are disabled when GitHub would reject them (request changes needs a body; comment needs a body or a draft). No backend yet, so submit just shows the payload that would be sent.
- **File approval (ported from the TUI):** approving a file approves *through* its latest commit (every commit that touched it). Approve / Unapprove buttons in the Diff toolbar, plus "Next unapproved file". No auto-advance.
- **Commit ranges:** any `from..to` can be diffed for any file. The Diff toolbar has a single-row commit matrix you drag across; selecting commits a..b means "changes introduced by commits a through b" = `{from: a-1, to: b}`. The default range is "since last approval" (the full PR diff if nothing or everything is approved). The Summary matrix can set one range for all files. A thread's "View base..#c" button sets the range to exactly `base..#c` (no fallback to the file's last touching commit) and only appears when the comment was on an earlier version and clicking would change the diff.
- **Commenting is range-dependent:** the left gutter needs `from == 0` (GitHub LEFT lines are the PR base); the right gutter needs the range to end at the file's latest version (GitHub anchors new comments to the PR head). Inactive gutters get a tooltip.
- **Thread placement per range** (`threadPlacement` in `lib/range.ts`): new-side threads show at their current line when the range ends at the file's latest version, at their original line only when the range ends exactly at their commit; old-side threads need `from == 0`; otherwise hidden, with a "N threads not shown in this range" strip linking to a range that shows them.
- **Markdown:** `react-markdown` + `remark-gfm` for other people's text; your own drafts stay raw in a textarea (previewed on the Review tab).

## Code map (`ui/src`)

- `lib/diff.ts` — unified-diff parser, row builder (pairing, gaps, whole-file mode), word ranges, drag clamping, suggestion blocks. Pure; this is the part most likely to survive into the real app.
- `lib/range.ts` — commit ranges, content *states*, commentable sides, thread placement. Pure.
- `lib/approval.ts` — approval status, since-approval start point, next-unapproved. Pure.
- `lib/threads.ts` — thread types, resolve/new logic, and the **fake** PR title/description/threads (`PR`, `ALL_THREADS`). Replace with real data later.
- `lib/data.ts` — lazy, cached fetching of file states and diffs; `useRangeData`.
- `lib/route.ts` — URL <-> route parsing. `lib/highlight.ts` — Shiki wrapper. `lib/snippet.ts`, `lib/suggest.ts` — small pure helpers.
- `store.tsx` — the single PR-level store (React context): drafts, replies, resolutions, approvals, ranges, current tab/file, jump requests. URL is the source of truth for tab/file/jump.
- `components/` — `DiffView` (the grid and gutter drag), `CommentBox`, `ThreadBox`, `Markdown`, `FileMatrix`, `RangeBar`, `useCommitDrag`. `tabs/` — `SummaryTab`, `DiffTab`, `ReviewTab`.
- Every `*.test.ts` sits next to its module; `smoke.test.tsx` server-renders the real components over the real fixtures.

## Fixtures and the data model

`ui/scripts/make-fixtures.py` builds a fake PR from this repo's real history (`76578ea..a91e901`: 18 commits, 13 files). Per file, a *state* is a version of its content: the PR base (state 0) plus each commit that touched it. It writes `public/pr/files/<slug>/text-<s>` for every state and `diff-<i>-<j>` (a real `git diff`) for every pair of states, plus `public/pr/manifest.json`. A commit range maps to a state pair via `stateOf` (a commit that didn't touch the file is the same state as the previous one).

The fake threads' `commit`, `originalLine` and `line` are tied to these fixtures. If you change the commit range or regenerate from different commits, recompute them.

The frontend deliberately consumes only "text + `git diff`", which is the contract the Rust backend should satisfy later (`git show`, `git diff A B -- path` on demand).

## Gotchas

- **Never delete and recreate `ui/public/pr` while the dev server runs.** Vite's watcher loses the directory and serves `index.html` for its files, so the app can't load its data. The generator empties the directory instead; if it still happens, restart the dev server.
- Fast Refresh is noisy because several files export non-components. A stale browser tab after big refactors shows module errors that mean nothing; reload it.
- `ui/dist/` is a build artifact and not gitignored; don't commit it.
- `ui/public/fixtures/` is a leftover from an older fixture layout and is unused. Delete it.
- Browser verification: the claude-in-chrome tools work but are sometimes blocked by a temporary classifier outage; wait and retry rather than looping. The user has declined headless-Chrome screenshots run through Bash, so don't use them unprompted.

## Not built yet

Tauri shell and Rust backend; sqlite storage for drafts, resolutions and approvals; fetching a real PR (git fetch + GitHub GraphQL) and submitting a real review; the selected commit range in the URL; keyboard shortcuts; large-file performance work beyond `content-visibility`.

## Porting from the TUI (before it is deleted)

Worth reading when building the backend: `src/git.rs` (fetch, merge-base, commits in range, `git diff` helpers, owner/repo from the remote), `src/github.rs` (GraphQL for review threads, `isResolved`, `originalCommit`, conversation comments, PR title/refs), `src/db.rs` plus `migrations/` (approvals and `thread_resolutions` schema), `src/types.rs` (approval model). GitHub's API provides `originalCommit { oid }` per comment; the TUI only fetches `line`, but the GUI also needs `originalLine`, `originalStartLine`, `diffSide` and `isOutdated`.

## Working with this user

- For design work they want to be **grilled one question at a time**, each with a recommended answer; they usually answer "yes"/"A". Don't build until the design questions are settled, then build and report concisely.
- Keep responses short. Say plainly what was and wasn't verified (especially anything not checked in a browser).
- Don't commit unless asked.
