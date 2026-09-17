// Burst-level proving benchmarks: the REAL gateway workload for a burst of B
// messages, i.e. m = ceil(B/k) sequential batch proofs, swept over capacities
// k in {8, 16, 32, 64} and bursts B in {8..1024} (only fully occupied proofs).
// Validates the model assumption L_g(burst) = m * L_g^batch(k) (three_methods.rs
// measures one proof; this file measures whole bursts).
//   cases: burst_B{8,16,32,64,128,256,512}_k8
//   NOTE: B=512 is ~17 s per iteration — run it as its own invocation
//   (cargo bench -p rln --bench bursts -- burst_B512), not with the rest.
//
// == RUNNING A SUBSET ==
//   cargo bench -p rln --bench bursts                 # all burst sizes
//   cargo bench -p rln --bench bursts -- burst_B64    # one burst size
// Sampling: BENCH_SAMPLES (default 10 here — one iteration proves m times),
//           BENCH_WARMUP_MS.
use criterion::{criterion_group, criterion_main, Criterion};
use rln::circuit::{graph_from_raw, prove_from_named_inputs, zkey_from_raw, Fr, Graph, Zkey};
use std::fs;
use std::time::Duration;

const DEPTH: usize = 20;
const KS: [u64; 4] = [8, 16, 32, 64]; // batch capacities swept per burst
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

/// Inputs for the g-th proof group of a burst at capacity k: slot ids
/// (g*k+1 ..= g*k+k) and distinct message hashes, exactly as a gateway
/// partitions a burst (§3.2.1).
fn group_inputs(k: u64, g: u64) -> Vec<(String, Vec<Fr>)> {
    let (pe, pi) = (vec![Fr::from(0u64); DEPTH], vec![Fr::from(0u64); DEPTH]);
    vec![
        (s("a0"), v(987654321)),
        (s("Qs"), v(2048)), // ids reach B; keep the quota above them
        (s("j"), (1..=k).map(|i| Fr::from(g * k + i)).collect()),
        (s("pathElements"), pe),
        (s("identityPathIndex"), pi),
        (s("x"), (0..k).map(|i| Fr::from(1000 + g * k + i)).collect()),
        (s("e"), v(424242)),
        (s("s"), (0..k).map(|_| Fr::from(1u64)).collect()),
    ]
}

fn bench(c: &mut Criterion) {
    for k in KS {
        let tag = format!("our_batch_{k}");
        if !std::path::Path::new(&format!("{ART}/{tag}/graph.bin")).exists() {
            continue;
        }
        let (graph, zkey) = load(&tag);
        for b_total in [8u64, 16, 32, 64, 128, 256, 512, 1024] {
            if b_total < k {
                continue; // only fully occupied proofs
            }
            let name = format!("burst_B{b_total}_k{k}");
            if !selected(&name) {
                continue;
            }
            let m = b_total.div_ceil(k);
            let groups: Vec<_> = (0..m).map(|g| group_inputs(k, g)).collect();
            let (g, z) = (&graph, &zkey);
            c.bench_function(&name, move |bch| {
                bch.iter(|| {
                    for inp in &groups {
                        prove_from_named_inputs(inp.clone(), g, z);
                    }
                })
            });
        }
    }
}

fn cfg() -> Criterion {
    let samples = std::env::var("BENCH_SAMPLES").ok().and_then(|x| x.parse().ok()).unwrap_or(10);
    let warmup = std::env::var("BENCH_WARMUP_MS").ok().and_then(|x| x.parse().ok()).unwrap_or(1500);
    Criterion::default()
        .sample_size(samples)
        .warm_up_time(Duration::from_millis(warmup))
}
criterion_group! { name = benches; config = cfg(); targets = bench }
criterion_main!(benches);
