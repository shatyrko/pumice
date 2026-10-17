//! Installed applications from every source on a Linux desktop (pacman, apt,
//! dnf, Flatpak, Snap, AppImage), their leftovers, and uninstalling.

use crate::clean::{self, Mode};
use crate::maintenance;
use crate::safety::{self, Policy};
use crate::scan;
use crate::util;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone, Debug)]
pub struct App {
    pub id: String,
    pub name: String,
    pub source: String,
    /// Package name, Flatpak/Snap id, or the AppImage path.
    pub package: String,
    pub version: String,
    pub size: u64,
    pub icon: Option<String>,
    pub comment: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct Leftover {
    pub path: String,
    pub size: u64,
    pub kind: String,
    /// Settings or data that may be personal; the interface asks twice.
    pub caution: bool,
}

struct Desktop {
    file: PathBuf,
    name: String,
    icon: String,
    comment: String,
    exec: String,
}

fn parse_desktop(path: &Path) -> Option<Desktop> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut in_entry = false;
    let (mut name, mut icon, mut comment, mut exec) = (String::new(), String::new(), String::new(), String::new());
    let (mut hidden, mut kind) = (false, String::new());
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            match k {
                "Name" => name = v.to_string(),
                "Icon" => icon = v.to_string(),
                "Comment" => comment = v.to_string(),
                "Exec" => exec = v.to_string(),
                "Type" => kind = v.to_string(),
                "NoDisplay" | "Hidden" if v == "true" => hidden = true,
                _ => {}
            }
        }
    }
    if hidden || kind != "Application" || name.is_empty() {
        return None;
    }
    Some(Desktop { file: path.to_path_buf(), name, icon, comment, exec })
}

fn desktop_dirs() -> Vec<PathBuf> {
    let h = safety::home();
    vec![
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
        h.join(".local/share/flatpak/exports/share/applications"),
        PathBuf::from("/var/lib/snapd/desktop/applications"),
        h.join(".local/share/applications"),
    ]
}

fn icon_data(icon: &str) -> Option<String> {
    if icon.is_empty() {
        return None;
    }
    let h = safety::home();
    let mut candidates: Vec<PathBuf> = Vec::new();
    if icon.starts_with('/') {
        candidates.push(PathBuf::from(icon));
    } else {
        let roots = [
            PathBuf::from("/usr/share/icons/hicolor"),
            PathBuf::from("/var/lib/flatpak/exports/share/icons/hicolor"),
            h.join(".local/share/flatpak/exports/share/icons/hicolor"),
            h.join(".local/share/icons/hicolor"),
        ];
        for size in ["128x128", "64x64", "96x96", "48x48", "256x256", "scalable"] {
            for r in &roots {
                candidates.push(r.join(size).join("apps").join(format!("{icon}.png")));
                candidates.push(r.join(size).join("apps").join(format!("{icon}.svg")));
            }
        }
        for ext in ["png", "svg"] {
            candidates.push(PathBuf::from(format!("/usr/share/pixmaps/{icon}.{ext}")));
        }
    }
    for c in candidates {
        let ext = c.extension().and_then(|e| e.to_str()).unwrap_or("");
        let mime = match ext {
            "png" => "image/png",
            "svg" => "image/svg+xml",
            _ => continue,
        };
        if let Ok(meta) = std::fs::metadata(&c) {
            if meta.len() > 200_000 {
                continue;
            }
            if let Ok(bytes) = std::fs::read(&c) {
                return Some(format!("data:{mime};base64,{}", util::base64(&bytes)));
            }
        }
    }
    None
}

fn pacman_owners(files: &[String]) -> HashMap<String, (String, String)> {
    let mut map = HashMap::new();
    for chunk in files.chunks(200) {
        let mut args = vec!["-Qo"];
        args.extend(chunk.iter().map(|s| s.as_str()));
        for line in util::run_any("pacman", &args).lines() {
            if let Some((file, rest)) = line.split_once(" is owned by ") {
                let mut it = rest.split_whitespace();
                if let (Some(pkg), Some(ver)) = (it.next(), it.next()) {
                    map.insert(file.to_string(), (pkg.to_string(), ver.to_string()));
                }
            }
        }
    }
    map
}

fn pacman_sizes(pkgs: &[String]) -> HashMap<String, u64> {
    let mut map = HashMap::new();
    for chunk in pkgs.chunks(100) {
        let mut args = vec!["-Qi"];
        args.extend(chunk.iter().map(|s| s.as_str()));
        let mut name = String::new();
        for line in util::run_any("pacman", &args).lines() {
            if let Some((k, v)) = line.split_once(':') {
                match k.trim() {
                    "Name" => name = v.trim().to_string(),
                    "Installed Size" => {
                        map.insert(name.clone(), util::parse_size(v));
                    }
                    _ => {}
                }
            }
        }
    }
    map
}

fn dpkg_owners(files: &[String]) -> HashMap<String, (String, String)> {
    let mut map = HashMap::new();
    for chunk in files.chunks(200) {
        let mut args = vec!["-S"];
        args.extend(chunk.iter().map(|s| s.as_str()));
        for line in util::run_any("dpkg", &args).lines() {
            if let Some((pkg, file)) = line.split_once(": ") {
                let pkg = pkg.split(':').next().unwrap_or(pkg);
                map.insert(file.trim().to_string(), (pkg.to_string(), String::new()));
            }
        }
    }
    map
}

fn dpkg_sizes(pkgs: &[String]) -> HashMap<String, (u64, String)> {
    let mut map = HashMap::new();
    if pkgs.is_empty() {
        return map;
    }
    let mut args = vec!["-W", "-f=${Package}|${Version}|${Installed-Size}\n"];
    args.extend(pkgs.iter().map(|s| s.as_str()));
    for line in util::run_any("dpkg-query", &args).lines() {
        let f: Vec<&str> = line.split('|').collect();
        if f.len() == 3 {
            map.insert(f[0].to_string(), (f[2].parse::<u64>().unwrap_or(0) * 1024, f[1].to_string()));
        }
    }
    map
}

fn rpm_owner(file: &str) -> Option<(String, u64, String)> {
    let out = util::run("rpm", &["-qf", "--qf", "%{NAME}|%{VERSION}|%{SIZE}", file])?;
    let f: Vec<&str> = out.split('|').collect();
    if f.len() == 3 { Some((f[0].to_string(), f[2].parse().unwrap_or(0), f[1].to_string())) } else { None }
}

pub fn list() -> Vec<App> {
    let mut desktops: Vec<Desktop> = Vec::new();
    for dir in desktop_dirs() {
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                if e.path().extension().map(|x| x == "desktop").unwrap_or(false) {
                    if let Some(d) = parse_desktop(&e.path()) {
                        desktops.push(d);
                    }
                }
            }
        }
    }

    let system_files: Vec<String> = desktops
        .iter()
        .filter(|d| d.file.starts_with("/usr/share/applications"))
        .map(|d| d.file.to_string_lossy().into_owned())
        .collect();

    let pacman = util::have("pacman");
    let dpkg = util::have("dpkg");
    let rpm = util::have("rpm");
    let owners = if pacman { pacman_owners(&system_files) } else if dpkg { dpkg_owners(&system_files) } else { HashMap::new() };
    let source = if pacman { "pacman" } else if dpkg { "apt" } else { "dnf" };

    let mut apps: BTreeMap<String, App> = BTreeMap::new();
    let mut system_pkgs: Vec<String> = Vec::new();

    for d in &desktops {
        let path = d.file.to_string_lossy().into_owned();
        let flatpak = path.contains("/flatpak/exports/");
        let snap = path.starts_with("/var/lib/snapd/");
        if flatpak {
            let id = d.file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            apps.entry(format!("flatpak:{id}")).or_insert(App {
                id: format!("flatpak:{id}"), name: d.name.clone(), source: "flatpak".into(), package: id, version: String::new(),
                size: 0, icon: icon_data(&d.icon), comment: d.comment.clone(),
            });
        } else if snap {
            let stem = d.file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let pkg = stem.split('_').next().unwrap_or(&stem).to_string();
            apps.entry(format!("snap:{pkg}")).or_insert(App {
                id: format!("snap:{pkg}"), name: d.name.clone(), source: "snap".into(), package: pkg, version: String::new(),
                size: 0, icon: icon_data(&d.icon), comment: d.comment.clone(),
            });
        } else if path.starts_with("/usr/share/applications") {
            let owned = owners.get(&path).cloned().or_else(|| if rpm { rpm_owner(&path).map(|(n, _, v)| (n, v)) } else { None });
            if let Some((pkg, ver)) = owned {
                system_pkgs.push(pkg.clone());
                apps.entry(format!("{source}:{pkg}")).or_insert(App {
                    id: format!("{source}:{pkg}"), name: d.name.clone(), source: source.into(), package: pkg, version: ver,
                    size: 0, icon: icon_data(&d.icon), comment: d.comment.clone(),
                });
            }
        } else if d.exec.to_lowercase().contains(".appimage") {
            let exec_path = d.exec.split_whitespace().next().unwrap_or("").trim_matches('"').to_string();
            if Path::new(&exec_path).is_file() {
                apps.entry(format!("appimage:{exec_path}")).or_insert(App {
                    id: format!("appimage:{exec_path}"), name: d.name.clone(), source: "appimage".into(), package: exec_path.clone(),
                    version: String::new(), size: std::fs::metadata(&exec_path).map(|m| m.len()).unwrap_or(0), icon: icon_data(&d.icon), comment: d.comment.clone(),
                });
            }
        }
    }

    // AppImages without a desktop entry.
    let h = safety::home();
    for dir in [h.join("Applications"), h.join(".local/bin"), h.join("Downloads")] {
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().map(|x| x.eq_ignore_ascii_case("appimage")).unwrap_or(false) && p.is_file() {
                    let ps = p.to_string_lossy().into_owned();
                    if apps.values().any(|a| a.package == ps) {
                        continue;
                    }
                    apps.insert(format!("appimage:{ps}"), App {
                        id: format!("appimage:{ps}"),
                        name: p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
                        source: "appimage".into(), package: ps, version: String::new(),
                        size: std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0), icon: None, comment: String::new(),
                    });
                }
            }
        }
    }

    // Sizes and versions.
    system_pkgs.sort();
    system_pkgs.dedup();
    if pacman {
        let sizes = pacman_sizes(&system_pkgs);
        for a in apps.values_mut().filter(|a| a.source == "pacman") {
            a.size = sizes.get(&a.package).copied().unwrap_or(0);
        }
    } else if dpkg {
        let sizes = dpkg_sizes(&system_pkgs);
        for a in apps.values_mut().filter(|a| a.source == "apt") {
            if let Some((s, v)) = sizes.get(&a.package) {
                a.size = *s;
                a.version = v.clone();
            }
        }
    } else if rpm {
        for a in apps.values_mut().filter(|a| a.source == "dnf") {
            if let Some(out) = util::run("rpm", &["-q", "--qf", "%{SIZE}", &a.package]) {
                a.size = out.trim().parse().unwrap_or(0);
            }
        }
    }
    if util::have("flatpak") {
        for line in util::run_any("flatpak", &["list", "--app", "--columns=application,version,size"]).lines() {
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() >= 3 {
                if let Some(a) = apps.get_mut(&format!("flatpak:{}", f[0])) {
                    a.version = f[1].to_string();
                    a.size = util::parse_size(f[2]);
                }
            }
        }
    }
    if util::have("snap") {
        for line in util::run_any("snap", &["list"]).lines().skip(1) {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() >= 3 {
                if let Some(a) = apps.get_mut(&format!("snap:{}", f[0])) {
                    a.version = f[1].to_string();
                    a.size = std::fs::metadata(format!("/var/lib/snapd/snaps/{}_{}.snap", f[0], f[2])).map(|m| m.len()).unwrap_or(0);
                }
            }
        }
    }

    let mut out: Vec<App> = apps.into_values().collect();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

fn candidates(app: &App) -> Vec<String> {
    let mut c: Vec<String> = Vec::new();
    let mut add = |s: &str| {
        let s = s.trim();
        if s.len() >= 3 && !c.iter().any(|x| x.eq_ignore_ascii_case(s)) {
            c.push(s.to_string());
        }
    };
    match app.source.as_str() {
        "appimage" => {
            let stem = Path::new(&app.package).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            add(&app.name);
            add(&app.name.replace(' ', "-"));
            add(stem.split(['-', '_']).next().unwrap_or(""));
        }
        "flatpak" => add(&app.package),
        _ => {
            add(&app.package);
            add(&app.name);
            add(&app.name.replace(' ', "-"));
            add(&app.name.replace(' ', ""));
        }
    }
    c
}

pub fn leftovers(app: &App) -> Vec<Leftover> {
    let h = safety::home();
    let names = candidates(app);
    let mut out: Vec<Leftover> = Vec::new();
    let places: [(PathBuf, &str, bool); 5] = [
        (h.join(".config"), "settings", true),
        (h.join(".local/share"), "data", true),
        (h.join(".local/state"), "state", false),
        (h.join(".cache"), "cache", false),
        (h.clone(), "dotfolder", true),
    ];
    for (dir, kind, caution) in &places {
        let Ok(rd) = std::fs::read_dir(dir) else { continue };
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            let cmp = if *kind == "dotfolder" {
                match n.strip_prefix('.') { Some(x) => x.to_string(), None => continue }
            } else {
                n.clone()
            };
            if names.iter().any(|c| c.eq_ignore_ascii_case(&cmp)) {
                let p = e.path();
                if safety::check(&p, Policy::Rule).is_ok() {
                    out.push(Leftover { path: p.to_string_lossy().into_owned(), size: scan::disk_usage(&p), kind: kind.to_string(), caution: *caution });
                }
            }
        }
    }
    if app.source == "flatpak" {
        let p = h.join(".var/app").join(&app.package);
        if p.exists() {
            out.push(Leftover { path: p.to_string_lossy().into_owned(), size: scan::disk_usage(&p), kind: "flatpak data".into(), caution: true });
        }
    }
    if app.source == "snap" {
        let p = h.join("snap").join(&app.package);
        if p.exists() {
            out.push(Leftover { path: p.to_string_lossy().into_owned(), size: scan::disk_usage(&p), kind: "snap data".into(), caution: true });
        }
    }
    out.sort_by(|a, b| b.size.cmp(&a.size));
    out
}

pub struct UninstallResult {
    pub output: String,
    pub removed_leftovers: usize,
    pub freed: u64,
    pub errors: Vec<String>,
}

/// Removes the app, then the leftovers the person ticked (each one has to be
/// a leftover this module would list for the app).
pub fn uninstall(app: &App, leftover_paths: &[String], mode: Mode) -> crate::Result<UninstallResult> {
    let mut output = String::new();
    match app.source.as_str() {
        "pacman" | "apt" | "dnf" | "snap" => {
            output = maintenance::helper(&["remove-package", &app.source, &app.package])?;
        }
        "flatpak" => {
            let out = std::process::Command::new("flatpak")
                .args(["uninstall", "-y", "--noninteractive", &app.package])
                .output()?;
            output = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
            if !out.status.success() {
                anyhow::bail!("flatpak could not remove {}: {}", app.package, output.trim());
            }
        }
        "appimage" => {
            clean::remove_path(Path::new(&app.package), mode, Policy::User).map_err(anyhow::Error::msg)?;
        }
        other => anyhow::bail!("unknown source {other}"),
    }
    let known = leftovers(app);
    let mut res = UninstallResult { output, removed_leftovers: 0, freed: 0, errors: Vec::new() };
    for p in leftover_paths {
        if !known.iter().any(|l| &l.path == p) {
            res.errors.push(format!("{p}: not a leftover of {}", app.name));
            continue;
        }
        match clean::remove_path(Path::new(p), mode, Policy::Rule) {
            Ok(n) => {
                res.removed_leftovers += 1;
                res.freed += n;
            }
            Err(e) => res.errors.push(e),
        }
    }
    Ok(res)
}
