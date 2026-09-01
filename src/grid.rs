use crate::types::{ApprovalStatus, Approvals, Commit, PrFile, file_is_fully_approved, get_approval_status};

/// Column width for the FILE column, and the threshold at which paths get
/// smart-truncated (per spec: "more than 40 chars wide").
pub const MAX_FILE_WIDTH: usize = 60;

// Raw ANSI SGR codes. Approved cells/lines use fg-only set/reset (not a full
// `\x1b[0m` reset) so they can be nested inside a selected row's background
// without clobbering it.
const FG_GREEN: &str = "\x1b[32m";
const FG_DEFAULT: &str = "\x1b[39m";
const BG_SELECTED: &str = "\x1b[43m"; // yellow background; reads as amber/orange in most themes
const RESET: &str = "\x1b[0m";

pub fn render_grid(
    files: &[PrFile],
    commits: &[Commit],
    approvals: &Approvals,
    selected_idx: Option<usize>,
) -> String {
    let mut out = String::new();

    let header_left = format!("  {:<pad$}", "FILE", pad = MAX_FILE_WIDTH);
    out.push_str(&header_left);
    out.push_str(" | APPROVAL\n");
    out.push_str(&"-".repeat(header_left.len() + 1));
    out.push('|');
    out.push_str(&"-".repeat("APPROVAL".len() + 1));
    out.push('\n');

    for (idx, file) in files.iter().enumerate() {
        let abbrev = abbreviate_path(&file.path, MAX_FILE_WIDTH);
        let fully_approved = file_is_fully_approved(file, commits, approvals);
        let is_selected = selected_idx == Some(idx);

        let cursor = if is_selected { "\u{2192}" } else { " " };
        let prefix = format!("{cursor} {abbrev:>pad$} | ", pad = MAX_FILE_WIDTH);

        if is_selected {
            out.push_str(BG_SELECTED);
        }

        if fully_approved {
            out.push_str(FG_GREEN);
            out.push_str(&prefix);
            for c in 0..commits.len() {
                out.push(get_approval_status(file, c, commits, approvals).as_char());
            }
            out.push_str(FG_DEFAULT);
        } else {
            out.push_str(&prefix);
            for c in 0..commits.len() {
                let status = get_approval_status(file, c, commits, approvals);
                if status == ApprovalStatus::Approved {
                    out.push_str(FG_GREEN);
                    out.push(status.as_char());
                    out.push_str(FG_DEFAULT);
                } else {
                    out.push(status.as_char());
                }
            }
        }

        if is_selected {
            out.push_str(RESET);
        }
        out.push('\n');
    }

    out
}

/// Converts long paths to a short form while keeping the filename intact,
/// e.g. `src/very/long/path/to/file.rs` -> `s/v/l/p/t/file.rs`.
pub fn abbreviate_path(path: &str, max_width: usize) -> String {
    let parts: Vec<&str> = path.split('/').collect();

    let mut result = if parts.len() <= 2 {
        path.to_string()
    } else {
        let filename = parts[parts.len() - 1];
        let dirs = &parts[..parts.len() - 1];

        let abbrev: Vec<char> = dirs.iter().map(|d| d.chars().next().unwrap_or('?')).collect();

        let mut result = abbrev
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join("/");
        result.push('/');
        result.push_str(filename);
        result
    };

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
    fn short_paths_pass_through() {
        assert_eq!(abbreviate_path("fileC.md", 40), "fileC.md");
        assert_eq!(abbreviate_path("path/to/fileB.ts", 40), "p/t/fileB.ts");
    }

    #[test]
    fn abbreviates_directories_to_first_char() {
        assert_eq!(
            abbreviate_path("src/very/long/path/to/file.rs", 40),
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
    fn colors_approved_cells_green_but_not_the_whole_line() {
        let commits = vec![commit("c0"), commit("c1")];
        let file = PrFile {
            path: "fileB".to_string(),
            commits_that_changed_it: vec![0, 1],
        };
        let mut approvals = Approvals::new();
        approvals.insert("fileB".to_string(), "c0".to_string());

        let out = render_grid(&[file], &commits, &approvals, None);
        assert!(out.contains(&format!("{FG_GREEN}A{FG_DEFAULT}*")));
        assert!(!out.contains(BG_SELECTED));
    }

    #[test]
    fn colors_whole_line_green_when_fully_approved() {
        let commits = vec![commit("c0"), commit("c1")];
        let file = PrFile {
            path: "fileB".to_string(),
            commits_that_changed_it: vec![0, 1],
        };
        let mut approvals = Approvals::new();
        approvals.insert("fileB".to_string(), "c1".to_string());

        let out = render_grid(&[file], &commits, &approvals, None);
        assert!(out.contains(&format!("{FG_GREEN}  {:>pad$} | AA{FG_DEFAULT}", "fileB", pad = MAX_FILE_WIDTH)));
    }

    #[test]
    fn selected_row_gets_background_and_full_reset() {
        let commits = vec![commit("c0")];
        let file = PrFile {
            path: "fileB".to_string(),
            commits_that_changed_it: vec![0],
        };
        let approvals = Approvals::new();

        let out = render_grid(&[file], &commits, &approvals, Some(0));
        let row = out.lines().last().unwrap();
        assert!(row.starts_with(BG_SELECTED));
        assert!(row.ends_with(RESET));
    }
}
