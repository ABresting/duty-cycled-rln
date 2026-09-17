// Relayer-side verification benchmarks: Groth16 verify of one batch proof at
// capacity k, with a prepared verifying key (relayers cache it), plus the
// serialized proof size. Complements the prove-side benches.
//   cases: verify_k{4,8,16,32,64} and verify_single
//   cargo bench -p rln --bench verify              # all
//   cargo bench -p rln --bench verify -- verify_k32
use ark_serialize::{CanonicalSerialize, Compress};
use criterion::{criterion_group, criterion_main, Criterion};
use rln::circuit::{
    default_graph_single, default_zkey_single, graph_from_raw, prepared_vk,
    prove_with_publics_from_named_inputs, verify_with_prepared_vk, zkey_from_raw, Fr, Graph, Zkey,
};
use std::fs;
use std::time::Duration;

const DEPTH: usize = 20;
// Rewritten to this repository's zerokit-bench/artifacts by bench-kit/setup.sh.
const ART: &str = "/ARTIFACTS_DIR";

fn load(tag: &str) -> (Graph, Zkey) {
    let g = graph_from_raw(&fs::read(format!("{ART}/{tag}/graph.bin")).unwrap(), Some(DEPTH), None)
        .unwrap();
    let z = zkey_from_raw(&fs::read(format!("{ART}/{tag}/rln_final.arkzkey")).unwrap()).unwrap();
    (g, z)
}

fn selected(name: &str) -> bool {
    let args: Vec<String> = std::env::args().collect();
    let filters: Vec<&String> = args[1..]
        .iter()
        .filter(|a| !a.starts_with('-') && *a != "--bench")
        .collect();
    filters.is_empty() || filters.iter().any(|f| name.contains(f.as_str()))
}

fn s(k: &str) -> String {
    k.to_string()
}
fn v(n: u64) -> Vec<Fr> {
    vec![Fr::from(n)]
}
fn path() -> (Vec<Fr>, Vec<Fr>) {
    (vec![Fr::from(0u64); DEPTH], vec![Fr::from(0u64); DEPTH])
}

fn single_inputs() -> Vec<(String, Vec<Fr>)> {
    let (pe, pi) = path();
    vec![
        (s("identitySecret"), v(987654321)),
        (s("userMessageLimit"), v(100)),
        (s("pathElements"), pe),
        (s("identityPathIndex"), pi),
        (s("externalNullifier"), v(424242)),
        (s("messageId"), v(1)),
        (s("x"), v(1000)),
    ]
}

// our_batch_k: field names follow the paper's notation (a0, Qs, e, j, s; Section 5.1).
fn batch_inputs(k: u64) -> Vec<(String, Vec<Fr>)> {
    let (pe, pi) = path();
    vec![
        (s("a0"), v(987654321)),
        (s("Qs"), v(100)),
        (s("pathElements"), pe),
        (s("identityPathIndex"), pi),
        (s("e"), v(424242)),
        (s("j"), (1..=k).map(Fr::from).collect()),
        (s("s"), (0..k).map(|_| Fr::from(1u64)).collect()),
        (s("x"), (0..k).map(|i| Fr::from(1000 + i)).collect()),
    ]
}

fn bench(c: &mut Criterion) {
    if selected("verify_single") {
        let (g, z) = (default_graph_single(), default_zkey_single());
        let (proof, publics) = prove_with_publics_from_named_inputs(single_inputs(), g, z);
        let mut bytes = Vec::new();
        proof.serialize_with_mode(&mut bytes, Compress::Yes).unwrap();
        println!("verify_single: proof {} B, {} public inputs", bytes.len(), publics.len());
        let pvk = prepared_vk(z);
        c.bench_function("verify_single", move |b| {
            b.iter(|| assert!(verify_with_prepared_vk(&pvk, &proof, &publics)))
        });
    }
    for k in [4u64, 8, 16, 32, 64] {
        let name = format!("verify_k{k}");
        if !selected(&name) || !std::path::Path::new(&format!("{ART}/our_batch_{k}/graph.bin")).exists() {
            continue;
        }
        let (g, z) = load(&format!("our_batch_{k}"));
        let (proof, publics) = prove_with_publics_from_named_inputs(batch_inputs(k), &g, &z);
        let mut bytes = Vec::new();
        proof.serialize_with_mode(&mut bytes, Compress::Yes).unwrap();
        println!("{name}: proof {} B, {} public inputs", bytes.len(), publics.len());
        let pvk = prepared_vk(&z);
        c.bench_function(&name, move |b| {
            b.iter(|| assert!(verify_with_prepared_vk(&pvk, &proof, &publics)))
        });
    }
}

fn cfg() -> Criterion {
    let samples = std::env::var("BENCH_SAMPLES").ok().and_then(|x| x.parse().ok()).unwrap_or(100);
    let warmup = std::env::var("BENCH_WARMUP_MS").ok().and_then(|x| x.parse().ok()).unwrap_or(1000);
    Criterion::default()
        .sample_size(samples)
        .warm_up_time(Duration::from_millis(warmup))
}
criterion_group! { name = benches; config = cfg(); targets = bench }
criterion_main!(benches);
