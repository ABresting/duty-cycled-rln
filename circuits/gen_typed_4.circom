pragma circom 2.1.0;
include "./utils.circom";
include "../node_modules/circomlib/circuits/poseidon.circom";
include "../node_modules/circomlib/circuits/comparators.circom";

template RLNTyped(DEPTH, LIMIT_BIT_SIZE, MAX_OUT) {
    signal input a0;
    signal input rateCommitmentLimits;      // H of the per-class limit table, bound in R
    signal input c[MAX_OUT];                // small class index per slot (0..255)
    signal input Qc[MAX_OUT];               // the class's short-window budget for this slot
    signal input j[MAX_OUT];
    signal input pathElements[DEPTH];
    signal input identityPathIndex[DEPTH];
    signal input x[MAX_OUT];
    signal input e[MAX_OUT];                // per-slot H(epoch, app, class) — public
    signal input s[MAX_OUT];
    signal output y[MAX_OUT];
    signal output r;
    signal output nf[MAX_OUT];

    signal identityCommitment <== Poseidon(1)([a0]);
    // limits table committed once (its correctness/audit is a relayer-side concern)
    signal R <== Poseidon(2)([identityCommitment, rateCommitmentLimits]);
    r <== MerkleTreeInclusionProof(DEPTH)(R, identityPathIndex, pathElements);

    var selectorSumVar = 0;
    for (var i = 0; i < MAX_OUT; i++) { selectorSumVar += s[i]; }
    signal selectorSum <== selectorSumVar;
    signal noActiveSelector <== IsZero()(selectorSum);
    noActiveSelector === 0;

    // short-window per-class range + packed (class,id) monotonic dedup
    for (var i = 0; i < MAX_OUT; i++) {
        ConditionalRangeCheck(LIMIT_BIT_SIZE)(j[i], Qc[i], s[i]);
    }
    signal packed[MAX_OUT];
    for (var i = 0; i < MAX_OUT; i++) {
        packed[i] <== c[i] * 65536 + j[i];
    }
    signal incr[MAX_OUT > 0 ? MAX_OUT - 1 : 0];
    for (var i = 0; i + 1 < MAX_OUT; i++) {
        incr[i] <== LessThan(LIMIT_BIT_SIZE + 8)([packed[i], packed[i+1]]);
        incr[i] === 1;
    }

    signal a1[MAX_OUT];
    signal yUnmasked[MAX_OUT];
    signal nfUnmasked[MAX_OUT];
    for (var i = 0; i < MAX_OUT; i++) {
        a1[i] <== Poseidon(3)([a0, e[i], j[i]]);
        yUnmasked[i] <== a0 + a1[i] * x[i];
        nfUnmasked[i] <== Poseidon(1)([a1[i]]);
        y[i] <== s[i] * yUnmasked[i];
        nf[i] <== s[i] * nfUnmasked[i];
    }
}
component main { public [x, e, s] } = RLNTyped(20, 16, 4);
