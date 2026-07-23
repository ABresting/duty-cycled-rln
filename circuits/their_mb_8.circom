pragma circom 2.1.0;

include "./utils.circom";
include "../node_modules/circomlib/circuits/poseidon.circom";

template RLN(DEPTH, LIMIT_BIT_SIZE, MAX_OUT) {
    // Private signals
    signal input identitySecret;
    signal input userMessageLimit;
    signal input messageId[MAX_OUT];
    signal input pathElements[DEPTH];
    signal input identityPathIndex[DEPTH];

    // Public signals
    signal input x;
    signal input externalNullifier;
    signal input selectorUsed[MAX_OUT];

    // Outputs
    signal output y[MAX_OUT];
    signal output root;
    signal output nullifier[MAX_OUT];

    signal identityCommitment <== Poseidon(1)([identitySecret]);
    signal rateCommitment <== Poseidon(2)([identityCommitment, userMessageLimit]);

    // Membership check
    root <== MerkleTreeInclusionProof(DEPTH)(rateCommitment, identityPathIndex, pathElements);

    // At least one active messageId
    var selectorSumVar = 0;
    for (var i = 0; i < MAX_OUT; i++) {
        selectorSumVar += selectorUsed[i];
    }
    signal selectorSum <== selectorSumVar;
    signal noActiveSelector <== IsZero()(selectorSum);
    noActiveSelector === 0;

    // Active messageId checks
    signal pairBothActive[MAX_OUT][MAX_OUT];
    signal pairSameId[MAX_OUT][MAX_OUT];
    for (var i = 0; i < MAX_OUT; i++) {
        ConditionalRangeCheck(LIMIT_BIT_SIZE)(messageId[i], userMessageLimit, selectorUsed[i]);
        for (var j = i + 1; j < MAX_OUT; j++) {
            pairBothActive[i][j] <== selectorUsed[i] * selectorUsed[j];
            pairSameId[i][j] <== IsZero()(messageId[i] - messageId[j]);
            pairBothActive[i][j] * pairSameId[i][j] === 0;
        }
    }

    // SSS share and nullifier calculations
    signal a1[MAX_OUT];
    signal yUnmasked[MAX_OUT];
    signal nullifierUnmasked[MAX_OUT];
    for (var i = 0; i < MAX_OUT; i++) {
        a1[i] <== Poseidon(3)([identitySecret, externalNullifier, messageId[i]]);
        yUnmasked[i] <== identitySecret + a1[i] * x;
        nullifierUnmasked[i] <== Poseidon(1)([a1[i]]);
        y[i] <== selectorUsed[i] * yUnmasked[i];
        nullifier[i] <== selectorUsed[i] * nullifierUnmasked[i];
    }
}

component main { public [x, externalNullifier, selectorUsed] } = RLN(20, 16, 8);
