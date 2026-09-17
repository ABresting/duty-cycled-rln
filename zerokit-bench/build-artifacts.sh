#!/bin/bash
# Build zerokit circuit artifacts (r1cs, graph.bin, arkzkey) for each circuit tag.
# Produces artifacts/<tag>/{graph.bin, rln_final.arkzkey} that zerokit can load.
#
# Prerequisites (see README, "Proving artifacts"):
#   circuits/circom-rln/                 circom-rln checkout at the pinned commit, with
#                                        `npm install` run inside it (circomlib, snarkjs)
#                                        and ../circuits/*.circom copied into its circuits/
#   zerokit-bench/tools/circom210        circom 2.1.0 binary
#   zerokit-bench/tools/circom-witnesscalc/target/release/build-circuit   (iden3/circom-witnesscalc)
#   zerokit-bench/tools/ark-zkey/target/release/arkzkey-util             (seemenkina/ark-zkey)
#   circuits/circom-rln/build/pot17.ptau  BN254 powers of tau of size 2^17: the largest
#                                        circuits (k=64 dual-window and typed dual-window)
#                                        have 73,000-74,000 constraints, above the 2^16 limit
# Override locations with CIRCOM_RLN, CIRCOM, WC, AZ, PTAU_OVERRIDE.
set -e
BASE="$(cd "$(dirname "$0")/.." && pwd)"
CR="${CIRCOM_RLN:-$BASE/circuits/circom-rln}"
CIRCOM="${CIRCOM:-$BASE/zerokit-bench/tools/circom210}"
WC="${WC:-$BASE/zerokit-bench/tools/circom-witnesscalc/target/release/build-circuit}"
AZ="${AZ:-$BASE/zerokit-bench/tools/ark-zkey/target/release/arkzkey-util}"
PTAU="${PTAU_OVERRIDE:-$CR/build/pot17.ptau}"
OUT=$BASE/zerokit-bench/artifacts

for need in "$CIRCOM" "$WC" "$AZ" "$PTAU" "$CR/node_modules/circomlib" "$CR/circuits/gen_monotonic_4.circom"; do
  [ -e "$need" ] || { echo "missing prerequisite: $need (see header of this script)"; exit 1; }
done

# tag -> circuit file (relative to $CR)
declare -A CIRCUITS=(
  [their_mb_4]="circuits/rln.circom"
  [their_mb_8]="circuits/their_mb_8.circom"
  [our_batch_4]="circuits/gen_monotonic_4.circom"
  [our_batch_8]="circuits/gen_monotonic_8.circom"
  [our_batch_16]="circuits/gen_monotonic_16.circom"
  [our_batch_32]="circuits/gen_monotonic_32.circom"
  [our_batch_64]="circuits/gen_monotonic_64.circom"
  [our_dw_4]="circuits/gen_monotonic_dw_4.circom"
  [our_dw_8]="circuits/gen_monotonic_dw_8.circom"
  [our_dw_16]="circuits/gen_monotonic_dw_16.circom"
  [our_dw_32]="circuits/gen_monotonic_dw_32.circom"
  [our_dw_64]="circuits/gen_monotonic_dw_64.circom"
  [our_typed_4]="circuits/gen_typed_4.circom"
  [our_typed_8]="circuits/gen_typed_8.circom"
  [our_typed_16]="circuits/gen_typed_16.circom"
  [our_typed_32]="circuits/gen_typed_32.circom"
  [our_typed_64]="circuits/gen_typed_64.circom"
  [our_typed_dw_4]="circuits/gen_typed_dw_4.circom"
  [our_typed_dw_8]="circuits/gen_typed_dw_8.circom"
  [our_typed_dw_16]="circuits/gen_typed_dw_16.circom"
  [our_typed_dw_32]="circuits/gen_typed_dw_32.circom"
  [our_typed_dw_64]="circuits/gen_typed_dw_64.circom"
)

# usage: build-artifacts.sh [tag ...]   (no args = build everything)
TAGS=("$@")
[ ${#TAGS[@]} -eq 0 ] && TAGS=("${!CIRCUITS[@]}")
for tag in "${TAGS[@]}"; do
  src="${CIRCUITS[$tag]}"
  dir="$OUT/$tag"; mkdir -p "$dir"
  echo "########## $tag  ($src) ##########"
  cd "$CR"
  # 1. r1cs
  $CIRCOM "$src" --r1cs -o "$dir" >/dev/null
  r1cs=$(ls "$dir"/*.r1cs | head -1)
  # 2. graph.bin
  $WC "$src" "$dir/graph.bin" >/dev/null
  # 3. zkey (groth16 setup + one contribution)
  npx snarkjs groth16 setup "$r1cs" "$PTAU" "$dir/0.zkey" >/dev/null
  npx snarkjs zkey contribute "$dir/0.zkey" "$dir/rln_final.zkey" --name=x -e="seed-$tag" >/dev/null 2>&1
  # 4. arkzkey (arkzkey-util writes <name>.arkzkey next to the zkey)
  cd "$dir"; $AZ rln_final.zkey >/dev/null; cd "$CR"
  echo "  -> $(ls -la "$dir/graph.bin" "$dir/rln_final.arkzkey" 2>&1 | awk '{print $NF": "$5" B"}' | tr '\n' '  ')"
done
echo "ALL ARTIFACTS BUILT"
