// Policy-variant proving benchmarks on zerokit's PRODUCTION engine (iden3 graph
// witness + ark-groth16), separate from the core cases in three_methods.rs:
//   our_dualwindow_k  — batching + long-window burst cap        (k = 4, 8, 16)
//   our_typed_k       — batching + per-class budgets            (k = 4, 8, 16)
//   our_typed_dw_k    — batching + classes + dual-window        (k = 4, 8, 16)
//
// == RUNNING A SUBSET (substring match on case names) ==
//   cargo bench -p rln --bench variants                       # ALL variant cases
//   cargo bench -p rln --bench variants -- our_typed_16       # exactly one case
//   cargo bench -p rln --bench variants -- dualwindow         # all dual-window cases
// Sampling: BENCH_SAMPLES (default 20; use 100 for final runs), BENCH_WARMUP_MS.
use criterion::{criterion_group, criterion_main, Criterion};
use rln::circuit::{graph_from_raw, prove_from_named_inputs, zkey_from_raw, Fr, Graph, Zkey};
use std::fs;
use std::time::Duration;

const DEPTH: usize = 20;
// Rewritten to this repository's zerokit-bench/artifacts by bench-kit/setup.sh.
const ART: &str = "/ARTIFACTS_DIR";

fn have(tag: &str) -> bool {
    std::path::Path::new(&format!("{ART}/{tag}/graph.bin")).exists()
}

fn load(tag: &str) -> (Graph, Zkey) {
    let g = graph_from_raw(&fs::read(format!("{ART}/{tag}/graph.bin")).unwrap(), Some(DEPTH), None)
        .unwrap();
    let z = zkey_from_raw(&fs::read(format!("{ART}/{tag}/rln_final.arkzkey")).unwrap()).unwrap();
    (g, z)
}

// CLI case filter, checked BEFORE artifact I/O (missing artifacts never crash a run).
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

// dual-window inputs: batch (distinct x_i) + a long window (extra messageIdLong array,
// userMessageLimitLong, externalNullifierLong). Long window shared across the batch.
fn dw_inputs(k: u64) -> Vec<(String, Vec<Fr>)> {
    let (pe, pi) = path();
    vec![
        (s("identitySecret"), v(987654321)),
        (s("userMessageLimit"), v(100)),
        (s("userMessageLimitLong"), v(100)),
        (s("messageId"), (1..=k).map(Fr::from).collect()),
        (s("messageIdLong"), (1..=k).map(Fr::from).collect()),
        (s("pathElements"), pe),
        (s("identityPathIndex"), pi),
        (s("x"), (0..k).map(|i| Fr::from(1000 + i)).collect()),
        (s("externalNullifier"), v(424242)),
        (s("externalNullifierLong"), v(777)),
        (s("selectorUsed"), (0..k).map(|_| Fr::from(1u64)).collect()),
    ]
}

// typed-quotas inputs: mixed-class batch, slots sorted by (class, id). Three classes:
// bulk readings (class 0, budget 100), events (class 1, budget 20), alarm (class 2, budget 5).
fn typed_inputs(k: u64, dw: bool) -> Vec<(String, Vec<Fr>)> {
    let (pe, pi) = path();
    let class_of = |i: u64| if i < k / 2 { 0u64 } else if i < k - 1 { 1 } else { 2 };
    let ids: Vec<u64> = (0..k).map(|i| (i % (k / 2).max(1)) + 1).collect();
    let mut inp = vec![
        (s("identitySecret"), v(987654321)),
        (s("rateCommitmentLimits"), v(111222333)),
        (s("classId"), (0..k).map(|i| Fr::from(class_of(i))).collect()),
        (s("classLimit"), (0..k).map(|i| Fr::from([100u64, 20, 5][class_of(i) as usize])).collect()),
        (s("messageId"), ids.iter().map(|&i| Fr::from(i)).collect()),
        (s("pathElements"), pe),
        (s("identityPathIndex"), pi),
        (s("x"), (0..k).map(|i| Fr::from(1000 + i)).collect()),
        (s("extNull"), (0..k).map(|i| Fr::from(424242 + class_of(i))).collect()),
        (s("selectorUsed"), (0..k).map(|_| Fr::from(1u64)).collect()),
    ];
    if dw {
        inp.push((s("messageIdLong"), ids.iter().map(|&i| Fr::from(i)).collect()));
        inp.push((s("classLimitLong"), (0..k).map(|i| Fr::from([1000u64, 100, 10][class_of(i) as usize])).collect()));
        inp.push((s("extNullLong"), (0..k).map(|i| Fr::from(777000 + class_of(i))).collect()));
    }
    inp
}

fn bench(c: &mut Criterion) {
    for k in [4u64, 8, 16, 32, 64] {
        let name = format!("our_dualwindow_{k}");
        if selected(&name) && have(&format!("our_dw_{k}")) {
            let (g, z) = load(&format!("our_dw_{k}"));
            let inp = dw_inputs(k);
            c.bench_function(&name, move |b| {
                b.iter(|| prove_from_named_inputs(inp.clone(), &g, &z))
            });
        }
        let name = format!("our_typed_{k}");
        if selected(&name) && have(&format!("our_typed_{k}")) {
            let (g, z) = load(&format!("our_typed_{k}"));
            let inp = typed_inputs(k, false);
            c.bench_function(&name, move |b| {
                b.iter(|| prove_from_named_inputs(inp.clone(), &g, &z))
            });
        }
        let name = format!("our_typed_dw_{k}");
        if selected(&name) && have(&format!("our_typed_dw_{k}")) {
            let (g, z) = load(&format!("our_typed_dw_{k}"));
            let inp = typed_inputs(k, true);
            c.bench_function(&name, move |b| {
                b.iter(|| prove_from_named_inputs(inp.clone(), &g, &z))
            });
        }
    }
}

fn cfg() -> Criterion {
    let samples = std::env::var("BENCH_SAMPLES").ok().and_then(|x| x.parse().ok()).unwrap_or(20);
    let warmup = std::env::var("BENCH_WARMUP_MS").ok().and_then(|x| x.parse().ok()).unwrap_or(1500);
    Criterion::default()
        .sample_size(samples)
        .warm_up_time(Duration::from_millis(warmup))
}
criterion_group! { name = benches; config = cfg(); targets = bench }
criterion_main!(benches);
