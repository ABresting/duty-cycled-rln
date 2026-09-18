// Batch envelope, end to end: a gateway-side `make` and a relayer-side `verify`.
//
// `make` proves a burst of B messages with the batch circuit at capacity k and
// packs the result into an envelope with the wire layout of the paper (Figure 5):
// the B payloads, one revealed (nf, y) pair per message, and m = ceil(B/k) Groth16
// proofs. Nothing else is written: message hashes, slot indices, and selectors are
// reconstructed by the verifier, and the membership root is network state, kept
// in a separate `<out>.net` file that stands in for what every relayer knows.
//
// `verify` validates envelopes in the order of the relay pipeline (Section 5.1):
// freshness first, then message hashes recomputed from the delivered payloads,
// then every proof, then the nullifier log. Validation is atomic: one failing
// proof rejects the whole envelope.
//
//   cargo run --release -p rln --example envelope -- make --burst 32 --out burst.env
//   cargo run --release -p rln --example envelope -- verify --net burst.env.net burst.env
//
// Fault cases (bench-kit/run-envelope-demo.sh runs them all):
//   make --corrupt proof     flips bytes inside one proof
//   make --corrupt payload   alters one delivered payload after proving
//   make --split             one envelope per proof group instead of a single envelope
//   verify --log FILE        keeps a nullifier log across calls; a reused slot is
//                            detected and the identity secret a0 is recovered
use ark_ff::Field;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use rln::circuit::{
    graph_from_raw, prepared_vk, prove_with_publics_from_named_inputs, verify_with_prepared_vk,
    zkey_from_raw, Fr, Graph, Proof, Zkey,
};
use rln::hashers::{hash_to_field_le, poseidon_hash};
use std::collections::HashMap;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

const DEPTH: usize = 20;
// Rewritten to this repository's zerokit-bench/artifacts by bench-kit/setup.sh.
const ART: &str = "/ARTIFACTS_DIR";
const MAGIC: &[u8; 9] = b"DCRLNENV1";
const PROOF_BYTES: usize = 128;
const EPOCH_SEC: u64 = 20; // window length tau of the example configuration
const MAX_AGE_SEC: u64 = 20; // freshness gap g
const APP: &[u8] = b"duty-cycled-rln/envelope-example"; // application identifier

fn load(k: usize) -> (Graph, Zkey) {
    let dir = format!("{ART}/our_batch_{k}");
    let g = graph_from_raw(&fs::read(format!("{dir}/graph.bin")).expect("missing artifacts"), Some(DEPTH), None)
        .unwrap();
    let z = zkey_from_raw(&fs::read(format!("{dir}/rln_final.arkzkey")).unwrap()).unwrap();
    (g, z)
}

fn fr_bytes(f: &Fr) -> [u8; 32] {
    let mut b = Vec::with_capacity(32);
    f.serialize_compressed(&mut b).unwrap();
    b.try_into().unwrap()
}
fn fr_from(b: &[u8]) -> Option<Fr> {
    Fr::deserialize_compressed(b).ok()
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}
fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}
fn external_nullifier(timestamp: u64) -> Fr {
    poseidon_hash(&[Fr::from(timestamp / EPOCH_SEC), hash_to_field_le(APP)])
}

struct Args(Vec<String>);
impl Args {
    fn flag(&self, name: &str) -> bool {
        self.0.iter().any(|a| a == name)
    }
    fn opt(&self, name: &str) -> Option<String> {
        self.0.iter().position(|a| a == name).and_then(|i| self.0.get(i + 1).cloned())
    }
    fn num(&self, name: &str, default: u64) -> u64 {
        self.opt(name).map(|v| v.parse().expect("number expected")).unwrap_or(default)
    }
    fn files(&self) -> Vec<String> {
        let valued = ["--burst", "--k", "--payload", "--out", "--corrupt", "--group", "--net", "--log", "--timestamp", "--seed"];
        let mut out = Vec::new();
        let mut i = 1; // skip subcommand
        while i < self.0.len() {
            if valued.contains(&self.0[i].as_str()) {
                i += 2;
            } else if self.0[i].starts_with("--") {
                i += 1;
            } else {
                out.push(self.0[i].clone());
                i += 1;
            }
        }
        out
    }
}

// ---------------------------------------------------------------- envelope format
// header: MAGIC | B u32 | k u32 | P u32 | timestamp u64   (little endian)
// body:   B payloads of P bytes | B pairs (nf 32 | y 32) | m proofs of 128 bytes
struct Envelope {
    k: usize,
    payload_len: usize,
    timestamp: u64,
    payloads: Vec<Vec<u8>>,
    pairs: Vec<([u8; 32], [u8; 32])>, // (nf, y) per message
    proofs: Vec<Vec<u8>>,
}

impl Envelope {
    fn to_bytes(&self) -> Vec<u8> {
        let mut b = MAGIC.to_vec();
        b.extend((self.payloads.len() as u32).to_le_bytes());
        b.extend((self.k as u32).to_le_bytes());
        b.extend((self.payload_len as u32).to_le_bytes());
        b.extend(self.timestamp.to_le_bytes());
        self.payloads.iter().for_each(|p| b.extend(p));
        self.pairs.iter().for_each(|(nf, y)| {
            b.extend(nf);
            b.extend(y);
        });
        self.proofs.iter().for_each(|p| b.extend(p));
        b
    }
    fn from_bytes(b: &[u8]) -> Option<Envelope> {
        if b.len() < 29 || &b[..9] != MAGIC {
            return None;
        }
        let u32_at = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap()) as usize;
        let (n, k, p) = (u32_at(9), u32_at(13), u32_at(17));
        let timestamp = u64::from_le_bytes(b[21..29].try_into().unwrap());
        if k == 0 || n == 0 {
            return None;
        }
        let m = n.div_ceil(k);
        if b.len() != 29 + n * p + n * 64 + m * PROOF_BYTES {
            return None;
        }
        let mut o = 29;
        let payloads = (0..n).map(|i| b[o + i * p..o + (i + 1) * p].to_vec()).collect();
        o += n * p;
        let pairs = (0..n)
            .map(|i| {
                let q = o + i * 64;
                (b[q..q + 32].try_into().unwrap(), b[q + 32..q + 64].try_into().unwrap())
            })
            .collect();
        o += n * 64;
        let proofs = (0..m).map(|g| b[o + g * PROOF_BYTES..o + (g + 1) * PROOF_BYTES].to_vec()).collect();
        Some(Envelope { k, payload_len: p, timestamp, payloads, pairs, proofs })
    }
}

// public inputs of one proof group, in the order the circuit exposes them:
// y[k] | r | nf[k] | x[k] | e | s[k]   (inactive slots: y = nf = x = s = 0)
fn group_publics(k: usize, root: Fr, e: Fr, ys: &[Fr], nfs: &[Fr], xs: &[Fr]) -> Vec<Fr> {
    let pad = |v: &[Fr]| (0..k).map(|i| v.get(i).copied().unwrap_or(Fr::from(0u64))).collect::<Vec<_>>();
    let mut p = pad(ys);
    p.push(root);
    p.extend(pad(nfs));
    p.extend(pad(xs));
    p.push(e);
    p.extend((0..k).map(|i| Fr::from(u64::from(i < xs.len()))));
    p
}

// ---------------------------------------------------------------- make (gateway)
fn make(a: &Args) {
    let burst = a.num("--burst", 32) as usize;
    let k = a.num("--k", 8) as usize;
    let plen = a.num("--payload", 100) as usize;
    let seed = a.num("--seed", 0);
    let timestamp = a.num("--timestamp", now());
    let out = a.opt("--out").expect("--out FILE required");
    let m = burst.div_ceil(k);
    let corrupt = a.opt("--corrupt");
    let bad_group = a.num("--group", if m > 1 { 1 } else { 0 }) as usize;
    assert!(bad_group < m, "--group must be below the number of proof groups ({m})");

    // example member: leaf 0 of the membership tree (all-zero siblings), as in the benches
    let (a0, qs) = (Fr::from(987654321u64), Fr::from(2048u64));
    let e = external_nullifier(timestamp);
    let (graph, zkey) = load(k);

    let payloads: Vec<Vec<u8>> = (0..burst)
        .map(|i| {
            let mut p = format!("sensor-reading seed={seed} #{i:05} ").into_bytes();
            p.resize(plen.max(p.len()), b'.');
            p.truncate(plen.max(32));
            p
        })
        .collect();
    let plen = payloads[0].len();

    let (mut pairs, mut proofs, mut root) = (Vec::new(), Vec::new(), Fr::from(0u64));
    for g in 0..m {
        let active: Vec<usize> = (g * k..((g + 1) * k).min(burst)).collect();
        let xs: Vec<Fr> = active.iter().map(|&i| hash_to_field_le(&payloads[i])).collect();
        let pad_x: Vec<Fr> = (0..k).map(|i| xs.get(i).copied().unwrap_or(Fr::from(0u64))).collect();
        let inputs = vec![
            ("a0".to_string(), vec![a0]),
            ("Qs".to_string(), vec![qs]),
            ("pathElements".to_string(), vec![Fr::from(0u64); DEPTH]),
            ("identityPathIndex".to_string(), vec![Fr::from(0u64); DEPTH]),
            ("e".to_string(), vec![e]),
            // slot indices keep increasing through inactive slots (Equation 6)
            ("j".to_string(), (0..k).map(|i| Fr::from((g * k + i + 1) as u64)).collect()),
            ("s".to_string(), (0..k).map(|i| Fr::from(u64::from(i < active.len()))).collect()),
            ("x".to_string(), pad_x),
        ];
        let (proof, publics) = prove_with_publics_from_named_inputs(inputs, &graph, &zkey);
        assert_eq!(publics.len(), 4 * k + 2, "unexpected public-input layout");
        root = publics[k];
        let (ys, nfs) = (&publics[..active.len()], &publics[k + 1..k + 1 + active.len()]);
        assert_eq!(publics, group_publics(k, root, e, ys, nfs, &xs), "public-input order mismatch");
        pairs.extend(ys.iter().zip(nfs).map(|(y, nf)| (fr_bytes(nf), fr_bytes(y))));
        let mut pb = Vec::new();
        proof.serialize_compressed(&mut pb).unwrap();
        assert_eq!(pb.len(), PROOF_BYTES);
        proofs.push(pb);
    }

    let mut payloads = payloads;
    match corrupt.as_deref() {
        Some("proof") => (8..16).for_each(|i| proofs[bad_group][i] ^= 0xff),
        Some("payload") => payloads[bad_group * k][plen - 1] ^= 0x01,
        Some(other) => panic!("--corrupt takes 'proof' or 'payload', got {other}"),
        None => {}
    }

    fs::write(format!("{out}.net"), format!("root {}\n", hex(&fr_bytes(&root)))).unwrap();
    let write = |path: String, lo: usize, hi: usize, gs: std::ops::Range<usize>| {
        let env = Envelope {
            k,
            payload_len: plen,
            timestamp,
            payloads: payloads[lo..hi].to_vec(),
            pairs: pairs[lo..hi].to_vec(),
            proofs: proofs[gs].to_vec(),
        };
        let bytes = env.to_bytes();
        fs::write(&path, &bytes).unwrap();
        let n = hi - lo;
        println!(
            "wrote {path}: {n} messages, {} proof(s), {} bytes ({} bytes overhead per message)",
            env.proofs.len(),
            bytes.len(),
            (bytes.len() - n * plen) / n
        );
    };
    if a.flag("--split") {
        (0..m).for_each(|g| write(format!("{out}.{g}"), g * k, ((g + 1) * k).min(burst), g..g + 1));
    } else {
        write(out.clone(), 0, burst, 0..m);
    }
    if let Some(c) = corrupt {
        println!("corrupted: {c} of proof group {bad_group}");
    }
}

// ---------------------------------------------------------------- verify (relayer)
fn verify(a: &Args) {
    let net = fs::read_to_string(a.opt("--net").expect("--net FILE required")).unwrap();
    let root = fr_from(&unhex(net.trim().strip_prefix("root ").expect("bad .net file"))).unwrap();
    let log_path = a.opt("--log");
    // nullifier log: nf -> (x, y) of the message that used the slot
    let mut log: HashMap<[u8; 32], (Fr, Fr)> = HashMap::new();
    if let Some(p) = &log_path {
        for line in fs::read_to_string(p).unwrap_or_default().lines() {
            let f: Vec<Vec<u8>> = line.split(' ').map(unhex).collect();
            log.insert(f[0].clone().try_into().unwrap(), (fr_from(&f[1]).unwrap(), fr_from(&f[2]).unwrap()));
        }
    }
    let mut keys: HashMap<usize, ark_groth16::PreparedVerifyingKey<rln::circuit::Curve>> = HashMap::new();
    let (mut accepted, mut total) = (0usize, 0usize);

    for path in a.files() {
        let Some(env) = Envelope::from_bytes(&fs::read(&path).unwrap()) else {
            println!("{path}: REJECTED, not a well-formed envelope");
            continue;
        };
        let n = env.payloads.len();
        total += n;

        // 1. freshness, before any cryptography
        let age = now().saturating_sub(env.timestamp);
        if !a.flag("--skip-freshness") && age > MAX_AGE_SEC {
            println!("{path}: REJECTED, stale ({age} s old, limit {MAX_AGE_SEC} s); 0 of {n} messages accepted");
            continue;
        }
        let e = external_nullifier(env.timestamp);

        // 2. message hashes are recomputed from the delivered payloads, never trusted
        let xs: Vec<Fr> = env.payloads.iter().map(|p| hash_to_field_le(p)).collect();
        let (nfs, ys): (Vec<Option<Fr>>, Vec<Option<Fr>>) =
            env.pairs.iter().map(|(nf, y)| (fr_from(nf), fr_from(y))).unzip();

        // 3. every proof must verify; validation is atomic
        let pvk = keys.entry(env.k).or_insert_with(|| prepared_vk(&load(env.k).1));
        let mut failure = None;
        for (g, pb) in env.proofs.iter().enumerate() {
            let (lo, hi) = (g * env.k, ((g + 1) * env.k).min(n));
            let fields: Option<(Vec<Fr>, Vec<Fr>)> =
                ys[lo..hi].iter().copied().collect::<Option<Vec<Fr>>>().zip(nfs[lo..hi].iter().copied().collect::<Option<Vec<Fr>>>());
            let ok = match (Proof::deserialize_compressed(&pb[..]), fields) {
                (Ok(proof), Some((y, nf))) => {
                    verify_with_prepared_vk(pvk, &proof, &group_publics(env.k, root, e, &y, &nf, &xs[lo..hi]))
                }
                _ => false, // proof or field element does not even decode
            };
            if !ok {
                failure = Some(g);
                break;
            }
        }
        if let Some(g) = failure {
            println!("{path}: REJECTED, proof group {g} invalid; 0 of {n} messages accepted");
            continue;
        }

        // 4. nullifier log: a slot used twice puts two shares on one line
        let mut reused = None;
        for i in 0..n {
            let (nf, x, y) = (env.pairs[i].0, xs[i], ys[i].unwrap());
            if let Some(&(x0, y0)) = log.get(&nf) {
                if x0 != x {
                    let a1 = (y - y0) * (x - x0).inverse().unwrap();
                    reused = Some(y - a1 * x);
                    break;
                }
            }
        }
        if let Some(a0) = reused {
            println!(
                "{path}: REJECTED, quota slot reused; identity secret recovered: a0 = {a0}; 0 of {n} messages accepted"
            );
            continue;
        }
        for i in 0..n {
            log.insert(env.pairs[i].0, (xs[i], ys[i].unwrap()));
        }
        accepted += n;
        println!("{path}: ACCEPTED, {n} of {n} messages ({} proof(s) verified)", env.proofs.len());
    }

    if let Some(p) = &log_path {
        let lines: Vec<String> = log
            .iter()
            .map(|(nf, (x, y))| format!("{} {} {}", hex(nf), hex(&fr_bytes(x)), hex(&fr_bytes(y))))
            .collect();
        fs::write(p, lines.join("\n") + "\n").unwrap();
    }
    println!("total: {accepted} of {total} messages accepted");
}

fn main() {
    let args = Args(std::env::args().skip(1).collect());
    match args.0.first().map(String::as_str) {
        Some("make") => make(&args),
        Some("verify") => verify(&args),
        _ => eprintln!("usage: envelope make --burst B [--k K] [--payload P] [--split] [--corrupt proof|payload] [--group G] [--timestamp T] [--seed S] --out FILE\n       envelope verify --net FILE.net [--skip-freshness] [--log FILE] ENVELOPE..."),
    }
}
