pragma circom 2.1.0;

// Batched RLN: k DISTINCT messages under ONE proof.
// Diff vs shipped multi-burn (rln.circom @ multi-message-id):
//   (1) x: scalar -> x[MAX_OUT]  (each slot binds its OWN message hash)
//   (2) yUnmasked[i]: a0 + a1[i] * x      ->  ... * x[i]
// Everything else identical: one shared membership check, selector bits,
// >=1-active, pairwise distinct ids, conditional range check, per-slot
// nullifiers, selector masking.

include "./utils.circom";
include "../node_modules/circomlib/circuits/poseidon.circom";

template RLNBatchA(DEPTH, LIMIT_BIT_SIZE, MAX_OUT) {
    // Private signals
    signal input a0;
    signal input Qs;
    signal input j[MAX_OUT];
    signal input pathElements[DEPTH];
    signal input identityPathIndex[DEPTH];

    // Public signals
    signal input x[MAX_OUT];                    // <-- batching change (1)
    signal input e;
    signal input s[MAX_OUT];

    // Outputs
    signal output y[MAX_OUT];
    signal output r;
    signal output nf[MAX_OUT];

    signal identityCommitment <== Poseidon(1)([a0]);
    signal R <== Poseidon(2)([identityCommitment, Qs]);

    // Membership check (shared across ALL slots — the amortized cost)
    r <== MerkleTreeInclusionProof(DEPTH)(R, identityPathIndex, pathElements);

    // At least one active slot
    var selectorSumVar = 0;
    for (var i = 0; i < MAX_OUT; i++) {
        selectorSumVar += s[i];
    }
    signal selectorSum <== selectorSumVar;
    signal noActiveSelector <== IsZero()(selectorSum);
    noActiveSelector === 0;

    // Active slot checks (unchanged: pairwise distinctness + range)
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

    // SSS share and nullifier calculations — per-slot message binding
    signal a1[MAX_OUT];
    signal yUnmasked[MAX_OUT];
    signal nfUnmasked[MAX_OUT];
    for (var i = 0; i < MAX_OUT; i++) {
        a1[i] <== Poseidon(3)([a0, e, j[i]]);
        yUnmasked[i] <== a0 + a1[i] * x[i];   // <-- batching change (2)
        nfUnmasked[i] <== Poseidon(1)([a1[i]]);
        y[i] <== s[i] * yUnmasked[i];
        nf[i] <== s[i] * nfUnmasked[i];
    }
}

component main { public [x, e, s] } = RLNBatchA(20, 16, 4);
