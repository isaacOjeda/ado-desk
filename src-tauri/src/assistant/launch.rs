//! How the assistant panel finds and starts the user's own `claude` CLI: pure helpers for the
//! executable lookup, the login-shell PATH and the terminal size.
//!
//! The binary runs unmodified, signed in with the user's own Claude account; the app never
//! reads, stores or passes Claude credentials.

use std::path::{Path, PathBuf};

/// Executable names tried in each PATH directory.
#[cfg(windows)]
pub const CLAUDE_NAMES: &[&str] = &["claude.exe", "claude.cmd"];
#[cfg(not(windows))]
pub const CLAUDE_NAMES: &[&str] = &["claude"];

/// Wraps the PATH printed by the login shell, so text that shell startup files print is ignored.
pub const PATH_MARKER: &str = "__ADO_DESK_PATH__";

/// Longest path accepted from Settings.
pub const MAX_PATH_LEN: usize = 1024;

/// Terminal size limits (columns, rows) applied to what the webview reports.
pub const MAX_COLS: u16 = 1000;
pub const MAX_ROWS: u16 = 500;

/// The PATH between the two markers in the login shell's output, if present and not empty.
pub fn parse_login_path(output: &str) -> Option<String> {
    let start = output.find(PATH_MARKER)? + PATH_MARKER.len();
    let len = output[start..].find(PATH_MARKER)?;
    let path = output[start..start + len].trim();
    (!path.is_empty()).then(|| path.to_string())
}

/// Directories where Claude Code's installers put the binary, tried after the PATH.
pub fn fallback_dirs(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join(".local").join("bin"),
        home.join(".claude").join("local"),
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ]
}

/// The first `names` entry found in `dirs`, in order, for which `is_executable` holds.
pub fn find_executable(names: &[&str], dirs: &[PathBuf], is_executable: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    dirs.iter()
        .filter(|dir| dir.is_absolute())
        .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
        .find(|candidate| is_executable(candidate))
}

/// A trimmed absolute path from Settings, or `None` for empty, relative or oversized values.
pub fn configured_path(value: Option<&str>) -> Option<PathBuf> {
    let value = value?.trim();
    if value.is_empty() || value.len() > MAX_PATH_LEN || value.contains('\0') {
        return None;
    }
    let path = PathBuf::from(value);
    path.is_absolute().then_some(path)
}

/// Columns and rows clamped to what a PTY accepts; zero becomes the minimum.
pub fn pty_size(cols: u16, rows: u16) -> (u16, u16) {
    (cols.clamp(2, MAX_COLS), rows.clamp(1, MAX_ROWS))
}

/// Length of the longest prefix of `bytes` that does not end inside a UTF-8 sequence. The rest
/// is kept for the next read, so a character split across two reads is not garbled. Invalid
/// bytes are not held back (they are decoded lossily).
pub fn complete_utf8_len(bytes: &[u8]) -> usize {
    // A sequence is at most 4 bytes, so only the last 3 can start an unfinished one.
    for back in 1..=bytes.len().min(3) {
        let index = bytes.len() - back;
        let byte = bytes[index];
        if byte & 0b1100_0000 == 0b1000_0000 {
            continue; // continuation byte: look further back for the lead byte
        }
        let needed = match byte {
            b if b & 0b1110_0000 == 0b1100_0000 => 2,
            b if b & 0b1111_0000 == 0b1110_0000 => 3,
            b if b & 0b1111_1000 == 0b1111_0000 => 4,
            _ => 1,
        };
        return if back < needed { index } else { bytes.len() };
    }
    bytes.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_path_is_read_between_the_markers_ignoring_startup_noise() {
        let output = format!("Welcome to zsh!\n{PATH_MARKER}/Users/me/.local/bin:/usr/bin{PATH_MARKER}\nbye");
        assert_eq!(parse_login_path(&output).as_deref(), Some("/Users/me/.local/bin:/usr/bin"));
    }

    #[test]
    fn login_path_needs_both_markers_and_a_value() {
        assert_eq!(parse_login_path("/usr/bin"), None);
        assert_eq!(parse_login_path(&format!("{PATH_MARKER}/usr/bin")), None);
        assert_eq!(parse_login_path(&format!("{PATH_MARKER}  {PATH_MARKER}")), None);
    }

    #[test]
    fn find_executable_takes_the_first_match_in_directory_order() {
        let dirs = vec![PathBuf::from("/a"), PathBuf::from("/b"), PathBuf::from("/c")];
        let found = find_executable(&["claude"], &dirs, |p| p == Path::new("/b/claude") || p == Path::new("/c/claude"));
        assert_eq!(found, Some(PathBuf::from("/b/claude")));
        assert_eq!(find_executable(&["claude"], &dirs, |_| false), None);
    }

    #[test]
    fn find_executable_skips_relative_directories() {
        // An empty PATH entry means the current directory; it is never searched.
        let dirs = vec![PathBuf::from(""), PathBuf::from("bin"), PathBuf::from("/usr/bin")];
        let found = find_executable(&["claude"], &dirs, |_| true);
        assert_eq!(found, Some(PathBuf::from("/usr/bin/claude")));
    }

    #[test]
    fn fallback_dirs_start_with_the_native_installer_location() {
        let dirs = fallback_dirs(Path::new("/Users/me"));
        assert_eq!(dirs[0], PathBuf::from("/Users/me/.local/bin"));
        assert!(dirs.iter().all(|d| d.is_absolute()));
    }

    #[test]
    fn configured_path_accepts_only_trimmed_absolute_paths() {
        assert_eq!(configured_path(Some(" /opt/claude/bin/claude ")), Some(PathBuf::from("/opt/claude/bin/claude")));
        assert_eq!(configured_path(Some("claude")), None);
        assert_eq!(configured_path(Some("   ")), None);
        assert_eq!(configured_path(None), None);
        assert_eq!(configured_path(Some(&format!("/{}", "a".repeat(MAX_PATH_LEN)))), None);
        assert_eq!(configured_path(Some("/tmp/a\0b")), None);
    }

    #[test]
    fn pty_size_is_clamped() {
        assert_eq!(pty_size(0, 0), (2, 1));
        assert_eq!(pty_size(120, 40), (120, 40));
        assert_eq!(pty_size(u16::MAX, u16::MAX), (MAX_COLS, MAX_ROWS));
    }

    #[test]
    fn complete_utf8_len_holds_back_a_split_character() {
        let text = "añ€😀".as_bytes(); // 1 + 2 + 3 + 4 bytes
        assert_eq!(complete_utf8_len(text), text.len());
        // Cut inside each multi-byte character.
        assert_eq!(complete_utf8_len(&text[..2]), 1); // ñ's first byte
        assert_eq!(complete_utf8_len(&text[..4]), 3); // € lead byte only
        assert_eq!(complete_utf8_len(&text[..5]), 3); // € missing its last byte
        assert_eq!(complete_utf8_len(&text[..9]), 6); // 😀 missing one byte
        assert_eq!(complete_utf8_len(b""), 0);
    }

    #[test]
    fn complete_utf8_len_never_holds_back_invalid_bytes() {
        assert_eq!(complete_utf8_len(&[b'a', 0xFF]), 2);
        assert_eq!(complete_utf8_len(&[0x80, 0x80, 0x80]), 3);
    }
}
