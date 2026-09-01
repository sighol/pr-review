use crate::types::{Approvals, Commit, PrFile, file_is_fully_approved, get_approval_status, last_approved_idx};

/// Column width for the FILE column, and the threshold at which paths get
/// smart-truncated (per spec: "more than 40 chars wide").
pub const MAX_FILE_WIDTH: usize = 60;

// Raw ANSI SGR codes. Foreground/background use their own specific-attribute
// reset (not a full `\x1b[0m` reset) so they compose safely when nested
// inside a selected row's background.
const FG_GREEN: &str = "\x1b[32m";
const FG_RED: &str = "\x1b[31m";
const FG_DEFAULT: &str = "\x1b[39m";
const BG_GREEN: &str = "\x1b[42m";
const BG_DEFAULT: &str = "\x1b[49m";
const BG_SELECTED: &str = "\x1b[43m"; // yellow background; reads as amber/orange in most themes
const BOLD: &str = "\x1b[1m";
const BOLD_RESET: &str = "\x1b[22m";
const RESET: &str = "\x1b[0m";

pub fn render_grid(
    files: &[PrFile],
    commits: &[Commit],
    approvals: &Approvals,
    selected_idx: Option<usize>,
) -> String {
    let mut out = String::new();

    let added_width = files
        .iter()
        .map(|f| f.added.to_string().chars().count())
        .max()
        .unwrap_or(0);
    let removed_width = files
        .iter()
        .map(|f| f.removed.to_string().chars().count())
        .max()
        .unwrap_or(0);
    let stat_width = added_width + 1 + removed_width;

    let header_left = format!(
        "  {:<pad$} {:<stat_width$}",
        "FILE",
        "",
        pad = MAX_FILE_WIDTH,
        stat_width = stat_width
    );
    out.push_str(&header_left);
    out.push_str(" | APPROVAL\n");
    out.push_str(&"-".repeat(header_left.len() + 1));
    out.push('|');
    out.push_str(&"-".repeat("APPROVAL".len() + 1));
    out.push('\n');

    for (idx, file) in files.iter().enumerate() {
        let abbrev = abbreviate_path(&file.path, MAX_FILE_WIDTH);
        let fully_approved = file_is_fully_approved(file, commits, approvals);
        let approved_through_idx = last_approved_idx(file, commits, approvals);
        let is_selected = selected_idx == Some(idx);

        let cursor = if is_selected { "\u{2192}" } else { " " };
        let padded_path = pad_and_bold_filename(&abbrev, MAX_FILE_WIDTH);
        let styled_stat = style_diffstat(file, added_width, removed_width);
        let prefix = format!("{cursor} {padded_path} {styled_stat} | ");

        if is_selected {
            out.push_str(BG_SELECTED);
        }
        out.push_str(&prefix);

        for c in 0..commits.len() {
            let ch = get_approval_status(file, c, commits, approvals).as_char();
            // Selection highlight takes over the whole row, so it wins over
            // the per-cell approval background.
            let is_approved_span = fully_approved || approved_through_idx.is_some_and(|a| c <= a);
            if !is_selected && is_approved_span {
                out.push_str(FG_GREEN);
                out.push_str(BOLD);
                out.push(ch);
                out.push_str(FG_DEFAULT);
                out.push_str(BOLD_RESET);
            } else {
                out.push(ch);
            }
        }

        if is_selected {
            out.push_str(RESET);
        }
        out.push('\n');
    }

    out
}

/// Renders `added/removed` (added in green, removed in red), right-aligning
/// each number within its own column width so the `/` lines up across rows.
fn style_diffstat(file: &PrFile, added_width: usize, removed_width: usize) -> String {
    format!(
        "{FG_GREEN}{added:>added_width$}{FG_DEFAULT}/{FG_RED}{removed:>removed_width$}{FG_DEFAULT}",
        added = file.added,
        removed = file.removed,
    )
}

/// Right-pads `abbrev` to `width` and wraps its filename (the part after the
/// last `/`, or the whole string if there's no `/`) in bold. Padding is
/// computed on the plain text first, since the bold escape codes would
/// otherwise be counted as visible width.
fn pad_and_bold_filename(abbrev: &str, width: usize) -> String {
    let pad_spaces = width.saturating_sub(abbrev.chars().count());
    let filename_start = abbrev.rfind('/').map(|i| i + 1).unwrap_or(0);
    let (dir_part, name_part) = abbrev.split_at(filename_start);
    format!(
        "{}{dir_part}{BOLD}{name_part}{BOLD_RESET}",
        " ".repeat(pad_spaces)
    )
}

/// Shortens a path to fit within `max_width`, abbreviating only as many
/// leading directory components (to their first letter) as needed — the
/// filename and any directories that already fit are left untouched.
/// e.g. `src/very/long/path/to/file.rs` at width 18 -> `s/v/l/p/to/file.rs`.
pub fn abbreviate_path(path: &str, max_width: usize) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    let filename = parts[parts.len() - 1];
    let mut dirs: Vec<String> = parts[..parts.len() - 1].iter().map(|s| s.to_string()).collect();

    let join = |dirs: &[String], filename: &str| -> String {
        let mut result = dirs.join("/");
        if !dirs.is_empty() {
            result.push('/');
        }
        result.push_str(filename);
        result
    };

    let mut result = join(&dirs, filename);
    for i in 0..dirs.len() {
        if result.chars().count() <= max_width {
            break;
        }
        if dirs[i].chars().count() > 1 {
            dirs[i] = dirs[i].chars().next().unwrap_or('?').to_string();
            result = join(&dirs, filename);
        }
    }

    if result.chars().count() > max_width {
        let chars: Vec<char> = result.chars().collect();
        let keep = max_width.saturating_sub(3);
        let truncate_at = chars.len().saturating_sub(keep);
        result = format!("...{}", chars[truncate_at..].iter().collect::<String>());
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_that_already_fit_are_left_untouched() {
        assert_eq!(abbreviate_path("fileC.md", 40), "fileC.md");
        assert_eq!(abbreviate_path("path/to/fileB.ts", 40), "path/to/fileB.ts");
        assert_eq!(
            abbreviate_path("src/very/long/path/to/file.rs", 40),
            "src/very/long/path/to/file.rs"
        );
    }

    #[test]
    fn abbreviates_only_as_many_leading_dirs_as_needed_to_fit() {
        // Full path is 29 chars; abbreviating just the first 4 dirs (leaving
        // "to" intact) is enough to fit width 20.
        assert_eq!(
            abbreviate_path("src/very/long/path/to/file.rs", 20),
            "s/v/l/p/to/file.rs"
        );
    }

    #[test]
    fn abbreviates_all_dirs_when_nothing_less_fits() {
        assert_eq!(
            abbreviate_path("src/very/long/path/to/file.rs", 17),
            "s/v/l/p/t/file.rs"
        );
    }

    #[test]
    fn truncates_from_the_left_when_still_too_long() {
        let result = abbreviate_path("a/b/c/d/e/f/g/h/i/very_long_file", 20);
        assert_eq!(result, ".../i/very_long_file");
        assert_eq!(result.chars().count(), 20);
    }

    fn commit(oid: &str) -> Commit {
        Commit {
            oid: oid.to_string(),
            message: String::new(),
            committed_date: chrono::Utc::now(),
        }
    }

    #[test]
    fn green_background_spans_only_up_to_the_last_approval() {
        // Changed at 0, 2, 3; approved through commit 2 (not the last change,
        // so not fully approved). Green should cover 0..=2 (including the
        // untouched commit 1), but not the still-unapproved commit 3.
        let commits = vec![commit("c0"), commit("c1"), commit("c2"), commit("c3")];
        let file = PrFile {
            path: "fileB".to_string(),
            commits_that_changed_it: vec![0, 2, 3],
            added: 0,
            removed: 0,
        };
        let mut approvals = Approvals::new();
        // Approving "through" c2 records every commit that changed the file
        // up to it — here, c0 and c2 (c3 is still unapproved).
        approvals.insert(
            "fileB".to_string(),
            std::collections::HashSet::from(["c0".to_string(), "c2".to_string()]),
        );

        let out = render_grid(&[file], &commits, &approvals, None);
        assert!(out.contains(&format!(
            "{BG_GREEN}A{BG_DEFAULT}{BG_GREEN} {BG_DEFAULT}{BG_GREEN}A{BG_DEFAULT}*"
        )));
        assert!(!out.contains(BG_SELECTED));
        // File name/stat portion (before " | ") must not carry the green background.
        let (before_bar, _) = out.split_once(" | ").unwrap();
        assert!(!before_bar.contains(BG_GREEN));
    }

    #[test]
    fn fully_approved_row_extends_green_through_trailing_untouched_commits() {
        // Changed at 0, 2; approved through commit 2, which is also the last
        // change, so the file is fully approved. Commits 3, 4 never touched
        // this file, but since it's fully caught up, they turn green too.
        let commits = vec![
            commit("c0"),
            commit("c1"),
            commit("c2"),
            commit("c3"),
            commit("c4"),
        ];
        let file = PrFile {
            path: "fileB".to_string(),
            commits_that_changed_it: vec![0, 2],
            added: 0,
            removed: 0,
        };
        let mut approvals = Approvals::new();
        approvals.insert(
            "fileB".to_string(),
            std::collections::HashSet::from(["c0".to_string(), "c2".to_string()]),
        );

        let out = render_grid(&[file], &commits, &approvals, None);
        assert!(out.contains(&format!(
            "{BG_GREEN}A{BG_DEFAULT}{BG_GREEN} {BG_DEFAULT}{BG_GREEN}A{BG_DEFAULT}{BG_GREEN} {BG_DEFAULT}{BG_GREEN} {BG_DEFAULT}"
        )));
    }

    #[test]
    fn no_green_background_before_any_approval() {
        let commits = vec![commit("c0")];
        let file = PrFile {
            path: "fileB".to_string(),
            commits_that_changed_it: vec![0],
            added: 0,
            removed: 0,
        };
        let out = render_grid(&[file], &commits, &Approvals::new(), None);
        assert!(!out.contains(BG_GREEN));
    }

    #[test]
    fn diffstat_slashes_align_and_bar_column_stays_aligned() {
        let commits = vec![commit("c0")];
        let short = PrFile {
            path: "a.rs".to_string(),
            commits_that_changed_it: vec![0],
            added: 1,
            removed: 0,
        };
        let long = PrFile {
            path: "b.rs".to_string(),
            commits_that_changed_it: vec![0],
            added: 100,
            removed: 50,
        };

        let out = render_grid(&[short, long], &commits, &Approvals::new(), None);
        let lines: Vec<&str> = out.lines().collect();

        // "1/0" pads "1" to width 3 (matching "100") and "0" to width 2
        // (matching "50"), so both '/' land at the same column.
        assert!(lines[2].contains(&format!("{FG_GREEN}  1{FG_DEFAULT}/{FG_RED} 0{FG_DEFAULT}")));
        assert!(lines[3].contains(&format!("{FG_GREEN}100{FG_DEFAULT}/{FG_RED}50{FG_DEFAULT}")));

        let slash_col = |line: &str| line.find('/').unwrap();
        assert_eq!(slash_col(lines[2]), slash_col(lines[3]));

        let bar_col = |line: &str| line.find(" | ").unwrap();
        assert_eq!(bar_col(lines[2]), bar_col(lines[3]));
    }

    #[test]
    fn bolds_only_the_filename_not_the_directories() {
        let out = pad_and_bold_filename("s/v/l/p/to/file.rs", 20);
        assert!(out.contains(&format!("{BOLD}file.rs{BOLD_RESET}")));
        assert!(!out.contains(&format!("{BOLD}s/v/l/p/to/file.rs{BOLD_RESET}")));
        assert!(out.starts_with(' '));
    }

    #[test]
    fn bolds_whole_string_when_no_directory_separator() {
        let out = pad_and_bold_filename("fileB", 10);
        assert!(out.contains(&format!("{BOLD}fileB{BOLD_RESET}")));
    }

    #[test]
    fn selected_row_gets_background_and_full_reset() {
        let commits = vec![commit("c0")];
        let file = PrFile {
            path: "fileB".to_string(),
            commits_that_changed_it: vec![0],
            added: 0,
            removed: 0,
        };
        let approvals = Approvals::new();

        let out = render_grid(&[file], &commits, &approvals, Some(0));
        let row = out.lines().last().unwrap();
        assert!(row.starts_with(BG_SELECTED));
        assert!(row.ends_with(RESET));
    }
}
