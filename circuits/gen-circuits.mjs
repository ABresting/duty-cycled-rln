// Generate the batched-circuit variants at multiple batch capacities:
//   - "pairwise"  : distinctness via O(N^2) all-pairs IsZero (the shipped multi-burn approach)
//   - "monotonic" : distinctness via O(N) strictly-increasing slot indices (our fix)
// N=1 is the single-message baseline (both variants identical there).
// Signal names follow the paper's notation (Table 1 / Section 5.1):
//   a0 = identity secret, Qs = short-window quota, j = slot index, x = message hash,
//   e = external nullifier, s = selector bits, y = share, nf = nullifier, r = root,
//   R = rate commitment, a1 = slot key.
import { writeFileSync } from "fs";

const Ns = [1, 4, 8, 16, 32, 64];

function pairwiseBody() {
  return `
    // O(N^2) pairwise distinctness (baseline multi-burn approach)
    signal pairBothActive[MAX_OUT][MAX_OUT];
    signal pairSameId[MAX_OUT][MAX_OUT];
    for (var i = 0; i < MAX_OUT; i++) {
        ConditionalRangeCheck(LIMIT_BIT_SIZE)(j[i], Qs, s[i]);
        for (var j2 = i + 1; j2 < MAX_OUT; j2++) {
            pairBothActive[i][j2] <== s[i] * s[j2];
            pairSameId[i][j2] <== IsZero()(j[i] - j[j2]);
            pairBothActive[i][j2] * pairSameId[i][j2] === 0;
        }
    }`;
}

function monotonicBody() {
  // Strictly increasing over ALL slots => all distinct, in O(N).
  // Gateway assigns its own token numbers sequentially, so this is natural.
  // N-1 LessThan gadgets. (For N=1 the loop is empty.)
  return `
    // O(N) strictly-increasing distinctness (our fix): j[i] < j[i+1]
    for (var i = 0; i < MAX_OUT; i++) {
        ConditionalRangeCheck(LIMIT_BIT_SIZE)(j[i], Qs, s[i]);
    }
    signal incr[MAX_OUT > 0 ? MAX_OUT - 1 : 0];
    for (var i = 0; i + 1 < MAX_OUT; i++) {
        incr[i] <== LessThan(LIMIT_BIT_SIZE)([j[i], j[i+1]]);
        incr[i] === 1;
    }`;
}

function circuit(variant, N) {
  const dedup = variant === "pairwise" ? pairwiseBody() : monotonicBody();
  return `pragma circom 2.1.0;
include "./utils.circom";
include "../node_modules/circomlib/circuits/poseidon.circom";
include "../node_modules/circomlib/circuits/comparators.circom";

template RLNBatch(DEPTH, LIMIT_BIT_SIZE, MAX_OUT) {
    signal input a0;
    signal input Qs;
    signal input j[MAX_OUT];
    signal input pathElements[DEPTH];
    signal input identityPathIndex[DEPTH];
    signal input x[MAX_OUT];
    signal input e;
    signal input s[MAX_OUT];
    signal output y[MAX_OUT];
    signal output r;
    signal output nf[MAX_OUT];

    signal identityCommitment <== Poseidon(1)([a0]);
    signal R <== Poseidon(2)([identityCommitment, Qs]);
    r <== MerkleTreeInclusionProof(DEPTH)(R, identityPathIndex, pathElements);

    var selectorSumVar = 0;
    for (var i = 0; i < MAX_OUT; i++) { selectorSumVar += s[i]; }
    signal selectorSum <== selectorSumVar;
    signal noActiveSelector <== IsZero()(selectorSum);
    noActiveSelector === 0;
${dedup}

    signal a1[MAX_OUT];
    signal yUnmasked[MAX_OUT];
    signal nfUnmasked[MAX_OUT];
    for (var i = 0; i < MAX_OUT; i++) {
        a1[i] <== Poseidon(3)([a0, e, j[i]]);
        yUnmasked[i] <== a0 + a1[i] * x[i];
        nfUnmasked[i] <== Poseidon(1)([a1[i]]);
        y[i] <== s[i] * yUnmasked[i];
        nf[i] <== s[i] * nfUnmasked[i];
    }
}
component main { public [x, e, s] } = RLNBatch(20, 16, ${N});
`;
}

for (const N of Ns) {
  for (const v of ["pairwise", "monotonic"]) {
    const fn = `circuits/gen_${v}_${N}.circom`;
    writeFileSync(fn, circuit(v, N));
    console.log("wrote", fn);
  }
}
