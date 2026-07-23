pragma circom 2.1.0;
include "./utils.circom";
include "../node_modules/circomlib/circuits/poseidon.circom";
include "../node_modules/circomlib/circuits/comparators.circom";

// Batched circuit (distinct x_i + monotonic slot dedup) + dual-window burst cap.
// Each message additionally burns a LONG-window token (shared long epoch), so a
// sustained-burst adversary is throttled to the long-window average. Membership + x
// shared with the short window; second SSS block for the long window.
template RLNBatchDW(DEPTH, LIMIT_BIT_SIZE, MAX_OUT) {
    signal input identitySecret;
    signal input userMessageLimit;        // short-window (per-epoch) limit
    signal input userMessageLimitLong;    // long-window (e.g. daily) limit
    signal input messageId[MAX_OUT];       // short-window token counters
    signal input messageIdLong[MAX_OUT];   // long-window token counters
    signal input pathElements[DEPTH];
    signal input identityPathIndex[DEPTH];
    signal input x[MAX_OUT];
    signal input externalNullifier;        // short epoch
    signal input externalNullifierLong;    // long epoch
    signal input selectorUsed[MAX_OUT];
    signal output y[MAX_OUT];
    signal output root;
    signal output nullifier[MAX_OUT];
    signal output yLong[MAX_OUT];
    signal output nullifierLong[MAX_OUT];

    signal identityCommitment <== Poseidon(1)([identitySecret]);
    // bind BOTH limits so a member cannot lie about either window
    signal rateCommitment <== Poseidon(3)([identityCommitment, userMessageLimit, userMessageLimitLong]);
    root <== MerkleTreeInclusionProof(DEPTH)(rateCommitment, identityPathIndex, pathElements);

    var selectorSumVar = 0;
    for (var i = 0; i < MAX_OUT; i++) { selectorSumVar += selectorUsed[i]; }
    signal selectorSum <== selectorSumVar;
    signal noActiveSelector <== IsZero()(selectorSum);
    noActiveSelector === 0;

    // short-window: range + O(N) monotonic distinctness
    for (var i = 0; i < MAX_OUT; i++) {
        ConditionalRangeCheck(LIMIT_BIT_SIZE)(messageId[i], userMessageLimit, selectorUsed[i]);
    }
    signal incr[MAX_OUT > 0 ? MAX_OUT - 1 : 0];
    for (var i = 0; i + 1 < MAX_OUT; i++) {
        incr[i] <== LessThan(LIMIT_BIT_SIZE)([messageId[i], messageId[i+1]]);
        incr[i] === 1;
    }
    // long-window: range + O(N) monotonic distinctness
    for (var i = 0; i < MAX_OUT; i++) {
        ConditionalRangeCheck(LIMIT_BIT_SIZE)(messageIdLong[i], userMessageLimitLong, selectorUsed[i]);
    }
    signal incrL[MAX_OUT > 0 ? MAX_OUT - 1 : 0];
    for (var i = 0; i + 1 < MAX_OUT; i++) {
        incrL[i] <== LessThan(LIMIT_BIT_SIZE)([messageIdLong[i], messageIdLong[i+1]]);
        incrL[i] === 1;
    }

    signal a1[MAX_OUT];
    signal yUnmasked[MAX_OUT];
    signal nullifierUnmasked[MAX_OUT];
    signal a1L[MAX_OUT];
    signal yUnmaskedL[MAX_OUT];
    signal nullifierUnmaskedL[MAX_OUT];
    for (var i = 0; i < MAX_OUT; i++) {
        // short window share + nullifier
        a1[i] <== Poseidon(3)([identitySecret, externalNullifier, messageId[i]]);
        yUnmasked[i] <== identitySecret + a1[i] * x[i];
        nullifierUnmasked[i] <== Poseidon(1)([a1[i]]);
        y[i] <== selectorUsed[i] * yUnmasked[i];
        nullifier[i] <== selectorUsed[i] * nullifierUnmasked[i];
        // long window share + nullifier (same x[i], same secret, different epoch)
        a1L[i] <== Poseidon(3)([identitySecret, externalNullifierLong, messageIdLong[i]]);
        yUnmaskedL[i] <== identitySecret + a1L[i] * x[i];
        nullifierUnmaskedL[i] <== Poseidon(1)([a1L[i]]);
        yLong[i] <== selectorUsed[i] * yUnmaskedL[i];
        nullifierLong[i] <== selectorUsed[i] * nullifierUnmaskedL[i];
    }
}
component main { public [x, externalNullifier, externalNullifierLong, selectorUsed] } = RLNBatchDW(20, 16, 4);
