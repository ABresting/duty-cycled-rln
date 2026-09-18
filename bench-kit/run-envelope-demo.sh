#!/bin/bash
# Envelope demonstration: builds real batch envelopes (k = 8) and validates them
# as a relayer would, on five cases. Run bench-kit/setup.sh first; needs only the
# our_batch_8 artifacts. Takes a few seconds.
set -euo pipefail
BASE="$(cd "$(dirname "$0")/.." && pwd)"
[ -f "$BASE/zerokit-bench/artifacts/our_batch_8/graph.bin" ] || {
  echo "missing artifacts: build our_batch_8 first (bash zerokit-bench/build-artifacts.sh our_batch_8)"; exit 1; }
cd "$BASE/zerokit-bench/zerokit"
cargo build --release -q -p rln --example envelope
E="$PWD/target/release/examples/envelope"
D="$(mktemp -d)"; trap 'rm -rf "$D"' EXIT; cd "$D"
T=1700000000   # fixed timestamp for the cases that must share a rate-limiting window

echo "== 1. honest burst of 32 messages"
"$E" make --burst 32 --out ok.env >/dev/null
"$E" verify --net ok.env.net ok.env | grep -v "^total:"

echo "== 2. one proof corrupted"
"$E" make --burst 32 --corrupt proof --out badproof.env >/dev/null
"$E" verify --net badproof.env.net badproof.env | grep -v "^total:"

echo "== 3. one payload altered after proving"
"$E" make --burst 32 --corrupt payload --out badpayload.env >/dev/null
"$E" verify --net badpayload.env.net badpayload.env | grep -v "^total:"

echo "== 4. two envelopes spending the same quota slots in one window"
"$E" make --burst 8 --timestamp $T --seed 1 --out first.env >/dev/null
"$E" make --burst 8 --timestamp $T --seed 2 --out second.env >/dev/null
"$E" verify --skip-freshness --log nf.log --net first.env.net first.env | grep -v "^total:"
"$E" verify --skip-freshness --log nf.log --net second.env.net second.env | grep -v "^total:"

echo "== 5. stale envelope"
"$E" verify --net first.env.net first.env | grep -v "^total:"
