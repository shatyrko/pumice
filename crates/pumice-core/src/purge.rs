//! Finds build artifacts of projects (node_modules, target, .venv, ...).
//! An artifact only counts when the project's own marker file sits next to
//! it, so a folder that merely has the same name is left alone.

use crate::scan;
use crate::util;
use rayon::prelude::*;
use serde::Serialize;
use std::path::{Path, PathBuf};

struct Kind {
    dir: &'static str,
    label: &'static str,
    /// Any of these names next to the folder (or inside it, with `inside`).
    markers: &'static [&'static str],
    inside: bool,
}

const KINDS: &[Kind] = &[
    Kind { dir: "node_modules", label: "Node.js", markers: &["package.json"], inside: false },
    Kind { dir: "target", label: "Rust", markers: &["Cargo.toml"], inside: false },
    Kind { dir: ".venv", label: "Python venv", markers: &["pyvenv.cfg"], inside: true },
    Kind { dir: "venv", label: "Python venv", markers: &["pyvenv.cfg"], inside: true },
    Kind { dir: "__pycache__", label: "Python bytecode", markers: &[], inside: false },
    Kind { dir: ".tox", label: "Python tox", markers: &["tox.ini", "pyproject.toml", "setup.cfg"], inside: false },
    Kind { dir: ".mypy_cache", label: "Python mypy", markers: &[], inside: false },
    Kind { dir: ".pytest_cache", label: "Python pytest", markers: &[], inside: false },
    Kind { dir: ".next", label: "Next.js", markers: &["package.json"], inside: false },
    Kind { dir: ".nuxt", label: "Nuxt", markers: &["package.json"], inside: false },
    Kind { dir: ".svelte-kit", label: "SvelteKit", markers: &["package.json"], inside: false },
    Kind { dir: ".parcel-cache", label: "Parcel", markers: &["package.json"], inside: false },
    Kind { dir: ".turbo", label: "Turborepo", markers: &["package.json", "turbo.json"], inside: false },
    Kind { dir: ".gradle", label: "Gradle", markers: &["build.gradle", "build.gradle.kts", "settings.gradle", "settings.gradle.kts"], inside: false },
    Kind { dir: "build", label: "Gradle/CMake build", markers: &["build.gradle", "build.gradle.kts", "CMakeLists.txt"], inside: false },
    Kind { dir: "dist", label: "JS build output", markers: &["package.json"], inside: false },
    Kind { dir: ".dart_tool", label: "Dart", markers: &["pubspec.yaml"], inside: false },
    Kind { dir: ".zig-cache", label: "Zig", markers: &["build.zig"], inside: false },
    Kind { dir: "zig-cache", label: "Zig", markers: &["build.zig"], inside: false },
    Kind { dir: "obj", label: ".NET", markers: &[], inside: false },
];

#[derive(Serialize, Clone, Debug)]
pub struct Artifact {
    pub path: String,
    pub project: String,
    pub kind: String,
    pub size: u64,
    /// Newest change in the project outside the artifact, seconds since 1970.
    pub project_mtime: u64,
    pub age_days: u64,
}

fn has_marker(parent: &Path, dir: &Path, kind: &Kind) -> bool {
    if kind.dir == "__pycache__" || kind.dir == ".mypy_cache" || kind.dir == ".pytest_cache" {
        return true;
    }
    if kind.dir == "obj" {
        return std::fs::read_dir(parent)
            .map(|rd| rd.flatten().any(|e| e.path().extension().map(|x| x == "csproj" || x == "fsproj").unwrap_or(false)))
            .unwrap_or(false);
    }
    let base = if kind.inside { dir } else { parent };
    kind.markers.iter().any(|m| base.join(m).exists())
}

fn find_in(dir: &Path, depth: u32, out: &mut Vec<(PathBuf, &'static Kind)>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let Ok(m) = std::fs::symlink_metadata(e.path()) else { continue };
        if !m.is_dir() {
            continue;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        if name == ".git" {
            continue;
        }
        if let Some(kind) = KINDS.iter().find(|k| k.dir == name) {
            if has_marker(dir, &e.path(), kind) {
                out.push((e.path(), kind));
                continue;
            }
        }
        if depth > 0 {
            find_in(&e.path(), depth - 1, out);
        }
    }
}

pub fn default_roots() -> Vec<PathBuf> {
    let h = crate::safety::home();
    ["Projects", "projects", "dev", "code", "Code", "src", "work", "workspace", "repos", "git", "Developer"]
        .iter()
        .map(|d| h.join(d))
        .filter(|p| p.is_dir())
        .collect()
}

pub fn find(roots: &[PathBuf], depth: u32) -> Vec<Artifact> {
    let mut hits: Vec<(PathBuf, &'static Kind)> = Vec::new();
    for r in roots {
        find_in(r, depth, &mut hits);
    }
    hits.sort_by(|a, b| a.0.cmp(&b.0));
    hits.dedup_by(|a, b| a.0 == b.0);
    let now = util::now_secs();
    let mut out: Vec<Artifact> = hits
        .par_iter()
        .map(|(path, kind)| {
            let project = path.parent().unwrap_or(path);
            // Newest change in the project's own files (two levels), skipping the artifact.
            let mut newest = 0;
            if let Ok(rd) = std::fs::read_dir(project) {
                for e in rd.flatten() {
                    if e.path() == *path {
                        continue;
                    }
                    let n = e.file_name().to_string_lossy().into_owned();
                    if n == "node_modules" || n == "target" || n == ".venv" {
                        continue;
                    }
                    if let Ok(m) = e.metadata() {
                        use std::os::unix::fs::MetadataExt;
                        newest = newest.max(m.mtime().max(0) as u64);
                        if m.is_dir() && !n.starts_with('.') {
                            newest = newest.max(scan::newest_mtime(&e.path(), 2));
                        }
                    }
                }
            }
            Artifact {
                path: path.to_string_lossy().into_owned(),
                project: project.to_string_lossy().into_owned(),
                kind: kind.label.to_string(),
                size: scan::disk_usage(path),
                project_mtime: newest,
                age_days: now.saturating_sub(newest) / 86_400,
            }
        })
        .filter(|a| a.size > 0)
        .collect();
    out.sort_by(|a, b| b.size.cmp(&a.size));
    out
}

/// True when `path` is an artifact `find` would report (used to re-check a
/// request before anything is removed).
pub fn is_artifact(path: &Path) -> bool {
    let (Some(name), Some(parent)) = (path.file_name().and_then(|n| n.to_str()), path.parent()) else { return false };
    KINDS.iter().find(|k| k.dir == name).map(|k| has_marker(parent, path, k)).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_the_marker() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("a/node_modules/x")).unwrap();
        std::fs::write(d.path().join("a/node_modules/x/f"), vec![0u8; 8192]).unwrap();
        std::fs::create_dir_all(d.path().join("b/node_modules")).unwrap();
        std::fs::write(d.path().join("a/package.json"), "{}").unwrap();
        let hits = find(&[d.path().to_path_buf()], 4);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].path.ends_with("a/node_modules"));
        assert!(is_artifact(&d.path().join("a/node_modules")));
        assert!(!is_artifact(&d.path().join("b/node_modules")));
    }
}
