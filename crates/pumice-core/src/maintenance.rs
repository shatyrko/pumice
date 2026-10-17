//! System health checks and maintenance actions.
//!
//! Checks only read. Actions that need root go through `pumice-helper`
//! (pkexec + polkit): a separate program with a fixed list of actions.

use crate::apps;
use crate::util;
use serde::Serialize;
use std::path::Path;

pub const HELPER_PATH: &str = "/usr/lib/pumice/pumice-helper";

#[derive(Serialize, Clone, Debug)]
pub struct ActionDef {
    pub id: String,
    pub label: String,
    pub privileged: bool,
    pub confirm: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct Check {
    pub id: String,
    pub title: String,
    /// ok | warn | info | na
    pub status: String,
    pub detail: String,
    pub size: Option<u64>,
    pub items: Vec<String>,
    pub actions: Vec<ActionDef>,
}

pub fn helper_installed() -> bool {
    Path::new(HELPER_PATH).is_file()
}

/// Runs one fixed helper action under pkexec.
pub fn helper(args: &[&str]) -> crate::Result<String> {
    if !helper_installed() {
        anyhow::bail!("The Pumice helper is not installed ({HELPER_PATH}). Run scripts/install-helper.sh once.");
    }
    let out = std::process::Command::new("pkexec").arg(HELPER_PATH).args(args).output()?;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    match out.status.code() {
        Some(0) => Ok(text),
        Some(126) | Some(127) => anyhow::bail!("Administrator permission was not granted."),
        _ => anyhow::bail!("{}", text.trim()),
    }
}

fn action(id: &str, label: &str, privileged: bool, confirm: &str) -> ActionDef {
    ActionDef { id: id.into(), label: label.into(), privileged, confirm: confirm.into() }
}

fn check(id: &str, title: &str, status: &str, detail: impl Into<String>) -> Check {
    Check { id: id.into(), title: title.into(), status: status.into(), detail: detail.into(), size: None, items: Vec::new(), actions: Vec::new() }
}

fn dir_size(p: &str) -> u64 {
    crate::scan::disk_usage(Path::new(p))
}

pub fn orphans() -> Vec<(String, u64)> {
    if util::have("pacman") {
        let names: Vec<String> = util::run_any("pacman", &["-Qtdq"]).lines().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        if names.is_empty() {
            return Vec::new();
        }
        let mut args = vec!["-Qi"];
        args.extend(names.iter().map(|s| s.as_str()));
        let mut out = Vec::new();
        let mut name = String::new();
        for line in util::run_any("pacman", &args).lines() {
            if let Some((k, v)) = line.split_once(':') {
                match k.trim() {
                    "Name" => name = v.trim().to_string(),
                    "Installed Size" => out.push((name.clone(), util::parse_size(v))),
                    _ => {}
                }
            }
        }
        out.sort_by(|a, b| b.1.cmp(&a.1));
        return out;
    }
    if util::have("apt-get") {
        return util::run_any("apt-get", &["-s", "autoremove"])
            .lines()
            .filter_map(|l| l.strip_prefix("Remv ").map(|r| (r.split_whitespace().next().unwrap_or("").to_string(), 0)))
            .collect();
    }
    Vec::new()
}

fn journal_usage() -> Option<u64> {
    // "Archived and active journals take up 1.2G in the file system."
    let out = util::run("journalctl", &["--disk-usage"])?;
    let word = out.split("take up ").nth(1)?.split_whitespace().next()?;
    let (num, unit) = word.split_at(word.find(|c: char| c.is_alphabetic())?);
    let unit = match unit.trim_end_matches('.') {
        "K" => "KiB",
        "M" => "MiB",
        "G" => "GiB",
        "T" => "TiB",
        _ => "B",
    };
    Some(util::parse_size(&format!("{num} {unit}")))
}

pub fn checks() -> Vec<Check> {
    let mut v = Vec::new();

    // Orphaned packages.
    let orph = orphans();
    let mut c = check("orphans", "Unused dependencies", if orph.is_empty() { "ok" } else { "warn" }, "");
    if util::have("pacman") || util::have("apt-get") {
        if orph.is_empty() {
            c.detail = "No packages are left over from removed software.".into();
        } else {
            c.detail = format!("{} packages were installed as dependencies and nothing needs them now.", orph.len());
            c.size = Some(orph.iter().map(|o| o.1).sum());
            c.items = orph.iter().map(|o| o.0.clone()).collect();
            c.actions.push(action("remove-orphans", "Remove them", true, "Removes these packages with their configuration files."));
        }
    } else {
        c.status = "na".into();
        c.detail = "Supported on Arch (pacman) and Debian (apt) systems.".into();
    }
    v.push(c);

    // Package cache.
    if util::have("pacman") {
        let n = dir_size("/var/cache/pacman/pkg");
        let mut c = check("package-cache", "Downloaded packages", if n > 2 << 30 { "warn" } else { "info" }, "Old package files kept in /var/cache/pacman/pkg.");
        c.size = Some(n);
        if util::have("paccache") {
            c.actions.push(action("clean-package-cache", "Keep the latest 2 versions", true, "Deletes older cached versions of each package (paccache -rk2)."));
        } else {
            c.detail += " Install pacman-contrib to clean it from here.";
        }
        v.push(c);
    } else if util::have("apt-get") {
        let n = dir_size("/var/cache/apt/archives");
        let mut c = check("package-cache", "Downloaded packages", "info", "Package files kept in /var/cache/apt/archives.");
        c.size = Some(n);
        c.actions.push(action("clean-package-cache", "Clean the cache", true, "Runs apt-get clean."));
        v.push(c);
    } else if util::have("dnf") {
        let mut c = check("package-cache", "Downloaded packages", "info", "Package files kept by dnf.");
        c.size = Some(dir_size("/var/cache/libdnf5") + dir_size("/var/cache/dnf"));
        c.actions.push(action("clean-package-cache", "Clean the cache", true, "Runs dnf clean all."));
        v.push(c);
    }

    // Journal.
    if let Some(n) = journal_usage() {
        let mut c = check("journal", "System journal", if n > 1 << 30 { "warn" } else { "info" }, "Logs kept by systemd-journald.");
        c.size = Some(n);
        c.actions.push(action("journal-vacuum", "Keep the last 200 MB", true, "Removes the oldest journal files until 200 MB are left."));
        v.push(c);
    }

    // Core dumps.
    {
        let n = dir_size("/var/lib/systemd/coredump");
        if n > 0 {
            let mut c = check("coredumps", "Crash dumps", "info", "Core files saved by systemd-coredump.");
            c.size = Some(n);
            c.actions.push(action("coredumps-clear", "Delete them", true, "Deletes every file in /var/lib/systemd/coredump."));
            v.push(c);
        }
    }

    // Flatpak runtimes.
    if util::have("flatpak") {
        let mut c = check("flatpak-unused", "Unused Flatpak runtimes", "info", "Runtimes that no installed Flatpak app needs any more.");
        c.actions.push(action("flatpak-unused", "Remove unused runtimes", false, "Runs flatpak uninstall --unused."));
        v.push(c);
    }

    // Containers.
    for tool in ["docker", "podman"] {
        if util::have(tool) {
            let df = util::run(tool, &["system", "df", "--format", "{{.Type}}: {{.Size}} ({{.Reclaimable}} reclaimable)"]);
            if let Some(df) = df {
                let mut c = check(&format!("{tool}-prune"), &format!("{} storage", if tool == "docker" { "Docker" } else { "Podman" }), "info", "Images, containers and build cache.");
                c.items = df.lines().map(|s| s.to_string()).collect();
                c.actions.push(action(&format!("{tool}-prune"), "Remove dangling images and build cache", false, "Removes images without a tag and the build cache. Running containers and tagged images stay."));
                v.push(c);
            }
        }
    }

    // TRIM.
    {
        let enabled = util::run_any("systemctl", &["is-enabled", "fstrim.timer"]).trim().to_string();
        let mut c = check("trim", "SSD trim", if enabled == "enabled" { "ok" } else { "info" }, if enabled == "enabled" { "The weekly fstrim timer is enabled." } else { "The weekly fstrim timer is not enabled (systemctl enable --now fstrim.timer)." });
        c.actions.push(action("fstrim", "Trim now", true, "Runs fstrim on all mounted filesystems that support it."));
        v.push(c);
    }

    // Failed units.
    {
        let mut failed: Vec<String> = Vec::new();
        for scope in [&["--failed", "--no-legend", "--no-pager"][..], &["--user", "--failed", "--no-legend", "--no-pager"][..]] {
            for line in util::run_any("systemctl", scope).lines() {
                let l = line.trim_start_matches(['●', ' ', '*']).trim();
                if let Some(u) = l.split_whitespace().next() {
                    failed.push(u.to_string());
                }
            }
        }
        let mut c = check("failed-units", "Failed services", if failed.is_empty() { "ok" } else { "warn" }, if failed.is_empty() { "No systemd service has failed." } else { "These services failed. Check them with journalctl -u <name>." });
        c.items = failed;
        v.push(c);
    }

    // Config leftovers of pacman.
    if util::have("pacman") {
        let found = util::run_any("find", &["/etc", "-xdev", "(", "-name", "*.pacnew", "-o", "-name", "*.pacsave", ")"]);
        let items: Vec<String> = found.lines().map(|s| s.to_string()).filter(|s| !s.is_empty()).collect();
        let mut c = check("pacnew", "Configuration files to merge", if items.is_empty() { "ok" } else { "warn" }, if items.is_empty() { "No .pacnew or .pacsave files in /etc." } else { "pacman left new versions of these files. Merge them (pacdiff) when you can." });
        c.items = items;
        v.push(c);
    }

    // Disk health.
    {
        let mut c = check("smart", "Disk health (SMART)", "info", "Reads the SMART health of each disk. Needs administrator permission.");
        c.items = disks();
        if c.items.is_empty() {
            c.status = "na".into();
        } else if util::have("smartctl") {
            c.actions.push(action("smart", "Check disks", true, "Runs smartctl -H on each disk."));
        } else {
            c.detail = "Install smartmontools to read SMART health.".into();
        }
        v.push(c);
    }

    // User-level repairs.
    {
        let mut c = check("caches", "Rebuild desktop caches", "info", "Fixes missing fonts, stale menu entries and wrong icons.");
        c.actions.push(action("fc-cache", "Rebuild the font cache", false, "Runs fc-cache -f."));
        c.actions.push(action("desktop-db", "Rebuild the application menu", false, "Runs update-desktop-database for your applications folder."));
        v.push(c);
    }
    {
        let mut c = check("dns", "Network", "info", "Clears the DNS cache when a site resolves to an old address.");
        c.actions.push(action("flush-dns", "Flush the DNS cache", true, "Runs resolvectl flush-caches."));
        v.push(c);
    }
    v
}

pub fn disks() -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir("/sys/block") {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if n.starts_with("sd") || n.starts_with("nvme") || n.starts_with("vd") {
                out.push(format!("/dev/{n}"));
            }
        }
    }
    out.sort();
    out
}

/// Runs a maintenance action by id. `args` are the packages or devices the
/// person picked (the helper checks them again).
pub fn run_action(id: &str, args: &[String]) -> crate::Result<String> {
    match id {
        "remove-orphans" => {
            let current: Vec<String> = orphans().into_iter().map(|o| o.0).collect();
            let picked: Vec<&String> = args.iter().filter(|a| current.contains(a)).collect();
            if picked.is_empty() {
                anyhow::bail!("Nothing to remove.");
            }
            let mut a = vec!["remove-orphans"];
            a.extend(picked.iter().map(|s| s.as_str()));
            helper(&a)
        }
        "clean-package-cache" | "journal-vacuum" | "coredumps-clear" | "fstrim" | "flush-dns" => helper(&[id]),
        "smart" => {
            let known = disks();
            let picked: Vec<&String> = if args.is_empty() { known.iter().collect() } else { args.iter().filter(|a| known.contains(a)).collect() };
            let mut a = vec!["smart"];
            a.extend(picked.iter().map(|s| s.as_str()));
            helper(&a)
        }
        "flatpak-unused" => user_cmd("flatpak", &["uninstall", "--unused", "-y", "--noninteractive"]),
        "docker-prune" => {
            let a = user_cmd("docker", &["image", "prune", "-f"])?;
            let b = user_cmd("docker", &["builder", "prune", "-f"])?;
            Ok(format!("{a}{b}"))
        }
        "podman-prune" => {
            let a = user_cmd("podman", &["image", "prune", "-f"])?;
            Ok(a)
        }
        "fc-cache" => user_cmd("fc-cache", &["-f"]),
        "desktop-db" => {
            let dir = crate::safety::home().join(".local/share/applications");
            user_cmd("update-desktop-database", &[&dir.to_string_lossy()])
        }
        _ => anyhow::bail!("unknown action {id}"),
    }
}

fn user_cmd(program: &str, args: &[&str]) -> crate::Result<String> {
    if !util::have(program) {
        anyhow::bail!("{program} is not installed");
    }
    let out = std::process::Command::new(program).args(args).output()?;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    if out.status.success() { Ok(if text.trim().is_empty() { "Done.".into() } else { text }) } else { anyhow::bail!("{}", text.trim()) }
}

#[allow(dead_code)]
fn _link(_: &apps::App) {}
