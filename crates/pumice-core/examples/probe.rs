//! Read-only look at this machine through every module. `cargo run -p pumice-core --example probe`
use pumice_core::*;

fn gib(n: u64) -> String { format!("{:.2} GiB", n as f64 / 1073741824.0) }

fn main() {
    let t = std::time::Instant::now();
    let rules = rules::all();
    println!("{} rules", rules.len());
    let res = clean::scan_rules(&rules, None);
    for r in &res {
        println!("  {:<28} {:>10}  {} items{}", r.rule.id, gib(r.total), r.items.len(), r.blocked.as_deref().map(|b| format!("  [{b}]")).unwrap_or_default());
    }
    println!("rules scan: {:?}, total {}", t.elapsed(), gib(res.iter().map(|r| r.total).sum()));

    let t = std::time::Instant::now();
    let apps = apps::list();
    println!("{} apps in {:?}", apps.len(), t.elapsed());
    for a in apps.iter().take(8) { println!("  {:<10} {:<30} {:<28} {}", a.source, a.name, a.package, gib(a.size)); }

    let s = startup::list();
    println!("{} startup entries; boot: {}", s.len(), startup::boot().summary);

    for c in maintenance::checks() { println!("  [{}] {} {:?} {}", c.status, c.title, c.size.map(gib), c.items.len()); }

    let t = std::time::Instant::now();
    let p = purge::find(&purge::default_roots(), 5);
    println!("{} purge artifacts, {} in {:?}", p.len(), gib(p.iter().map(|a| a.size).sum()), t.elapsed());

    let t = std::time::Instant::now();
    let r = scan::scan(&safety::home(), &scan::Options::default(), &scan::Progress::default()).unwrap();
    println!("home scan: {} files, {} in {} ms ({:?}), {} nodes", r.total_files, gib(r.total_size), r.millis, t.elapsed(), r.nodes.len());
    for f in r.top_files(5, |_| true) { println!("  {} {}", gib(f.size), f.path); }
}
