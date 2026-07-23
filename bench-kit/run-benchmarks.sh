#!/bin/bash
# The full benchmark pipeline. Run AFTER setup.sh, on an otherwise idle
# machine:
#     nohup bash bench-kit/run-benchmarks.sh > campaign.log 2>&1 &
# Stages run strictly in sequence; each writes a CSV into zerokit-bench/results/:
#   campaign-*        proving medians, all circuits (100 samples per case)
#   verify-*          verification medians, prepared verifying key (100 samples)
#   sustained-*       per-proof wall times inside consecutive-proof runs
#   bursts-family-*   whole-burst medians, k=8 and k=64 (10 samples per point);
#                     set ALL_BURST_CAPACITIES=1 to add the k=16 and k=32 families
# SKIP_CAMPAIGN=1 skips the first stage when resuming an interrupted run.
set -euo pipefail
source "$HOME/.cargo/env" 2>/dev/null || true
BASE="$(cd "$(dirname "$0")/.." && pwd)"
cd "$BASE/zerokit-bench"
DATE=$(date +%Y%m%d-%H%M)
mkdir -p results

collect() { # collect <outfile> <python-expr over `case`> <samples-label>
    OUT="results/$1" FILT="$2" SAMPLES="$3" python3 - <<'PYEOF'
import json, glob, os, platform, datetime
out, filt = os.environ['OUT'], os.environ['FILT']
rows = []
for est in sorted(glob.glob('zerokit/target/criterion/*/new/estimates.json')):
    case = est.split('/')[-3]
    if not eval(filt, {'case': case}):
        continue
    e = json.load(open(est))
    rows.append((case, e['median']['point_estimate'] / 1e6))
cpu = ''
try:
    for line in open('/proc/cpuinfo'):
        if line.startswith('model name'):
            cpu = line.split(':', 1)[1].strip(); break
except OSError:
    import subprocess
    cpu = subprocess.run(['sysctl', '-n', 'machdep.cpu.brand_string'],
                         capture_output=True, text=True).stdout.strip()
with open(out, 'w') as f:
    f.write(f'# host={platform.node()} cpu="{cpu}" cores={os.cpu_count()} '
            f"samples={os.environ['SAMPLES']} date={datetime.date.today()}\n")
    f.write("case,median_ms\n")
    for c, m in rows:
        f.write(f"{c},{m:.2f}\n")
print(f"wrote {out} ({len(rows)} cases)", flush=True)
PYEOF
}

if [ -z "${SKIP_CAMPAIGN:-}" ]; then
echo "== campaign: three_methods + variants (100 samples) =="
(cd zerokit &&
 BENCH_SAMPLES=100 cargo bench -p rln --bench three_methods &&
 BENCH_SAMPLES=100 cargo bench -p rln --bench variants)
collect "campaign-$(hostname)-$DATE.csv" "not case.startswith(('burst_', 'verify_'))" 100
fi

echo "== verify (100 samples) =="
(cd zerokit && BENCH_SAMPLES=100 cargo bench -p rln --bench verify)
collect "verify-$(hostname)-$DATE.csv" "case.startswith('verify_')" 100

echo "== sustained =="
run_sustained() {
    (cd zerokit && TAG="$1" SLOTS="$2" COUNT="$3" cargo run --release -q -p rln --example sustained) \
        > "results/sustained-$1-$(hostname)-$DATE.csv"
    echo "wrote results/sustained-$1-$(hostname)-$DATE.csv"
}
run_sustained single 1 100
run_sustained our_ra_8 8 64
run_sustained our_ra_16 16 32
run_sustained our_ra_32 32 16
run_sustained our_ra_64 64 16

echo "== bursts: k=8 and k=64 families (10 samples) =="
(cd zerokit &&
 BENCH_SAMPLES=10 cargo bench -p rln --bench bursts -- _k8 &&
 BENCH_SAMPLES=10 cargo bench -p rln --bench bursts -- _k64)
collect "bursts-family-$(hostname)-$DATE.csv" "case.startswith('burst_') and (case.endswith('_k8') or case.endswith('_k64'))" 10

# optional: the middle burst capacities (set ALL_BURST_CAPACITIES=1 to enable)
if [ -n "${ALL_BURST_CAPACITIES:-}" ]; then
    echo "== bursts: k=16 and k=32 families (10 samples) =="
    (cd zerokit &&
     BENCH_SAMPLES=10 cargo bench -p rln --bench bursts -- _k16 &&
     BENCH_SAMPLES=10 cargo bench -p rln --bench bursts -- _k32)
    collect "bursts-family-k16k32-$(hostname)-$DATE.csv" \
        "case.startswith('burst_') and (case.endswith('_k16') or case.endswith('_k32'))" 10
fi

echo "ALL DONE"
