# PR Reviewer CLI - Complete Specification

## Problem Statement

GitHub's PR review model has a critical gap: there's no way to track approval per-commit. You can only approve a whole PR. When a PR has multiple commits, you lose track of which files you've reviewed at which commits.

This tool solves this problem: **track which files you've approved at which commits**.

## Solution Overview

An interactive terminal CLI that lets you:
1. Browse PR files in a grid showing approval status per commit
2. View colored diffs for any file
3. Mark files as approved through specific commits
4. Persist state in SQLite

The key insight: **approval is "through" a commit**, meaning if you approve file X at commit 5, you've also approved commits 0-4 for that file.

---

## UX Workflow

### The GRID

The main tool for showing the state of approval for the PR is the GRID:

```
FILE                   | APPROVAL
-----------------------|---------
     path/to/file/A      AAAAA
l/a/s/k/path/file/B       A  A *
              fileB      *
```

* Each column in APPROVAL represents one commit.
* If there are no changes for that file in the commit, show empty space ' '.
* If there are changes in the commit that I have not yet approved, show '*'
* If there are changes in the commit I have approved, show 'A'
* If the filepath name is more than 40 chars wide, use smart truncation.

### Entry Point

```bash
$ cd /path/to/repo
$ pr-review sync owner/repo#123
```

### Case 1: All Changes Already Approved

Program displays:
```
FILE                   | APPROVAL
-----------------------|---------
     path/to/file/A      AAAAA
l/a/s/k/path/file/B      AA A A
              fileB      A

✓ All files approved!
[program exits]
```

### Case 2: Changes Need Approval

Program displays grid and enters interactive mode:
```
FILE                   | APPROVAL
-----------------------|---------
 → path/to/file/A        AAAAA
   l/a/s/k/file/B         A  A *
            fileB         *

Use arrow keys (↑↓) to navigate
Press ENTER to review file
Press 'q' to quit
```

**User presses DOWN arrow:**
```
FILE                   | APPROVAL
-----------------------|---------
   path/to/file/A        AAAAA
 → l/a/s/k/file/B         A  A *
            fileB         *
```

**User presses ENTER on l/a/s/k/file/B:**

Colored diff view appears:
```
--- a/src/path/to/file/B.ts
+++ b/src/path/to/file/B.ts
@@ -10,5 +10,7 @@
 function foo() {
-    old_code();
+    new_code();
+    more_code();
 }

Approve this file through commit #7 (of 8)? (y/n): _
```

**User types `y` and presses ENTER:**
```
[File marked as approved]
[Returns to grid view]
[l/a/s/k/file/B now shows: ` A  A A` instead of ` A  A *`]
```

**User types `n` and presses ENTER** (or any other character + ENTER):
```
[File NOT marked as approved]
[Returns to grid view]
[l/a/s/k/file/B still shows: A  A *]
```

**After all files are approved:**
```
✓ All files approved!
[program exits interactive mode]
```

---

## Grid Format Explained

Each character in the APPROVAL column represents one commit.

**Legend:**
- `A` = file changed in this commit AND you approved it
- `*` = file changed in this commit AND you haven't approved it yet
- ` ` (space) = file didn't change in this commit

**Example:**

```
fileB      | * A *
           |[0][1][2] (3 commits total)
```

Meaning:
- Commit 0: file changed, not approved → `*`
- Commit 1: file changed, approved → `A`
- Commit 2: file changed, not approved → `*`

---

## Data Model

### Commit

```rust
struct Commit {
    oid: String,           // 40-char git hash
    message: String,       // commit message
    committed_date: DateTime,
}
```

### File

```rust
struct File {
    path: String,
    commits_that_changed_it: Vec<usize>,  // indices into commits array
}
```

Example:
- If `commits_that_changed_it = [0, 3, 5, 7]` and there are 8 commits
- Then file changed in commits 0, 3, 5, and 7
- File didn't change in commits 1, 2, 4, 6

### Approval Status (per cell in grid)

```rust
enum ApprovalStatus {
    NoChange,    // file not changed in this commit → render ' '
    Unapproved,  // file changed, not approved yet → render '*'
    Approved,    // file changed, approved → render 'A'
}
```

---

## SQLite Database

### Schema

```sql
CREATE TABLE pr_review_state (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    pr_number INTEGER NOT NULL,
    file_path TEXT NOT NULL,
    approved_up_to_commit TEXT NOT NULL,  -- commit OID
    approved_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(pr_number, file_path)
);
```

### Database Location

```
~/.cache/pr-review/{owner}/{repo}/reviews.db
```

Examples:
```
~/.cache/pr-review/anthropic/anthropic-sdk-python/reviews.db
~/.cache/pr-review/kubernetes/kubernetes/reviews.db
```

Each PR's approvals are stored in its repo's database.

---

## Approval Logic

### Rendering a Grid Cell

When deciding what character to render for file F at commit C:

```javascript (pseudocode)
function get_status(file F, commit_idx C):
    // Step 1: Does this file change in commit C?
    if C not in F.commits_that_changed_it:
        return NoChange          // render ' '

    // Step 2: Is this file approved through commit C or later?
    if F has no approval record:
        return Unapproved        // render '*'

    approval_record = database.get(F.path)
    approved_commit_idx = find_index(approval_record.approved_up_to_commit)

    if C <= approved_commit_idx:
        return Approved          // render 'A'
    else:
        return Unapproved        // render '*'
```

### Example

File `fileB` changed in commits: [0, 3, 5, 7]
Approved through commit 5

| Commit | Changed? | <= approved? | Render |
|--------|----------|------------|--------|
| 0      | Yes      | Yes (0≤5)  | A      |
| 1      | No       | —          | (space)|
| 2      | No       | —          | (space)|
| 3      | Yes      | Yes (3≤5)  | A      |
| 4      | No       | —          | (space)|
| 5      | Yes      | Yes (5≤5)  | A      |
| 6      | No       | —          | (space)|
| 7      | Yes      | No (7>5)   | *      |

Grid shows: `A  A  A *`

### Handling New Commits

Scenario: You approved fileB through commit 5. A new commit 8 is pushed.

1. Fetch new commits from GitHub
2. Old approval record stays unchanged: `approved_up_to_commit = <oid of commit 5>`
3. When rendering commit 8:
   - If fileB changed in commit 8: render `*` (not approved yet)
   - If fileB didn't change in commit 8: render ` ` (space)

---

## Implementation Details

### Prerequisites

- Repo must be git cloned locally
- User must be in the repo directory (or pass `--repo` flag)
- GitHub CLI (`gh`) must be installed and authenticated

### Commands (MVP)

```bash
# Main command: sync from GitHub, display grid, enter interactive mode
$ pr-review sync [OWNER/REPO#PR] [--repo /path/to/repo]

# Just show the grid without interactive mode
$ pr-review status [OWNER/REPO#PR]
```

### Dependencies

**Core**
- `sqlx` - SQLite database, with features [sqlite, derive, migrate, chrono] (maybe more.)
- `git2` or shell `git` commands - Get commits, diffs, file history

**GitHub API**
- Shell out to `gh` CLI (uses existing auth)
- Parse JSON responses with `serde_json`

**TUI**
- `crossterm` - Terminal input (keyboard, cursor)
- `syntect` - Syntax highlighting for diffs

**Utilities**
- `chrono` - Timestamps

---

## Workflow: Detailed Steps

### Step 1: Sync

```bash
$ pr-review sync owner/repo#123
```

1. Parse `owner/repo#123` from argument
2. Fetch PR info via `gh api graphql`:
   - All commits in the PR (in order)
   - All files changed in the PR
3. For each file: run `git log` to find which commits touched it
4. Load existing approvals from SQLite
5. Render grid

### Step 2: Check if Done

```rust
if !has_any_unapproved_changes(files, commits, approvals) {
    println!("✓ All files approved!");
    exit(0);
}
```

### Step 3: Interactive Mode (if needed)

```
Loop:
  - Render grid with cursor
  - Wait for key input
  - If UP/j: move cursor up
  - If DOWN/k: move cursor down
  - If ENTER: show diff view for selected file
  - If 'q': exit loop
  - After each action, check if all approved
    - If yes: print "✓ All files approved!" and exit
```

### Diff View

```
1. Show colored diff for the selected file
   (diff from last approved state through the latest commit that touched the file)
2. Prompt: "Approve this file through commit #X (of Y)? (y/n): "
3. Wait for user input + ENTER
4. If 'y':
   - Store approval in SQLite
   - approved_up_to_commit = latest_commit_that_changed_this_file
5. Return to grid view
```

---

## Grid Rendering Details

### Algorithm

```rust
fn render_grid(
    files: &[File],
    commits: &[Commit],
    approvals: &Database,
    selected_idx: usize,
) {
    let max_file_width = 30;

    // Header
    println!("FILE{:pad$} | APPROVAL", "", pad = max_file_width);
    println!("{:─<width$}", "", width = max_file_width + 12);

    // Rows
    for (idx, file) in files.iter().enumerate() {
        let abbrev_path = abbreviate_path(&file.path, max_file_width);

        // Build approval string (one char per commit)
        let approval_str = (0..commits.len())
            .map(|commit_idx| {
                match get_approval_status(file, commit_idx, commits, approvals) {
                    ApprovalStatus::NoChange => ' ',
                    ApprovalStatus::Unapproved => '*',
                    ApprovalStatus::Approved => 'A',
                }
            })
            .collect::<String>();

        // Show cursor (→) on selected line
        let cursor = if idx == selected_idx { "→" } else { " " };

        println!("{} {:pad$} | {}", cursor, abbrev_path, approval_str, pad = max_file_width);
    }
}
```

### Path Abbreviation

Convert long paths to short form while keeping filename intact.

**Examples:**
```
src/very/long/path/to/file.rs      → s/v/l/p/t/file.rs
path/to/fileB.ts                   → p/t/fileB.ts
fileC.md                           → fileC.md
a/b/c/d/e/f/g/h/i/very_long_file  → a/b/c.../very_long_file
```

**Algorithm:**
```rust
fn abbreviate_path(path: &str, max_width: usize) -> String {
    let parts: Vec<&str> = path.split('/').collect();

    // If 2 or fewer parts, return as-is
    if parts.len() <= 2 {
        return path.to_string();
    }

    let filename = parts[parts.len() - 1];
    let dirs = &parts[..parts.len() - 1];

    // Abbreviate each directory to first character
    let abbrev = dirs
        .iter()
        .map(|d| d.chars().next().unwrap_or('?'))
        .collect::<Vec<_>>();

    let mut result = abbrev
        .iter()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join("/");
    result.push('/');
    result.push_str(filename);

    // If still too long, truncate from the left with "..."
    if result.len() > max_width {
        let truncate_at = result.len() - (max_width - 3);
        result = format!("...{}", &result[truncate_at..]);
    }

    result
}
```

---

## Technical Decisions

### Why SQLite?

- Lightweight, no external service needed
- Easy to query and update
- Can have multiple databases (one per repo)
- Portable (file-based)

### Why Shell Out to `gh` CLI?

- Already has GitHub auth configured
- No need to manage tokens in the app
- Simpler than maintaining a GitHub API client
- Can parse JSON responses easily

### Why Use Local Git?

- Repo is already cloned
- Can get accurate file history
- Can show real diffs with syntax highlighting
- Faster than fetching everything from GitHub API

### Why Approval is "Through" a Commit?

- Reflects reality: once you approve file X at commit 5, you implicitly approve commits 0-4
- New commits don't invalidate old work
- Simpler logic than per-commit approval

---

## MVP Scope

### Phase 1 (Must Have)

- [x] Parse `owner/repo#PR` argument
- [x] Fetch PR commits from GitHub via `gh api graphql`
- [x] Fetch PR files from GitHub via `gh api graphql`
- [x] Determine which commits changed which files (using local git)
- [x] Load existing approvals from SQLite
- [x] Render grid with:
  - File paths (smart abbreviation)
  - Approval status per commit (A, *, or space)
  - Cursor indicator (→)
- [x] Check if all approved → exit if yes
- [x] Interactive navigation:
  - Arrow keys and j/k to move cursor
  - ENTER to view diff
  - 'q' to quit
- [x] Diff view:
  - Show colored diff from `git diff`
  - Prompt for approval
  - 'y' + ENTER to approve
  - Any other key + ENTER to skip
- [x] SQLite persistence:
  - Store approvals with commit OID
  - Update grid on return from diff view
- [x] Handle new commits gracefully

### Phase 2 (Nice to Have)

- [ ] Show which commits touched which file in diff view
- [ ] Keyboard shortcuts to approve multiple files at once
- [ ] Search/filter files by name
- [ ] Sync approvals back to GitHub (mark files as viewed)
- [ ] Config file for aliases and defaults
- [ ] Shell completion (bash, zsh)
- [ ] Better error messages
- [ ] Colored terminal output

### Phase 3 (Future)

- [ ] Pagination for large PRs (100+ files)
- [ ] Discussion thread tracking
- [ ] Integration with code review tools
- [ ] Web UI option

---

## File Structure (Proposed)

```
pr-review/
├── Cargo.toml
├── Cargo.lock
├── src/
│   ├── main.rs
│   ├── cli.rs              # Command-line argument parsing
│   ├── github.rs           # GitHub API calls via gh CLI
│   ├── git.rs              # Git operations
│   ├── db.rs               # SQLite database
│   ├── grid.rs             # Grid rendering
│   ├── diff.rs             # Diff viewing
│   ├── interactive.rs      # Interactive mode (keyboard input)
│   └── types.rs            # Data structures
└── README.md
```

---

## Key Pseudocode

### Main Flow

```
fn main():
    args = parse_arguments()
    owner, repo, pr_number = parse_pr_reference(args.pr_ref)

    // Step 1: Fetch data
    commits = fetch_commits_from_github(owner, repo, pr_number)
    pr_files = fetch_files_from_github(owner, repo, pr_number)

    // Step 2: Enrich with local git info
    for each file in pr_files:
        file.commits_that_changed_it = get_commits_that_changed(file.path, commits)

    // Step 3: Load approvals
    db = Database.open(owner, repo)
    approvals = db.load_all_approvals(pr_number)

    // Step 4: Render grid
    render_grid(pr_files, commits, approvals)

    // Step 5: Check if done
    if not has_unapproved_changes(pr_files, commits, approvals):
        print "✓ All files approved!"
        exit(0)

    // Step 6: Interactive mode
    selected_idx = 0
    loop:
        render_grid(pr_files, commits, approvals, selected_idx)
        key = read_key()

        if key == UP:
            selected_idx = (selected_idx - 1) % pr_files.len()
        elif key == DOWN:
            selected_idx = (selected_idx + 1) % pr_files.len()
        elif key == ENTER:
            show_diff_view(pr_files[selected_idx], commits, db)
            // After diff view, approvals may have changed
        elif key == 'q':
            break

        // Check if done after each action
        if not has_unapproved_changes(pr_files, commits, db.load_all_approvals()):
            print "✓ All files approved!"
            break
```

### Diff View Flow

```
fn show_diff_view(file, commits, db):
    // Find the latest commit that changed this file
    latest_idx = max(file.commits_that_changed_it)

    // Show diff from base through this commit
    diff = git_diff(file.path, latest_idx, commits)
    print_colored_diff(diff)

    // Prompt for approval
    print "Approve this file through commit #{latest_idx+1} (of {commits.len()})? (y/n): "
    user_input = read_line()

    if user_input.trim() == "y":
        commit_oid = commits[latest_idx].oid
        db.insert_or_update_approval(file.path, commit_oid)

    // Return to grid (approvals updated)
```

---

## Approval Status Checks

### Has Unapproved Changes?

```rust
fn has_unapproved_changes(
    files: &[File],
    commits: &[Commit],
    approvals: &Database,
) -> bool {
    for file in files {
        for (commit_idx, _) in commits.iter().enumerate() {
            if get_approval_status(file, commit_idx, commits, approvals)
                == ApprovalStatus::Unapproved {
                return true;
            }
        }
    }
    false
}
```

---

## Summary

This tool provides a simple, intuitive way to track file-level code review progress across commits in a PR. The key features are:

1. **Grid View**: See approval status at a glance
2. **Commit-Based Approval**: Approve files "through" a commit (includes all earlier commits)
3. **Interactive Navigation**: Arrow keys and j/k to browse files
4. **Diff Viewing**: See changes before approving
5. **Persistent State**: SQLite tracks approvals per repo
6. **Smart Exit**: Exits when all files approved

The tool works entirely offline (except for initial sync with GitHub) and requires only a local git clone and the `gh` CLI.
