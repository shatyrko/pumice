//! Finds what the rules match and removes it again, always through the
//! safety policy. The frontend never sends a free path to delete: it sends a
//! rule id and paths, and those must still be matches of that rule.

use crate::rules::{Action, Risk, Rule};
use crate::safety::{self, Policy};
use crate::scan;
use crate::util;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Delete,
    Trash,
}

#[derive(Serialize, Clone, Debug)]
pub struct Item {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub mtime: u64,
}

#[derive(Serialize, Clone, Debug)]
pub struct RuleResult {
    #[serde(flatten)]
    pub rule: Rule,
    pub items: Vec<Item>,
    pub total: u64,
    pub blocked: Option<String>,
    pub selected: bool,
}

#[derive(Deserialize, Debug)]
pub struct ApplyRequest {
    pub rule_id: String,
    pub paths: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Outcome {
    pub path: String,
    pub freed: u64,
    pub error: Option<String>,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Report {
    pub dry_run: bool,
    pub freed: u64,
    pub removed: usize,
    pub failed: usize,
    pub outcomes: Vec<Outcome>,
}

fn mtime_of(p: &Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    std::fs::symlink_metadata(p).map(|m| m.mtime().max(0) as u64).unwrap_or(0)
}

/// Everything on disk the rule matches right now (policy-checked, nested
/// matches folded into the outer one, young files left out).
pub fn matches(rule: &Rule) -> Vec<PathBuf> {
    let opts = glob::MatchOptions { case_sensitive: true, require_literal_separator: true, require_literal_leading_dot: false };
    let excludes: Vec<glob::Pattern> = rule.exclude.iter().filter_map(|e| glob::Pattern::new(&safety::expand(e)).ok()).collect();
    let cutoff = util::now_secs().saturating_sub(rule.min_age_days * 86_400);
    let protected: Vec<PathBuf> = crate::settings::load().protected.iter().map(|p| PathBuf::from(safety::expand(p))).collect();
    let mut found: Vec<PathBuf> = Vec::new();
    let mut seen = HashSet::new();
    for pattern in &rule.paths {
        let Ok(paths) = glob::glob_with(&safety::expand(pattern), opts) else { continue };
        for p in paths.flatten() {
            if !seen.insert(p.clone()) {
                continue;
            }
            if excludes.iter().any(|e| e.matches_path_with(&p, opts)) {
                continue;
            }
            if std::fs::symlink_metadata(&p).is_err() {
                continue;
            }
            if rule.min_age_days > 0 && mtime_of(&p) > cutoff {
                continue;
            }
            if protected.iter().any(|q| p.starts_with(q)) || safety::check(&p, Policy::Rule).is_err() {
                continue;
            }
            found.push(p);
        }
    }
    found.sort();
    let mut out: Vec<PathBuf> = Vec::new();
    for p in found {
        if out.last().map(|prev| p.starts_with(prev)).unwrap_or(false) {
            continue;
        }
        out.push(p);
    }
    out
}

fn blocked(rule: &Rule, running: &[String]) -> Option<String> {
    for name in &rule.blocked_by {
        if running.iter().any(|r| r == &name.to_lowercase()) {
            return Some(format!("{name} is running. Close it first."));
        }
    }
    None
}

pub fn scan_rules(rules: &[Rule], only: Option<&[String]>) -> Vec<RuleResult> {
    let running = util::running_process_names();
    let mut results: Vec<RuleResult> = rules
        .par_iter()
        .filter(|r| only.map(|ids| ids.contains(&r.id)).unwrap_or(true))
        .filter_map(|rule| {
            let paths = matches(rule);
            if paths.is_empty() {
                return None;
            }
            let mut items: Vec<Item> = paths
                .par_iter()
                .map(|p| {
                    let meta = std::fs::symlink_metadata(p).ok();
                    Item {
                        path: p.to_string_lossy().into_owned(),
                        name: p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                        size: scan::disk_usage(p),
                        is_dir: meta.as_ref().map(|m| m.is_dir()).unwrap_or(false),
                        mtime: mtime_of(p),
                    }
                })
                .collect();
            items.sort_by(|a, b| b.size.cmp(&a.size));
            let total = items.iter().map(|i| i.size).sum();
            if total == 0 {
                return None;
            }
            Some(RuleResult {
                blocked: blocked(rule, &running),
                selected: rule.risk == Risk::Low,
                rule: rule.clone(),
                items,
                total,
            })
        })
        .collect();
    results.sort_by(|a, b| b.total.cmp(&a.total));
    results
}

/// Go's module cache and some tools make folders read-only; make them
/// writable below `path` so removal can proceed.
fn make_writable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() {
            return;
        }
        let mut perm = meta.permissions();
        perm.set_mode(perm.mode() | 0o700);
        let _ = std::fs::set_permissions(path, perm);
        if meta.is_dir() {
            if let Ok(rd) = std::fs::read_dir(path) {
                for e in rd.flatten() {
                    make_writable(&e.path());
                }
            }
        }
    }
}

/// Removes one path. A symlink is removed itself, never followed.
pub fn remove_path(path: &Path, mode: Mode, policy: Policy) -> Result<u64, String> {
    let p = safety::check(path, policy)?;
    let meta = std::fs::symlink_metadata(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    let size = scan::disk_usage(&p);
    match mode {
        Mode::Trash => {
            trash::delete(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        }
        Mode::Delete => {
            if meta.is_dir() {
                if let Err(first) = std::fs::remove_dir_all(&p) {
                    make_writable(&p);
                    std::fs::remove_dir_all(&p).map_err(|e| format!("{}: {e} ({first})", p.display()))?;
                }
            } else {
                std::fs::remove_file(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            }
        }
    }
    Ok(size)
}

pub fn apply(rules: &[Rule], requests: &[ApplyRequest], override_mode: Option<Mode>, dry_run: bool) -> Report {
    let running = util::running_process_names();
    let mut report = Report { dry_run, ..Default::default() };
    for req in requests {
        let Some(rule) = rules.iter().find(|r| r.id == req.rule_id) else {
            for p in &req.paths {
                report.outcomes.push(Outcome { path: p.clone(), freed: 0, error: Some("unknown rule".into()) });
            }
            continue;
        };
        let block = blocked(rule, &running);
        let current: HashSet<String> = matches(rule).iter().map(|p| p.to_string_lossy().into_owned()).collect();
        let mode = override_mode.unwrap_or(match rule.action {
            Action::Delete => Mode::Delete,
            Action::Trash => Mode::Trash,
        });
        for path in &req.paths {
            let outcome = if let Some(b) = &block {
                Outcome { path: path.clone(), freed: 0, error: Some(b.clone()) }
            } else if !current.contains(path) {
                Outcome { path: path.clone(), freed: 0, error: Some("not a match of this rule any more".into()) }
            } else if dry_run {
                Outcome { path: path.clone(), freed: scan::disk_usage(Path::new(path)), error: None }
            } else {
                match remove_path(Path::new(path), mode, Policy::Rule) {
                    Ok(freed) => Outcome { path: path.clone(), freed, error: None },
                    Err(e) => Outcome { path: path.clone(), freed: 0, error: Some(e) },
                }
            };
            if outcome.error.is_some() {
                report.failed += 1;
            } else {
                report.removed += 1;
                report.freed += outcome.freed;
            }
            report.outcomes.push(outcome);
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn go_style_readonly_tree_is_removed() {
        use std::os::unix::fs::PermissionsExt;
        let base = std::env::temp_dir().join(format!("pumice-ro-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("a/b")).unwrap();
        std::fs::write(base.join("a/b/f"), b"x").unwrap();
        std::fs::set_permissions(base.join("a/b"), std::fs::Permissions::from_mode(0o555)).unwrap();
        std::fs::set_permissions(base.join("a"), std::fs::Permissions::from_mode(0o555)).unwrap();
        remove_path(&base.join("a"), Mode::Delete, Policy::User).unwrap();
        assert!(!base.join("a").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn refuses_unsafe_paths() {
        assert!(remove_path(&safety::home(), Mode::Delete, Policy::User).is_err());
        assert!(remove_path(Path::new("/etc/hostname"), Mode::Delete, Policy::User).is_err());
    }
}
