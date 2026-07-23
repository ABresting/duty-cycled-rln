// RLN methods on zerokit's PRODUCTION engine (iden3 graph witness + ark-groth16),
// at N=4 and N=8. Apples-to-apples: same engine, same tool, same tree depth 20.
//   (1) single            — one message per proof (existing baseline; N msgs => N proofs)
//   (2) their_multiburn_N — shipped multi_message_id, single x, k message_ids (existing)
//   (3) our_routeA_N      — batching: distinct x_i per message + monotonic dedup   (OURS)
//   (dual-window / typed variants live in benches/variants.rs — separate runs)
//
// == RUNNING A SUBSET (criterion name filter — substring match on case names) ==
//   cargo bench -p rln --bench three_methods                      # ALL cases
//   cargo bench -p rln --bench three_methods -- our_typed         # both typed + typed_dw, N=4+8
//   cargo bench -p rln --bench three_methods -- our_routeA_8      # exactly one case
//   cargo bench -p rln --bench three_methods -- _8                # every N=8 case
//   cargo bench -p rln --bench three_methods -- single            # just the baseline
// Sample count: see cfg() at the bottom (currently 20 timed proofs per case + ~1.5s warmup).
use criterion::{criterion_group, criterion_main, Criterion};
use std::time::Duration;
use rln::circuit::{
    default_graph_multi, default_graph_single, default_zkey_multi, default_zkey_single,
    graph_from_raw, prove_from_named_inputs, zkey_from_raw, Fr, Graph, Zkey,
};
use std::fs;

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

fn s(k: &str) -> String {
    k.to_string()
}
fn v(n: u64) -> Vec<Fr> {
    vec![Fr::from(n)]
}
fn path() -> (Vec<Fr>, Vec<Fr>) {
    (vec![Fr::from(0u64); DEPTH], vec![Fr::from(0u64); DEPTH])
}

// common fields shared by all circuits
fn base_inputs() -> Vec<(String, Vec<Fr>)> {
    let (pe, pi) = path();
    vec![
        (s("identitySecret"), v(987654321)),
        (s("userMessageLimit"), v(100)),
        (s("pathElements"), pe),
        (s("identityPathIndex"), pi),
        (s("externalNullifier"), v(424242)),
    ]
}

// single-message circuit inputs (scalar messageId + scalar x, no selectorUsed)
fn single_inputs() -> Vec<(String, Vec<Fr>)> {
    let mut inp = base_inputs();
    inp.push((s("messageId"), v(1)));
    inp.push((s("x"), v(1000)));
    inp
}

// multi circuits: N message_ids + selectorUsed. `x_array` = true => distinct x_i (ours),
// false => single shared x (their multi-burn).
fn multi_inputs(n: u64, x_array: bool) -> Vec<(String, Vec<Fr>)> {
    let mut inp = base_inputs();
    inp.push((s("messageId"), (1..=n).map(Fr::from).collect()));
    inp.push((s("selectorUsed"), (0..n).map(|_| Fr::from(1u64)).collect()));
    if x_array {
        inp.push((s("x"), (0..n).map(|i| Fr::from(1000 + i)).collect()));
    } else {
        inp.push((s("x"), v(1000)));
    }
    inp
}


// partial occupancy: only the first p of n slots active (selector 0 for the rest).
// The monotonic chain is unconditional, so messageId stays strictly increasing
// across all slots; range checks apply to active slots only.
fn multi_inputs_partial(n: u64, p: u64) -> Vec<(String, Vec<Fr>)> {
    let mut inp = base_inputs();
    inp.push((s("messageId"), (1..=n).map(Fr::from).collect()));
    inp.push((s("selectorUsed"), (0..n).map(|i| Fr::from(u64::from(i < p))).collect()));
    inp.push((s("x"), (0..n).map(|i| Fr::from(1000 + i)).collect()));
    inp
}

// Does this case name match the CLI filter (`cargo bench ... -- <filter>`)?
// Substring match, same as criterion's own filtering. No filter arg => run everything.
// We check it OURSELVES before loading artifacts, so filtered-out cases cost zero I/O
// and missing artifacts for cases you didn't ask for can't crash the run.
fn selected(name: &str) -> bool {
    let args: Vec<String> = std::env::args().collect();
    // bench binaries receive: [bin, ...criterion flags..., <filter>?, "--bench"]
    let filters: Vec<&String> = args[1..]
        .iter()
        .filter(|a| !a.starts_with('-') && *a != "--bench")
        .collect();
    filters.is_empty() || filters.iter().any(|f| name.contains(f.as_str()))
}

fn bench(c: &mut Criterion) {
    // (1) single — shipped production single circuit
    if selected("single") {
        let (gs, zs) = (default_graph_single(), default_zkey_single());
        let si = single_inputs();
        c.bench_function("single", |b| {
            b.iter(|| prove_from_named_inputs(si.clone(), gs, zs))
        });
    }

    // (2) their multi-burn: N=4 uses the shipped max_out_4, N=8 our built artifact
    if selected("their_multiburn_4") {
        let (gm4, zm4) = (default_graph_multi(), default_zkey_multi());
        let their4 = multi_inputs(4, false);
        c.bench_function("their_multiburn_4", |b| {
            b.iter(|| prove_from_named_inputs(their4.clone(), gm4, zm4))
        });
    }
    if selected("their_multiburn_8") {
        let (gt8, zt8) = load("their_mb_8");
        let their8 = multi_inputs(8, false);
        c.bench_function("their_multiburn_8", |b| {
            b.iter(|| prove_from_named_inputs(their8.clone(), &gt8, &zt8))
        });
    }

    // (3) batched circuit (distinct x_i per message) — uniform loop over capacities
    for n in [4u64, 8, 16, 32, 64] {
        let name = format!("our_routeA_{n}");
        if selected(&name) && have(&format!("our_ra_{n}")) {
            let (g, z) = load(&format!("our_ra_{n}"));
            let inp = multi_inputs(n, true);
            c.bench_function(&name, move |b| {
                b.iter(|| prove_from_named_inputs(inp.clone(), &g, &z))
            });
        }
    }

    // partial occupancy: the circuit is fixed, so cost should equal a full proof;
    // these cases confirm that the ceil(B/k) model holds for non-multiple bursts.
    for (n, p) in [(8u64, 3u64), (64, 5), (64, 32)] {
        let name = format!("our_partial_{p}of{n}");
        if selected(&name) && have(&format!("our_ra_{n}")) {
            let (g, z) = load(&format!("our_ra_{n}"));
            let inp = multi_inputs_partial(n, p);
            c.bench_function(&name, move |b| {
                b.iter(|| prove_from_named_inputs(inp.clone(), &g, &z))
            });
        }
    }
}

// sample_size 20 + short warmup: fast re-runs (~30s) for iteration. Bump back up for
// camera-ready numbers if desired.
fn cfg() -> Criterion {
    let samples = std::env::var("BENCH_SAMPLES").ok().and_then(|x| x.parse().ok()).unwrap_or(20);
    let warmup = std::env::var("BENCH_WARMUP_MS").ok().and_then(|x| x.parse().ok()).unwrap_or(1500);
    Criterion::default()
        .sample_size(samples)
        .warm_up_time(Duration::from_millis(warmup))
}
criterion_group! { name = benches; config = cfg(); targets = bench }
criterion_main!(benches);
