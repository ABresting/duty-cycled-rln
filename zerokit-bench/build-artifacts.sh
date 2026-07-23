#!/bin/bash
# Build zerokit circuit artifacts (r1cs, graph.bin, arkzkey) for each method/N.
# Produces artifacts/<tag>/{graph.bin, rln_final.arkzkey} that zerokit can load.
set -e
BASE="$(cd "$(dirname "$0")/.." && pwd)"
# circom-rln checkout (provides the rln template tree our circuits build against);
# clone https://github.com/rate-limiting-nullifier/circom-rln here and copy
# ../circuits/*.circom into its circuits/ directory before building.
CR="${CIRCOM_RLN:-$BASE/circuits/circom-rln}"
CIRCOM=$BASE/zerokit-bench/tools/circom210
WC=$BASE/zerokit-bench/tools/circom-witnesscalc/target/release/build-circuit
AZ=$BASE/zerokit-bench/tools/ark-zkey/target/release/arkzkey-util
PTAU="${PTAU_OVERRIDE:-$CR/build/pot16.ptau}"
OUT=$BASE/zerokit-bench/artifacts

# tag -> circuit file (relative to $CR)
declare -A CIRCUITS=(
  [their_mb_4]="circuits/rln.circom"
  [their_mb_8]="circuits/their_mb_8.circom"
  [our_ra_4]="circuits/gen_monotonic_4.circom"
  [our_ra_8]="circuits/gen_monotonic_8.circom"
  [our_ra_16]="circuits/gen_monotonic_16.circom"
  [our_ra_32]="circuits/gen_monotonic_32.circom"
  [our_ra_64]="circuits/gen_monotonic_64.circom"
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
  $CIRCOM "$src" --r1cs -o "$dir" >/dev/null 2>&1
  r1cs=$(ls "$dir"/*.r1cs | head -1)
  # 2. graph.bin
  $WC "$src" "$dir/graph.bin" >/dev/null 2>&1
  # 3. zkey (groth16 setup + one contribution)
  npx snarkjs groth16 setup "$r1cs" "$PTAU" "$dir/0.zkey" >/dev/null 2>&1
  npx snarkjs zkey contribute "$dir/0.zkey" "$dir/rln_final.zkey" --name=x -e="seed-$tag" >/dev/null 2>&1
  # 4. arkzkey (arkzkey-util writes <name>.arkzkey next to the zkey)
  cd "$dir"; $AZ rln_final.zkey >/dev/null 2>&1; cd "$CR"
  nl=$(grep -a "" /dev/null; head -c0 </dev/null; echo)
  echo "  -> $(ls -la "$dir/graph.bin" "$dir/rln_final.arkzkey" 2>&1 | awk '{print $NF": "$5" B"}' | tr '\n' '  ')"
done
echo "ALL ARTIFACTS BUILT"
