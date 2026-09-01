use crate::types::{Approvals, Commit, PrFile, get_approval_status};

/// Column width for the FILE column, and the threshold at which paths get
/// smart-truncated (per spec: "more than 40 chars wide").
pub const MAX_FILE_WIDTH: usize = 40;

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
        let approval_str: String = (0..commits.len())
            .map(|c| get_approval_status(file, c, commits, approvals).as_char())
            .collect();
        let cursor = match selected_idx {
            Some(i) if i == idx => "\u{2192}",
            _ => " ",
        };
        out.push_str(&format!(
            "{cursor} {abbrev:>pad$} | {approval_str}\n",
            pad = MAX_FILE_WIDTH
        ));
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
}
