pragma circom 2.1.0;
include "./utils.circom";
include "../node_modules/circomlib/circuits/poseidon.circom";
include "../node_modules/circomlib/circuits/comparators.circom";

template RLNTypedDW(DEPTH, LIMIT_BIT_SIZE, MAX_OUT) {
    signal input identitySecret;
    signal input rateCommitmentLimits;      // H of the per-class limit table, bound in rateCommitment
    signal input classId[MAX_OUT];          // small class index per slot (0..255)
    signal input classLimit[MAX_OUT];       // the class's short-window budget for this slot
    signal input messageId[MAX_OUT];
    signal input pathElements[DEPTH];
    signal input identityPathIndex[DEPTH];
    signal input x[MAX_OUT];
    signal input extNull[MAX_OUT];          // per-slot H(epoch, app, class) — public
    signal input messageIdLong[MAX_OUT];
    signal input classLimitLong[MAX_OUT];
    signal input extNullLong[MAX_OUT];
    signal input selectorUsed[MAX_OUT];
    signal output y[MAX_OUT];
    signal output root;
    signal output nullifier[MAX_OUT];
    signal output yLong[MAX_OUT];
    signal output nullifierLong[MAX_OUT];

    signal identityCommitment <== Poseidon(1)([identitySecret]);
    // limits table committed once (its correctness/audit is a relayer-side concern)
    signal rateCommitment <== Poseidon(2)([identityCommitment, rateCommitmentLimits]);
    root <== MerkleTreeInclusionProof(DEPTH)(rateCommitment, identityPathIndex, pathElements);

    var selectorSumVar = 0;
    for (var i = 0; i < MAX_OUT; i++) { selectorSumVar += selectorUsed[i]; }
    signal selectorSum <== selectorSumVar;
    signal noActiveSelector <== IsZero()(selectorSum);
    noActiveSelector === 0;

    // short-window per-class range + packed (class,id) monotonic dedup
    for (var i = 0; i < MAX_OUT; i++) {
        ConditionalRangeCheck(LIMIT_BIT_SIZE)(messageId[i], classLimit[i], selectorUsed[i]);
    }
    signal packed[MAX_OUT];
    for (var i = 0; i < MAX_OUT; i++) {
        packed[i] <== classId[i] * 65536 + messageId[i];
    }
    signal incr[MAX_OUT > 0 ? MAX_OUT - 1 : 0];
    for (var i = 0; i + 1 < MAX_OUT; i++) {
        incr[i] <== LessThan(LIMIT_BIT_SIZE + 8)([packed[i], packed[i+1]]);
        incr[i] === 1;
    }
    // long-window per-class range + packed monotonic dedup
    for (var i = 0; i < MAX_OUT; i++) {
        ConditionalRangeCheck(LIMIT_BIT_SIZE)(messageIdLong[i], classLimitLong[i], selectorUsed[i]);
    }
    signal packedL[MAX_OUT];
    for (var i = 0; i < MAX_OUT; i++) {
        packedL[i] <== classId[i] * 65536 + messageIdLong[i];
    }
    signal incrL[MAX_OUT > 0 ? MAX_OUT - 1 : 0];
    for (var i = 0; i + 1 < MAX_OUT; i++) {
        incrL[i] <== LessThan(LIMIT_BIT_SIZE + 8)([packedL[i], packedL[i+1]]);
        incrL[i] === 1;
    }

    signal a1[MAX_OUT];
    signal yUnmasked[MAX_OUT];
    signal nullifierUnmasked[MAX_OUT];
    signal a1L[MAX_OUT];
    signal yUnmaskedL[MAX_OUT];
    signal nullifierUnmaskedL[MAX_OUT];
    for (var i = 0; i < MAX_OUT; i++) {
        a1[i] <== Poseidon(3)([identitySecret, extNull[i], messageId[i]]);
        yUnmasked[i] <== identitySecret + a1[i] * x[i];
        nullifierUnmasked[i] <== Poseidon(1)([a1[i]]);
        y[i] <== selectorUsed[i] * yUnmasked[i];
        nullifier[i] <== selectorUsed[i] * nullifierUnmasked[i];
        a1L[i] <== Poseidon(3)([identitySecret, extNullLong[i], messageIdLong[i]]);
        yUnmaskedL[i] <== identitySecret + a1L[i] * x[i];
        nullifierUnmaskedL[i] <== Poseidon(1)([a1L[i]]);
        yLong[i] <== selectorUsed[i] * yUnmaskedL[i];
        nullifierLong[i] <== selectorUsed[i] * nullifierUnmaskedL[i];
    }
}
component main { public [x, extNull, extNullLong, selectorUsed] } = RLNTypedDW(20, 16, 64);
