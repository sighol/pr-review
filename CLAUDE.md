# pr-review

A desktop app for reviewing GitHub PRs: side-by-side diffs, per-file/per-commit approval tracking, local draft comments, and a final review submit. Stack: Tauri v2 + Rust backend with a React frontend.

## State of the repo

- **`ui/` — the React frontend.** Runs in a plain browser against fake fixtures derived from this repo's own git history, or inside Tauri against the real backend (`lib/backend.ts` picks by `__TAURI_INTERNALS__`). Threads, drafts, resolutions and approvals are still fake / in memory.
- **`backend/` (`pr-review-core`) — pure-Rust library crate**: the git layer (`git.rs`) and the manifest builder (`manifest.rs`). No Tauri dependency, so `cargo test -p pr-review-core` works without webkit.
- **`src-tauri/` (`pr-review-app`) — thin Tauri shell** exposing `open_pr`, `file_text`, `file_diff` commands over `pr-review-core`. Building it needs the Tauri v2 Linux system libraries, which the flake dev shell provides.
- Root `Cargo.toml` is just the workspace.
- The old TUI/CLI was deleted. It is still in git history at commit `cdfab13` (see "Porting from the old TUI").
- `UI_SPEC.md` — the original (rough) product brief. The decisions below supersede it where they differ.

## Running

This is NixOS: `flake.nix` provides the dev shell (rust, cargo-tauri, bun, webkit2gtk-4.1 and the other Tauri system libraries). Enter it with `nix develop` before running anything below. Flakes only see git-tracked files, so `git add flake.nix` (and `flake.lock`) after changing them. Use **bun** for the frontend.

```
cd ui
bun install
bun run dev        # http://localhost:5173 (browser, fixtures)
bun test           # unit tests + a render-time smoke test
bun run typecheck  # tsc --noEmit
bun run fixtures   # regenerate public/pr from git history

cargo test -p pr-review-core    # backend tests, no webkit needed
just                            # cargo test, then typecheck, then bun test (run from the repo root)
just install                    # release build via Tauri's bundler; binary -> ~/.cargo/bin, .desktop + icons -> ~/.local/share

# The real app: pr-review [-C|--repository <path>] <PR number>. Without -C/--repository, run it
# from inside a clone of the GitHub repo; -C/--repository works like git's -C (reviews that repo
# instead of the current directory). Needs `git` and an authenticated `gh`. Args are parsed with
# clap. By default it opens the window in the background and returns the terminal once the window
# is up (see "Running in the background"); --wait (-w) stays in the foreground, --verbose (-v) logs
# everything and implies --wait. Under `cargo tauri dev` the app's working directory is src-tauri/ (still
# inside this repo), app args go after two `--`, and it needs --wait (cargo-tauri would otherwise
# think the app exited at once):
cargo tauri dev -- -- --wait 13486
```

## How a PR is opened

`pr-review <N>` (src-tauri/src/lib.rs) does all the work *before* the window opens, printing progress and errors on stderr and exiting non-zero on failure. `pr-review-core::pr::open_pr` runs: `git rev-parse --show-toplevel` -> owner/repo from `origin` -> `gh pr view` (title, body, author, base branch) -> `git fetch origin +refs/pull/N/head:refs/pr-review/N/head +refs/heads/<base>:refs/remotes/origin/<base>` (works for fork PRs; never touches your branches) -> base = merge-base(origin/<base>, head) -> manifest. Review threads (`gh api graphql`, paged) and conversation comments (`gh pr view --json comments`) are fetched in parallel with the git fetch. It always fetches; there is no cache and no refresh while the app is open (restart to see new comments). GitHub is reached only through the `gh` CLI (auth comes from `gh auth login`); everything git can do goes through `git`. The window then gets the stored manifest (including `pr` and `threads`) via the `open_pr` command. The fake threads only appear in browser/fixture mode, whose manifest has no `threads`.

Thread mapping (`backend/src/threads.rs`): `originalCommit` becomes a commit number; **0 means the commit is not in the PR** (force-push/rebase). A thread with commit 0 that is also outdated on the new side is `placeable() === false` (`lib/range.ts`): it is listed on Summary/Review but never placed in a diff, has no "View base..#c" button, and is left out of the "not shown in this range" strip. Multi-line threads are placed under their end line (the range is not highlighted in the diff), but `startLine` is carried (`Thread.start_line`, `FakeThread.startLine`, null for a single line or an outdated thread) because a suggestion replaces the whole range: `replacedLines()` in `lib/threads.ts` gives the lines `startLine..=line` of the head text for the suggestion diff. GitHub comments in your own unsubmitted (PENDING) review are dropped. Conversation comments become one thread with id `conversation` (omitted when empty).

Renames (`git::numstat` runs `git diff -M base head`, like GitHub does for the whole PR; nothing follows renames commit by commit, so a file renamed twice inside one PR is still a delete plus an add): a renamed file is one `ManifestFile`/`FileData` keyed by its head path with `oldPath` set, and its `changedBy` is the union of the commits that touched either name. `git::file_text`/`file_diff` get `old_path` and read it at any commit where the new name does not exist yet; when the two ends of a range have different names, the diff is `git diff <from>:<old> <to>:<new>` (a direct comparison, so a rename with edits is one diff, never delete + add). Threads written on the old name are moved to the new one (`threads::follow_renames`, via `Manifest::set_threads`/`threads_from`). Drafts on a renamed file are submitted with the new path; whether GitHub accepts that for LEFT-side comments has never been tried. The Diff toolbar shows "Renamed from ...", the Summary matrix a tooltip.

Routes: `/` summary (default), `/diff?file=<path>[&thread=<id> | &line=<side>:<n>]`, `/review`. Browser back/forward work (History API, no router library). The selected commit range is **not** in the URL yet.

## Installing with Nix

`flake.nix` exposes `packages.<system>.default` (`nix/package.nix`), `apps.default` and `overlays.default`, besides the dev shell. `nix run .#default -- 13486`, `nix profile install .`, or on NixOS/home-manager add the flake as an input and use `inputs.pr-review.packages.${system}.default` (or `nixpkgs.overlays = [ pr-review.overlays.default ]` and `pkgs.pr-review`). It builds in two steps: the frontend (`ui/dist`, with `bun run build` after `patchShebangs node_modules`, since the sandbox has no `/usr/bin/env`), then `nativeBuildInputs = [ cargo-tauri.hook ... ]` runs the real `cargo tauri build --bundles deb` (`buildAndTestSubdir = "src-tauri"`, `buildFeatures = [ "tauri/custom-protocol" ]` since Tauri doesn't enable that on its own — without it the binary loads the dev server URL instead of the embedded `dist`). `postPatch` copies the built frontend to `ui/dist` (matching `tauri.conf.json`'s `frontendDist`) and deletes `build.beforeBuildCommand` from `tauri.conf.json` so the offline sandbox doesn't try to re-run `bun run build`. `tauriInstallHook` then lifts the binary, `.desktop` entry and icons straight out of the `.deb`'s `data/usr/*` — the same bundle and the same copy the justfile's `install` recipe uses outside Nix — so `tauri.conf.json`'s `bundle` config (icon paths, category, the `pr-review.desktop` template) is the single source of truth instead of being duplicated in Nix. The package is wrapped by `wrapGAppsHook3` for GTK/WebKit and has `git` and `gh` on its PATH as fallbacks (`--suffix`, so yours win). The `.desktop` entry is `NoDisplay` (it only gives Wayland the icon via the app id; without a PR number it just prints usage).

The frontend's dependencies (`nodeModules` in `nix/package.nix`) are a fixed-output derivation with a hash. **When `ui/bun.lock` changes the build fails with a hash mismatch**: put `lib.fakeHash` in `outputHash`, run `nix build .#default`, and paste the `got:` hash. Flakes only see git-tracked files, so `git add` new files (and `Cargo.lock`/`bun.lock` changes) before building. Verified: it builds and `pr-review --help` runs from the store path; the window itself was not opened from the Nix build.

## Logging and diagnosing

**Known webview problem, "tiny header, empty window":** the startup log line `frontend: window -134400x-86400, devicePixelRatio -0.0104...` (negative size and ratio, root font-size in the millions of px) means the webview got a negative pixel ratio, which is exactly `GDK screen resolution / 96` with the resolution at its unset value of -1. Seen on KDE Wayland (WebKitGTK), only sometimes, even though `gtk-xft-dpi` was a fine 96 and a reload did not help. `ensure_gtk_dpi()` in `src-tauri/src/lib.rs` runs before the window is created, logs the display environment (`env ...`, `GDK screen resolution`, monitors) and, if the screen resolution is not positive, sets it from `gtk-xft-dpi` (or 96). If the symptom persists, look at the `GDK screen resolution is now ...` line: if it is positive but the page is still broken, the web process (a separate process) computes its own value and this fix cannot reach it. As a last resort `ui/src/main.tsx` reloads the webview up to 5 times when it starts with a broken viewport (`isViewportBroken`), which was observed not to help when the cause is the resolution.

Everything logs to **stderr** (tracing in Rust, `src-tauri/src/lib.rs::init_logging`), but **logs are off by default**: only the startup phase (warnings only) is shown, and logging is switched off through a `tracing` reload handle when the window is up (`logs_off_after_startup`). `pr-review --verbose`/`-v` (implies `--wait`) turns on info for this app's own crates and the frontend for the whole run (every git and `gh` call with exit status and timing, every Tauri command, the frontend's startup trace). `PR_REVIEW_LOG` does the same and overrides `--verbose`: a level (`debug` etc.) or a full `tracing` filter (e.g. `PR_REVIEW_LOG=pr_review_core=debug,sqlx=warn`). So to diagnose a window problem, run with `--verbose` first.

## Running in the background

`src-tauri/src/launch.rs`. Without `--wait`/`--verbose`, the command you type starts a copy of itself (same args, env `PR_REVIEW_BACKGROUND_COPY=1`, stdout/stdin null, stderr piped), relays that copy's stderr to your terminal, and exits 0 when the copy prints a ready marker line (from the Tauri `setup` hook, i.e. the window exists) or with the copy's exit code if it dies first. So startup errors still print and fail the command. Until the marker the copy is in the terminal's process group, so Ctrl-C stops both; at the marker it calls `setsid`, prints the marker and points its own stderr at `/dev/null` (the pipe has no reader any more). Unix only; elsewhere it runs in the foreground. Only the failing path (error relayed, exit code) has been exercised; the window-up handoff needs a display and was not tested.

The frontend writes to the same log through the `frontend_log` command (`ui/src/lib/log.ts`): a startup trace (`frontend starting`, viewport/font sizes, manifest/saved-state counts and timings, `rendered the PR`), uncaught errors, unhandled promise rejections, failed script/stylesheet loads and `console.error`/`warn`. In a browser only warnings and errors reach the console. If loading fails the window shows the error (`FatalError`) instead of a blank "Loading…", and an error boundary does the same for render errors. If the window looks wrong, the last `frontend:` line in the terminal says how far it got.

Links: every markdown link (`components/Markdown.tsx`) and each comment's relative time (links to the comment on GitHub, `Comment.url`) go through `components/ExternalLink.tsx`, which hands http(s)/mailto URLs to the default browser via `tauri-plugin-opener` (`lib/external.ts`; capability `opener:default`) and blocks other links so nothing navigates inside the app.

## Submitting a review

`submit_review` (`backend/src/review.rs`, `backend/src/submit.rs`) reads drafts, replies and the review body from the **database** (the frontend flushes its debounced saves first and sends only the event), and posts through `gh api graphql --input -` using GitHub's pending-review flow: `addPullRequestReview` (anchored to the fetched head) -> `addPullRequestReviewThread` per draft (single line, or `startLine`..`line`; old side = LEFT) -> `addPullRequestReviewThreadReply` per thread reply -> `submitPullRequestReview(event, body)`. If any step fails the pending review is deleted, so nothing is half-posted and local drafts stay intact. Rules:

- Refuses when GitHub has a **pending review of yours** already (never touches comments written elsewhere), and when the PR's head on GitHub is no longer the head we fetched ("restart to pick up new commits").
- **Stale drafts block submitting.** A draft is stale when the version of its file it was written against (the head for the right side, the base for the left; `head_oid`/`base_oid` stamped on every save) differs from the current one *and the file differs between them* (`git diff --quiet`), or the old commit is gone. The Review tab flags them; "Keep as is" re-stamps the draft (`acknowledge_draft`), deleting it also clears the flag. An empty `base_oid` (drafts saved before migration 0002) counts as unknown, so left-side ones are stale.
- A reply to the PR **conversation** is not part of a review: it is posted afterwards as a plain comment (`addComment`). If only that fails the review stays submitted, the conversation reply stays as a draft, and the UI says so.
- On success: drafts, thread replies and the review body are cleared (approvals and resolutions are kept), threads are refetched and swapped into the store (`threads` is store state, `submitReview` in `store.tsx`), and the review URL is shown. If the refetch fails the old threads stay and the UI says to restart.
- `submit` is generic over a small `GraphQl` trait so the ordering and the rollback are unit-tested with a fake; the real GitHub calls have never been exercised in tests.

## Persistence

Approvals, draft comments, draft replies, local thread resolutions and the review body are saved in sqlite (`backend/src/db.rs`, sqlx **without** the compile-time query macros; only `sqlx::migrate!`, which just embeds `backend/migrations/*.sql`). Change the schema by adding a new numbered migration, never by editing an applied one.

- One global file, `$XDG_DATA_HOME/pr-review/pr-review.db` (`~/.local/share/...`), every row keyed by (owner, repo, pr_number). `Db` is scoped to the open PR.
- Approvals are stored per (file, commit **oid**), so they survive new pushes. Drafts store the PR head oid they were written against; nothing uses it yet (no stale-draft UI).
- Write-through: the store (`store.tsx`) keeps its in-memory Maps for rendering and calls `persist.*` from `lib/backend.ts` after each change. Saves run through one queue in order; typing is debounced 400ms and flushed on any `focusout`/`pagehide`. Blank drafts, replies and review bodies are deleted rather than stored. Everything saved arrives at startup via the `saved_state` command and seeds the store (`StoreProvider`'s `saved` prop).
- Browser/fixture mode persists nothing: `saved` is null, so the store starts from the seeded prototype approvals and fake threads.

## Product decisions already made

Don't re-litigate these without the user asking.

- **Layout:** header with PR title, three tabs (Summary, Diff, Review), a pending-count badge on Review, and a "Finish review" button. All tabs stay mounted so scroll positions survive. A file picker (a `<select>`) sits in the Diff toolbar; there is no file sidebar.
- **Diff view:** custom CSS-grid row renderer, not CodeMirror. Input is `git diff` text plus old/new file text; the frontend parses hunks. Shiki for syntax highlighting, `diff`'s `diffWordsWithSpace` for the stronger changed-word highlight. Soft wrap, `content-visibility: auto` instead of virtualization. Dark mode only. Deleted/added lines pair positionally within a hunk (as GitHub does), with word-level highlights on every pair.
- **Comments:** drag in a gutter on a changed line; the gutter you drag in sets the comment side (left = old/LEFT, right = new/RIGHT). A drag starts and ends on changed lines, may include unchanged lines in between, and can't cross a hunk or collapsed gap. No submit step: a box is a draft the moment it exists, an empty draft is discarded on blur. A **Suggest** button (right side only, single or multi-line) inserts a ` ```suggestion ` block with the current code. Suggestions render as a GitHub-style red/green diff.
- **Threads:** resolution is *local* and modelled on the TUI: a thread is resolved only while the comment you resolved through is still its last comment, so any new comment reopens it. Comments after that are "new since you resolved". GitHub's own `isResolved` is shown separately. Replies are drafts too, shown in an indented card with a vertical DRAFT tab.
- **Review tab:** pending drafts grouped by file, each with a code snippet, the thread above for replies, editable text with a rendered preview, jump and delete. Then the review body and three direct-submit buttons in this order: **Comment, Request changes, Approve**. Buttons are disabled when GitHub would reject them (request changes needs a body; comment needs a body or a draft). In Tauri the buttons submit for real (see "Submitting a review"); in a browser nothing is sent and the payload that would be sent is shown instead.
- **File approval (ported from the TUI):** approving a file approves *through* its latest commit (every commit that touched it). Approve / Unapprove buttons in the Diff toolbar. Approving also opens the next file that still has unapproved commits (`nextUnapproved`, wrapping; stays put when it was the last one). There is no separate "Next unapproved file" button.
- **Commit ranges:** any `from..to` can be diffed for any file. The Diff toolbar has a single-row commit matrix you drag across; selecting commits a..b means "changes introduced by commits a through b" = `{from: a-1, to: b}`. The default range is "since last approval" (the full PR diff if nothing or everything is approved); there are no buttons for these two, you drag the matrix. The Summary matrix can set one range for all files. A thread's first comment shows a `#<commit>` link in its header whenever that commit differs from the range's `to`, even if `threadPlacement` is already showing the thread there via the same-content fallback below; clicking it sets the range to exactly `base..#commit` (`viewAsOf`, no fallback to the file's last touching commit).
- **Commenting is range-dependent:** the left gutter needs `from == 0` (GitHub LEFT lines are the PR base); the right gutter needs the range to end at the file's latest version (GitHub anchors new comments to the PR head). Inactive gutters get a tooltip.
- **Thread placement per range** (`threadPlacement` in `lib/range.ts`): new-side threads show at their current line when the range ends at the file's latest version; otherwise at their original line whenever the range's `to` shows the same file content as their commit (`stateOf` equality, not an exact commit-number match — a commit that didn't touch the file shows the same content as the one before it) and the commit is known (0 means force-pushed away, never placeable this way); old-side threads need `from == 0`; otherwise hidden, with a "N threads not shown in this range" strip linking to a range that shows them.
- **Discussion navigation** (`components/DiscussionNav.tsx`, `lib/nav.ts`; Diff toolbar buttons plus keys `n`/`p` = next/previous discussion, `Shift+N`/`Shift+P` = next/previous *unresolved*, ignored while typing in an input/textarea/select and on other tabs): deliberately simple, **no DOM or scroll measuring**. The store remembers the discussion last jumped to (`lastThreadId`, set whenever a route carries a thread: next/previous, a Summary click, a thread link, a deep link) and `pickNeighbor` in `lib/nav.ts` steps through all navigable discussions in reading order (file-list order, then line) from it, via `jumpTo` (which adjusts the commit range). With no last discussion, or one on a different file than the one being viewed (e.g. you used the file picker), it steps from the start of the viewed file. Scrolling by hand does not move the position. At either end it shows a notice and does not wrap. "Unresolved" is the *local* resolution state. Not navigable: the conversation, threads on files not in the PR, and unplaceable threads.
- **Markdown:** `react-markdown` + `remark-gfm` for other people's text; your own drafts stay raw in a textarea (previewed on the Review tab).

## Code map (`ui/src`)

- `lib/diff.ts` — unified-diff parser, row builder (pairing, gaps, whole-file mode), word ranges, drag clamping, suggestion blocks. Pure; this is the part most likely to survive into the real app.
- `lib/range.ts` — commit ranges, content *states*, commentable sides, thread placement. Pure.
- `lib/approval.ts` — approval status, since-approval start point, next-unapproved. Pure.
- `lib/threads.ts` — thread types, resolve/new logic, `threadFromJson` for backend threads, and the **fake** PR title/description/threads (`PR`, `ALL_THREADS`) used only in browser/fixture mode. `lib/time.ts` formats "3 hours ago".
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

refreshing threads while the app is open (they are refetched only at startup and after a submit); highlighting a multi-line thread's range; the selected commit range in the URL; keyboard shortcuts beyond the discussion keys; large-file performance work beyond `content-visibility`.

## Porting from the old TUI

Read with `git show cdfab13:<path>`. Worth reading when building the rest of the backend: `src/github.rs` (GraphQL for review threads, `isResolved`, `originalCommit`, conversation comments; PR title/refs are already ported to `backend/src/github.rs`), `src/db.rs` plus `migrations/` (approvals and `thread_resolutions` schema), `src/types.rs` (approval model). GitHub's API provides `originalCommit { oid }` per comment; the TUI only fetches `line`, but the GUI also needs `originalLine`, `originalStartLine`, `diffSide` and `isOutdated`.

## Working with this user

- For design work they want to be **grilled one question at a time**, each with a recommended answer; they usually answer "yes"/"A". Don't build until the design questions are settled, then build and report concisely.
- Keep responses short. Say plainly what was and wasn't verified (especially anything not checked in a browser).
- Don't commit unless asked.
