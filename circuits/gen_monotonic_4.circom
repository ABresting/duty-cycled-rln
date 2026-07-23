pragma circom 2.1.0;
include "./utils.circom";
include "../node_modules/circomlib/circuits/poseidon.circom";
include "../node_modules/circomlib/circuits/comparators.circom";

template RLNBatch(DEPTH, LIMIT_BIT_SIZE, MAX_OUT) {
    signal input identitySecret;
    signal input userMessageLimit;
    signal input messageId[MAX_OUT];
    signal input pathElements[DEPTH];
    signal input identityPathIndex[DEPTH];
    signal input x[MAX_OUT];
    signal input externalNullifier;
    signal input selectorUsed[MAX_OUT];
    signal output y[MAX_OUT];
    signal output root;
    signal output nullifier[MAX_OUT];

    signal identityCommitment <== Poseidon(1)([identitySecret]);
    signal rateCommitment <== Poseidon(2)([identityCommitment, userMessageLimit]);
    root <== MerkleTreeInclusionProof(DEPTH)(rateCommitment, identityPathIndex, pathElements);

    var selectorSumVar = 0;
    for (var i = 0; i < MAX_OUT; i++) { selectorSumVar += selectorUsed[i]; }
    signal selectorSum <== selectorSumVar;
    signal noActiveSelector <== IsZero()(selectorSum);
    noActiveSelector === 0;

    // O(N) strictly-increasing distinctness (our fix): messageId[i] < messageId[i+1]
    for (var i = 0; i < MAX_OUT; i++) {
        ConditionalRangeCheck(LIMIT_BIT_SIZE)(messageId[i], userMessageLimit, selectorUsed[i]);
    }
    signal incr[MAX_OUT > 0 ? MAX_OUT - 1 : 0];
    for (var i = 0; i + 1 < MAX_OUT; i++) {
        incr[i] <== LessThan(LIMIT_BIT_SIZE)([messageId[i], messageId[i+1]]);
        incr[i] === 1;
    }

    signal a1[MAX_OUT];
    signal yUnmasked[MAX_OUT];
    signal nullifierUnmasked[MAX_OUT];
    for (var i = 0; i < MAX_OUT; i++) {
        a1[i] <== Poseidon(3)([identitySecret, externalNullifier, messageId[i]]);
        yUnmasked[i] <== identitySecret + a1[i] * x[i];
        nullifierUnmasked[i] <== Poseidon(1)([a1[i]]);
        y[i] <== selectorUsed[i] * yUnmasked[i];
        nullifier[i] <== selectorUsed[i] * nullifierUnmasked[i];
    }
}
component main { public [x, externalNullifier, selectorUsed] } = RLNBatch(20, 16, 4);
