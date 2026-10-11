//! Local, append-only browsing history.
//!
//! The file is JSON Lines. Each line is a small record: a visit, a title
//! update for the most recent visit of a URL, or a clear marker. Loading
//! replays the lines, ignoring anything malformed (for example a line cut off
//! by a crash), so a damaged file never prevents the browser from starting.

use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use url::Url;

/// Most entries kept. Older ones are dropped.
pub const MAX_ENTRIES: usize = 5000;
/// Visits to the same URL closer together than this (seconds) are merged.
/// This absorbs reloads and redirect chains.
pub const MERGE_WINDOW_SECS: u64 = 5;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub url: String,
    pub title: String,
    /// Unix timestamp, seconds.
    pub ts: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "t")]
enum Record {
    #[serde(rename = "v")]
    Visit { url: String, title: String, ts: u64 },
    #[serde(rename = "t")]
    Title { url: String, title: String },
    #[serde(rename = "c")]
    Clear,
}

pub struct History {
    path: Option<PathBuf>,
    /// Oldest first.
    entries: Vec<Entry>,
    lines_on_disk: usize,
}

impl History {
    /// A history that is never written to disk.
    pub fn in_memory() -> Self {
        Self { path: None, entries: Vec::new(), lines_on_disk: 0 }
    }

    /// Load (or start) the history stored at `path`.
    pub fn open(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        let mut history = Self { path: Some(path.clone()), entries: Vec::new(), lines_on_disk: 0 };

        match File::open(&path) {
            Ok(file) => {
                for line in BufReader::new(file).split(b'\n') {
                    let Ok(line) = line else { break };
                    history.lines_on_disk += 1;
                    let Ok(record) = serde_json::from_slice::<Record>(&line) else { continue };
                    history.apply(record);
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }

        history.trim();
        if history.lines_on_disk > MAX_ENTRIES * 2 {
            history.compact()?;
        }
        Ok(history)
    }

    fn apply(&mut self, record: Record) {
        match record {
            Record::Visit { url, title, ts } => self.entries.push(Entry { url, title, ts }),
            Record::Title { url, title } => {
                if let Some(e) = self.entries.iter_mut().rev().find(|e| e.url == url) {
                    e.title = title;
                }
            }
            Record::Clear => self.entries.clear(),
        }
    }

    fn trim(&mut self) {
        if self.entries.len() > MAX_ENTRIES {
            let excess = self.entries.len() - MAX_ENTRIES;
            self.entries.drain(..excess);
        }
    }

    fn append(&mut self, record: &Record) -> io::Result<()> {
        let Some(path) = &self.path else { return Ok(()) };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        let mut line = serde_json::to_vec(record).map_err(io::Error::other)?;
        line.push(b'\n');
        file.write_all(&line)?;
        self.lines_on_disk += 1;
        Ok(())
    }

    /// Rewrite the file with one line per surviving entry.
    fn compact(&mut self) -> io::Result<()> {
        let Some(path) = self.path.clone() else { return Ok(()) };
        let tmp = path.with_extension("jsonl.tmp");
        {
            let mut file = File::create(&tmp)?;
            for e in &self.entries {
                let record = Record::Visit { url: e.url.clone(), title: e.title.clone(), ts: e.ts };
                let mut line = serde_json::to_vec(&record).map_err(io::Error::other)?;
                line.push(b'\n');
                file.write_all(&line)?;
            }
            file.sync_all()?;
        }
        fs::rename(&tmp, &path)?;
        self.lines_on_disk = self.entries.len();
        Ok(())
    }

    /// Record a finished page load. Returns `true` if a new entry was added.
    /// Only `http` and `https` pages are recorded.
    pub fn record_visit(&mut self, url: &str, title: &str, ts: u64) -> io::Result<bool> {
        match Url::parse(url) {
            Ok(u) if matches!(u.scheme(), "http" | "https") => {}
            _ => return Ok(false),
        }

        if let Some(last) = self.entries.last_mut() {
            if last.url == url && ts.saturating_sub(last.ts) < MERGE_WINDOW_SECS {
                if !title.is_empty() && last.title != title {
                    last.title = title.to_string();
                    let record = Record::Title { url: url.to_string(), title: title.to_string() };
                    self.append(&record)?;
                }
                return Ok(false);
            }
        }

        let record = Record::Visit { url: url.to_string(), title: title.to_string(), ts };
        self.entries.push(Entry { url: url.to_string(), title: title.to_string(), ts });
        self.append(&record)?;
        if self.entries.len() > MAX_ENTRIES {
            self.trim();
        }
        if self.lines_on_disk > MAX_ENTRIES * 2 {
            self.compact()?;
        }
        Ok(true)
    }

    /// Update the title of the most recent visit of `url`.
    pub fn set_title(&mut self, url: &str, title: &str) -> io::Result<()> {
        if title.is_empty() {
            return Ok(());
        }
        let Some(entry) = self.entries.iter_mut().rev().find(|e| e.url == url) else { return Ok(()) };
        if entry.title == title {
            return Ok(());
        }
        entry.title = title.to_string();
        self.append(&Record::Title { url: url.to_string(), title: title.to_string() })
    }

    /// Newest first.
    pub fn recent(&self, limit: usize) -> Vec<Entry> {
        self.entries.iter().rev().take(limit).cloned().collect()
    }

    /// Newest first. Every whitespace-separated term must appear in the URL or
    /// title, ignoring case.
    pub fn search(&self, query: &str, limit: usize) -> Vec<Entry> {
        let terms: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        if terms.is_empty() {
            return self.recent(limit);
        }
        self.entries
            .iter()
            .rev()
            .filter(|e| {
                let hay = format!("{} {}", e.title.to_lowercase(), e.url.to_lowercase());
                terms.iter().all(|t| hay.contains(t.as_str()))
            })
            .take(limit)
            .cloned()
            .collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Forget everything, including on disk.
    pub fn clear(&mut self) -> io::Result<()> {
        self.entries.clear();
        self.lines_on_disk = 0;
        if let Some(path) = &self.path {
            if let Some(dir) = path.parent() {
                fs::create_dir_all(dir)?;
            }
            File::create(path)?;
        }
        Ok(())
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn records_and_lists_newest_first() {
        let mut h = History::in_memory();
        assert!(h.record_visit("https://a.com/", "A", 100).unwrap());
        assert!(h.record_visit("https://b.com/", "B", 200).unwrap());
        let r = h.recent(10);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].url, "https://b.com/");
        assert_eq!(r[1].url, "https://a.com/");
        assert_eq!(h.recent(1).len(), 1);
    }

    #[test]
    fn ignores_non_web_urls() {
        let mut h = History::in_memory();
        assert!(!h.record_visit("about:blank", "", 1).unwrap());
        assert!(!h.record_visit("file:///etc/passwd", "", 1).unwrap());
        assert!(!h.record_visit("tauri://localhost/history.html", "", 1).unwrap());
        assert!(!h.record_visit("not a url", "", 1).unwrap());
        assert!(h.is_empty());
    }

    #[test]
    fn rapid_repeat_visits_merge_and_keep_latest_title() {
        let mut h = History::in_memory();
        assert!(h.record_visit("https://a.com/", "", 100).unwrap());
        assert!(!h.record_visit("https://a.com/", "Title", 102).unwrap());
        assert_eq!(h.len(), 1);
        assert_eq!(h.recent(1)[0].title, "Title");
        // After the window a second entry is created.
        assert!(h.record_visit("https://a.com/", "Title", 100 + MERGE_WINDOW_SECS + 1).unwrap());
        assert_eq!(h.len(), 2);
    }

    #[test]
    fn set_title_updates_latest_matching_visit() {
        let mut h = History::in_memory();
        h.record_visit("https://a.com/", "old", 100).unwrap();
        h.record_visit("https://b.com/", "B", 200).unwrap();
        h.record_visit("https://a.com/", "old", 300).unwrap();
        h.set_title("https://a.com/", "new").unwrap();
        let r = h.recent(10);
        assert_eq!(r[0].title, "new");
        assert_eq!(r[2].title, "old");
        h.set_title("https://nowhere.com/", "x").unwrap();
        h.set_title("https://a.com/", "").unwrap();
        assert_eq!(h.recent(1)[0].title, "new");
    }

    #[test]
    fn search_matches_all_terms_case_insensitively() {
        let mut h = History::in_memory();
        h.record_visit("https://docs.rs/tauri", "Tauri docs", 1).unwrap();
        h.record_visit("https://docs.rs/serde", "Serde docs", 10).unwrap();
        h.record_visit("https://example.com/", "Example", 20).unwrap();
        assert_eq!(h.search("DOCS", 10).len(), 2);
        assert_eq!(h.search("docs tauri", 10).len(), 1);
        assert_eq!(h.search("docs nothing", 10).len(), 0);
        assert_eq!(h.search("  ", 10).len(), 3);
        assert_eq!(h.search("docs", 1)[0].url, "https://docs.rs/serde");
    }

    #[test]
    fn persists_and_reloads() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("sub").join("history.jsonl");
        {
            let mut h = History::open(&path).unwrap();
            h.record_visit("https://a.com/", "", 100).unwrap();
            h.set_title("https://a.com/", "A").unwrap();
            h.record_visit("https://b.com/", "B", 200).unwrap();
        }
        let h = History::open(&path).unwrap();
        let r = h.recent(10);
        assert_eq!(r.len(), 2);
        assert_eq!(r[1].title, "A");
        assert_eq!(r[0].url, "https://b.com/");
    }

    #[test]
    fn survives_corrupt_lines() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("history.jsonl");
        fs::write(
            &path,
            "{\"t\":\"v\",\"url\":\"https://a.com/\",\"title\":\"A\",\"ts\":1}\n\
             garbage line\n\
             {\"t\":\"v\",\"url\":\"https://b.com/\",\"title\":\"B\",\"ts\":2}\n\
             {\"t\":\"v\",\"url\":\"https://c.com/\",\"ti",
        )
        .unwrap();
        let h = History::open(&path).unwrap();
        assert_eq!(h.len(), 2);
    }

    #[test]
    fn clear_empties_memory_and_disk() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("history.jsonl");
        let mut h = History::open(&path).unwrap();
        h.record_visit("https://a.com/", "A", 1).unwrap();
        h.clear().unwrap();
        assert!(h.is_empty());
        assert_eq!(fs::read(&path).unwrap().len(), 0);
        assert!(History::open(&path).unwrap().is_empty());
    }

    #[test]
    fn clear_record_in_file_is_honoured() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("history.jsonl");
        fs::write(
            &path,
            "{\"t\":\"v\",\"url\":\"https://a.com/\",\"title\":\"A\",\"ts\":1}\n{\"t\":\"c\"}\n{\"t\":\"v\",\"url\":\"https://b.com/\",\"title\":\"B\",\"ts\":2}\n",
        )
        .unwrap();
        let h = History::open(&path).unwrap();
        assert_eq!(h.len(), 1);
        assert_eq!(h.recent(1)[0].url, "https://b.com/");
    }

    #[test]
    fn caps_entries_and_compacts_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("history.jsonl");
        let mut h = History::open(&path).unwrap();
        let total = MAX_ENTRIES * 2 + 50;
        for i in 0..total {
            h.record_visit(&format!("https://site{i}.com/"), "", (i as u64) * 10).unwrap();
        }
        assert_eq!(h.len(), MAX_ENTRIES);
        assert_eq!(h.recent(1)[0].url, format!("https://site{}.com/", total - 1));

        let lines = fs::read_to_string(&path).unwrap().lines().count();
        assert!(lines <= MAX_ENTRIES * 2 + 1, "file was not compacted: {lines} lines");

        let reloaded = History::open(&path).unwrap();
        assert_eq!(reloaded.len(), MAX_ENTRIES);
        assert_eq!(reloaded.recent(1)[0].url, format!("https://site{}.com/", total - 1));
    }
}
