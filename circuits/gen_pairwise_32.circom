pragma circom 2.1.0;
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
    }

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
component main { public [x, e, s] } = RLNBatch(20, 16, 32);
