//! The one place that decides whether Pumice may remove a path.
//!
//! Two policies:
//! - `Rule`: a path a cleanup rule found. Never touches the person's own
//!   folders (Documents, Pictures, ...).
//! - `User`: a path the person picked by hand (a large file, a duplicate, a
//!   project artifact). Their folders are allowed, the critical ones are not.
//!
//! Both refuse the home folder itself, the XDG base folders themselves,
//! secrets (ssh, gpg, keyrings), anything outside the home folder and the
//! temporary folders, and any path whose parent folders resolve outside them.

use std::path::{Component, Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Policy {
    Rule,
    User,
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Expands a leading `~` and nothing else.
pub fn expand(pattern: &str) -> String {
    if pattern == "~" {
        return home().to_string_lossy().into_owned();
    }
    if let Some(rest) = pattern.strip_prefix("~/") {
        return format!("{}/{}", home().to_string_lossy().trim_end_matches('/'), rest);
    }
    pattern.to_string()
}

/// Removes `.` parts and refuses `..`.
pub fn normalize(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }
    let mut out = PathBuf::from("/");
    for c in path.components() {
        match c {
            Component::RootDir | Component::CurDir => {}
            Component::ParentDir | Component::Prefix(_) => return None,
            Component::Normal(n) => out.push(n),
        }
    }
    Some(out)
}

fn allowed_roots() -> Vec<PathBuf> {
    let mut roots = vec![home(), PathBuf::from("/tmp"), PathBuf::from("/var/tmp")];
    if let Ok(h) = home().canonicalize() {
        roots.push(h);
    }
    roots
}

fn forbidden_exact() -> Vec<PathBuf> {
    let h = home();
    let mut v = vec![
        h.clone(),
        h.join(".config"),
        h.join(".cache"),
        h.join(".local"),
        h.join(".local/share"),
        h.join(".local/state"),
        h.join(".local/bin"),
        h.join(".var"),
        h.join(".var/app"),
        PathBuf::from("/tmp"),
        PathBuf::from("/var/tmp"),
    ];
    if let Ok(c) = h.canonicalize() {
        v.push(c);
    }
    v
}

fn secret_trees() -> Vec<PathBuf> {
    let h = home();
    vec![
        h.join(".ssh"),
        h.join(".gnupg"),
        h.join(".local/share/keyrings"),
        h.join(".password-store"),
        h.join(".pki"),
    ]
}

fn user_content_trees() -> Vec<PathBuf> {
    let h = home();
    ["Documents", "Pictures", "Videos", "Music", "Desktop", "Downloads", "Templates", "Public"]
        .iter()
        .map(|d| h.join(d))
        .collect()
}

/// Checks a path against the policy. `Ok` carries the normalised path.
pub fn check(path: &Path, policy: Policy) -> Result<PathBuf, String> {
    let p = normalize(path).ok_or_else(|| format!("{}: not an absolute path without `..`", path.display()))?;

    if !allowed_roots().iter().any(|r| p.starts_with(r)) {
        return Err(format!("{}: outside the home folder and the temporary folders", p.display()));
    }
    if forbidden_exact().iter().any(|f| &p == f) {
        return Err(format!("{}: a base folder, never removed as a whole", p.display()));
    }
    if secret_trees().iter().any(|s| p.starts_with(s)) {
        return Err(format!("{}: holds secrets (keys, keyrings, passwords)", p.display()));
    }
    if policy == Policy::Rule && user_content_trees().iter().any(|s| p.starts_with(s)) {
        return Err(format!("{}: inside one of your own folders", p.display()));
    }
    // Folders above the path must not lead outside through a symlink.
    if let Some(parent) = p.parent() {
        match parent.canonicalize() {
            Ok(real) => {
                if !allowed_roots().iter().any(|r| real.starts_with(r)) {
                    return Err(format!("{}: its folder resolves to {}, outside the allowed places", p.display(), real.display()));
                }
                if let Some(name) = p.file_name() {
                    let real_full = real.join(name);
                    if forbidden_exact().iter().any(|f| &real_full == f) {
                        return Err(format!("{}: a base folder", real_full.display()));
                    }
                    if secret_trees().iter().any(|s| real_full.starts_with(s)) {
                        return Err(format!("{}: holds secrets", real_full.display()));
                    }
                }
            }
            Err(e) => return Err(format!("{}: {}", parent.display(), e)),
        }
    }
    Ok(p)
}

/// Same checks for a glob pattern of a rule: the part before the first
/// wildcard has to be inside the allowed places.
pub fn check_pattern(pattern: &str, policy: Policy) -> Result<(), String> {
    if !pattern.starts_with("~/") {
        return Err(format!("{pattern}: rules must start with ~/"));
    }
    if pattern.contains("..") {
        return Err(format!("{pattern}: `..` is not allowed"));
    }
    let expanded = expand(pattern);
    let literal: String = expanded
        .split('/')
        .take_while(|part| !part.contains(['*', '?', '[']))
        .collect::<Vec<_>>()
        .join("/");
    let literal = PathBuf::from(if literal.is_empty() { "/".into() } else { literal });
    let h = home();
    if !literal.starts_with(&h) {
        return Err(format!("{pattern}: outside the home folder"));
    }
    // A pattern whose literal part is a base folder is fine only when it
    // has something after it (a wildcard); `~/.cache` alone is not.
    let has_wild = expanded.contains(['*', '?', '[']);
    if !has_wild && forbidden_exact().iter().any(|f| f == &literal) {
        return Err(format!("{pattern}: a base folder"));
    }
    if secret_trees().iter().any(|s| literal.starts_with(s)) {
        return Err(format!("{pattern}: holds secrets"));
    }
    if policy == Policy::Rule && user_content_trees().iter().any(|s| literal.starts_with(s)) {
        return Err(format!("{pattern}: inside one of your own folders"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_base_folders_and_secrets() {
        let h = home();
        assert!(check(&h, Policy::User).is_err());
        assert!(check(&h.join(".ssh/id_ed25519"), Policy::User).is_err());
        assert!(check(&h.join(".cache"), Policy::Rule).is_err());
        assert!(check(Path::new("/etc/passwd"), Policy::User).is_err());
        assert!(check(Path::new("/usr/lib/../../etc"), Policy::User).is_err());
        assert!(check(&h.join("Documents/a.txt"), Policy::Rule).is_err());
    }

    #[test]
    fn patterns() {
        assert!(check_pattern("~/.cache/*", Policy::Rule).is_ok());
        assert!(check_pattern("~/.cache", Policy::Rule).is_err());
        assert!(check_pattern("~/.ssh/*", Policy::Rule).is_err());
        assert!(check_pattern("/etc/*", Policy::Rule).is_err());
        assert!(check_pattern("~/Documents/*", Policy::Rule).is_err());
    }
}
