//! Things that start with the desktop session: XDG autostart entries and
//! systemd user services. Nothing is deleted, entries are only switched off.

use crate::safety;
use crate::util;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone, Debug)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub comment: String,
    pub command: String,
    pub enabled: bool,
    pub scope: String,
}

fn user_autostart() -> PathBuf {
    safety::home().join(".config/autostart")
}

fn field(text: &str, key: &str) -> Option<String> {
    let mut in_entry = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if in_entry {
            if let Some((k, v)) = line.split_once('=') {
                if k == key {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

fn autostart_entries() -> Vec<Entry> {
    let mut map: HashMap<String, Entry> = HashMap::new();
    for (dir, scope) in [(PathBuf::from("/etc/xdg/autostart"), "system"), (user_autostart(), "user")] {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().map(|x| x != "desktop").unwrap_or(true) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&p) else { continue };
            let file = e.file_name().to_string_lossy().into_owned();
            let hidden = field(&text, "Hidden").map(|v| v == "true").unwrap_or(false);
            let gnome_off = field(&text, "X-GNOME-Autostart-enabled").map(|v| v == "false").unwrap_or(false);
            map.insert(
                file.clone(),
                Entry {
                    id: format!("autostart:{file}"),
                    name: field(&text, "Name").unwrap_or_else(|| file.clone()),
                    kind: "autostart".into(),
                    comment: field(&text, "Comment").unwrap_or_default(),
                    command: field(&text, "Exec").unwrap_or_default(),
                    enabled: !hidden && !gnome_off,
                    // A user file overrides the system one of the same name.
                    scope: scope.into(),
                },
            );
        }
    }
    map.into_values().collect()
}

fn service_entries() -> Vec<Entry> {
    if !util::have("systemctl") {
        return Vec::new();
    }
    let list = util::run_any("systemctl", &["--user", "list-unit-files", "--type=service,timer", "--no-legend", "--no-pager"]);
    let mut units: Vec<(String, bool)> = Vec::new();
    for line in list.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() >= 2 && (f[1] == "enabled" || f[1] == "disabled") {
            units.push((f[0].to_string(), f[1] == "enabled"));
        }
    }
    if units.is_empty() {
        return Vec::new();
    }
    let mut args = vec!["--user", "show", "-p", "Id,Description,ExecStart", "--no-pager"];
    args.extend(units.iter().map(|(u, _)| u.as_str()));
    let shown = util::run_any("systemctl", &args);
    let mut info: HashMap<String, (String, String)> = HashMap::new();
    for block in shown.split("\n\n") {
        let (mut id, mut desc, mut exec) = (String::new(), String::new(), String::new());
        for line in block.lines() {
            if let Some(v) = line.strip_prefix("Id=") {
                id = v.to_string();
            } else if let Some(v) = line.strip_prefix("Description=") {
                desc = v.to_string();
            } else if let Some(v) = line.strip_prefix("ExecStart=") {
                // "{ path=/usr/bin/x ; argv[]=... }"
                exec = v.split("path=").nth(1).and_then(|s| s.split(" ;").next()).unwrap_or("").to_string();
            }
        }
        if !id.is_empty() {
            info.insert(id, (desc, exec));
        }
    }
    units
        .into_iter()
        .map(|(unit, enabled)| {
            let (desc, exec) = info.get(&unit).cloned().unwrap_or_default();
            Entry {
                id: format!("service:{unit}"),
                name: unit.trim_end_matches(".service").to_string(),
                kind: if unit.ends_with(".timer") { "timer".into() } else { "service".into() },
                comment: desc,
                command: exec,
                enabled,
                scope: "user".into(),
            }
        })
        .collect()
}

pub fn list() -> Vec<Entry> {
    let mut v = autostart_entries();
    v.extend(service_entries());
    v.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    v
}

fn valid_file_name(n: &str) -> bool {
    !n.is_empty() && n.len() < 200 && !n.contains('/') && n != "." && n != ".." && n.ends_with(".desktop")
}

fn valid_unit(n: &str) -> bool {
    n.len() < 200
        && (n.ends_with(".service") || n.ends_with(".timer"))
        && n.bytes().all(|b| b.is_ascii_alphanumeric() || b"@._-:\\".contains(&b))
}

/// Sets `key=value` inside the [Desktop Entry] group, adding the line when missing.
fn set_field(text: &str, key: &str, value: &str) -> String {
    let mut out = Vec::new();
    let (mut in_entry, mut done) = (false, false);
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            if in_entry && !done {
                out.push(format!("{key}={value}"));
                done = true;
            }
            in_entry = t == "[Desktop Entry]";
        }
        if in_entry && t.split_once('=').map(|(k, _)| k == key).unwrap_or(false) {
            out.push(format!("{key}={value}"));
            done = true;
            continue;
        }
        out.push(line.to_string());
    }
    if !done {
        out.push(format!("{key}={value}"));
    }
    out.join("\n") + "\n"
}

pub fn set_enabled(id: &str, enabled: bool) -> crate::Result<()> {
    if let Some(file) = id.strip_prefix("autostart:") {
        if !valid_file_name(file) {
            anyhow::bail!("bad autostart name");
        }
        let user = user_autostart().join(file);
        let system = Path::new("/etc/xdg/autostart").join(file);
        let text = std::fs::read_to_string(&user).or_else(|_| std::fs::read_to_string(&system))?;
        let text = set_field(&text, "Hidden", if enabled { "false" } else { "true" });
        let text = set_field(&text, "X-GNOME-Autostart-enabled", if enabled { "true" } else { "false" });
        std::fs::create_dir_all(user_autostart())?;
        std::fs::write(&user, text)?;
        return Ok(());
    }
    if let Some(unit) = id.strip_prefix("service:") {
        if !valid_unit(unit) {
            anyhow::bail!("bad unit name");
        }
        let out = std::process::Command::new("systemctl")
            .args(["--user", if enabled { "enable" } else { "disable" }, unit])
            .output()?;
        if !out.status.success() {
            anyhow::bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
        }
        return Ok(());
    }
    anyhow::bail!("unknown entry")
}

#[derive(Serialize, Clone, Debug)]
pub struct BootUnit {
    pub unit: String,
    pub millis: u64,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Boot {
    pub summary: String,
    pub slowest: Vec<BootUnit>,
}

fn parse_duration(s: &str) -> u64 {
    // "1min 2.345s", "812ms", "3.2s"
    let mut total = 0.0;
    for part in s.split_whitespace() {
        if let Some(v) = part.strip_suffix("min") {
            total += v.parse::<f64>().unwrap_or(0.0) * 60_000.0;
        } else if let Some(v) = part.strip_suffix("ms") {
            total += v.parse::<f64>().unwrap_or(0.0);
        } else if let Some(v) = part.strip_suffix('s') {
            total += v.parse::<f64>().unwrap_or(0.0) * 1000.0;
        }
    }
    total as u64
}

pub fn boot() -> Boot {
    if !util::have("systemd-analyze") {
        return Boot::default();
    }
    let summary = util::run_any("systemd-analyze", &["--no-pager"]).lines().next().unwrap_or("").to_string();
    let slowest = util::run_any("systemd-analyze", &["blame", "--no-pager"])
        .lines()
        .take(12)
        .filter_map(|l| {
            let l = l.trim();
            let idx = l.rfind(' ')?;
            Some(BootUnit { unit: l[idx + 1..].to_string(), millis: parse_duration(&l[..idx]) })
        })
        .collect();
    Boot { summary, slowest }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_fields() {
        let t = "[Desktop Entry]\nName=X\nExec=x\n[Desktop Action a]\nName=Y\n";
        let a = set_field(t, "Hidden", "true");
        assert!(a.contains("Name=X\nExec=x\nHidden=true\n[Desktop Action a]"));
        let b = set_field(&a, "Hidden", "false");
        assert!(b.contains("Hidden=false") && !b.contains("Hidden=true"));
        assert_eq!(parse_duration("1min 2.5s"), 62_500);
        assert_eq!(parse_duration("812ms"), 812);
        assert!(valid_unit("foo@bar.service") && !valid_unit("a b.service") && !valid_unit("../x.service"));
    }
}
