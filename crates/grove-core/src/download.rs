//! Picking a safe place for downloaded files.

use std::path::{Path, PathBuf};

const MAX_NAME_CHARS: usize = 150;
const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4",
    "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Reduce a server-suggested file name to something that is safe to create
/// inside the downloads folder: no directories, no control or reserved
/// characters, never empty.
pub fn safe_file_name(raw: &str) -> String {
    let last = raw.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = last.chars().map(|c| if c.is_control() || "<>:\"|?*".contains(c) { '_' } else { c }).collect();
    let cleaned = cleaned.trim_matches(|c: char| c == ' ' || c == '.').to_string();

    let mut name: String = cleaned.chars().take(MAX_NAME_CHARS).collect();
    if name.is_empty() {
        return "download".into();
    }

    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    if RESERVED.contains(&stem.as_str()) {
        name.insert(0, '_');
    }
    name
}

/// `dir/name`, or `dir/name (1).ext`, `dir/name (2).ext`, ... if taken.
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let name = safe_file_name(name);
    let first = dir.join(&name);
    if !first.exists() {
        return first;
    }

    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name.as_str(), ""),
    };
    for n in 1..10_000 {
        let candidate = dir.join(format!("{stem} ({n}){ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{stem} (copy){ext}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn strips_directories_and_bad_characters() {
        assert_eq!(safe_file_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_file_name("C:\\Windows\\evil.exe"), "evil.exe");
        assert_eq!(safe_file_name("a<b>c:d.txt"), "a_b_c_d.txt");
        assert_eq!(safe_file_name("tab\tname\n.txt"), "tab_name_.txt");
    }

    #[test]
    fn never_empty_or_dotty() {
        assert_eq!(safe_file_name(""), "download");
        assert_eq!(safe_file_name("..."), "download");
        assert_eq!(safe_file_name("/"), "download");
        assert_eq!(safe_file_name("  .hidden  "), "hidden");
    }

    #[test]
    fn reserved_windows_names_are_prefixed() {
        assert_eq!(safe_file_name("con.txt"), "_con.txt");
        assert_eq!(safe_file_name("NUL"), "_NUL");
        assert_eq!(safe_file_name("console.txt"), "console.txt");
    }

    #[test]
    fn long_names_are_truncated() {
        let long = "a".repeat(400);
        assert_eq!(safe_file_name(&long).chars().count(), MAX_NAME_CHARS);
    }

    #[test]
    fn picks_unused_path() {
        let dir = tempdir().unwrap();
        assert_eq!(unique_path(dir.path(), "a.txt"), dir.path().join("a.txt"));
        fs::write(dir.path().join("a.txt"), "x").unwrap();
        assert_eq!(unique_path(dir.path(), "a.txt"), dir.path().join("a (1).txt"));
        fs::write(dir.path().join("a (1).txt"), "x").unwrap();
        assert_eq!(unique_path(dir.path(), "a.txt"), dir.path().join("a (2).txt"));
        fs::write(dir.path().join("noext"), "x").unwrap();
        assert_eq!(unique_path(dir.path(), "noext"), dir.path().join("noext (1)"));
    }

    #[test]
    fn unique_path_sanitizes_input() {
        let dir = tempdir().unwrap();
        assert_eq!(unique_path(dir.path(), "../x.txt"), dir.path().join("x.txt"));
    }
}
