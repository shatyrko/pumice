//! pumice-helper: the only part of Pumice that runs as root.
//!
//! It is started through `pkexec` (see packaging/dev.pumice.helper.policy)
//! and does one thing from a fixed list. It never takes a path or a command
//! line from its caller: package names and disks are checked against strict
//! patterns and against the system's own lists before anything runs.

use std::io::Write;
use std::process::{Command, ExitCode, Stdio};

const SEARCH: [&str; 4] = ["/usr/bin", "/usr/sbin", "/bin", "/sbin"];

fn bin(name: &str) -> Result<String, String> {
    for dir in SEARCH {
        let p = format!("{dir}/{name}");
        if std::path::Path::new(&p).is_file() {
            return Ok(p);
        }
    }
    Err(format!("{name} is not installed"))
}

fn command(name: &str, args: &[&str]) -> Result<Command, String> {
    let mut c = Command::new(bin(name)?);
    c.args(args).env_clear().env("PATH", "/usr/bin:/usr/sbin").env("LC_ALL", "C").env("DEBIAN_FRONTEND", "noninteractive");
    Ok(c)
}

/// Runs a command, passes its output through, and returns whether it succeeded.
fn run(name: &str, args: &[&str]) -> Result<bool, String> {
    let status = command(name, args)?.stdin(Stdio::null()).status().map_err(|e| format!("{name}: {e}"))?;
    Ok(status.success())
}

fn capture(name: &str, args: &[&str]) -> String {
    command(name, args)
        .ok()
        .and_then(|mut c| c.stdin(Stdio::null()).output().ok())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

fn valid_package(n: &str) -> bool {
    !n.is_empty() && n.len() <= 100 && !n.starts_with('-') && n.bytes().all(|b| b.is_ascii_alphanumeric() || b"@._+-".contains(&b))
}

fn valid_disk(d: &str) -> bool {
    fn digits(s: &str, max: usize) -> bool {
        !s.is_empty() && s.len() <= max && s.bytes().all(|b| b.is_ascii_digit())
    }
    let Some(rest) = d.strip_prefix("/dev/") else { return false };
    if let Some(l) = rest.strip_prefix("sd").or_else(|| rest.strip_prefix("vd")) {
        return !l.is_empty() && l.len() <= 2 && l.bytes().all(|b| b.is_ascii_lowercase());
    }
    if let Some(r) = rest.strip_prefix("nvme") {
        return match r.split_once('n') {
            Some((a, b)) => digits(a, 2) && digits(b, 2),
            None => false,
        };
    }
    false
}

/// Only packages that ship an application menu entry may be removed here,
/// so this can never be used on `systemd`, `glibc` or the kernel.
fn is_application(manager: &str, name: &str) -> bool {
    let files = match manager {
        "pacman" => capture("pacman", &["-Ql", name]),
        "apt" => capture("dpkg", &["-L", name]),
        "dnf" => capture("rpm", &["-ql", name]),
        "snap" => return !capture("snap", &["list", name]).is_empty(),
        _ => return false,
    };
    files.lines().any(|l| l.contains("/usr/share/applications/") && l.trim_end().ends_with(".desktop"))
}

fn orphans_pacman() -> Vec<String> {
    capture("pacman", &["-Qtdq"]).lines().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

fn do_action(args: &[String]) -> Result<bool, String> {
    let rest: Vec<&str> = args.iter().skip(1).map(|s| s.as_str()).collect();
    match args.first().map(|s| s.as_str()) {
        Some("remove-package") => {
            let [manager, name] = rest[..] else { return Err("usage: remove-package <manager> <name>".into()) };
            if !valid_package(name) {
                return Err("bad package name".into());
            }
            if !is_application(manager, name) {
                return Err(format!("{name} is not an application package (it ships no menu entry)"));
            }
            match manager {
                "pacman" => run("pacman", &["-Rns", "--noconfirm", name]),
                "apt" => run("apt-get", &["remove", "--purge", "-y", name]),
                "dnf" => run("dnf", &["remove", "-y", name]),
                "snap" => run("snap", &["remove", name]),
                _ => Err("unknown package manager".into()),
            }
        }
        Some("remove-orphans") => {
            if bin("pacman").is_ok() {
                let orphans = orphans_pacman();
                for p in &rest {
                    if !valid_package(p) || !orphans.iter().any(|o| o == p) {
                        return Err(format!("{p} is not an unused dependency"));
                    }
                }
                if rest.is_empty() {
                    return Err("no packages given".into());
                }
                let mut a = vec!["-Rns", "--noconfirm"];
                a.extend(rest.iter().copied());
                run("pacman", &a)
            } else {
                run("apt-get", &["autoremove", "--purge", "-y"])
            }
        }
        Some("clean-package-cache") => {
            if bin("pacman").is_ok() {
                run("paccache", &["-rk2"])
            } else if bin("apt-get").is_ok() {
                run("apt-get", &["clean"])
            } else {
                run("dnf", &["clean", "all"])
            }
        }
        Some("journal-vacuum") => run("journalctl", &["--vacuum-size=200M"]),
        Some("coredumps-clear") => {
            let dir = "/var/lib/systemd/coredump";
            let mut n = 0;
            if let Ok(rd) = std::fs::read_dir(dir) {
                for e in rd.flatten() {
                    if std::fs::symlink_metadata(e.path()).map(|m| m.is_file()).unwrap_or(false) && std::fs::remove_file(e.path()).is_ok() {
                        n += 1;
                    }
                }
            }
            println!("Removed {n} files.");
            Ok(true)
        }
        Some("fstrim") => run("fstrim", &["-av"]),
        Some("flush-dns") => run("resolvectl", &["flush-caches"]),
        Some("smart") => {
            for d in &rest {
                if !valid_disk(d) {
                    return Err(format!("{d}: not a disk"));
                }
            }
            if rest.is_empty() {
                return Err("no disks given".into());
            }
            for d in &rest {
                println!("== {d}");
                // smartctl uses bit flags in its exit code; a failing disk is not our failure.
                let _ = run("smartctl", &["-H", d]);
            }
            Ok(true)
        }
        _ => Err("unknown action".into()),
    }
}

fn main() -> ExitCode {
    // Only root makes sense; pkexec sets this up.
    if unsafe { libc::geteuid() } != 0 {
        eprintln!("pumice-helper must run as root (through pkexec)");
        return ExitCode::from(2);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    match do_action(&args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "{e}");
            ExitCode::from(3)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_disks() {
        assert!(valid_package("firefox") && valid_package("libfoo++") && valid_package("a@b"));
        assert!(!valid_package("-rf") && !valid_package("a b") && !valid_package("a;b") && !valid_package(""));
        assert!(valid_disk("/dev/sda") && valid_disk("/dev/nvme0n1") && valid_disk("/dev/vdb"));
        assert!(!valid_disk("/dev/sda1") && !valid_disk("/dev/../etc") && !valid_disk("sda") && !valid_disk("/dev/nvme0"));
    }
}
