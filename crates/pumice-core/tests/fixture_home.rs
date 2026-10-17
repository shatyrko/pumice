//! Runs the real removal paths against a throwaway HOME (never the person's own files).

use pumice_core::clean::{self, ApplyRequest, Mode};
use pumice_core::safety::{self, Policy};
use pumice_core::{apps, duplicates, purge, rules, startup};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

fn write(p: &Path, bytes: usize) {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, vec![b'x'; bytes]).unwrap();
}

fn age(p: &Path, days: u64) {
    let t = SystemTime::now() - Duration::from_secs(days * 86_400);
    fs::File::open(p).unwrap().set_modified(t).unwrap();
}

#[test]
fn everything_against_a_throwaway_home() {
    let base = tempfile::tempdir().unwrap();
    let home = base.path().join("home");
    fs::create_dir_all(&home).unwrap();
    std::env::set_var("HOME", &home);
    std::env::set_var("XDG_DATA_HOME", home.join(".local/share"));
    std::env::set_var("XDG_CONFIG_HOME", home.join(".config"));
    assert_eq!(safety::home(), home);

    // ---- cleanup rules ----
    write(&home.join(".cache/someapp/blob.bin"), 200_000);
    age(&home.join(".cache/someapp/blob.bin"), 5);
    age(&home.join(".cache/someapp"), 5);
    write(&home.join(".cache/fresh/new.bin"), 1000); // used today: left alone
    write(&home.join("Documents/keep.txt"), 5000);
    write(&home.join(".ssh/id_ed25519"), 400);
    write(&home.join(".npm/_cacache/x/data"), 100_000);
    let all = rules::all();
    let results = clean::scan_rules(&all, Some(&["user-cache".to_string(), "npm-cache".to_string()]));
    let uc = results.iter().find(|r| r.rule.id == "user-cache").expect("user-cache found");
    assert!(uc.items.iter().any(|i| i.path.ends_with(".cache/someapp")));
    assert!(!uc.items.iter().any(|i| i.path.ends_with(".cache/fresh")), "young cache must stay");
    let npm = results.iter().find(|r| r.rule.id == "npm-cache").expect("npm found");

    // A forged request: another path, a symlink to /etc, a secret.
    std::os::unix::fs::symlink("/etc", home.join(".cache/etclink")).unwrap();
    let reqs = vec![
        ApplyRequest { rule_id: "user-cache".into(), paths: vec![
            home.join(".cache/someapp").to_string_lossy().into(),
            home.join("Documents/keep.txt").to_string_lossy().into(),
            home.join(".ssh/id_ed25519").to_string_lossy().into(),
            home.join(".cache/etclink/hostname").to_string_lossy().into(),
        ] },
        ApplyRequest { rule_id: "npm-cache".into(), paths: vec![npm.items[0].path.clone()] },
    ];
    let dry = clean::apply(&all, &reqs, None, true);
    assert!(home.join(".cache/someapp/blob.bin").exists(), "dry run removes nothing");
    assert_eq!(dry.removed, 2);
    let rep = clean::apply(&all, &reqs, None, false);
    assert_eq!(rep.removed, 2, "{:?}", rep.outcomes);
    assert_eq!(rep.failed, 3);
    assert!(!home.join(".cache/someapp").exists());
    assert!(!home.join(".npm/_cacache").exists());
    assert!(home.join("Documents/keep.txt").exists());
    assert!(home.join(".ssh/id_ed25519").exists());
    assert!(Path::new("/etc/hostname").exists());
    assert!(home.join(".cache/fresh/new.bin").exists());

    // ---- projects ----
    write(&home.join("Projects/web/package.json"), 10);
    write(&home.join("Projects/web/node_modules/dep/index.js"), 300_000);
    write(&home.join("Projects/rust/Cargo.toml"), 10);
    write(&home.join("Projects/rust/target/debug/bin"), 500_000);
    write(&home.join("Projects/plain/node_modules/x"), 50_000); // no package.json: not an artifact
    let found = purge::find(&purge::default_roots(), 5);
    assert_eq!(found.len(), 2, "{found:?}");
    for a in &found {
        assert!(purge::is_artifact(Path::new(&a.path)));
        clean::remove_path(Path::new(&a.path), Mode::Delete, Policy::User).unwrap();
    }
    assert!(!home.join("Projects/web/node_modules").exists());
    assert!(home.join("Projects/web/package.json").exists());
    assert!(home.join("Projects/plain/node_modules/x").exists());
    assert!(!purge::is_artifact(&home.join("Projects/plain/node_modules")));

    // ---- duplicates, then the Trash ----
    write(&home.join("Pictures/a.jpg"), 3_000_000);
    write(&home.join("Pictures/copy/a.jpg"), 3_000_000);
    write(&home.join("Pictures/other.jpg"), 3_000_001);
    let groups = duplicates::find(&[home.join("Pictures")], 1_000_000, false, &duplicates::Progress::default());
    assert_eq!(groups.len(), 1);
    let g = &groups[0];
    assert_eq!(g.files.len(), 2);
    let hash = g.hash.clone();
    assert_eq!(duplicates::hash_full(Path::new(&g.files[1].path)).as_deref(), Some(hash.as_str()));
    let victim = PathBuf::from(&g.files[1].path);
    let freed = clean::remove_path(&victim, Mode::Trash, Policy::User).unwrap();
    assert!(freed >= 3_000_000);
    assert!(!victim.exists());
    assert!(home.join(".local/share/Trash/files").read_dir().unwrap().count() >= 1, "went to the Trash");
    assert!(Path::new(&g.files[0].path).exists());

    // ---- startup entries ----
    let auto = home.join(".config/autostart");
    fs::create_dir_all(&auto).unwrap();
    fs::write(auto.join("demo.desktop"), "[Desktop Entry]\nType=Application\nName=Demo\nExec=demo\n").unwrap();
    let list = startup::list();
    let e = list.iter().find(|e| e.id == "autostart:demo.desktop").expect("listed");
    assert!(e.enabled);
    startup::set_enabled("autostart:demo.desktop", false).unwrap();
    assert!(!startup::list().iter().find(|e| e.id == "autostart:demo.desktop").unwrap().enabled);
    startup::set_enabled("autostart:demo.desktop", true).unwrap();
    assert!(startup::list().iter().find(|e| e.id == "autostart:demo.desktop").unwrap().enabled);
    assert!(startup::set_enabled("autostart:../../evil.desktop", false).is_err());
    assert!(startup::set_enabled("service:a b.service", false).is_err());

    // ---- app leftovers and an AppImage ----
    write(&home.join(".config/FooBar/settings.json"), 4000);
    write(&home.join(".cache/foobar/c"), 4000);
    write(&home.join(".local/share/unrelated/x"), 4000);
    let app = apps::App { id: "pacman:foobar".into(), name: "FooBar".into(), source: "appimage".into(), package: home.join("Applications/FooBar.AppImage").to_string_lossy().into(), version: String::new(), size: 0, icon: None, comment: String::new() };
    write(Path::new(&app.package), 10_000);
    let left = apps::leftovers(&app);
    assert_eq!(left.len(), 2, "{left:?}");
    assert!(left.iter().any(|l| l.path.ends_with(".config/FooBar") && l.caution));
    assert!(left.iter().any(|l| l.path.ends_with(".cache/foobar") && !l.caution));
    let res = apps::uninstall(&app, &[left[0].path.clone(), home.join(".local/share/unrelated").to_string_lossy().into()], Mode::Delete).unwrap();
    assert_eq!(res.removed_leftovers, 1);
    assert_eq!(res.errors.len(), 1, "a path that is no leftover is refused");
    assert!(!Path::new(&app.package).exists());
    assert!(home.join(".local/share/unrelated/x").exists());
}
