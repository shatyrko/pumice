#!/usr/bin/env bash
# Installs the privileged helper and its polkit policy. Run once; it asks for sudo.
# Usage: scripts/install-helper.sh [path/to/pumice-helper]
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
helper="${1:-$here/target/release/pumice-helper}"
if [[ ! -x "$helper" ]]; then
  echo "Build it first: cargo build --release -p pumice-helper" >&2
  exit 1
fi
sudo install -Dm755 "$helper" /usr/lib/pumice/pumice-helper
sudo install -Dm644 "$here/packaging/dev.pumice.helper.policy" /usr/share/polkit-1/actions/dev.pumice.helper.policy
echo "Installed /usr/lib/pumice/pumice-helper and the polkit policy."
