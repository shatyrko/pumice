//! Operation history: one JSON line per cleanup, kept in the data folder.

use crate::util;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Entry {
    pub ts: u64,
    pub kind: String,
    pub title: String,
    pub freed: u64,
    pub ok: usize,
    pub failed: usize,
    pub dry_run: bool,
    pub details: Vec<String>,
}

pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| crate::safety::home().join(".local/share"));
    base.join("pumice")
}

fn file() -> PathBuf {
    data_dir().join("history.jsonl")
}

pub fn record(kind: &str, title: &str, freed: u64, ok: usize, failed: usize, dry_run: bool, details: Vec<String>) {
    let entry = Entry { ts: util::now_secs(), kind: kind.into(), title: title.into(), freed, ok, failed, dry_run, details };
    let _ = std::fs::create_dir_all(data_dir());
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(file()) {
        if let Ok(line) = serde_json::to_string(&entry) {
            let _ = writeln!(f, "{line}");
        }
    }
}

pub fn list(limit: usize) -> Vec<Entry> {
    let text = std::fs::read_to_string(file()).unwrap_or_default();
    let mut v: Vec<Entry> = text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    v.reverse();
    v.truncate(limit);
    v
}

pub fn clear() {
    let _ = std::fs::remove_file(file());
}
