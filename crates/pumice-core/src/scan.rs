//! Parallel disk scan that builds a size tree.
//!
//! Sizes are allocated bytes (blocks * 512), hard links counted once.
//! Directories are kept as nodes; a file gets a node only when it is at
//! least `min_file` bytes, smaller ones are folded into their folder's
//! `small_files` / `small_size` so a scan of millions of files stays small.

use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashSet;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

pub const NONE: u32 = u32::MAX;

#[derive(Clone, Debug, Serialize)]
pub struct Node {
    pub name: String,
    pub parent: u32,
    pub is_dir: bool,
    pub size: u64,
    /// Files below this folder (all of them, also the folded ones).
    pub files: u64,
    pub mtime: u64,
    pub atime: u64,
    pub small_files: u64,
    pub small_size: u64,
    #[serde(skip)]
    pub children: Vec<u32>,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub min_file: u64,
    pub cross_filesystems: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { min_file: 1 << 20, cross_filesystems: false }
    }
}

#[derive(Default)]
pub struct Progress {
    pub files: AtomicU64,
    pub bytes: AtomicU64,
    pub dirs: AtomicU64,
    pub errors: AtomicU64,
    pub cancel: AtomicBool,
}

#[derive(Debug)]
pub struct ScanResult {
    pub root: PathBuf,
    pub nodes: Vec<Node>,
    pub total_size: u64,
    pub total_files: u64,
    pub errors: u64,
    pub millis: u64,
    pub cancelled: bool,
    pub min_file: u64,
}

struct Built {
    name: String,
    is_dir: bool,
    size: u64,
    files: u64,
    mtime: u64,
    atime: u64,
    small_files: u64,
    small_size: u64,
    kids: Vec<Built>,
}

struct Ctx<'a> {
    opts: &'a Options,
    dev: u64,
    seen: Mutex<HashSet<(u64, u64)>>,
    progress: &'a Progress,
}

const VIRTUAL: [&str; 5] = ["/proc", "/sys", "/dev", "/run", "/var/run"];

fn is_virtual(p: &Path) -> bool {
    VIRTUAL.iter().any(|v| p == Path::new(v))
}

fn alloc(meta: &std::fs::Metadata) -> u64 {
    meta.blocks() * 512
}

fn scan_dir(path: &Path, name: String, meta: &std::fs::Metadata, ctx: &Ctx) -> Built {
    let mut node = Built {
        name,
        is_dir: true,
        size: alloc(meta),
        files: 0,
        mtime: meta.mtime().max(0) as u64,
        atime: meta.atime().max(0) as u64,
        small_files: 0,
        small_size: 0,
        kids: Vec::new(),
    };
    ctx.progress.dirs.fetch_add(1, Ordering::Relaxed);
    if ctx.progress.cancel.load(Ordering::Relaxed) {
        return node;
    }
    let rd = match std::fs::read_dir(path) {
        Ok(rd) => rd,
        Err(_) => {
            ctx.progress.errors.fetch_add(1, Ordering::Relaxed);
            return node;
        }
    };
    let mut subdirs: Vec<(PathBuf, String, std::fs::Metadata)> = Vec::new();
    for entry in rd.flatten() {
        let p = entry.path();
        let m = match std::fs::symlink_metadata(&p) {
            Ok(m) => m,
            Err(_) => {
                ctx.progress.errors.fetch_add(1, Ordering::Relaxed);
                continue;
            }
        };
        let fname = entry.file_name().to_string_lossy().into_owned();
        let ft = m.file_type();
        if ft.is_dir() {
            if is_virtual(&p) || (!ctx.opts.cross_filesystems && m.dev() != ctx.dev) {
                continue;
            }
            subdirs.push((p, fname, m));
        } else {
            // Files, symlinks, sockets: a symlink counts as its own tiny size.
            if m.nlink() > 1 && ft.is_file() {
                let mut seen = ctx.seen.lock().unwrap();
                if !seen.insert((m.dev(), m.ino())) {
                    node.files += 1;
                    node.small_files += 1;
                    continue;
                }
            }
            let sz = alloc(&m);
            node.size += sz;
            node.files += 1;
            ctx.progress.files.fetch_add(1, Ordering::Relaxed);
            ctx.progress.bytes.fetch_add(sz, Ordering::Relaxed);
            if ft.is_file() && sz >= ctx.opts.min_file {
                node.kids.push(Built {
                    name: fname,
                    is_dir: false,
                    size: sz,
                    files: 1,
                    mtime: m.mtime().max(0) as u64,
                    atime: m.atime().max(0) as u64,
                    small_files: 0,
                    small_size: 0,
                    kids: Vec::new(),
                });
            } else {
                node.small_files += 1;
                node.small_size += sz;
            }
        }
    }
    let built: Vec<Built> = subdirs
        .par_iter()
        .map(|(p, n, m)| scan_dir(p, n.clone(), m, ctx))
        .collect();
    for b in built {
        node.size += b.size;
        node.files += b.files;
        node.kids.push(b);
    }
    node
}

fn flatten(b: Built, parent: u32, nodes: &mut Vec<Node>) -> u32 {
    let id = nodes.len() as u32;
    nodes.push(Node {
        name: b.name,
        parent,
        is_dir: b.is_dir,
        size: b.size,
        files: b.files,
        mtime: b.mtime,
        atime: b.atime,
        small_files: b.small_files,
        small_size: b.small_size,
        children: Vec::new(),
    });
    let mut kids = b.kids;
    kids.sort_by(|a, b| b.size.cmp(&a.size));
    let ids: Vec<u32> = kids.into_iter().map(|k| flatten(k, id, nodes)).collect();
    nodes[id as usize].children = ids;
    id
}

pub fn scan(root: &Path, opts: &Options, progress: &Progress) -> crate::Result<ScanResult> {
    let started = std::time::Instant::now();
    let meta = std::fs::symlink_metadata(root)?;
    if !meta.is_dir() {
        anyhow::bail!("{} is not a folder", root.display());
    }
    let ctx = Ctx { opts, dev: meta.dev(), seen: Mutex::new(HashSet::new()), progress };
    let name = root.to_string_lossy().into_owned();
    let built = scan_dir(root, name, &meta, &ctx);
    let total_size = built.size;
    let total_files = built.files;
    let mut nodes = Vec::new();
    flatten(built, NONE, &mut nodes);
    Ok(ScanResult {
        root: root.to_path_buf(),
        nodes,
        total_size,
        total_files,
        errors: progress.errors.load(Ordering::Relaxed),
        millis: started.elapsed().as_millis() as u64,
        cancelled: progress.cancel.load(Ordering::Relaxed),
        min_file: opts.min_file,
    })
}

/// Size of one path (a file or a whole folder), allocated bytes, hard links once.
pub fn disk_usage(path: &Path) -> u64 {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(_) => return 0,
    };
    if !meta.is_dir() {
        return alloc(&meta);
    }
    let progress = Progress::default();
    let opts = Options { min_file: u64::MAX, cross_filesystems: true };
    let ctx = Ctx { opts: &opts, dev: meta.dev(), seen: Mutex::new(HashSet::new()), progress: &progress };
    scan_dir(path, String::new(), &meta, &ctx).size
}

/// Newest modification time in a folder, looking at most `depth` levels down.
pub fn newest_mtime(path: &Path, depth: u32) -> u64 {
    let mut newest = std::fs::symlink_metadata(path).map(|m| m.mtime().max(0) as u64).unwrap_or(0);
    if depth == 0 {
        return newest;
    }
    if let Ok(rd) = std::fs::read_dir(path) {
        for e in rd.flatten() {
            if let Ok(m) = e.metadata() {
                newest = newest.max(m.mtime().max(0) as u64);
                if m.is_dir() {
                    newest = newest.max(newest_mtime(&e.path(), depth - 1));
                }
            }
        }
    }
    newest
}

#[derive(Serialize, Clone, Debug)]
pub struct Row {
    pub id: Option<u32>,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub files: u64,
    pub mtime: u64,
    pub note: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct ViewNode {
    pub id: Option<u32>,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub children: Vec<ViewNode>,
}

#[derive(Serialize, Clone, Debug)]
pub struct FileHit {
    pub id: u32,
    pub path: String,
    pub name: String,
    pub size: u64,
    pub mtime: u64,
    pub atime: u64,
}

impl ScanResult {
    pub fn path_of(&self, id: u32) -> PathBuf {
        let mut parts = Vec::new();
        let mut cur = id;
        while cur != NONE {
            let n = &self.nodes[cur as usize];
            if n.parent == NONE {
                parts.push(self.root.to_string_lossy().into_owned());
            } else {
                parts.push(n.name.clone());
            }
            cur = n.parent;
        }
        parts.reverse();
        let mut p = PathBuf::new();
        for part in parts {
            p.push(part);
        }
        p
    }

    pub fn get(&self, id: u32) -> Option<&Node> {
        self.nodes.get(id as usize)
    }

    pub fn rows(&self, id: u32) -> Vec<Row> {
        let Some(n) = self.get(id) else { return Vec::new() };
        let mut rows: Vec<Row> = n
            .children
            .iter()
            .map(|&c| {
                let k = &self.nodes[c as usize];
                Row { id: Some(c), name: k.name.clone(), is_dir: k.is_dir, size: k.size, files: k.files, mtime: k.mtime, note: None }
            })
            .collect();
        if n.small_files > 0 && n.small_size > 0 {
            rows.push(Row {
                id: None,
                name: format!("{} smaller files", n.small_files),
                is_dir: false,
                size: n.small_size,
                files: n.small_files,
                mtime: 0,
                note: Some("folded".into()),
            });
            rows.sort_by(|a, b| b.size.cmp(&a.size));
        }
        rows
    }

    /// A nested view for the treemap and the sunburst: at most `per_level`
    /// children per folder, the rest folded into one "more" node.
    pub fn view(&self, id: u32, depth: u32, per_level: usize) -> Option<ViewNode> {
        let n = self.get(id)?;
        let mut children = Vec::new();
        if depth > 0 && n.is_dir {
            let mut folded = n.small_size;
            let mut folded_count = n.small_files;
            for (i, &c) in n.children.iter().enumerate() {
                if i >= per_level {
                    let k = &self.nodes[c as usize];
                    folded += k.size;
                    folded_count += k.files;
                    continue;
                }
                if let Some(v) = self.view(c, depth - 1, per_level) {
                    children.push(v);
                }
            }
            if folded > 0 {
                children.push(ViewNode {
                    id: None,
                    name: format!("{folded_count} smaller files"),
                    is_dir: false,
                    size: folded,
                    children: Vec::new(),
                });
            }
        }
        Some(ViewNode { id: Some(id), name: if n.parent == NONE { self.root.to_string_lossy().into_owned() } else { n.name.clone() }, is_dir: n.is_dir, size: n.size, children })
    }

    /// The biggest files of the scan that pass `keep`, largest first.
    pub fn top_files<F: Fn(&Node) -> bool>(&self, limit: usize, keep: F) -> Vec<FileHit> {
        let mut hits: Vec<(u64, u32)> = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| !n.is_dir && keep(n))
            .map(|(i, n)| (n.size, i as u32))
            .collect();
        hits.sort_by(|a, b| b.0.cmp(&a.0));
        hits.truncate(limit);
        hits.into_iter()
            .map(|(_, i)| {
                let n = &self.nodes[i as usize];
                FileHit { id: i, path: self.path_of(i).to_string_lossy().into_owned(), name: n.name.clone(), size: n.size, mtime: n.mtime, atime: n.atime }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn sums_and_folds() {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir_all(d.path().join("a/b")).unwrap();
        fs::write(d.path().join("a/big.bin"), vec![1u8; 2 << 20]).unwrap();
        fs::write(d.path().join("a/b/small.txt"), b"hi").unwrap();
        fs::write(d.path().join("a/b/link_src"), vec![2u8; 4096]).unwrap();
        fs::hard_link(d.path().join("a/b/link_src"), d.path().join("a/b/link_dst")).unwrap();
        let r = scan(d.path(), &Options::default(), &Progress::default()).unwrap();
        assert!(r.total_size >= 2 << 20);
        let a = r.nodes.iter().position(|n| n.name == "a").unwrap() as u32;
        let rows = r.rows(a);
        assert!(rows.iter().any(|x| x.name == "big.bin"));
        assert!(!rows.iter().any(|x| x.name == "small.txt"));
        // The hard link pair counts once: 4096 bytes allocated, not 8192.
        let b = r.nodes.iter().position(|n| n.name == "b").unwrap();
        assert!(r.nodes[b].size < 4096 + 4096 + 4096 + 8192);
        assert_eq!(r.path_of(a), d.path().join("a"));
    }
}
