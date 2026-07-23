#!/bin/bash
# One-time setup: turns a fresh Linux or macOS machine (x86_64 or aarch64) into
# a runner for the paper's proving benchmarks. From a clone of this repository:
#
#     bash bench-kit/setup.sh
#
# It installs Rust 1.93.0 if absent, clones zerokit at the pinned commit
# (zerokit-bench/zerokit-additions/BASE-COMMIT.txt), applies the patch and the
# benchmark sources, points them at zerokit-bench/artifacts/, and builds.
# Finishes by printing KIT READY.
set -euo pipefail
BASE="$(cd "$(dirname "$0")/.." && pwd)"
ADD="$BASE/zerokit-bench/zerokit-additions"

# --- toolchain (Rust pinned to match the reference runs) ---
if ! command -v cargo >/dev/null; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain 1.93.0
  source "$HOME/.cargo/env"
fi
rustup toolchain install 1.93.0 >/dev/null
rustup default 1.93.0

# --- zerokit at the pinned base commit + our additions ---
cd "$BASE/zerokit-bench"
if [ ! -d zerokit ]; then
  git clone https://github.com/vacp2p/zerokit.git
  cd zerokit && git checkout "$(grep -oE '[0-9a-f]{40}' "$ADD/BASE-COMMIT.txt" | head -1)" && cd ..
fi
cd zerokit
git apply --check "$ADD/zerokit-modifications.patch" 2>/dev/null && git apply "$ADD/zerokit-modifications.patch" || true
cp "$ADD"/benches/*.rs rln/benches/
mkdir -p rln/examples && cp "$ADD"/examples/*.rs rln/examples/ 2>/dev/null || true
python3 - <<'PYEOF'
# register our bench targets (idempotent)
s = open('rln/Cargo.toml').read()
for name in ['three_methods', 'variants', 'bursts', 'verify', 'batch_vs_single']:
    if f'name = "{name}"' not in s:
        s = s.rstrip() + f'\n\n[[bench]]\nname = "{name}"\nharness = false\nrequired-features = ["pmtree-ft"]\n'
open('rln/Cargo.toml','w').write(s)
PYEOF

# --- artifact path: replace the /ARTIFACTS_DIR token in the bench sources ---
REF=/ARTIFACTS_DIR
ART_REAL="$BASE/zerokit-bench/artifacts" REF_PATH="$REF" python3 - <<'PYEOF'
import glob, io, os
ref, real = os.environ['REF_PATH'], os.environ['ART_REAL']
if ref != real:
    for f in glob.glob('rln/benches/*.rs') + glob.glob('rln/examples/*.rs'):
        t = io.open(f).read()
        if ref in t:
            io.open(f, 'w').write(t.replace(ref, real))
            print(f'rewrote artifact path in {f}')
PYEOF

cargo bench -p rln --bench three_methods --no-run
echo "KIT READY. Record before running: cat /proc/cpuinfo | grep 'model name' | head -1; nproc; free -h."
echo "Pi only: check throttling AFTER the run: vcgencmd get_throttled  (0x0 = clean run)."
