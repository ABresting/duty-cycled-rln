pragma circom 2.1.0;
include "./utils.circom";
include "../node_modules/circomlib/circuits/poseidon.circom";
include "../node_modules/circomlib/circuits/comparators.circom";

// Batched circuit (distinct x_i + monotonic slot dedup) + dual-window burst cap.
// Each message additionally burns a LONG-window token (shared long epoch), so a
// sustained-burst adversary is throttled to the long-window average. Membership + x
// shared with the short window; second SSS block for the long window.
template RLNBatchDW(DEPTH, LIMIT_BIT_SIZE, MAX_OUT) {
    signal input a0;
    signal input Qs;             // short-window (per-epoch) limit
    signal input Ql;             // long-window (e.g. daily) limit
    signal input j[MAX_OUT];      // short-window slot indices
    signal input jLong[MAX_OUT];  // long-window slot indices
    signal input pathElements[DEPTH];
    signal input identityPathIndex[DEPTH];
    signal input x[MAX_OUT];
    signal input e;                // short epoch
    signal input eLong;            // long epoch
    signal input s[MAX_OUT];
    signal output y[MAX_OUT];
    signal output r;
    signal output nf[MAX_OUT];
    signal output yLong[MAX_OUT];
    signal output nfLong[MAX_OUT];

    signal identityCommitment <== Poseidon(1)([a0]);
    // bind BOTH limits so a member cannot lie about either window
    signal R <== Poseidon(3)([identityCommitment, Qs, Ql]);
    r <== MerkleTreeInclusionProof(DEPTH)(R, identityPathIndex, pathElements);

    var selectorSumVar = 0;
    for (var i = 0; i < MAX_OUT; i++) { selectorSumVar += s[i]; }
    signal selectorSum <== selectorSumVar;
    signal noActiveSelector <== IsZero()(selectorSum);
    noActiveSelector === 0;

    // short-window: range + O(N) monotonic distinctness
    for (var i = 0; i < MAX_OUT; i++) {
        ConditionalRangeCheck(LIMIT_BIT_SIZE)(j[i], Qs, s[i]);
    }
    signal incr[MAX_OUT > 0 ? MAX_OUT - 1 : 0];
    for (var i = 0; i + 1 < MAX_OUT; i++) {
        incr[i] <== LessThan(LIMIT_BIT_SIZE)([j[i], j[i+1]]);
        incr[i] === 1;
    }
    // long-window: range + O(N) monotonic distinctness
    for (var i = 0; i < MAX_OUT; i++) {
        ConditionalRangeCheck(LIMIT_BIT_SIZE)(jLong[i], Ql, s[i]);
    }
    signal incrL[MAX_OUT > 0 ? MAX_OUT - 1 : 0];
    for (var i = 0; i + 1 < MAX_OUT; i++) {
        incrL[i] <== LessThan(LIMIT_BIT_SIZE)([jLong[i], jLong[i+1]]);
        incrL[i] === 1;
    }

    signal a1[MAX_OUT];
    signal yUnmasked[MAX_OUT];
    signal nfUnmasked[MAX_OUT];
    signal a1L[MAX_OUT];
    signal yUnmaskedL[MAX_OUT];
    signal nfUnmaskedL[MAX_OUT];
    for (var i = 0; i < MAX_OUT; i++) {
        // short window share + nullifier
        a1[i] <== Poseidon(3)([a0, e, j[i]]);
        yUnmasked[i] <== a0 + a1[i] * x[i];
        nfUnmasked[i] <== Poseidon(1)([a1[i]]);
        y[i] <== s[i] * yUnmasked[i];
        nf[i] <== s[i] * nfUnmasked[i];
        // long window share + nullifier (same x[i], same secret, different epoch)
        a1L[i] <== Poseidon(3)([a0, eLong, jLong[i]]);
        yUnmaskedL[i] <== a0 + a1L[i] * x[i];
        nfUnmaskedL[i] <== Poseidon(1)([a1L[i]]);
        yLong[i] <== s[i] * yUnmaskedL[i];
        nfLong[i] <== s[i] * nfUnmaskedL[i];
    }
}
component main { public [x, e, eLong, s] } = RLNBatchDW(20, 16, 16);
