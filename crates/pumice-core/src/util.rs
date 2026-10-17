use std::process::Command;

/// Runs a command with a fixed argument list (never through a shell) and
/// returns stdout when it succeeded.
pub fn run(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).env("LC_ALL", "C").output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// True when the program can be found on PATH.
pub fn have(program: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|p| p.join(program).is_file()))
        .unwrap_or(false)
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Names of the running processes (the `comm` field), lower-cased.
pub fn running_process_names() -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(dir) = std::fs::read_dir("/proc") {
        for entry in dir.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            if let Ok(comm) = std::fs::read_to_string(entry.path().join("comm")) {
                names.push(comm.trim().to_lowercase());
            }
        }
    }
    names
}

pub fn is_running(names: &[String], wanted: &str) -> bool {
    let wanted = wanted.to_lowercase();
    names.iter().any(|n| n == &wanted || n.starts_with(&wanted) && wanted.len() >= 7)
}

/// Like `run`, but gives stdout back even when the command exits non-zero
/// (`pacman -Qo` does that when one file is unowned).
pub fn run_any(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len() * 4 / 3 + 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

/// Parses "12.5 MiB", "3 GB", "800 kB" into bytes.
pub fn parse_size(text: &str) -> u64 {
    let t = text.trim().replace(',', ".");
    let mut it = t.split_whitespace();
    let num: f64 = it.next().and_then(|n| n.parse().ok()).unwrap_or(0.0);
    let unit = it.next().unwrap_or("B").to_lowercase();
    let mult = match unit.as_str() {
        "kib" => 1024.0,
        "mib" => 1024.0 * 1024.0,
        "gib" => 1024.0 * 1024.0 * 1024.0,
        "tib" => 1024f64.powi(4),
        "kb" => 1000.0,
        "mb" => 1e6,
        "gb" => 1e9,
        "tb" => 1e12,
        _ => 1.0,
    };
    (num * mult) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_and_sizes() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(parse_size("1.5 MiB"), 1_572_864);
        assert_eq!(parse_size("2 GB"), 2_000_000_000);
    }
}
