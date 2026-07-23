#!/bin/bash
# Folding benchmark sweep: one burst of B messages folded with several
# step capacities K. Appends the example's CSV output to results/fold-<host>-<date>.csv.
#
# Usage:
#   bash run-fold-sweep.sh              # default: B=32, K in 1,4,8,16
#   bash run-fold-sweep.sh 64 1,8,16    # custom burst and K list
set -e
cd "$(dirname "$0")/nova"
B="${1:-32}"
KS="${2:-1,4,8,16}"
mkdir -p ../results
OUT="../results/fold-$(hostname)-$(date +%Y%m%d-%H%M).csv"
echo "# host=$(hostname) B=$B Ks=$KS date=$(date +%F)" | tee "$OUT"
cargo run --release --example rln_fold --features test-utils -- "$B" "$KS" | tee -a "$OUT"
echo "wrote $OUT"
