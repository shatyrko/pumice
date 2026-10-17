//! Live system readings straight from /proc and /sys (no extra crate).
//! `Monitor::sample` needs to be called repeatedly: rates and CPU load are
//! computed from the difference to the previous call.

use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::time::Instant;

#[derive(Serialize, Clone, Debug, Default)]
pub struct Cpu {
    pub model: String,
    pub cores: usize,
    pub usage: f32,
    pub per_core: Vec<f32>,
    pub load: [f32; 3],
    pub temp_c: Option<f32>,
    pub freq_mhz: Option<u32>,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Memory {
    pub total: u64,
    pub used: u64,
    pub available: u64,
    pub cached: u64,
    pub swap_total: u64,
    pub swap_used: u64,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Disk {
    pub device: String,
    pub mount: String,
    pub fs: String,
    pub total: u64,
    pub used: u64,
    pub free: u64,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct DiskIo {
    pub read_bps: f64,
    pub write_bps: f64,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Net {
    pub down_bps: f64,
    pub up_bps: f64,
    pub interfaces: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Battery {
    pub percent: u32,
    pub status: String,
    pub watts: Option<f32>,
    pub health_percent: Option<u32>,
    pub cycles: Option<u32>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Proc {
    pub pid: u32,
    pub name: String,
    pub cpu: f32,
    pub rss: u64,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Snapshot {
    pub host: String,
    pub kernel: String,
    pub os: String,
    pub uptime_secs: u64,
    pub cpu: Cpu,
    pub memory: Memory,
    pub disks: Vec<Disk>,
    pub io: DiskIo,
    pub net: Net,
    pub battery: Option<Battery>,
    pub top_cpu: Vec<Proc>,
    pub top_mem: Vec<Proc>,
    pub health: u32,
    pub health_notes: Vec<String>,
}

#[derive(Default)]
pub struct Monitor {
    last: Option<Prev>,
}

struct Prev {
    at: Instant,
    cpu: Vec<(u64, u64)>,
    net: (u64, u64),
    io: (u64, u64),
    procs: HashMap<u32, u64>,
}

fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

fn cpu_times() -> Vec<(u64, u64)> {
    // (busy, total) per line: the first is the sum, then one per core.
    read("/proc/stat")
        .lines()
        .filter(|l| l.starts_with("cpu"))
        .map(|l| {
            let v: Vec<u64> = l.split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
            let total: u64 = v.iter().take(8).sum();
            let idle = v.get(3).copied().unwrap_or(0) + v.get(4).copied().unwrap_or(0);
            (total.saturating_sub(idle), total)
        })
        .collect()
}

fn net_totals() -> (u64, u64, Vec<String>) {
    let (mut rx, mut tx) = (0, 0);
    let mut names = Vec::new();
    for line in read("/proc/net/dev").lines().skip(2) {
        let Some((name, rest)) = line.split_once(':') else { continue };
        let name = name.trim();
        if name == "lo" || name.starts_with("veth") || name.starts_with("docker") || name.starts_with("br-") {
            continue;
        }
        let v: Vec<u64> = rest.split_whitespace().filter_map(|x| x.parse().ok()).collect();
        if v.len() >= 9 {
            rx += v[0];
            tx += v[8];
            names.push(name.to_string());
        }
    }
    (rx, tx, names)
}

/// Sectors read/written on whole disks (not partitions).
fn io_totals() -> (u64, u64) {
    let (mut r, mut w) = (0, 0);
    for line in read("/proc/diskstats").lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 10 {
            continue;
        }
        let name = f[2];
        let whole = (name.starts_with("sd") || name.starts_with("vd") || name.starts_with("xvd")) && !name.chars().last().map(|c| c.is_ascii_digit()).unwrap_or(false)
            || (name.starts_with("nvme") && !name.contains('p'))
            || (name.starts_with("mmcblk") && !name.contains('p'));
        if whole {
            r += f[5].parse::<u64>().unwrap_or(0);
            w += f[9].parse::<u64>().unwrap_or(0);
        }
    }
    (r * 512, w * 512)
}

fn proc_times() -> HashMap<u32, (u64, String, u64)> {
    let mut map = HashMap::new();
    let page = 4096u64;
    let Ok(rd) = fs::read_dir("/proc") else { return map };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Ok(pid) = name.parse::<u32>() else { continue };
        let Ok(stat) = fs::read_to_string(e.path().join("stat")) else { continue };
        // The name sits in parentheses and may contain spaces.
        let (Some(open), Some(close)) = (stat.find('('), stat.rfind(')')) else { continue };
        let comm = stat[open + 1..close].to_string();
        let rest: Vec<&str> = stat[close + 2..].split_whitespace().collect();
        if rest.len() < 22 {
            continue;
        }
        let utime: u64 = rest[11].parse().unwrap_or(0);
        let stime: u64 = rest[12].parse().unwrap_or(0);
        let rss: u64 = rest[21].parse::<u64>().unwrap_or(0) * page;
        map.insert(pid, (utime + stime, comm, rss));
    }
    map
}

fn mounts() -> Vec<Disk> {
    let mut out: Vec<Disk> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for line in read("/proc/mounts").lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 3 {
            continue;
        }
        let (dev, mount, fs_type) = (f[0], f[1].replace("\\040", " "), f[2]);
        if !dev.starts_with("/dev/") || dev.starts_with("/dev/loop") || mount.starts_with("/boot/efi") && false {
            continue;
        }
        if mount.starts_with("/var/lib/docker") || mount.starts_with("/snap") || mount.starts_with("/run/user") {
            continue;
        }
        if !seen.insert(dev.to_string()) {
            continue;
        }
        let Ok(c) = std::ffi::CString::new(mount.clone()) else { continue };
        let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
            continue;
        }
        let bs = st.f_frsize as u64;
        let total = st.f_blocks as u64 * bs;
        let free = st.f_bavail as u64 * bs;
        let used = total.saturating_sub(st.f_bfree as u64 * bs);
        if total == 0 {
            continue;
        }
        out.push(Disk { device: dev.into(), mount, fs: fs_type.into(), total, used, free });
    }
    out
}

fn cpu_temp() -> Option<f32> {
    let mut best: Option<f32> = None;
    for e in fs::read_dir("/sys/class/hwmon").ok()?.flatten() {
        let name = read(&format!("{}/name", e.path().display()));
        let name = name.trim();
        if !["coretemp", "k10temp", "zenpower", "cpu_thermal", "acpitz"].contains(&name) {
            continue;
        }
        for i in 1..16 {
            if let Ok(t) = fs::read_to_string(e.path().join(format!("temp{i}_input"))) {
                if let Ok(v) = t.trim().parse::<f32>() {
                    let c = v / 1000.0;
                    best = Some(best.map(|b| b.max(c)).unwrap_or(c));
                }
            }
        }
        if best.is_some() && name != "acpitz" {
            break;
        }
    }
    best
}

fn battery() -> Option<Battery> {
    for e in fs::read_dir("/sys/class/power_supply").ok()?.flatten() {
        let p = e.path();
        if read(&format!("{}/type", p.display())).trim() != "Battery" {
            continue;
        }
        let g = |f: &str| read(&format!("{}/{}", p.display(), f)).trim().to_string();
        let percent = g("capacity").parse().ok()?;
        let watts = g("power_now").parse::<f32>().ok().map(|v| v / 1_000_000.0).or_else(|| {
            let (c, v) = (g("current_now").parse::<f32>().ok()?, g("voltage_now").parse::<f32>().ok()?);
            Some(c * v / 1e12)
        });
        let full = g("energy_full").parse::<f32>().ok().or_else(|| g("charge_full").parse().ok());
        let design = g("energy_full_design").parse::<f32>().ok().or_else(|| g("charge_full_design").parse().ok());
        let health = match (full, design) {
            (Some(f), Some(d)) if d > 0.0 => Some(((f / d) * 100.0).round() as u32),
            _ => None,
        };
        return Some(Battery { percent, status: g("status"), watts, health_percent: health, cycles: g("cycle_count").parse().ok().filter(|c| *c > 0) });
    }
    None
}

fn os_name() -> String {
    for line in read("/etc/os-release").lines() {
        if let Some(v) = line.strip_prefix("PRETTY_NAME=") {
            return v.trim_matches('"').to_string();
        }
    }
    "Linux".into()
}

fn memory() -> Memory {
    let mut m: HashMap<String, u64> = HashMap::new();
    for line in read("/proc/meminfo").lines() {
        if let Some((k, v)) = line.split_once(':') {
            let kb: u64 = v.split_whitespace().next().and_then(|x| x.parse().ok()).unwrap_or(0);
            m.insert(k.to_string(), kb * 1024);
        }
    }
    let g = |k: &str| m.get(k).copied().unwrap_or(0);
    let total = g("MemTotal");
    let available = g("MemAvailable");
    Memory {
        total,
        available,
        used: total.saturating_sub(available),
        cached: g("Cached") + g("SReclaimable"),
        swap_total: g("SwapTotal"),
        swap_used: g("SwapTotal").saturating_sub(g("SwapFree")),
    }
}

fn health(s: &Snapshot) -> (u32, Vec<String>) {
    // Starts at 100; every problem takes points off and says why.
    let mut score: i32 = 100;
    let mut notes = Vec::new();
    let mem_pct = if s.memory.total > 0 { s.memory.used as f32 / s.memory.total as f32 * 100.0 } else { 0.0 };
    if mem_pct > 90.0 {
        score -= 20;
        notes.push(format!("Memory is {:.0}% used", mem_pct));
    } else if mem_pct > 80.0 {
        score -= 10;
        notes.push(format!("Memory is {:.0}% used", mem_pct));
    }
    if s.memory.swap_total > 0 && s.memory.swap_used as f32 / s.memory.swap_total as f32 > 0.5 {
        score -= 10;
        notes.push("Swap is more than half full".into());
    }
    if s.cpu.usage > 90.0 {
        score -= 10;
        notes.push(format!("CPU is at {:.0}%", s.cpu.usage));
    }
    let per_core_load = s.cpu.load[0] / s.cpu.cores.max(1) as f32;
    if per_core_load > 1.5 {
        score -= 10;
        notes.push(format!("Load average {:.1} on {} cores", s.cpu.load[0], s.cpu.cores));
    }
    if let Some(t) = s.cpu.temp_c {
        if t > 90.0 {
            score -= 15;
            notes.push(format!("CPU is hot ({t:.0} °C)"));
        } else if t > 80.0 {
            score -= 5;
            notes.push(format!("CPU is warm ({t:.0} °C)"));
        }
    }
    for d in &s.disks {
        let pct = d.used as f32 / d.total as f32 * 100.0;
        if (d.mount == "/" || d.mount == "/home") && pct > 95.0 {
            score -= 25;
            notes.push(format!("{} is {:.0}% full", d.mount, pct));
        } else if (d.mount == "/" || d.mount == "/home") && pct > 85.0 {
            score -= 10;
            notes.push(format!("{} is {:.0}% full", d.mount, pct));
        }
    }
    if let Some(b) = &s.battery {
        if let Some(h) = b.health_percent {
            if h < 60 {
                score -= 10;
                notes.push(format!("Battery holds {h}% of its design capacity"));
            }
        }
    }
    (score.clamp(0, 100) as u32, notes)
}

impl Monitor {
    pub fn sample(&mut self) -> Snapshot {
        let now = Instant::now();
        let cpu_now = cpu_times();
        let (rx, tx, ifaces) = net_totals();
        let io = io_totals();
        let procs_now = proc_times();

        let mut snap = Snapshot::default();
        snap.host = read("/proc/sys/kernel/hostname").trim().to_string();
        snap.kernel = read("/proc/sys/kernel/osrelease").trim().to_string();
        snap.os = os_name();
        snap.uptime_secs = read("/proc/uptime").split_whitespace().next().and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0) as u64;
        snap.memory = memory();
        snap.disks = mounts();
        snap.net.interfaces = ifaces;
        snap.battery = battery();

        let info = read("/proc/cpuinfo");
        snap.cpu.model = info.lines().find(|l| l.starts_with("model name")).and_then(|l| l.split_once(':')).map(|x| x.1.trim().to_string()).unwrap_or_default();
        snap.cpu.cores = info.lines().filter(|l| l.starts_with("processor")).count().max(1);
        snap.cpu.freq_mhz = {
            let v: Vec<f32> = info.lines().filter(|l| l.starts_with("cpu MHz")).filter_map(|l| l.split_once(':')?.1.trim().parse().ok()).collect();
            if v.is_empty() { None } else { Some((v.iter().sum::<f32>() / v.len() as f32) as u32) }
        };
        let la: Vec<f32> = read("/proc/loadavg").split_whitespace().take(3).filter_map(|x| x.parse().ok()).collect();
        if la.len() == 3 {
            snap.cpu.load = [la[0], la[1], la[2]];
        }
        snap.cpu.temp_c = cpu_temp();

        let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as u64;
        if let Some(prev) = &self.last {
            let dt = now.duration_since(prev.at).as_secs_f64().max(0.001);
            let pct = |a: (u64, u64), b: (u64, u64)| {
                let (db, dtot) = (b.0.saturating_sub(a.0), b.1.saturating_sub(a.1));
                if dtot == 0 { 0.0 } else { db as f32 / dtot as f32 * 100.0 }
            };
            if let (Some(a), Some(b)) = (prev.cpu.first(), cpu_now.first()) {
                snap.cpu.usage = pct(*a, *b);
            }
            for i in 1..cpu_now.len().min(prev.cpu.len()) {
                snap.cpu.per_core.push(pct(prev.cpu[i], cpu_now[i]));
            }
            snap.net.down_bps = rx.saturating_sub(prev.net.0) as f64 / dt;
            snap.net.up_bps = tx.saturating_sub(prev.net.1) as f64 / dt;
            snap.io.read_bps = io.0.saturating_sub(prev.io.0) as f64 / dt;
            snap.io.write_bps = io.1.saturating_sub(prev.io.1) as f64 / dt;
            let mut list: Vec<Proc> = procs_now
                .iter()
                .map(|(pid, (ticks, name, rss))| {
                    let before = prev.procs.get(pid).copied().unwrap_or(*ticks);
                    let cpu = (ticks.saturating_sub(before)) as f64 / hz as f64 / dt * 100.0;
                    Proc { pid: *pid, name: name.clone(), cpu: cpu as f32, rss: *rss }
                })
                .collect();
            list.sort_by(|a, b| b.rss.cmp(&a.rss));
            snap.top_mem = list.iter().take(6).cloned().collect();
            list.sort_by(|a, b| b.cpu.partial_cmp(&a.cpu).unwrap_or(std::cmp::Ordering::Equal));
            snap.top_cpu = list.into_iter().take(6).collect();
        } else {
            snap.cpu.per_core = vec![0.0; cpu_now.len().saturating_sub(1)];
        }
        let (h, notes) = health(&snap);
        snap.health = h;
        snap.health_notes = notes;
        self.last = Some(Prev {
            at: now,
            cpu: cpu_now,
            net: (rx, tx),
            io,
            procs: procs_now.into_iter().map(|(k, v)| (k, v.0)).collect(),
        });
        snap
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_this_machine() {
        let mut m = Monitor::default();
        let a = m.sample();
        assert!(a.memory.total > 0);
        assert!(!a.disks.is_empty());
        std::thread::sleep(std::time::Duration::from_millis(300));
        let b = m.sample();
        assert!(b.cpu.usage >= 0.0 && b.cpu.usage <= 100.0);
        assert!(b.health <= 100);
        assert!(!b.top_mem.is_empty());
    }
}
