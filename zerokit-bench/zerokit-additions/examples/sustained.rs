// Sustained proving: COUNT consecutive batch proofs, per-proof wall time to
// stdout as CSV. Emulates a gateway proving a whole burst back to back, so the
// steady-state median re-fits Lg^batch(k) without isolated-benchmark overheads.
//
//   TAG=our_batch_8 SLOTS=8 COUNT=128 cargo run --release -p rln --example sustained
//
// TAG=single benchmarks the shipped single-message circuit instead.
use rln::circuit::{
    default_graph_single, default_zkey_single, graph_from_raw, prove_from_named_inputs,
    zkey_from_raw, Fr,
};
use std::fs;
use std::time::Instant;

const DEPTH: usize = 20;
const ART: &str =
    "/ARTIFACTS_DIR";

fn s(k: &str) -> String {
    k.to_string()
}
fn v(n: u64) -> Vec<Fr> {
    vec![Fr::from(n)]
}

// deployed circuit (TAG=single): field names fixed by zerokit/circom-rln upstream.
fn base_inputs_deployed() -> Vec<(String, Vec<Fr>)> {
    vec![
        (s("identitySecret"), v(987654321)),
        (s("userMessageLimit"), v(100)),
        (s("pathElements"), vec![Fr::from(0u64); DEPTH]),
        (s("identityPathIndex"), vec![Fr::from(0u64); DEPTH]),
        (s("externalNullifier"), v(424242)),
    ]
}

// our batch circuit: field names follow the paper's notation (a0, Qs, e; Section 5.1).
fn base_inputs_ours() -> Vec<(String, Vec<Fr>)> {
    vec![
        (s("a0"), v(987654321)),
        (s("Qs"), v(100)),
        (s("pathElements"), vec![Fr::from(0u64); DEPTH]),
        (s("identityPathIndex"), vec![Fr::from(0u64); DEPTH]),
        (s("e"), v(424242)),
    ]
}

fn main() {
    let tag = std::env::var("TAG").unwrap_or_else(|_| "our_batch_8".into());
    let slots: u64 = std::env::var("SLOTS").ok().and_then(|x| x.parse().ok()).unwrap_or(8);
    let count: usize = std::env::var("COUNT").ok().and_then(|x| x.parse().ok()).unwrap_or(64);

    let mut inp = if tag == "single" { base_inputs_deployed() } else { base_inputs_ours() };
    if tag == "single" {
        inp.push((s("messageId"), v(1)));
        inp.push((s("x"), v(1000)));
    } else {
        inp.push((s("j"), (1..=slots).map(Fr::from).collect()));
        inp.push((s("s"), (0..slots).map(|_| Fr::from(1u64)).collect()));
        inp.push((s("x"), (0..slots).map(|i| Fr::from(1000 + i)).collect()));
    }

    eprintln!("sustained: tag={tag} slots={slots} count={count}");
    println!("proof_index,ms");
    if tag == "single" {
        let (g, z) = (default_graph_single(), default_zkey_single());
        for i in 0..count {
            let t = Instant::now();
            let _p = prove_from_named_inputs(inp.clone(), g, z);
            println!("{},{:.2}", i, t.elapsed().as_secs_f64() * 1e3);
        }
    } else {
        let g = graph_from_raw(&fs::read(format!("{ART}/{tag}/graph.bin")).unwrap(), Some(DEPTH), None)
            .unwrap();
        let z = zkey_from_raw(&fs::read(format!("{ART}/{tag}/rln_final.arkzkey")).unwrap()).unwrap();
        for i in 0..count {
            let t = Instant::now();
            let _p = prove_from_named_inputs(inp.clone(), &g, &z);
            println!("{},{:.2}", i, t.elapsed().as_secs_f64() * 1e3);
        }
    }
}
