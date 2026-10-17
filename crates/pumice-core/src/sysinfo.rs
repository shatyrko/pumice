//! Live system readings straight from /proc and /sys (no extra crate).
//!
//! This is a port of the collector of the System Stats widget of the bar
//! (Noctalia plugin): per-core CPU with P/E cores, clocks, RAPL watts, every
//! temperature and fan, GPU, battery, disks, network with Wi-Fi, grouped
//! processes, and a history for graphs. `Monitor::sample` is meant to be
//! called on a timer; rates are computed from the previous call.

use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fs;
use std::time::Instant;

pub const HISTORY: usize = 90;

#[derive(Serialize, Clone, Debug, Default)]
pub struct SystemInfo {
    pub host: String,
    pub kernel: String,
    pub os: String,
    pub uptime_secs: u64,
    pub model: String,
    pub ncpu: usize,
    pub p_count: usize,
    pub e_count: usize,
    pub hybrid: bool,
    /// "P", "E" or "" per logical CPU, in /proc/stat order.
    pub core_kinds: Vec<String>,
    pub max_freq: f32,
    pub e_max_freq: f32,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Cpu {
    pub usage: Option<f32>,
    pub cores: Vec<f32>,
    pub freq_p: Option<f32>,
    pub freq_e: Option<f32>,
    pub freq_max: Option<f32>,
    pub temp: Option<f32>,
    pub hottest: Option<f32>,
    pub load: [f32; 3],
    pub threads: u64,
    pub psi: Option<f32>,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Mem {
    pub total_mb: f64,
    pub avail_mb: f64,
    pub used_mb: f64,
    pub used_pct: f64,
    pub cached_mb: f64,
    pub shared_mb: f64,
    pub dirty_mb: f64,
    pub swap_total_mb: f64,
    pub swap_used_mb: f64,
    pub swap_pct: f64,
    pub psi: Option<f32>,
    pub io_psi: Option<f32>,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Battery {
    pub pct: Option<f32>,
    pub status: String,
    pub cycles: Option<u32>,
    pub watts: Option<f32>,
    pub now_wh: f32,
    pub full_wh: f32,
    pub design_wh: f32,
    pub health: Option<f32>,
    pub hours_left: Option<f32>,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Power {
    pub rapl: BTreeMap<String, f32>,
    pub cpu_w: Option<f32>,
    pub rapl_available: bool,
    pub battery: Option<Battery>,
    pub ac: bool,
    pub profile: Option<String>,
    pub epp: Option<String>,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Nvidia {
    pub present: bool,
    pub awake: bool,
    pub status: Option<String>,
    pub name: Option<String>,
    pub temp: Option<f32>,
    pub util: Option<f32>,
    pub watts: Option<f32>,
    pub vram_used_mb: Option<f32>,
    pub vram_total_mb: Option<f32>,
    pub clock: Option<f32>,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Gpu {
    pub igpu_freq: Option<f32>,
    pub igpu_max: Option<f32>,
    pub nvidia: Nvidia,
}

#[derive(Serialize, Clone, Debug)]
pub struct Temp {
    pub label: String,
    pub c: f32,
    pub kind: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct Fan {
    pub label: String,
    pub rpm: f32,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Mount {
    pub path: String,
    pub source: String,
    pub fs: String,
    pub total: u64,
    pub used: u64,
    pub pct: f32,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct DiskIo {
    pub name: String,
    pub block: String,
    pub read_bps: f64,
    pub write_bps: f64,
    pub temp: Option<f32>,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Disks {
    pub mounts: Vec<Mount>,
    pub io: Vec<DiskIo>,
    pub read_bps: f64,
    pub write_bps: f64,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Iface {
    pub name: String,
    pub up: bool,
    pub wireless: bool,
    pub ip: Option<String>,
    pub rx_bps: f64,
    pub tx_bps: f64,
    pub rx_total: u64,
    pub tx_total: u64,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Wifi {
    pub connected: bool,
    pub ssid: Option<String>,
    pub signal: Option<f32>,
    pub freq: Option<f32>,
    pub rx_rate: Option<f32>,
    pub tx_rate: Option<f32>,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Net {
    pub ifaces: Vec<Iface>,
    pub rx_bps: f64,
    pub tx_bps: f64,
    pub wifi: Option<Wifi>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Proc {
    pub name: String,
    pub pid: u32,
    /// Share of the whole CPU, 0..100.
    pub cpu: f32,
    pub mem_mb: f64,
    pub count: u32,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Procs {
    pub cpu: Vec<Proc>,
    pub mem: Vec<Proc>,
    pub count: usize,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Snapshot {
    pub ok: bool,
    pub system: SystemInfo,
    pub cpu: Cpu,
    pub mem: Mem,
    pub power: Power,
    pub gpu: Gpu,
    pub temps: Vec<Temp>,
    pub fans: Vec<Fan>,
    pub max_rpm: f32,
    pub disks: Disks,
    pub net: Net,
    pub procs: Procs,
    pub health: u32,
    pub health_notes: Vec<String>,
    pub hist: HashMap<String, Vec<f32>>,
}

// ---------- small readers ----------

fn read(path: &str) -> Option<String> {
    fs::read_to_string(path).ok()
}
fn read_num(path: &str) -> Option<f64> {
    let s = read(path)?;
    let t: String = s.trim().chars().take_while(|c| c.is_ascii_digit() || *c == '-' || *c == '.').collect();
    t.parse().ok()
}
fn read_str(path: &str) -> Option<String> {
    read(path).map(|s| s.trim().to_string())
}
fn list(path: &str) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(path).map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
    v.sort();
    v
}
fn exists(path: &str) -> bool {
    std::path::Path::new(path).exists()
}

/// "0-7,9,12-13" -> set of numbers.
fn parse_range(text: &str) -> HashSet<usize> {
    let mut set = HashSet::new();
    for part in text.split([',', ' ', '\n']).filter(|p| !p.is_empty()) {
        if let Some((a, b)) = part.split_once('-') {
            if let (Ok(a), Ok(b)) = (a.parse::<usize>(), b.parse::<usize>()) {
                set.extend(a..=b);
            }
        } else if let Ok(n) = part.parse() {
            set.insert(n);
        }
    }
    set
}

fn psi(kind: &str) -> Option<f32> {
    let s = read(&format!("/proc/pressure/{kind}"))?;
    let line = s.lines().find(|l| l.starts_with("some"))?;
    line.split_whitespace().find_map(|p| p.strip_prefix("avg10=")).and_then(|v| v.parse().ok())
}

// ---------- hardware discovery ----------

#[derive(Default)]
struct Hw {
    ncpu: usize,
    freq_files: Vec<Option<String>>,
    p_set: HashSet<usize>,
    e_set: HashSet<usize>,
    hybrid: bool,
    core_kinds: Vec<String>,
    max_freq: f32,
    e_max_freq: f32,
    model: String,
    pkg_temp: Option<String>,
    core_temps: Vec<String>,
    temps: Vec<(String, String, String)>, // label, path, kind
    fans: Vec<(String, String)>,
    nvme: Vec<(String, String, String, Option<String>)>, // ctrl, block, model, temp path
    rapl: BTreeMap<String, (String, f64)>,
    rapl_readable: bool,
    battery: Option<String>,
    ac: Option<String>,
    igpu: Option<(String, Option<f64>)>,
    nvidia: Option<String>,
    wifi: Option<String>,
}

fn friendly(name: &str) -> String {
    match name {
        "iwlwifi" => "Wi-Fi card",
        "r8169" => "Ethernet chip",
        "acpitz" => "Board (ACPI)",
        "amdgpu" => "AMD GPU",
        "thinkpad" => "ThinkPad EC",
        _ => name,
    }
    .to_string()
}

fn short_model(m: &str) -> String {
    let words: Vec<&str> = m.split(|c: char| c == '_' || c.is_whitespace()).filter(|w| !w.is_empty() && *w != "PC").take(2).collect();
    if words.is_empty() { m.to_string() } else { words.join(" ") }
}

fn discover() -> Hw {
    let mut h = Hw::default();
    for name in list("/sys/devices/system/cpu") {
        if let Some(idx) = name.strip_prefix("cpu").and_then(|n| n.parse::<usize>().ok()) {
            h.ncpu = h.ncpu.max(idx + 1);
        }
    }
    h.freq_files = (0..h.ncpu).map(|i| Some(format!("/sys/devices/system/cpu/cpu{i}/cpufreq/scaling_cur_freq"))).collect();
    h.p_set = parse_range(&read_str("/sys/devices/cpu_core/cpus").unwrap_or_default());
    h.e_set = parse_range(&read_str("/sys/devices/cpu_atom/cpus").unwrap_or_default());
    h.hybrid = !h.p_set.is_empty() && !h.e_set.is_empty();
    h.core_kinds = (0..h.ncpu).map(|i| if !h.hybrid { String::new() } else if h.e_set.contains(&i) { "E".into() } else { "P".into() }).collect();
    h.max_freq = read_num("/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq").unwrap_or(0.0) as f32 / 1000.0;
    if h.hybrid {
        if let Some(i) = (0..h.ncpu).find(|i| h.e_set.contains(i)) {
            h.e_max_freq = read_num(&format!("/sys/devices/system/cpu/cpu{i}/cpufreq/cpuinfo_max_freq")).unwrap_or(0.0) as f32 / 1000.0;
        }
    }
    if let Some(info) = read("/proc/cpuinfo") {
        if let Some(l) = info.lines().find(|l| l.starts_with("model name")) {
            h.model = l.split_once(':').map(|x| x.1.trim().replace("(R)", "").replace("(TM)", "")).unwrap_or_default();
        }
    }

    let mut dimm = 0;
    let mut nvme_temp: HashMap<String, String> = HashMap::new();
    let mut asus_fans = false;
    for d in list("/sys/class/hwmon") {
        let dir = format!("/sys/class/hwmon/{d}");
        let name = read_str(&format!("{dir}/name")).unwrap_or_else(|| d.clone());
        let files = list(&dir);
        match name.as_str() {
            "coretemp" => {
                for f in &files {
                    if let Some(n) = f.strip_prefix("temp").and_then(|x| x.strip_suffix("_input")) {
                        let label = read_str(&format!("{dir}/temp{n}_label")).unwrap_or_default();
                        if label.starts_with("Package") {
                            h.pkg_temp = Some(format!("{dir}/{f}"));
                        } else {
                            h.core_temps.push(format!("{dir}/{f}"));
                        }
                    }
                }
            }
            "k10temp" | "zenpower" => {
                if exists(&format!("{dir}/temp1_input")) {
                    h.pkg_temp = Some(format!("{dir}/temp1_input"));
                }
            }
            "nvme" => {
                let ctrl = read_str(&format!("{dir}/device/uevent")).and_then(|u| u.lines().find_map(|l| l.strip_prefix("DEVNAME=").map(|s| s.to_string())));
                if let Some(c) = ctrl {
                    nvme_temp.insert(c, format!("{dir}/temp1_input"));
                }
            }
            "spd5118" | "jc42" => {
                dimm += 1;
                h.temps.push((format!("RAM stick {dimm}"), format!("{dir}/temp1_input"), "ram".into()));
            }
            "BAT0" | "BAT1" | "ADP0" | "asus_custom_fan_curve" | "ucsi_source_psy_USBC000:001" => {}
            _ => {
                // Fans of this chip, then its first temperature.
                let mut fan_here = false;
                for f in &files {
                    if let Some(n) = f.strip_prefix("fan").and_then(|x| x.strip_suffix("_input")) {
                        let label = read_str(&format!("{dir}/fan{n}_label"));
                        let nice = match label.as_deref() { Some("cpu_fan") => "CPU fan".to_string(), Some("gpu_fan") => "GPU fan".to_string(), Some("mid_fan") => "Middle fan".to_string(), Some(l) => l.to_string(), None => format!("Fan {n}") };
                        if name == "asus" {
                            if !asus_fans { h.fans.clear(); asus_fans = true; }
                            h.fans.push((nice, format!("{dir}/{f}")));
                        } else if !asus_fans {
                            h.fans.push((nice, format!("{dir}/{f}")));
                        }
                        fan_here = true;
                    }
                }
                let base = name.split('_').next().unwrap_or(&name).to_string();
                if !fan_here || name != "asus" {
                    if exists(&format!("{dir}/temp1_input")) && name != "acpi_fan" && name != "asus" {
                        h.temps.push((friendly(&base).to_string(), format!("{dir}/temp1_input"), "other".into()));
                    }
                }
            }
        }
    }
    for c in list("/sys/class/nvme") {
        let model = short_model(&read_str(&format!("/sys/class/nvme/{c}/model")).unwrap_or_else(|| c.clone()));
        let block = list(&format!("/sys/class/nvme/{c}")).into_iter().rev().find(|b| b.starts_with(&format!("{c}n")) && b[c.len() + 1..].chars().all(|x| x.is_ascii_digit())).unwrap_or_else(|| format!("{c}n1"));
        let t = nvme_temp.get(&c).cloned();
        h.nvme.push((c, block, model, t));
    }
    for d in list("/sys/class/powercap") {
        let is_pkg = d.starts_with("intel-rapl:");
        if is_pkg {
            let dir = format!("/sys/class/powercap/{d}");
            if let Some(name) = read_str(&format!("{dir}/name")) {
                h.rapl.entry(name).or_insert((format!("{dir}/energy_uj"), read_num(&format!("{dir}/max_energy_range_uj")).unwrap_or(0.0)));
            }
        }
    }
    h.rapl_readable = h.rapl.get("package-0").map(|r| read_num(&r.0).is_some()).unwrap_or(false);
    for p in list("/sys/class/power_supply") {
        let dir = format!("/sys/class/power_supply/{p}");
        match read_str(&format!("{dir}/type")).as_deref() {
            Some("Battery") if h.battery.is_none() && read_str(&format!("{dir}/scope")).as_deref() != Some("Device") => h.battery = Some(dir),
            Some("Mains") => h.ac = Some(dir),
            _ => {}
        }
    }
    for c in list("/sys/class/drm") {
        if c.starts_with("card") && c[4..].chars().all(|x| x.is_ascii_digit()) {
            let dir = format!("/sys/class/drm/{c}");
            if exists(&format!("{dir}/gt_act_freq_mhz")) {
                h.igpu = Some((format!("{dir}/gt_act_freq_mhz"), read_num(&format!("{dir}/gt_RP0_freq_mhz")).or_else(|| read_num(&format!("{dir}/gt_max_freq_mhz")))));
            } else if exists(&format!("{dir}/device/tile0/gt0/freq0/act_freq")) {
                let f = format!("{dir}/device/tile0/gt0/freq0/");
                h.igpu = Some((format!("{f}act_freq"), read_num(&format!("{f}rp0_freq")).or_else(|| read_num(&format!("{f}max_freq")))));
            }
        }
    }
    for d in list("/sys/bus/pci/devices") {
        let dir = format!("/sys/bus/pci/devices/{d}");
        if read_str(&format!("{dir}/vendor")).as_deref() == Some("0x10de") && read_str(&format!("{dir}/class")).map(|c| c.starts_with("0x03")).unwrap_or(false) {
            h.nvidia = Some(dir);
        }
    }
    for n in list("/sys/class/net") {
        if exists(&format!("/sys/class/net/{n}/wireless")) {
            h.wifi = Some(n);
        }
    }
    h
}

// ---------- monitor ----------

type Jiffies = (u64, u64); // idle, total

#[derive(Default)]
pub struct Monitor {
    hw: Option<Hw>,
    last_discover: Option<Instant>,
    t: Option<Instant>,
    cpu: Option<Jiffies>,
    cores: Vec<Jiffies>,
    rapl: HashMap<String, f64>,
    disk: HashMap<String, (f64, f64)>,
    net: HashMap<String, (u64, u64)>,
    procs: HashMap<u32, u64>,
    hist: HashMap<String, VecDeque<f32>>,
    disk_usage: Vec<Mount>,
    tick: u64,
    wifi: Option<Wifi>,
    ips: HashMap<String, String>,
    last_slow: Option<Instant>,
    nvidia: Option<Nvidia>,
    last_nv: Option<Instant>,
}

fn jiffies(fields: &str) -> Jiffies {
    let v: Vec<u64> = fields.split_whitespace().filter_map(|x| x.parse().ok()).collect();
    let idle = v.get(3).copied().unwrap_or(0) + v.get(4).copied().unwrap_or(0);
    (idle, v.iter().take(8).sum())
}

fn busy(prev: Jiffies, now: Jiffies) -> Option<f32> {
    let (dt, di) = (now.1.saturating_sub(prev.1), now.0.saturating_sub(prev.0));
    if dt == 0 { None } else { Some((100.0 * (1.0 - di as f64 / dt as f64)).clamp(0.0, 100.0) as f32) }
}

fn mounts() -> Vec<Mount> {
    let mut out: Vec<Mount> = Vec::new();
    let mut seen = HashSet::new();
    for line in read("/proc/mounts").unwrap_or_default().lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 3 {
            continue;
        }
        let (dev, mount, fs_type) = (f[0], f[1].replace("\\040", " "), f[2]);
        if !dev.starts_with("/dev/") || dev.starts_with("/dev/loop") || mount.starts_with("/var/lib/docker") || mount.starts_with("/snap") || mount.starts_with("/run/user") {
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
        let used = total.saturating_sub(st.f_bfree as u64 * bs);
        if total == 0 {
            continue;
        }
        out.push(Mount { path: mount, source: dev.into(), fs: fs_type.into(), total, used, pct: (100.0 * used as f64 / total as f64) as f32 });
    }
    out
}

fn proc_table() -> HashMap<u32, (u64, String, u64)> {
    let mut map = HashMap::new();
    let page = 4096u64;
    let Ok(rd) = fs::read_dir("/proc") else { return map };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Ok(pid) = name.parse::<u32>() else { continue };
        let Ok(stat) = fs::read_to_string(e.path().join("stat")) else { continue };
        let (Some(open), Some(close)) = (stat.find('('), stat.rfind(')')) else { continue };
        let comm = stat[open + 1..close].to_string();
        let rest: Vec<&str> = stat[close + 2..].split_whitespace().collect();
        if rest.len() < 22 {
            continue;
        }
        let ticks: u64 = rest[11].parse::<u64>().unwrap_or(0) + rest[12].parse::<u64>().unwrap_or(0);
        let rss: u64 = rest[21].parse::<u64>().unwrap_or(0) * page;
        map.insert(pid, (ticks, comm, rss));
    }
    map
}

fn os_name() -> String {
    read("/etc/os-release")
        .and_then(|t| t.lines().find_map(|l| l.strip_prefix("PRETTY_NAME=").map(|v| v.trim_matches('"').to_string())))
        .unwrap_or_else(|| "Linux".into())
}

fn command_out(program: &str, args: &[&str]) -> Option<String> {
    crate::util::run(program, args)
}

impl Monitor {
    fn push(&mut self, name: &str, v: Option<f32>) {
        let h = self.hist.entry(name.to_string()).or_default();
        h.push_back(v.unwrap_or(0.0));
        while h.len() > HISTORY {
            h.pop_front();
        }
    }

    fn refresh_slow(&mut self) {
        let hw = self.hw.as_ref().unwrap();
        if let Some(w) = &hw.wifi {
            self.wifi = command_out("iw", &["dev", w, "link"]).map(|o| {
                if o.contains("Not connected") {
                    return Wifi { connected: false, ..Default::default() };
                }
                let grab = |key: &str| o.lines().find_map(|l| l.trim().strip_prefix(key).map(|v| v.trim().to_string()));
                let num = |key: &str| grab(key).and_then(|v| v.split_whitespace().next().and_then(|n| n.parse::<f32>().ok()));
                Wifi { connected: true, ssid: grab("SSID:"), signal: num("signal:"), freq: num("freq:"), rx_rate: num("rx bitrate:"), tx_rate: num("tx bitrate:") }
            });
        }
        if let Some(o) = command_out("ip", &["-br", "-4", "addr"]) {
            self.ips = o
                .lines()
                .filter_map(|l| {
                    let f: Vec<&str> = l.split_whitespace().collect();
                    if f.len() >= 3 { Some((f[0].to_string(), f[2].split('/').next()?.to_string())) } else { None }
                })
                .collect();
        }
    }

    fn refresh_nvidia(&mut self) {
        let Some(dir) = self.hw.as_ref().and_then(|h| h.nvidia.clone()) else { return };
        // Never wake a sleeping GPU just to read it.
        if read_str(&format!("{dir}/power/runtime_status")).as_deref() != Some("active") {
            return;
        }
        let Some(o) = command_out("nvidia-smi", &["--query-gpu=name,temperature.gpu,utilization.gpu,power.draw,memory.used,memory.total,clocks.gr", "--format=csv,noheader,nounits"]) else { return };
        let f: Vec<&str> = o.trim().split(',').map(|s| s.trim()).collect();
        if f.len() >= 7 {
            let n = |i: usize| f[i].parse::<f32>().ok();
            self.nvidia = Some(Nvidia { present: true, awake: true, status: Some("active".into()), name: Some(f[0].to_string()), temp: n(1), util: n(2), watts: n(3), vram_used_mb: n(4), vram_total_mb: n(5), clock: n(6) });
        }
    }

    /// `detailed` is true while the dashboard is on screen: only then the
    /// NVIDIA GPU is queried (it is never woken for the sake of a reading).
    pub fn sample(&mut self, detailed: bool) -> Snapshot {
        let now = Instant::now();
        if self.hw.is_none() || self.last_discover.map(|t| now.duration_since(t).as_secs() > 60).unwrap_or(true) {
            self.hw = Some(discover());
            self.last_discover = Some(now);
        }
        if self.last_slow.map(|t| now.duration_since(t).as_secs() >= 10).unwrap_or(true) {
            self.refresh_slow();
            self.last_slow = Some(now);
        }
        if detailed && self.last_nv.map(|t| now.duration_since(t).as_secs() >= 4).unwrap_or(true) {
            self.refresh_nvidia();
            self.last_nv = Some(now);
        }
        let dt = self.t.map(|t| now.duration_since(t).as_secs_f64()).filter(|d| *d > 0.0);
        self.t = Some(now);
        self.tick += 1;

        // ----- CPU -----
        let mut cpu = Cpu::default();
        {
            let hw = self.hw.as_ref().unwrap();
            let stat = read("/proc/stat").unwrap_or_default();
            let mut cores_now: Vec<Jiffies> = Vec::new();
            for line in stat.lines().filter(|l| l.starts_with("cpu")) {
                let (name, rest) = line.split_once(' ').unwrap_or((line, ""));
                let j = jiffies(rest);
                if name == "cpu" {
                    if let Some(p) = self.cpu {
                        cpu.usage = busy(p, j);
                    }
                    self.cpu = Some(j);
                } else {
                    cores_now.push(j);
                }
            }
            if self.cores.len() == cores_now.len() {
                cpu.cores = cores_now.iter().zip(&self.cores).map(|(n, p)| busy(*p, *n).unwrap_or(0.0)).collect();
            } else {
                cpu.cores = vec![0.0; cores_now.len()];
            }
            self.cores = cores_now;

            let (mut sp, mut np, mut se, mut ne, mut fastest) = (0.0f32, 0, 0.0f32, 0, 0.0f32);
            for i in 0..hw.ncpu {
                if let Some(f) = hw.freq_files[i].as_ref().and_then(|p| read_num(p)) {
                    let f = f as f32 / 1000.0;
                    fastest = fastest.max(f);
                    if hw.hybrid && hw.e_set.contains(&i) { se += f; ne += 1 } else { sp += f; np += 1 }
                }
            }
            cpu.freq_p = if np > 0 { Some(sp / np as f32) } else { None };
            cpu.freq_e = if ne > 0 { Some(se / ne as f32) } else { None };
            cpu.freq_max = if fastest > 0.0 { Some(fastest) } else { None };
            cpu.temp = hw.pkg_temp.as_ref().and_then(|p| read_num(p)).map(|v| v as f32 / 1000.0);
            cpu.hottest = hw.core_temps.iter().filter_map(|p| read_num(p)).fold(None, |a: Option<f64>, v| Some(a.map_or(v, |x| x.max(v)))).map(|v| v as f32 / 1000.0);
            if cpu.temp.is_none() {
                cpu.temp = cpu.hottest;
            }
            if let Some(la) = read("/proc/loadavg") {
                let p: Vec<&str> = la.split_whitespace().collect();
                for i in 0..3 {
                    cpu.load[i] = p.get(i).and_then(|x| x.parse().ok()).unwrap_or(0.0);
                }
                cpu.threads = p.get(3).and_then(|x| x.split('/').nth(1)).and_then(|x| x.parse().ok()).unwrap_or(0);
            }
            cpu.psi = psi("cpu");
        }

        // ----- memory -----
        let mem = {
            let text = read("/proc/meminfo").unwrap_or_default();
            let kb = |key: &str| -> f64 {
                text.lines().find_map(|l| l.strip_prefix(key).and_then(|r| r.strip_prefix(':')).and_then(|r| r.split_whitespace().next()).and_then(|v| v.parse().ok())).unwrap_or(0.0)
            };
            let (total, avail) = (kb("MemTotal"), kb("MemAvailable"));
            let (st, sf) = (kb("SwapTotal"), kb("SwapFree"));
            Mem {
                total_mb: total / 1024.0,
                avail_mb: avail / 1024.0,
                used_mb: (total - avail) / 1024.0,
                used_pct: if total > 0.0 { 100.0 * (total - avail) / total } else { 0.0 },
                cached_mb: (kb("Cached") + kb("Buffers")) / 1024.0,
                shared_mb: kb("Shmem") / 1024.0,
                dirty_mb: kb("Dirty") / 1024.0,
                swap_total_mb: st / 1024.0,
                swap_used_mb: (st - sf) / 1024.0,
                swap_pct: if st > 0.0 { 100.0 * (st - sf) / st } else { 0.0 },
                psi: psi("memory"),
                io_psi: psi("io"),
            }
        };

        // ----- power -----
        let mut power = Power::default();
        {
            let hw = self.hw.as_ref().unwrap();
            power.rapl_available = hw.rapl_readable;
            if hw.rapl_readable {
                if let Some(dt) = dt {
                    for (name, (path, range)) in &hw.rapl {
                        if let Some(e) = read_num(path) {
                            if let Some(last) = self.rapl.get(name) {
                                let mut d = e - last;
                                if d < 0.0 {
                                    d += range;
                                }
                                power.rapl.insert(name.clone(), (d / 1e6 / dt) as f32);
                            }
                            self.rapl.insert(name.clone(), e);
                        }
                    }
                } else {
                    for (name, (path, _)) in &hw.rapl {
                        if let Some(e) = read_num(path) {
                            self.rapl.insert(name.clone(), e);
                        }
                    }
                }
            }
            power.cpu_w = power.rapl.get("package-0").copied();
            if let Some(b) = &hw.battery {
                let volt = read_num(&format!("{b}/voltage_now")).unwrap_or(0.0);
                let pw = read_num(&format!("{b}/power_now")).or_else(|| read_num(&format!("{b}/current_now")).map(|c| c * volt / 1e6));
                let wh = |energy: &str, charge: &str| -> f64 {
                    read_num(&format!("{b}/{energy}")).map(|e| e / 1e6).or_else(|| read_num(&format!("{b}/{charge}")).map(|c| c * volt / 1e12)).unwrap_or(0.0)
                };
                let (now_wh, full_wh, design_wh) = (wh("energy_now", "charge_now"), wh("energy_full", "charge_full"), wh("energy_full_design", "charge_full_design"));
                let watts = pw.map(|p| (p / 1e6) as f32);
                let status = read_str(&format!("{b}/status")).unwrap_or_else(|| "Unknown".into());
                let hours_left = watts.filter(|w| *w > 0.3).and_then(|w| match status.as_str() {
                    "Discharging" => Some(now_wh as f32 / w),
                    "Charging" => Some((full_wh - now_wh).max(0.0) as f32 / w),
                    _ => None,
                });
                power.battery = Some(Battery {
                    pct: read_num(&format!("{b}/capacity")).map(|v| v as f32),
                    status,
                    cycles: read_num(&format!("{b}/cycle_count")).map(|v| v as u32).filter(|c| *c > 0),
                    watts,
                    now_wh: now_wh as f32,
                    full_wh: full_wh as f32,
                    design_wh: design_wh as f32,
                    health: if design_wh > 0.0 { Some((100.0 * full_wh / design_wh) as f32) } else { None },
                    hours_left,
                });
            }
            power.ac = hw.ac.as_ref().and_then(|a| read_num(&format!("{a}/online"))) == Some(1.0);
            power.profile = read_str("/sys/firmware/acpi/platform_profile");
            power.epp = read_str("/sys/devices/system/cpu/cpu0/cpufreq/energy_performance_preference");
        }

        // ----- GPU -----
        let gpu = {
            let hw = self.hw.as_ref().unwrap();
            let mut g = Gpu::default();
            if let Some((act, max)) = &hw.igpu {
                g.igpu_freq = read_num(act).map(|v| v as f32);
                g.igpu_max = max.map(|v| v as f32);
            }
            if let Some(dir) = &hw.nvidia {
                let rs = read_str(&format!("{dir}/power/runtime_status"));
                let awake = rs.as_deref() == Some("active");
                g.nvidia = Nvidia { present: true, awake, status: rs, ..Default::default() };
                if awake {
                    if let Some(n) = &self.nvidia {
                        g.nvidia = Nvidia { present: true, awake, status: g.nvidia.status.clone(), ..n.clone() };
                    }
                }
            }
            g
        };

        // ----- sensors -----
        let (mut temps, mut fans, mut max_rpm) = (Vec::new(), Vec::new(), 0.0f32);
        {
            let hw = self.hw.as_ref().unwrap();
            for (label, path, kind) in &hw.temps {
                if let Some(v) = read_num(path) {
                    temps.push(Temp { label: label.clone(), c: v as f32 / 1000.0, kind: kind.clone() });
                }
            }
            for (_, _, model, tp) in &hw.nvme {
                if let Some(v) = tp.as_ref().and_then(|p| read_num(p)) {
                    temps.push(Temp { label: format!("SSD {model}"), c: v as f32 / 1000.0, kind: "ssd".into() });
                }
            }
            for (label, path) in &hw.fans {
                if let Some(v) = read_num(path) {
                    fans.push(Fan { label: label.clone(), rpm: v as f32 });
                    max_rpm = max_rpm.max(v as f32);
                }
            }
        }

        // ----- disks -----
        let mut disks = Disks::default();
        {
            if self.tick % 10 == 1 {
                self.disk_usage = mounts();
            }
            disks.mounts = self.disk_usage.clone();
            let hw = self.hw.as_ref().unwrap();
            let ds = read("/proc/diskstats").unwrap_or_default();
            let mut new_disk: HashMap<String, (f64, f64)> = HashMap::new();
            for (_, block, model, tp) in &hw.nvme {
                let line = ds.lines().find(|l| l.split_whitespace().nth(2) == Some(block.as_str()));
                let Some(line) = line else { continue };
                let f: Vec<&str> = line.split_whitespace().collect();
                if f.len() < 10 {
                    continue;
                }
                let (r, w) = (f[5].parse::<f64>().unwrap_or(0.0) * 512.0, f[9].parse::<f64>().unwrap_or(0.0) * 512.0);
                let mut item = DiskIo { name: model.clone(), block: block.clone(), temp: tp.as_ref().and_then(|p| read_num(p)).map(|v| v as f32 / 1000.0), ..Default::default() };
                if let (Some(p), Some(dt)) = (self.disk.get(block), dt) {
                    item.read_bps = ((r - p.0) / dt).max(0.0);
                    item.write_bps = ((w - p.1) / dt).max(0.0);
                    disks.read_bps += item.read_bps;
                    disks.write_bps += item.write_bps;
                }
                new_disk.insert(block.clone(), (r, w));
                disks.io.push(item);
            }
            // SATA and virtual disks, when there is no NVMe drive in the list.
            for line in ds.lines() {
                let f: Vec<&str> = line.split_whitespace().collect();
                if f.len() < 10 {
                    continue;
                }
                let n = f[2];
                let whole = (n.starts_with("sd") || n.starts_with("vd")) && !n.ends_with(|c: char| c.is_ascii_digit());
                if !whole {
                    continue;
                }
                let (r, w) = (f[5].parse::<f64>().unwrap_or(0.0) * 512.0, f[9].parse::<f64>().unwrap_or(0.0) * 512.0);
                let mut item = DiskIo { name: n.to_string(), block: n.to_string(), ..Default::default() };
                if let (Some(p), Some(dt)) = (self.disk.get(n), dt) {
                    item.read_bps = ((r - p.0) / dt).max(0.0);
                    item.write_bps = ((w - p.1) / dt).max(0.0);
                    disks.read_bps += item.read_bps;
                    disks.write_bps += item.write_bps;
                }
                new_disk.insert(n.to_string(), (r, w));
                disks.io.push(item);
            }
            self.disk = new_disk;
        }

        // ----- network -----
        let mut net = Net::default();
        {
            let hw = self.hw.as_ref().unwrap();
            let text = read("/proc/net/dev").unwrap_or_default();
            let mut new_net = HashMap::new();
            for line in text.lines().skip(2) {
                let Some((name, rest)) = line.split_once(':') else { continue };
                let name = name.trim();
                if name == "lo" || name.starts_with("veth") || name.starts_with("br-") || name.starts_with("docker") {
                    continue;
                }
                let v: Vec<u64> = rest.split_whitespace().filter_map(|x| x.parse().ok()).collect();
                if v.len() < 9 {
                    continue;
                }
                let (rx, tx) = (v[0], v[8]);
                let up = read_str(&format!("/sys/class/net/{name}/operstate")).as_deref() == Some("up");
                let mut it = Iface { name: name.into(), up, wireless: hw.wifi.as_deref() == Some(name), ip: self.ips.get(name).cloned(), rx_total: rx, tx_total: tx, ..Default::default() };
                if let (Some(p), Some(dt)) = (self.net.get(name), dt) {
                    it.rx_bps = (rx.saturating_sub(p.0)) as f64 / dt;
                    it.tx_bps = (tx.saturating_sub(p.1)) as f64 / dt;
                    net.rx_bps += it.rx_bps;
                    net.tx_bps += it.tx_bps;
                }
                new_net.insert(name.to_string(), (rx, tx));
                if it.up || it.rx_bps > 0.0 {
                    net.ifaces.push(it);
                }
            }
            self.net = new_net;
            net.wifi = self.wifi.clone();
        }

        // ----- processes -----
        let mut procs = Procs::default();
        {
            let ncpu = self.hw.as_ref().unwrap().ncpu.max(1) as f64;
            let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as f64;
            let table = proc_table();
            if let Some(dt) = dt {
                let mut groups: HashMap<String, Proc> = HashMap::new();
                for (pid, (ticks, name, rss)) in &table {
                    if *rss == 0 {
                        continue; // kernel threads are not programs
                    }
                    let before = self.procs.get(pid).copied().unwrap_or(*ticks);
                    let cpu = (ticks.saturating_sub(before)) as f64 / hz / dt * 100.0 / ncpu;
                    let g = groups.entry(name.clone()).or_insert(Proc { name: name.clone(), pid: *pid, cpu: 0.0, mem_mb: 0.0, count: 0 });
                    g.cpu += cpu as f32;
                    g.mem_mb += *rss as f64 / 1_048_576.0;
                    g.count += 1;
                }
                let mut v: Vec<Proc> = groups.into_values().collect();
                procs.count = v.len();
                v.sort_by(|a, b| b.cpu.partial_cmp(&a.cpu).unwrap_or(std::cmp::Ordering::Equal));
                procs.cpu = v.iter().take(8).cloned().collect();
                v.sort_by(|a, b| b.mem_mb.partial_cmp(&a.mem_mb).unwrap_or(std::cmp::Ordering::Equal));
                procs.mem = v.into_iter().take(8).collect();
            }
            self.procs = table.into_iter().map(|(k, v)| (k, v.0)).collect();
        }

        // ----- history -----
        if cpu.usage.is_some() {
            let igpu_pct = match (gpu.igpu_freq, gpu.igpu_max) { (Some(f), Some(m)) if m > 0.0 => Some(100.0 * f / m), _ => None };
            self.push("cpu", cpu.usage);
            self.push("cpu_temp", cpu.temp);
            self.push("ram", Some(mem.used_pct as f32));
            self.push("swap", Some(mem.swap_pct as f32));
            self.push("rx", Some(net.rx_bps as f32));
            self.push("tx", Some(net.tx_bps as f32));
            self.push("read", Some(disks.read_bps as f32));
            self.push("write", Some(disks.write_bps as f32));
            self.push("cpu_w", power.cpu_w);
            self.push("bat_w", power.battery.as_ref().and_then(|b| b.watts));
            self.push("fan", Some(max_rpm));
            self.push("igpu", igpu_pct);
            self.push("nvidia", gpu.nvidia.util);
        }

        let mut snap = Snapshot {
            ok: cpu.usage.is_some(),
            system: {
                let hw = self.hw.as_ref().unwrap();
                SystemInfo {
                    host: read_str("/proc/sys/kernel/hostname").unwrap_or_default(),
                    kernel: read_str("/proc/sys/kernel/osrelease").unwrap_or_default(),
                    os: os_name(),
                    uptime_secs: read("/proc/uptime").and_then(|s| s.split_whitespace().next().and_then(|x| x.parse::<f64>().ok())).unwrap_or(0.0) as u64,
                    model: hw.model.clone(),
                    ncpu: hw.ncpu,
                    p_count: hw.p_set.len(),
                    e_count: hw.e_set.len(),
                    hybrid: hw.hybrid,
                    core_kinds: hw.core_kinds.clone(),
                    max_freq: hw.max_freq,
                    e_max_freq: hw.e_max_freq,
                }
            },
            cpu,
            mem,
            power,
            gpu,
            temps,
            fans,
            max_rpm,
            disks,
            net,
            procs,
            health: 100,
            health_notes: Vec::new(),
            hist: self.hist.iter().map(|(k, v)| (k.clone(), v.iter().copied().collect())).collect(),
        };
        let (h, notes) = health(&snap);
        snap.health = h;
        snap.health_notes = notes;
        snap
    }
}

/// Starts at 100; every problem takes points off and says why.
fn health(s: &Snapshot) -> (u32, Vec<String>) {
    let mut score: i32 = 100;
    let mut notes = Vec::new();
    if s.mem.used_pct > 90.0 {
        score -= 20;
        notes.push(format!("Memory is {:.0}% used", s.mem.used_pct));
    } else if s.mem.used_pct > 80.0 {
        score -= 10;
        notes.push(format!("Memory is {:.0}% used", s.mem.used_pct));
    }
    if s.mem.swap_pct > 50.0 {
        score -= 10;
        notes.push("Swap is more than half full".into());
    }
    if s.cpu.usage.unwrap_or(0.0) > 90.0 {
        score -= 10;
        notes.push(format!("CPU is at {:.0}%", s.cpu.usage.unwrap_or(0.0)));
    }
    if s.cpu.load[0] / s.system.ncpu.max(1) as f32 > 1.5 {
        score -= 10;
        notes.push(format!("Load average {:.1} on {} cores", s.cpu.load[0], s.system.ncpu));
    }
    let hottest = s.temps.iter().map(|t| t.c).chain(s.cpu.temp).fold(0.0f32, f32::max);
    if hottest > 95.0 {
        score -= 15;
        notes.push(format!("Something is very hot ({hottest:.0} °C)"));
    } else if hottest > 85.0 {
        score -= 5;
        notes.push(format!("Something is warm ({hottest:.0} °C)"));
    }
    for d in &s.disks.mounts {
        if (d.path == "/" || d.path == "/home") && d.pct > 95.0 {
            score -= 25;
            notes.push(format!("{} is {:.0}% full", d.path, d.pct));
        } else if (d.path == "/" || d.path == "/home") && d.pct > 85.0 {
            score -= 10;
            notes.push(format!("{} is {:.0}% full", d.path, d.pct));
        }
    }
    if let Some(h) = s.power.battery.as_ref().and_then(|b| b.health) {
        if h < 60.0 {
            score -= 10;
            notes.push(format!("Battery holds {h:.0}% of its design capacity"));
        }
    }
    (score.clamp(0, 100) as u32, notes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_this_machine() {
        let mut m = Monitor::default();
        let a = m.sample(false);
        assert!(a.mem.total_mb > 0.0);
        assert!(!a.disks.mounts.is_empty());
        std::thread::sleep(std::time::Duration::from_millis(400));
        let b = m.sample(false);
        assert!(b.cpu.usage.unwrap() <= 100.0);
        assert!(b.system.ncpu > 0 && b.cpu.cores.len() == b.system.ncpu);
        assert!(b.health <= 100);
        assert!(!b.procs.mem.is_empty());
        assert!(b.hist["cpu"].len() >= 1);
    }

    #[test]
    fn ranges() {
        let s = parse_range("0-3,8,10-11");
        assert_eq!(s.len(), 7);
        assert!(s.contains(&8) && s.contains(&11) && !s.contains(&9));
        assert_eq!(short_model("WD PC SN8000S SDEQNRG-2T00-1002"), "WD SN8000S");
    }
}
