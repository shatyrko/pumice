//! Pumice shell: thin Tauri commands over `pumice-core`. Long jobs run on
//! blocking threads and report progress with events; the frontend never
//! sends a free path to delete, only ids and paths the engine re-checks.

use pumice_core::{
    ai, apps, clean, duplicates, history, maintenance, purge, rules, safety, scan, settings, startup, sysinfo, util,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};

type Cmd<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

struct AppState {
    rules: Vec<rules::Rule>,
    latest: Arc<Mutex<sysinfo::Snapshot>>,
    dash_open: Arc<std::sync::atomic::AtomicBool>,
    scans: Mutex<HashMap<String, Arc<scan::ScanResult>>>,
    scan_progress: Mutex<Arc<scan::Progress>>,
    dup_progress: Mutex<Arc<duplicates::Progress>>,
    dup_groups: Mutex<Vec<duplicates::DupGroup>>,
}

fn gib(n: u64) -> String {
    let n = n as f64;
    if n >= 1073741824.0 { format!("{:.1} GB", n / 1073741824.0) } else if n >= 1048576.0 { format!("{:.0} MB", n / 1048576.0) } else { format!("{:.0} KB", n / 1024.0) }
}

// ---------- dashboard ----------

#[tauri::command]
fn system_snapshot(state: State<'_, AppState>) -> Cmd<sysinfo::Snapshot> {
    Ok(state.latest.lock().map_err(err)?.clone())
}

/// The dashboard tells the sampler when it is on screen (the NVIDIA GPU is only queried then).
#[tauri::command]
fn set_dashboard_open(state: State<'_, AppState>, open: bool) {
    state.dash_open.store(open, Ordering::Relaxed);
}

// ---------- analyze ----------

#[derive(Serialize, Clone)]
struct ScanSummary {
    purpose: String,
    root: String,
    total_size: u64,
    total_files: u64,
    millis: u64,
    errors: u64,
    cancelled: bool,
}

#[derive(Serialize, Clone)]
struct ProgressPayload {
    files: u64,
    bytes: u64,
    dirs: u64,
}

#[tauri::command]
async fn scan_start(app: AppHandle, state: State<'_, AppState>, purpose: String, path: String, min_file: Option<u64>) -> Cmd<ScanSummary> {
    let root = PathBuf::from(&path);
    let root = std::fs::canonicalize(&root).map_err(|e| format!("{path}: {e}"))?;
    let progress = Arc::new(scan::Progress::default());
    *state.scan_progress.lock().map_err(err)? = progress.clone();
    let opts = scan::Options { min_file: min_file.unwrap_or_else(|| settings::load().min_file_bytes), cross_filesystems: false };

    // A ticker reports counts while the scan runs.
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let (progress, done, app) = (progress.clone(), done.clone(), app.clone());
        std::thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                let _ = app.emit(
                    "scan-progress",
                    ProgressPayload { files: progress.files.load(Ordering::Relaxed), bytes: progress.bytes.load(Ordering::Relaxed), dirs: progress.dirs.load(Ordering::Relaxed) },
                );
                std::thread::sleep(std::time::Duration::from_millis(120));
            }
        });
    }
    let p2 = progress.clone();
    let result = tauri::async_runtime::spawn_blocking(move || scan::scan(&root, &opts, &p2)).await.map_err(err)?;
    done.store(true, Ordering::Relaxed);
    let result = result.map_err(err)?;
    let summary = ScanSummary {
        purpose: purpose.clone(),
        root: result.root.to_string_lossy().into_owned(),
        total_size: result.total_size,
        total_files: result.total_files,
        millis: result.millis,
        errors: result.errors,
        cancelled: result.cancelled,
    };
    state.scans.lock().map_err(err)?.insert(purpose, Arc::new(result));
    Ok(summary)
}

#[tauri::command]
fn scan_cancel(state: State<'_, AppState>) -> Cmd<()> {
    state.scan_progress.lock().map_err(err)?.cancel.store(true, Ordering::Relaxed);
    Ok(())
}

fn scan_of(state: &State<'_, AppState>, purpose: &str) -> Cmd<Arc<scan::ScanResult>> {
    state.scans.lock().map_err(err)?.get(purpose).cloned().ok_or_else(|| "Run a scan first.".to_string())
}

#[derive(Serialize)]
struct Listing {
    id: u32,
    path: String,
    size: u64,
    files: u64,
    parent: Option<u32>,
    rows: Vec<scan::Row>,
}

#[tauri::command]
fn scan_rows(state: State<'_, AppState>, purpose: String, id: u32) -> Cmd<Listing> {
    let s = scan_of(&state, &purpose)?;
    let n = s.get(id).ok_or("unknown folder")?;
    Ok(Listing {
        id,
        path: s.path_of(id).to_string_lossy().into_owned(),
        size: n.size,
        files: n.files,
        parent: if n.parent == scan::NONE { None } else { Some(n.parent) },
        rows: s.rows(id),
    })
}

#[tauri::command]
fn scan_view(state: State<'_, AppState>, purpose: String, id: u32, depth: u32, per_level: usize) -> Cmd<scan::ViewNode> {
    let s = scan_of(&state, &purpose)?;
    s.view(id, depth.min(6), per_level.clamp(4, 300)).ok_or_else(|| "unknown folder".into())
}

#[tauri::command]
fn scan_path(state: State<'_, AppState>, purpose: String, id: u32) -> Cmd<String> {
    Ok(scan_of(&state, &purpose)?.path_of(id).to_string_lossy().into_owned())
}

// ---------- large files and installers ----------

#[derive(Serialize)]
struct BigFile {
    #[serde(flatten)]
    hit: scan::FileHit,
    kind: String,
}

fn kind_of(name: &str) -> &'static str {
    let n = name.to_lowercase();
    let ext = n.rsplit('.').next().unwrap_or("");
    if n.ends_with(".pkg.tar.zst") || n.ends_with(".pkg.tar.xz") {
        return "installer";
    }
    match ext {
        "mp4" | "mkv" | "avi" | "mov" | "webm" | "m4v" | "wmv" | "flv" => "video",
        "mp3" | "flac" | "wav" | "ogg" | "m4a" | "opus" | "aac" => "audio",
        "jpg" | "jpeg" | "png" | "raw" | "cr2" | "nef" | "arw" | "dng" | "tiff" | "psd" | "heic" => "image",
        "zip" | "tar" | "gz" | "xz" | "zst" | "7z" | "rar" | "bz2" | "tgz" => "archive",
        "iso" | "img" | "qcow2" | "vdi" | "vmdk" | "vhd" | "vhdx" | "raw.xz" => "disk image",
        "deb" | "rpm" | "appimage" | "run" | "snap" | "flatpak" | "exe" | "msi" | "dmg" | "apk" => "installer",
        "pdf" | "epub" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" => "document",
        "log" => "log",
        _ => "other",
    }
}

#[tauri::command]
fn large_files(state: State<'_, AppState>, limit: usize, min_size: u64, older_than_days: u64, only_installers: bool) -> Cmd<Vec<BigFile>> {
    let s = scan_of(&state, "home")?;
    let cutoff = util::now_secs().saturating_sub(older_than_days * 86_400);
    let hits = s.top_files(limit.clamp(1, 1000), |n| {
        n.size >= min_size && (older_than_days == 0 || n.mtime.max(n.atime) <= cutoff) && (!only_installers || kind_of(&n.name) == "installer")
    });
    Ok(hits.into_iter().map(|h| BigFile { kind: kind_of(&h.name).to_string(), hit: h }).collect())
}

#[derive(Serialize)]
struct RemoveReport {
    removed: usize,
    failed: usize,
    freed: u64,
    errors: Vec<String>,
    dry_run: bool,
    to_trash: bool,
}

/// Removes files the person picked from a scan. Each path has to sit inside
/// the scanned folder and pass the User policy.
#[tauri::command]
async fn remove_picked(state: State<'_, AppState>, purpose: String, paths: Vec<String>, dry_run: bool) -> Cmd<RemoveReport> {
    let s = scan_of(&state, &purpose)?;
    let to_trash = settings::load().use_trash;
    let mode = if to_trash { clean::Mode::Trash } else { clean::Mode::Delete };
    let root = s.root.clone();
    let rep = tauri::async_runtime::spawn_blocking(move || {
        let mut rep = RemoveReport { removed: 0, failed: 0, freed: 0, errors: vec![], dry_run, to_trash };
        for p in &paths {
            let path = Path::new(p);
            if !path.starts_with(&root) {
                rep.failed += 1;
                rep.errors.push(format!("{p}: not in the scanned folder"));
                continue;
            }
            if dry_run {
                match safety::check(path, safety::Policy::User) {
                    Ok(_) => { rep.removed += 1; rep.freed += scan::disk_usage(path); }
                    Err(e) => { rep.failed += 1; rep.errors.push(e); }
                }
                continue;
            }
            match clean::remove_path(path, mode, safety::Policy::User) {
                Ok(n) => { rep.removed += 1; rep.freed += n; }
                Err(e) => { rep.failed += 1; rep.errors.push(e); }
            }
        }
        rep
    })
    .await
    .map_err(err)?;
    history::record(
        "large-files",
        &format!("{} files {}", rep.removed, if to_trash { "moved to the Trash" } else { "deleted" }),
        if to_trash { 0 } else { rep.freed },
        rep.removed,
        rep.failed,
        dry_run,
        paths_preview(&rep.errors),
    );
    Ok(rep)
}

fn paths_preview(v: &[String]) -> Vec<String> {
    v.iter().take(20).cloned().collect()
}

// ---------- clean ----------

#[tauri::command]
async fn clean_scan(state: State<'_, AppState>, only: Option<Vec<String>>) -> Cmd<Vec<clean::RuleResult>> {
    let rules = state.rules.clone();
    tauri::async_runtime::spawn_blocking(move || clean::scan_rules(&rules, only.as_deref())).await.map_err(err)
}

#[tauri::command]
async fn clean_apply(state: State<'_, AppState>, requests: Vec<clean::ApplyRequest>, dry_run: bool) -> Cmd<clean::Report> {
    let rules = state.rules.clone();
    let title_ids: Vec<String> = requests.iter().map(|r| r.rule_id.clone()).collect();
    let report = tauri::async_runtime::spawn_blocking(move || {
        let mode = if settings::load().use_trash { None } else { Some(clean::Mode::Delete) };
        clean::apply(&rules, &requests, mode, dry_run)
    })
    .await
    .map_err(err)?;
    let details: Vec<String> = report.outcomes.iter().filter_map(|o| o.error.as_ref().map(|e| format!("{}: {e}", o.path))).take(20).collect();
    history::record("clean", &format!("Cleanup: {}", title_ids.join(", ")), report.freed, report.removed, report.failed, dry_run, details);
    Ok(report)
}

// ---------- duplicates ----------

#[derive(Serialize, Clone)]
struct DupProgress {
    seen: u64,
    hashed: u64,
}

#[tauri::command]
async fn dup_find(app: AppHandle, state: State<'_, AppState>, roots: Vec<String>, min_size: u64, skip_hidden: bool) -> Cmd<Vec<duplicates::DupGroup>> {
    let roots: Vec<PathBuf> = roots.iter().map(|r| PathBuf::from(safety::expand(r))).filter(|p| p.is_dir()).collect();
    if roots.is_empty() {
        return Err("Pick at least one existing folder.".into());
    }
    let progress = Arc::new(duplicates::Progress::default());
    *state.dup_progress.lock().map_err(err)? = progress.clone();
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let (progress, done, app) = (progress.clone(), done.clone(), app.clone());
        std::thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                let _ = app.emit("dup-progress", DupProgress { seen: progress.seen.load(Ordering::Relaxed), hashed: progress.hashed.load(Ordering::Relaxed) });
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
        });
    }
    let p2 = progress.clone();
    let groups = tauri::async_runtime::spawn_blocking(move || duplicates::find(&roots, min_size, skip_hidden, &p2)).await.map_err(err)?;
    done.store(true, Ordering::Relaxed);
    *state.dup_groups.lock().map_err(err)? = groups.clone();
    Ok(groups)
}

#[tauri::command]
fn dup_cancel(state: State<'_, AppState>) -> Cmd<()> {
    state.dup_progress.lock().map_err(err)?.cancel.store(true, Ordering::Relaxed);
    Ok(())
}

#[derive(Deserialize)]
struct DupChoice {
    hash: String,
    keep: Vec<String>,
    remove: Vec<String>,
}

/// For every group: at least one kept file has to exist and still have the
/// group's hash, and every file to remove has to have that same hash.
#[tauri::command]
async fn dup_apply(choices: Vec<DupChoice>, dry_run: bool) -> Cmd<RemoveReport> {
    let to_trash = settings::load().use_trash;
    let mode = if to_trash { clean::Mode::Trash } else { clean::Mode::Delete };
    let rep = tauri::async_runtime::spawn_blocking(move || {
        let mut rep = RemoveReport { removed: 0, failed: 0, freed: 0, errors: vec![], dry_run, to_trash };
        for c in &choices {
            let kept_ok = c.keep.iter().any(|k| duplicates::hash_full(Path::new(k)).as_deref() == Some(c.hash.as_str()));
            if c.keep.is_empty() || !kept_ok {
                rep.failed += c.remove.len();
                rep.errors.push("A group has no intact copy to keep. Nothing was removed from it.".into());
                continue;
            }
            for r in &c.remove {
                if c.keep.contains(r) {
                    rep.failed += 1;
                    rep.errors.push(format!("{r}: is also marked to keep"));
                    continue;
                }
                if duplicates::hash_full(Path::new(r)).as_deref() != Some(c.hash.as_str()) {
                    rep.failed += 1;
                    rep.errors.push(format!("{r}: changed since the scan"));
                    continue;
                }
                if dry_run {
                    match safety::check(Path::new(r), safety::Policy::User) {
                        Ok(_) => { rep.removed += 1; rep.freed += scan::disk_usage(Path::new(r)); }
                        Err(e) => { rep.failed += 1; rep.errors.push(e); }
                    }
                    continue;
                }
                match clean::remove_path(Path::new(r), mode, safety::Policy::User) {
                    Ok(n) => { rep.removed += 1; rep.freed += n; }
                    Err(e) => { rep.failed += 1; rep.errors.push(e); }
                }
            }
        }
        rep
    })
    .await
    .map_err(err)?;
    history::record("duplicates", &format!("{} duplicate files {}", rep.removed, if to_trash { "moved to the Trash" } else { "deleted" }), if to_trash { 0 } else { rep.freed }, rep.removed, rep.failed, dry_run, paths_preview(&rep.errors));
    Ok(rep)
}

// ---------- purge ----------

#[tauri::command]
async fn purge_scan(extra_roots: Vec<String>) -> Cmd<Vec<purge::Artifact>> {
    tauri::async_runtime::spawn_blocking(move || {
        let s = settings::load();
        let mut roots = purge::default_roots();
        for r in s.purge_roots.iter().chain(extra_roots.iter()) {
            let p = PathBuf::from(safety::expand(r));
            if p.is_dir() && !roots.contains(&p) {
                roots.push(p);
            }
        }
        purge::find(&roots, 6)
    })
    .await
    .map_err(err)
}

#[tauri::command]
async fn purge_apply(paths: Vec<String>, dry_run: bool) -> Cmd<RemoveReport> {
    let rep = tauri::async_runtime::spawn_blocking(move || {
        let mut rep = RemoveReport { removed: 0, failed: 0, freed: 0, errors: vec![], dry_run, to_trash: false };
        for p in &paths {
            let path = Path::new(p);
            if !purge::is_artifact(path) {
                rep.failed += 1;
                rep.errors.push(format!("{p}: not a project artifact"));
                continue;
            }
            if dry_run {
                match safety::check(path, safety::Policy::User) {
                    Ok(_) => { rep.removed += 1; rep.freed += scan::disk_usage(path); }
                    Err(e) => { rep.failed += 1; rep.errors.push(e); }
                }
                continue;
            }
            match clean::remove_path(path, clean::Mode::Delete, safety::Policy::User) {
                Ok(n) => { rep.removed += 1; rep.freed += n; }
                Err(e) => { rep.failed += 1; rep.errors.push(e); }
            }
        }
        rep
    })
    .await
    .map_err(err)?;
    history::record("purge", &format!("{} project folders removed", rep.removed), rep.freed, rep.removed, rep.failed, dry_run, paths_preview(&rep.errors));
    Ok(rep)
}

// ---------- apps ----------

#[tauri::command]
async fn apps_list() -> Cmd<Vec<apps::App>> {
    tauri::async_runtime::spawn_blocking(apps::list).await.map_err(err)
}

#[tauri::command]
async fn app_leftovers(id: String) -> Cmd<Vec<apps::Leftover>> {
    tauri::async_runtime::spawn_blocking(move || {
        let app = apps::list().into_iter().find(|a| a.id == id).ok_or("unknown app")?;
        Ok::<_, String>(apps::leftovers(&app))
    })
    .await
    .map_err(err)?
}

#[derive(Serialize)]
struct UninstallReport {
    output: String,
    removed_leftovers: usize,
    freed: u64,
    errors: Vec<String>,
}

#[tauri::command]
async fn app_uninstall(id: String, leftovers: Vec<String>) -> Cmd<UninstallReport> {
    let (name, rep) = tauri::async_runtime::spawn_blocking(move || {
        let app = apps::list().into_iter().find(|a| a.id == id).ok_or("unknown app")?;
        let mode = if settings::load().use_trash { clean::Mode::Trash } else { clean::Mode::Delete };
        let r = apps::uninstall(&app, &leftovers, mode).map_err(err)?;
        Ok::<_, String>((app.name, r))
    })
    .await
    .map_err(err)??;
    history::record("apps", &format!("Uninstalled {name}"), rep.freed, 1 + rep.removed_leftovers, rep.errors.len(), false, rep.errors.clone());
    Ok(UninstallReport { output: rep.output, removed_leftovers: rep.removed_leftovers, freed: rep.freed, errors: rep.errors })
}

// ---------- startup ----------

#[tauri::command]
async fn startup_list() -> Cmd<Vec<startup::Entry>> {
    tauri::async_runtime::spawn_blocking(startup::list).await.map_err(err)
}

#[tauri::command]
async fn startup_set(id: String, enabled: bool) -> Cmd<()> {
    let id2 = id.clone();
    tauri::async_runtime::spawn_blocking(move || startup::set_enabled(&id2, enabled)).await.map_err(err)?.map_err(err)?;
    history::record("startup", &format!("{} {}", if enabled { "Enabled" } else { "Disabled" }, id), 0, 1, 0, false, vec![]);
    Ok(())
}

#[tauri::command]
async fn boot_info() -> Cmd<startup::Boot> {
    tauri::async_runtime::spawn_blocking(startup::boot).await.map_err(err)
}

// ---------- maintenance ----------

#[derive(Serialize)]
struct HelperStatus {
    installed: bool,
    path: &'static str,
}

#[tauri::command]
fn helper_status() -> HelperStatus {
    HelperStatus { installed: maintenance::helper_installed(), path: maintenance::HELPER_PATH }
}

#[tauri::command]
async fn maintenance_checks() -> Cmd<Vec<maintenance::Check>> {
    tauri::async_runtime::spawn_blocking(maintenance::checks).await.map_err(err)
}

#[tauri::command]
async fn maintenance_run(id: String, args: Vec<String>) -> Cmd<String> {
    let id2 = id.clone();
    let res = tauri::async_runtime::spawn_blocking(move || maintenance::run_action(&id2, &args)).await.map_err(err)?;
    match res {
        Ok(out) => {
            history::record("maintenance", &format!("Maintenance: {id}"), 0, 1, 0, false, vec![]);
            Ok(out)
        }
        Err(e) => {
            history::record("maintenance", &format!("Maintenance: {id}"), 0, 0, 1, false, vec![e.to_string()]);
            Err(e.to_string())
        }
    }
}

// ---------- AI explanations ----------

#[derive(Serialize)]
struct AiStatus {
    configured: bool,
    provider: String,
    model: String,
    has_key: bool,
}

#[tauri::command]
fn ai_status() -> AiStatus {
    let s = settings::load();
    let cfg = ai::config(&s);
    AiStatus { configured: cfg.is_some() && (s.ai_provider != "anthropic" || ai::has_key()), provider: s.ai_provider.clone(), model: cfg.map(|c| c.model).unwrap_or_default(), has_key: ai::has_key() }
}

#[tauri::command]
fn ai_set_key(key: String) -> Cmd<()> {
    ai::set_key(&key).map_err(err)
}

#[tauri::command]
fn ai_clear_key() -> Cmd<()> {
    ai::clear_key().map_err(err)
}

#[tauri::command]
async fn ai_explain(topic: String, facts: String) -> Cmd<String> {
    tauri::async_runtime::spawn_blocking(move || ai::explain(&settings::load(), &topic, &facts)).await.map_err(err)?.map_err(err)
}

// ---------- history, settings, misc ----------

#[tauri::command]
fn history_list(limit: usize) -> Vec<history::Entry> {
    history::list(limit.clamp(1, 1000))
}

#[tauri::command]
fn history_clear() {
    history::clear();
}

#[tauri::command]
fn settings_get() -> settings::Settings {
    settings::load()
}

#[tauri::command]
fn settings_set(value: settings::Settings) -> Cmd<()> {
    settings::save(&value).map_err(err)
}

#[derive(Serialize)]
struct Places {
    home: String,
    purge_roots: Vec<String>,
}

#[tauri::command]
fn places() -> Places {
    Places {
        home: safety::home().to_string_lossy().into_owned(),
        purge_roots: purge::default_roots().iter().map(|p| p.to_string_lossy().into_owned()).collect(),
    }
}

#[tauri::command]
fn format_bytes(n: u64) -> String {
    gib(n)
}

pub fn run() {
    // WebKitGTK 2.54 flashes the first frame of a transition with the new compositor.
    if std::env::var_os("WEBKIT_USE_SKIA_FOR_COMPOSITION").is_none() {
        std::env::set_var("WEBKIT_USE_SKIA_FOR_COMPOSITION", "0");
    }
    let latest = Arc::new(Mutex::new(sysinfo::Snapshot::default()));
    let dash_open = Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        // One sampler for the whole app: the graphs keep their history while other pages are open.
        let (latest, dash_open) = (latest.clone(), dash_open.clone());
        std::thread::spawn(move || {
            let mut monitor = sysinfo::Monitor::default();
            loop {
                let snap = monitor.sample(dash_open.load(Ordering::Relaxed));
                if let Ok(mut l) = latest.lock() {
                    *l = snap;
                }
                std::thread::sleep(std::time::Duration::from_millis(2000));
            }
        });
    }
    let state = AppState {
        rules: rules::all(),
        latest,
        dash_open,
        scans: Mutex::new(HashMap::new()),
        scan_progress: Mutex::new(Arc::new(scan::Progress::default())),
        dup_progress: Mutex::new(Arc::new(duplicates::Progress::default())),
        dup_groups: Mutex::new(Vec::new()),
    };
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            system_snapshot,
            set_dashboard_open,
            scan_start,
            scan_cancel,
            scan_rows,
            scan_view,
            scan_path,
            large_files,
            remove_picked,
            clean_scan,
            clean_apply,
            dup_find,
            dup_cancel,
            dup_apply,
            purge_scan,
            purge_apply,
            apps_list,
            app_leftovers,
            app_uninstall,
            startup_list,
            startup_set,
            boot_info,
            helper_status,
            maintenance_checks,
            maintenance_run,
            ai_status,
            ai_set_key,
            ai_clear_key,
            ai_explain,
            history_list,
            history_clear,
            settings_get,
            settings_set,
            places,
            format_bytes,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Pumice");
}
