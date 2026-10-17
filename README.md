<p align="center"><img src="packaging/icons/pumice-256.png" alt="Pumice" width="128"></p>

# Pumice

A disk cleaner and system care app for Linux, with a graphical interface. Free software (GPL-3.0).

Pumice finds what fills your disk, cleans what is safe to clean, and keeps an eye on the machine.
It is built with Tauri 2 (Rust back end, Vue 3 front end) and works on Arch first; the Debian and
Fedora paths are in place but less tested.

It is inspired by [Mole](https://github.com/tw93/mole) (macOS cleaner, GPL-3.0) and
[MangoDisk](https://github.com/harry0703/MangoDisk) (disk cleaner and analyzer, GPL-3.0). It is a new
code base, not a fork; the dashboard follows the System Stats widget of the author's desktop bar.

## What it does

| Page | What you get |
| --- | --- |
| **Dashboard** | System health score; CPU per core (P/E cores, clocks, watts), memory, GPU (the NVIDIA GPU is never woken to read it), every temperature and fan, battery and power, disks, network with Wi-Fi, grouped processes. Graphs keep their history. |
| **Clean** | About 40 declarative rules: caches, thumbnails, shader caches, Electron apps, browsers, AUR build caches, Cargo/npm/pip/Go/Gradle, AI model caches, logs, crash reports, privacy (cookies, recent files). Each rule has a risk level; nothing is removed until you press Clean. |
| **Analyze** | Parallel scan of a disk or folder (a 276 GB home folder in 0.3 s warm); list, treemap and sunburst views; drill down by clicking. |
| **Large files** | The biggest files, filtered by size, age and kind; installers and disk images. |
| **Duplicates** | Exact duplicates (size, then 64 KiB hash, then full BLAKE3). One copy is always kept; the check is repeated on the back end before anything moves. |
| **Projects** | `node_modules`, Rust `target`, `.venv`, build output and more, only where the project's own file sits next to it; idle projects are ticked. |
| **Apps** | Installed apps from pacman, apt, dnf, Flatpak, Snap and AppImage; uninstall together with the leftovers in your home folder. |
| **Startup** | XDG autostart entries and systemd user services (switched off, never deleted), plus boot time analysis. |
| **Maintenance** | Unused dependencies, package cache, journal, crash dumps, TRIM, failed services, `.pacnew` files, SMART health, container storage, DNS and desktop caches. |
| **History** | Every operation and the space it freed. |

**AI explanations** (off by default): an *Explain* button next to rules, apps, startup entries and
checks tells you what an item is and whether it is safe to remove. Connect Anthropic or any
OpenAI-compatible server (a local Ollama works) in Settings; the key is kept in the system keyring and
only the name, description and a few example paths of that one item are sent.

## Safety

- Scans only read.
- Every removal passes one policy (`crates/pumice-core/src/safety.rs`): only the home folder and the
  temporary folders; never the home folder or the XDG base folders themselves; never `~/.ssh`,
  `~/.gnupg`, keyrings or password stores; rules never touch Documents, Pictures and the like; a
  parent folder that resolves outside through a symlink is refused.
- The interface never sends a free path to delete. It sends a rule id and paths, and the back end
  checks that they are still matches of that rule (or still project artifacts, or still duplicates
  with the same hash).
- Files you pick go to the Trash unless you turn that off. A **Dry run** switch shows what would
  happen without removing anything.
- Actions that need root run through `pumice-helper` with `pkexec` and polkit: a separate program
  with a fixed list of actions, strict argument checks, and only application packages (packages
  that ship a menu entry) can be removed.

## Install

Arch (builds from this repository, installs the program, the helper and the polkit policy):

```bash
git clone https://github.com/shatyrko/pumice && cd pumice/packaging/arch && makepkg -si
```

Only for your user, without a package (program in `~/.local/bin`, launcher entry and icons in
`~/.local/share`):

```bash
scripts/install-user.sh       # then scripts/install-helper.sh once, for the administrator actions
```

Tagged releases also get a `.deb`, an `.rpm` and an AppImage from CI (not tested on those systems yet).

## Build and run

You need Rust, Node.js, and the Tauri 2 system libraries (WebKitGTK 4.1, GTK 3).

```bash
npm install
npm run tauri dev          # development window
npm run tauri build        # packages (deb, rpm, AppImage)
cargo test --workspace     # engine tests
cargo run -p pumice-core --example probe    # read-only look at this machine through every module
```

The administrator helper is installed once (it asks for sudo):

```bash
cargo build --release -p pumice-helper
scripts/install-helper.sh
```

`npm run dev` alone opens the interface in a browser with sample data (no Rust side), which is
handy for working on the layout.

## Layout

```
crates/pumice-core     the engine: scan, rules (TOML), safety, clean, duplicates, purge,
                       apps, startup, maintenance, system readers
crates/pumice-helper   the privileged helper (pkexec)
src-tauri              thin Tauri commands over the engine, background sampler
src                    Vue 3 interface
packaging              polkit policy, icon
```

Cleanup rules live in `crates/pumice-core/rules/*.toml`; a unit test checks that every pattern
passes the safety policy.

## Not yet

Windows and macOS, and a test of the `.deb`, `.rpm` and AppImage on their own systems. Pull requests
welcome.

## Licence

GPL-3.0-or-later. See `LICENSE`.
