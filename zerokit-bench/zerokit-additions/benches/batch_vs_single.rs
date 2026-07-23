// Batch-vs-single proving on zerokit's PRODUCTION engine (ark-groth16).
// Compares one multi_message_id (max_out=4) proof against a single-message proof.
// Batch(4) vs 4x single = the real-engine batching speedup at N=4.
// Criterion: 100-sample medians + CI (rigorous, unlike ad-hoc reps).
use criterion::{criterion_group, criterion_main, Criterion};
use rln::circuit::{default_graph_multi_v3, default_zkey_multi_v3};
use rln::prelude::*;
use zerokit_utils::merkle_tree::{ZerokitMerkleProof, ZerokitMerkleTree};

fn bench(c: &mut Criterion) {
    let (identity_secret, id_commitment) = keygen();
    let user_message_limit = Fr::from(100);
    let rate_commitment = poseidon_hash_pair(id_commitment, user_message_limit);

    let mut tree = PmTree::default(DEFAULT_TREE_DEPTH).unwrap();
    tree.set(3, rate_commitment).unwrap();
    let mp = tree.proof(3).unwrap();

    let external_nullifier =
        poseidon_hash_pair(hash_to_field_le(b"epoch"), hash_to_field_le(b"rlnid"));
    let x = hash_to_field_le(b"signal");

    // --- single-message (v3 single circuit) ---
    let rln_single = RLNBuilder::stateless().build();
    let w_single = RLNWitnessInputV3::new_single()
        .identity_secret(identity_secret.clone())
        .user_message_limit(user_message_limit)
        .path_elements(mp.get_path_elements())
        .identity_path_index(mp.get_path_index())
        .x(x)
        .external_nullifier(external_nullifier)
        .message_id(Fr::from(1))
        .build()
        .unwrap();
    c.bench_function("zerokit_single_proof", |b| {
        b.iter(|| {
            let _ = rln_single.generate_proof(&w_single).unwrap();
        })
    });

    // --- batch of 4 distinct message_ids in ONE proof (multi_message_id, max_out=4) ---
    let rln_multi = RLNBuilder::stateless()
        .graph(default_graph_multi_v3().clone())
        .zkey(default_zkey_multi_v3().clone())
        .build();
    let w_multi = RLNWitnessInputV3::new_multi()
        .identity_secret(identity_secret.clone())
        .user_message_limit(user_message_limit)
        .path_elements(mp.get_path_elements())
        .identity_path_index(mp.get_path_index())
        .x(x)
        .external_nullifier(external_nullifier)
        .message_ids(vec![Fr::from(1), Fr::from(2), Fr::from(3), Fr::from(4)])
        .selector_used(vec![true, true, true, true])
        .build()
        .unwrap();
    c.bench_function("zerokit_multi4_proof", |b| {
        b.iter(|| {
            let _ = rln_multi.generate_proof(&w_multi).unwrap();
        })
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
