// Dev only: lets the interface run in a plain browser (vite dev server) with
// sample data, so every page can be looked at without the Rust side.
const GB = 1e9;
const now = Math.floor(Date.now() / 1000);
const day = 86400;

const tree = (name, size, kids = []) => ({ id: tree.n++, name, is_dir: kids.length > 0 || size < 0, size, children: kids });
tree.n = 1;
const leaf = (name, size) => ({ id: tree.n++, name, is_dir: false, size, children: [] });
const dir = (name, kids) => ({ id: tree.n++, name, is_dir: true, size: kids.reduce((a, k) => a + k.size, 0), children: kids });

const demoTree = () => {
  tree.n = 1;
  const root = dir('/home/demo', [
    dir('.local', [dir('share', [dir('Steam', [leaf('Cyberpunk.archive', 14.4 * GB), leaf('basegame.archive', 12.9 * GB), dir('shadercache', [leaf('a', 1.2 * GB)])]), dir('gnome-boxes', [leaf('archlinux', 20.8 * GB)])])]),
    dir('Projects', [dir('web', [dir('node_modules', [leaf('big', 1.4 * GB), leaf('more', 0.6 * GB)]), leaf('src', 0.05 * GB)]), dir('rusty', [dir('target', [leaf('debug', 7.1 * GB)])])]),
    dir('.cache', [dir('paru', [leaf('clones', 13 * GB)]), dir('mozilla', [leaf('cache2', 0.12 * GB)])]),
    dir('Videos', [leaf('holiday.mkv', 4.2 * GB), leaf('talk.mp4', 1.1 * GB)]),
    dir('Downloads', [leaf('archlinux.iso', 1.1 * GB), leaf('tool.AppImage', 0.2 * GB)]),
  ]);
  root.id = 0;
  return root;
};
let T = demoTree();
const find = (n, id) => (n.id === id ? n : n.children.map((c) => find(c, id)).find(Boolean));
const rowsOf = (n) => n.children.map((c) => ({ id: c.id, name: c.name, is_dir: c.is_dir, size: c.size, files: c.is_dir ? 1200 : 1, mtime: now - 3 * day, note: null })).sort((a, b) => b.size - a.size);

const rules = [
  ['user-cache', 'Application caches', 'caches', 'low', 'Files programs keep in ~/.cache to start faster. They are rebuilt when needed.', 22 * GB, null],
  ['aur-helper', 'AUR build caches', 'packages', 'low', 'Sources and build folders of AUR packages kept by paru and yay.', 13 * GB, 'The next AUR install downloads and builds again.'],
  ['chrome-cache', 'Google Chrome cache', 'browsers', 'low', 'Pages, scripts and shaders Chrome saved for later.', 1.3 * GB, 'Close Chrome first.', 'chrome is running. Close it first.'],
  ['cargo-registry', 'Cargo downloads', 'developer', 'low', 'Downloaded crates and unpacked sources in ~/.cargo.', 1.9 * GB, null],
  ['huggingface', 'Hugging Face models and datasets', 'ai', 'medium', 'Models and datasets downloaded by Python tools.', 0.68 * GB, 'Large files.'],
  ['trash', 'Trash', 'caches', 'medium', 'Files you moved to the Trash.', 0.14 * GB, 'This cannot be undone.'],
  ['chrome-privacy', 'Chrome history and cookies', 'privacy', 'medium', 'Browsing history, cookies and top sites.', 0.01 * GB, null, 'chrome is running. Close it first.'],
].map(([id, name, category, risk, description, total, note, blocked]) => ({
  id, name, category, risk, description, note, paths: [], exclude: [], min_age_days: 0, blocked_by: [], action: 'delete', total, blocked: blocked || null, selected: risk === 'low',
  items: [1, 2, 3].map((i) => ({ path: `/home/demo/.cache/${id}/item-${i}`, name: `item-${i}`, size: total / (i * 2.2), is_dir: true, mtime: now - 5 * day })),
}));

const apps = [
  ['pacman:brave-bin', 'Brave', 'pacman', 'brave-bin', '1.80', 0.46 * GB],
  ['pacman:gimp', 'GNU Image Manipulation Program', 'pacman', 'gimp', '3.0.4', 0.31 * GB],
  ['flatpak:org.mozilla.firefox', 'Firefox', 'flatpak', 'org.mozilla.firefox', '141.0', 0.52 * GB],
  ['appimage:/home/demo/Applications/tool.AppImage', 'tool', 'appimage', '/home/demo/Applications/tool.AppImage', '', 0.2 * GB],
  ['pacman:btop', 'btop++', 'pacman', 'btop', '1.4.0', 0.003 * GB],
].map(([id, name, source, pkg, version, size]) => ({ id, name, source, package: pkg, version, size, icon: null, comment: 'Sample application' }));

const dupGroups = [
  { hash: 'h1', size: 1.1 * GB, files: [{ path: '/home/demo/Downloads/archlinux.iso', mtime: now - 60 * day }, { path: '/home/demo/Old/archlinux (1).iso', mtime: now - 20 * day }] },
  { hash: 'h2', size: 4e6, files: [{ path: '/home/demo/Pictures/a.jpg', mtime: now - 300 * day }, { path: '/home/demo/Pictures/backup/a.jpg', mtime: now - 100 * day }, { path: '/home/demo/Desktop/a.jpg', mtime: now - 90 * day }] },
];

let hist = [{ ts: now - 3600, kind: 'clean', title: 'Cleanup: user-cache, aur-helper', freed: 8.2 * GB, ok: 40, failed: 0, dry_run: false, details: [] }, { ts: now - 2 * day, kind: 'purge', title: '3 project folders removed', freed: 3.1 * GB, ok: 3, failed: 0, dry_run: false, details: [] }];
const settings = { purge_roots: [], use_trash: true, min_file_bytes: 1 << 20, protected: ['/home/demo/Projects/keep'], purge_idle_days: 30 };


const series = {};
const pushH = (k, v) => { (series[k] ||= []).push(v); if (series[k].length > 90) series[k].shift(); };
for (let i = 0; i < 60; i++) snapshot(true);
function snapshot(quiet) {
  const usage = 12 + Math.random() * 25;
  const temp = 48 + Math.random() * 12;
  const rx = Math.random() * 2e6, tx = Math.random() * 3e5;
  pushH('cpu', usage); pushH('cpu_temp', temp); pushH('ram', 44 + Math.random() * 2); pushH('swap', 0.4);
  pushH('rx', rx); pushH('tx', tx); pushH('read', Math.random() * 4e6); pushH('write', Math.random() * 1e6);
  pushH('cpu_w', 8 + Math.random() * 20); pushH('fan', 2400 + Math.random() * 300); pushH('nvidia', 0);
  if (quiet) return null;
  const kinds = [...Array(8).fill('P'), ...Array(16).fill('E')];
  return {
    ok: true,
    system: { host: 'demo', kernel: '7.2.10-arch1-1', os: 'Arch Linux', uptime_secs: 18000, model: 'Intel Core Ultra 9 275HX', ncpu: 24, p_count: 8, e_count: 16, hybrid: true, core_kinds: kinds, max_freq: 5400, e_max_freq: 4600 },
    cpu: { usage, cores: kinds.map(() => Math.random() * 70), freq_p: 3100, freq_e: 2400, freq_max: 4800, temp, hottest: temp + 4, load: [1.2, 0.9, 0.7], threads: 1900, psi: 0.4 },
    mem: { total_mb: 33000, avail_mb: 18000, used_mb: 15000, used_pct: 45, cached_mb: 22000, shared_mb: 900, dirty_mb: 12, swap_total_mb: 39000, swap_used_mb: 170, swap_pct: 0.4, psi: 0, io_psi: 1.2 },
    power: { rapl: { 'package-0': 14.2, core: 9.1, uncore: 1.3 }, cpu_w: 14.2, rapl_available: true, battery: { pct: 80, status: 'Discharging', cycles: 120, watts: 9.4, now_wh: 62, full_wh: 76, design_wh: 80, health: 94.6, hours_left: 6.6 }, ac: false, profile: 'balanced', epp: 'balance_performance' },
    gpu: { igpu_freq: 300, igpu_max: 2000, nvidia: { present: true, awake: false, status: 'suspended' } },
    temps: [{ label: 'SSD WD SN8000S', c: 47, kind: 'ssd' }, { label: 'Wi-Fi card', c: 41, kind: 'other' }, { label: 'RAM stick 1', c: 44, kind: 'ram' }, { label: 'Board (ACPI)', c: 52, kind: 'other' }],
    fans: [{ label: 'CPU fan', rpm: 2500 }, { label: 'GPU fan', rpm: 2300 }],
    max_rpm: 2500,
    disks: { mounts: [{ path: '/', source: '/dev/nvme1n1p2', fs: 'btrfs', total: 1020e9, used: 310e9, pct: 30.4 }, { path: '/data', source: '/dev/nvme0n1p1', fs: 'ext4', total: 500e9, used: 460e9, pct: 92 }], io: [{ name: 'WD SN8000S', block: 'nvme1n1', read_bps: 1.2e6, write_bps: 3.4e5, temp: 47 }], read_bps: 1.2e6, write_bps: 3.4e5 },
    net: { ifaces: [{ name: 'wlan0', up: true, wireless: true, ip: '192.168.1.23', rx_bps: rx, tx_bps: tx, rx_total: 1e10, tx_total: 1e9 }], rx_bps: rx, tx_bps: tx, wifi: { connected: true, ssid: 'Home', signal: -52, freq: 5180, rx_rate: 866, tx_rate: 780 } },
    procs: { count: 142, cpu: [{ name: 'firefox', pid: 1, cpu: 6.2, mem_mb: 2100, count: 14 }, { name: 'code', pid: 2, cpu: 2.1, mem_mb: 900, count: 9 }, { name: 'pumice', pid: 3, cpu: 0.4, mem_mb: 120, count: 1 }], mem: [{ name: 'firefox', pid: 1, cpu: 6.2, mem_mb: 2100, count: 14 }, { name: 'code', pid: 2, cpu: 2.1, mem_mb: 900, count: 9 }] },
    health: 78, health_notes: ['/data is 92% full'],
    hist: Object.fromEntries(Object.entries(series).map(([k, v]) => [k, [...v]])),
  };
}

let aiOn = true;
const handlers = {
  set_dashboard_open: () => null,
  ai_status: () => ({ configured: aiOn, provider: aiOn ? 'anthropic' : 'off', model: 'claude-haiku-5-5', has_key: aiOn }),
  ai_set_key: () => { aiOn = true; return null; },
  ai_clear_key: () => { aiOn = false; return null; },
  ai_explain: ({ facts }) => `This is sample text from the mock. About "${facts.split('\n')[0]}": it is rebuilt automatically, so removing it is safe; the next start is a little slower.`,
  places: () => ({ home: '/home/demo', purge_roots: ['/home/demo/Projects'] }),
  settings_get: () => settings,
  settings_set: ({ value }) => Object.assign(settings, value),
  helper_status: () => ({ installed: false, path: '/usr/lib/pumice/pumice-helper' }),
  system_snapshot: () => snapshot(),
  scan_start: ({ purpose }) => { T = demoTree(); return { purpose, root: '/home/demo', total_size: T.size, total_files: 381937, millis: 311, errors: 2, cancelled: false }; },
  scan_cancel: () => null,
  scan_rows: ({ id }) => { const n = find(T, id); return { id, path: id === 0 ? '/home/demo' : `/home/demo/${n.name}`, size: n.size, files: 4000, parent: id === 0 ? null : 0, rows: rowsOf(n) }; },
  scan_view: ({ id, depth }) => { const cut = (n, d) => ({ ...n, children: d > 0 ? n.children.map((c) => cut(c, d - 1)) : [] }); return cut(find(T, id), depth); },
  scan_path: () => '/home/demo',
  large_files: ({ onlyInstallers }) => [
    ['/home/demo/.local/share/gnome-boxes/images/archlinux', 20.9 * GB, 'other'], ['/home/demo/Videos/holiday.mkv', 4.2 * GB, 'video'], ['/home/demo/Downloads/archlinux.iso', 1.1 * GB, 'installer'], ['/home/demo/Downloads/tool.AppImage', 0.2 * GB, 'installer'],
  ].filter((f) => !onlyInstallers || f[2] === 'installer').map(([path, size, kind], i) => ({ id: i, path, name: path.split('/').pop(), size, mtime: now - 40 * day * (i + 1), atime: now - 20 * day * (i + 1), kind })),
  remove_picked: ({ paths, dryRun }) => ({ removed: paths.length, failed: 0, freed: 1e9, errors: [], dry_run: dryRun, to_trash: true }),
  clean_scan: () => rules,
  clean_apply: ({ requests, dryRun }) => ({ dry_run: dryRun, freed: 5e9, removed: requests.length * 3, failed: 0, outcomes: [] }),
  dup_find: () => dupGroups,
  dup_cancel: () => null,
  dup_apply: () => ({ removed: 3, failed: 0, freed: 1.1e9, errors: [], dry_run: false, to_trash: true }),
  purge_scan: () => [
    { path: '/home/demo/Projects/web/node_modules', project: '/home/demo/Projects/web', kind: 'Node.js', size: 2 * GB, project_mtime: now - 90 * day, age_days: 90 },
    { path: '/home/demo/Projects/rusty/target', project: '/home/demo/Projects/rusty', kind: 'Rust', size: 7.1 * GB, project_mtime: now - 2 * day, age_days: 2 },
  ],
  purge_apply: () => ({ removed: 1, failed: 0, freed: 2e9, errors: [], dry_run: false, to_trash: false }),
  apps_list: () => apps,
  app_leftovers: () => [{ path: '/home/demo/.config/BraveSoftware', size: 3e8, kind: 'settings', caution: true }, { path: '/home/demo/.cache/BraveSoftware', size: 1.5e7, kind: 'cache', caution: false }],
  app_uninstall: () => ({ output: '', removed_leftovers: 1, freed: 1e7, errors: [] }),
  startup_list: () => [
    { id: 'autostart:telegram.desktop', name: 'Telegram', kind: 'autostart', comment: 'Chat', command: 'telegram-desktop -autostart', enabled: true, scope: 'user' },
    { id: 'autostart:blueman.desktop', name: 'Blueman Applet', kind: 'autostart', comment: 'Bluetooth manager', command: 'blueman-applet', enabled: false, scope: 'system' },
    { id: 'service:syncthing.service', name: 'syncthing', kind: 'service', comment: 'Syncthing - Open Source Continuous File Synchronization', command: '/usr/bin/syncthing', enabled: true, scope: 'user' },
  ],
  startup_set: () => null,
  boot_info: () => ({ summary: 'Startup finished in 9.0s (firmware) + 1.3s (loader) + 3.2s (kernel) + 5.7s (userspace) = 19.4s', slowest: [{ unit: 'NetworkManager-wait-online.service', millis: 4200 }, { unit: 'systemd-journal-flush.service', millis: 1800 }, { unit: 'docker.service', millis: 900 }] }),
  maintenance_checks: () => [
    { id: 'orphans', title: 'Unused dependencies', status: 'warn', detail: '3 packages were installed as dependencies and nothing needs them now.', size: 80e6, items: ['libfoo', 'python-bar', 'qt5-baz'], actions: [{ id: 'remove-orphans', label: 'Remove them', privileged: true, confirm: 'Removes these packages with their configuration files.' }] },
    { id: 'package-cache', title: 'Downloaded packages', status: 'warn', detail: 'Old package files kept in /var/cache/pacman/pkg.', size: 8.8 * GB, items: [], actions: [{ id: 'clean-package-cache', label: 'Keep the latest 2 versions', privileged: true, confirm: 'Deletes older cached versions of each package.' }] },
    { id: 'failed-units', title: 'Failed services', status: 'ok', detail: 'No systemd service has failed.', size: null, items: [], actions: [] },
    { id: 'caches', title: 'Rebuild desktop caches', status: 'info', detail: 'Fixes missing fonts, stale menu entries and wrong icons.', size: null, items: [], actions: [{ id: 'fc-cache', label: 'Rebuild the font cache', privileged: false, confirm: 'Runs fc-cache -f.' }] },
  ],
  maintenance_run: () => 'Done.',
  history_list: () => hist,
  history_clear: () => { hist = []; },
};

window.__TAURI_INTERNALS__ = {
  transformCallback: () => 1,
  invoke: async (cmd, args) => {
    if (cmd.startsWith('plugin:event|')) return 1;
    if (cmd.startsWith('plugin:dialog|open')) return '/home/demo/Pictures';
    if (cmd.startsWith('plugin:')) return null;
    const h = handlers[cmd];
    if (!h) throw `mock: no handler for ${cmd}`;
    await new Promise((r) => setTimeout(r, 120));
    return h(args || {});
  },
};
