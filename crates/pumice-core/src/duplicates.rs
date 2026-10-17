//! Exact duplicate finder: group by size, then by a hash of the first 64 KiB,
//! then by the hash of the whole file. Hard links to one inode are not
//! duplicates (they take no extra space).

use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashMap;
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[derive(Default)]
pub struct Progress {
    pub seen: AtomicU64,
    pub hashed: AtomicU64,
    pub cancel: AtomicBool,
}

#[derive(Serialize, Clone, Debug)]
pub struct DupFile {
    pub path: String,
    pub mtime: u64,
}

#[derive(Serialize, Clone, Debug)]
pub struct DupGroup {
    pub hash: String,
    pub size: u64,
    pub files: Vec<DupFile>,
}

fn hash_file(path: &Path, limit: Option<usize>) -> Option<blake3::Hash> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut left = limit.unwrap_or(usize::MAX);
    while left > 0 {
        let want = buf.len().min(left);
        let n = f.read(&mut buf[..want]).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        left -= n;
    }
    Some(hasher.finalize())
}

pub fn hash_full(path: &Path) -> Option<String> {
    hash_file(path, None).map(|h| h.to_hex().to_string())
}

fn walk(dir: &Path, min: u64, skip_hidden: bool, out: &mut Vec<(PathBuf, u64, u64, u64, u64)>, progress: &Progress) {
    if progress.cancel.load(Ordering::Relaxed) {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name();
        if skip_hidden && name.to_string_lossy().starts_with('.') {
            continue;
        }
        let Ok(m) = std::fs::symlink_metadata(e.path()) else { continue };
        if m.is_dir() {
            walk(&e.path(), min, skip_hidden, out, progress);
        } else if m.is_file() && m.len() >= min {
            progress.seen.fetch_add(1, Ordering::Relaxed);
            out.push((e.path(), m.len(), m.mtime().max(0) as u64, m.dev(), m.ino()));
        }
    }
}

pub fn find(roots: &[PathBuf], min_size: u64, skip_hidden: bool, progress: &Progress) -> Vec<DupGroup> {
    let mut files = Vec::new();
    for r in roots {
        walk(r, min_size.max(1), skip_hidden, &mut files, progress);
    }
    // Remove hard links of one inode and files that appear under two roots.
    files.sort_by(|a, b| (a.3, a.4).cmp(&(b.3, b.4)));
    files.dedup_by(|a, b| (a.3, a.4) == (b.3, b.4));

    let mut by_size: HashMap<u64, Vec<(PathBuf, u64)>> = HashMap::new();
    for (p, size, mtime, _, _) in files {
        by_size.entry(size).or_default().push((p, mtime));
    }
    let candidates: Vec<(u64, Vec<(PathBuf, u64)>)> = by_size.into_iter().filter(|(_, v)| v.len() > 1).collect();

    let mut groups: Vec<DupGroup> = candidates
        .into_par_iter()
        .flat_map(|(size, list)| {
            if progress.cancel.load(Ordering::Relaxed) {
                return Vec::new();
            }
            let mut by_head: HashMap<[u8; 32], Vec<(PathBuf, u64)>> = HashMap::new();
            for (p, mtime) in list {
                if let Some(h) = hash_file(&p, Some(64 * 1024)) {
                    by_head.entry(*h.as_bytes()).or_default().push((p, mtime));
                }
            }
            let mut found = Vec::new();
            for (_, same_head) in by_head.into_iter().filter(|(_, v)| v.len() > 1) {
                let mut by_full: HashMap<[u8; 32], Vec<(PathBuf, u64)>> = HashMap::new();
                for (p, mtime) in same_head {
                    progress.hashed.fetch_add(1, Ordering::Relaxed);
                    if let Some(h) = hash_file(&p, None) {
                        by_full.entry(*h.as_bytes()).or_default().push((p, mtime));
                    }
                }
                for (hash, same) in by_full.into_iter().filter(|(_, v)| v.len() > 1) {
                    let mut files: Vec<DupFile> = same
                        .into_iter()
                        .map(|(p, mtime)| DupFile { path: p.to_string_lossy().into_owned(), mtime })
                        .collect();
                    files.sort_by(|a, b| a.mtime.cmp(&b.mtime).then(a.path.cmp(&b.path)));
                    found.push(DupGroup { hash: blake3::Hash::from_bytes(hash).to_hex().to_string(), size, files });
                }
            }
            found
        })
        .collect();
    groups.sort_by(|a, b| (b.size * b.files.len() as u64).cmp(&(a.size * a.files.len() as u64)));
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_exact_copies_only() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a"), vec![7u8; 5000]).unwrap();
        std::fs::write(d.path().join("b"), vec![7u8; 5000]).unwrap();
        let mut almost = vec![7u8; 5000];
        almost[4999] = 8;
        std::fs::write(d.path().join("c"), almost).unwrap();
        std::fs::hard_link(d.path().join("a"), d.path().join("a-link")).unwrap();
        let g = find(&[d.path().to_path_buf()], 1, false, &Progress::default());
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].files.len(), 2);
    }
}
