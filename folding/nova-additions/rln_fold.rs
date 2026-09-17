//! Folded proofs (paper Section 5.4) — fold a burst of B messages into ONE constant-size
//! proof with CONSTANT verify time and CONSTANT proof size, regardless of B. This is the
//! scaling / on-chain variant: the Groth16 batch exposes k message hashes as public
//! inputs, so its verify grows with k; folding hides every message inside the recursion.
//!
//! Each fold step includes the full membership check of the batched circuit
//! (Section 5.1) plus the accumulator of Equation (fold):
//!   identityCommitment = Poseidon(a0)
//!   R                  = Poseidon(identityCommitment, Qs)
//!   climb 20-level Merkle path from R; enforce == r (carried in state)
//!   a1 = Poseidon(a0, e, j);  y = a0 + a1*x;  nf = Poseidon(a1)
//!   acc' = Poseidon(acc, nf, x, y)          // accumulate the audit commitment
//! State z = [r, e, acc] (Equation fold). z_0 = [known_group_root, H(epoch, app), 0]: the
//! verifier supplies BOTH the group root and the external nullifier it derives from the
//! batch's cleartext epoch — Waku's freshness rule (|now - epoch| <= g) is only sound if
//! the epoch is cryptographically bound into the proof; carrying e as a private witness
//! would let a captured batch be replayed after nullifier-cache eviction.
//! After m steps, CompressedSNARK -> one constant proof; the network reveals the B
//! (nf, x, y) triples and checks they recompute acc (audit) + against the global
//! epoch nullifier set (cross-batch reuse -> slash, as in the batched scheme).
//!
//! Run: cargo run --release --example rln_fold --features test-utils
#![allow(non_snake_case)]
use ff::Field;
use generic_array::typenum::U24;
use nova_snark::{
  frontend::{
    gadgets::poseidon::{
      Elt, IOPattern, Simplex, Sponge, SpongeAPI, SpongeCircuit, SpongeOp, SpongeTrait, Strength,
    },
    num::AllocatedNum,
    ConstraintSystem, SynthesisError,
  },
  nova::{CompressedSNARK, PublicParams, RecursiveSNARK},
  provider::{Bn256EngineKZG, GrumpkinEngine},
  traits::{circuit::StepCircuit, snark::RelaxedR1CSSNARKTrait, Engine, Group},
};
use std::time::Instant;

type E1 = Bn256EngineKZG;
type E2 = GrumpkinEngine;
type EE1 = nova_snark::provider::hyperkzg::EvaluationEngine<E1>;
type EE2 = nova_snark::provider::ipa_pc::EvaluationEngine<E2>;
type S1 = nova_snark::spartan::snark::RelaxedR1CSSNARK<E1, EE1>;
type S2 = nova_snark::spartan::snark::RelaxedR1CSSNARK<E2, EE2>;
type Fr = <E1 as Engine>::Scalar;

const DEPTH: usize = 20; // Merkle tree depth (matches the deployed network: 2^20 members)

// ---- Poseidon: in-circuit and native must match (same U24 constants, absorb-k/squeeze-1) ----
fn poseidon_hash<G, CS>(
  cs: &mut CS,
  name: &str,
  inputs: &[AllocatedNum<G::Scalar>],
) -> Result<AllocatedNum<G::Scalar>, SynthesisError>
where
  G: Group,
  CS: ConstraintSystem<G::Scalar>,
{
  let elt: Vec<Elt<G::Scalar>> = inputs.iter().map(|x| Elt::Allocated(x.clone())).collect();
  let n = inputs.len() as u32;
  let parameter = IOPattern(vec![SpongeOp::Absorb(n), SpongeOp::Squeeze(1u32)]);
  let pc = Sponge::<G::Scalar, U24>::api_constants(Strength::Standard);
  let mut ns = cs.namespace(|| name.to_string());
  let z_out = {
    let mut sponge = SpongeCircuit::new_with_constants(&pc, Simplex);
    let acc = &mut ns;
    sponge.start(parameter, None, acc);
    SpongeAPI::absorb(&mut sponge, n, &elt, acc);
    let output = SpongeAPI::squeeze(&mut sponge, 1, acc);
    sponge.finish(acc).unwrap();
    Elt::ensure_allocated(&output[0], &mut ns.namespace(|| "ea"))?
  };
  Ok(z_out)
}

fn poseidon_native(inputs: &[Fr]) -> Fr {
  let pc = Sponge::<Fr, U24>::api_constants(Strength::Standard);
  let mut sponge = Sponge::new_with_constants(&pc, Simplex);
  let acc = &mut ();
  let n = inputs.len() as u32;
  sponge.start(IOPattern(vec![SpongeOp::Absorb(n), SpongeOp::Squeeze(1u32)]), None, acc);
  SpongeAPI::absorb(&mut sponge, n, inputs, acc);
  let out = SpongeAPI::squeeze(&mut sponge, 1, acc);
  sponge.finish(acc).unwrap();
  out[0]
}

#[derive(Clone, Debug)]
struct RlnStep<G: Group> {
  a0: G::Scalar,
  Qs: G::Scalar,
  j: Vec<G::Scalar>,             // k slot indices folded per step (batched folding)
  x: Vec<G::Scalar>,             // k message hashes
  pathElements: Vec<G::Scalar>,  // Merkle siblings (len DEPTH) — climbed ONCE per step
}

impl<G: Group> StepCircuit<G::Scalar> for RlnStep<G> {
  fn arity(&self) -> usize {
    3 // [r, e, acc]
  }

  fn synthesize<CS: ConstraintSystem<G::Scalar>>(
    &self,
    cs: &mut CS,
    z_in: &[AllocatedNum<G::Scalar>],
  ) -> Result<Vec<AllocatedNum<G::Scalar>>, SynthesisError> {
    let r_in = z_in[0].clone();
    // e comes from the PUBLIC state (verifier derives it from the cleartext epoch),
    // not from a private witness — this binds the whole batch to the epoch (replay protection).
    let e = z_in[1].clone();
    let acc_in = z_in[2].clone();

    let a0 = AllocatedNum::alloc(cs.namespace(|| "a0"), || Ok(self.a0))?;
    let Qs = AllocatedNum::alloc(cs.namespace(|| "Qs"), || Ok(self.Qs))?;
    // membership: R climbs the Merkle path to the group root
    let id_comm = poseidon_hash::<G, _>(cs, "idcomm", &[a0.clone()])?;
    let mut cur = poseidon_hash::<G, _>(cs, "R", &[id_comm, Qs])?;
    for d in 0..DEPTH {
      let sib = AllocatedNum::alloc(cs.namespace(|| format!("sib{d}")), || Ok(self.pathElements[d]))?;
      cur = poseidon_hash::<G, _>(cs, &format!("lvl{d}"), &[cur, sib])?; // index 0 => left child
    }
    cs.enforce(
      || "climbed root == group root",
      |lc| lc + cur.get_variable(),
      |lc| lc + CS::one(),
      |lc| lc + r_in.get_variable(),
    );

    // k messages per step (the batched relation inside the fold step):
    // membership was climbed once above; each message adds share+nullifier+acc only.
    let mut acc = acc_in;
    for (i, (jv, xv)) in self.j.iter().zip(self.x.iter()).enumerate() {
      let j = AllocatedNum::alloc(cs.namespace(|| format!("j{i}")), || Ok(*jv))?;
      let x = AllocatedNum::alloc(cs.namespace(|| format!("x{i}")), || Ok(*xv))?;
      let a1 =
        poseidon_hash::<G, _>(cs, &format!("a1_{i}"), &[a0.clone(), e.clone(), j])?;
      let a1x = a1.mul(cs.namespace(|| format!("a1x{i}")), &x)?;
      let y = AllocatedNum::alloc(cs.namespace(|| format!("y{i}")), || {
        Ok(a0.get_value().unwrap() + a1x.get_value().unwrap())
      })?;
      cs.enforce(
        || format!("y{i} = a0 + a1*x"),
        |lc| lc + a0.get_variable() + a1x.get_variable(),
        |lc| lc + CS::one(),
        |lc| lc + y.get_variable(),
      );
      let nf = poseidon_hash::<G, _>(cs, &format!("nf{i}"), &[a1])?;
      acc = poseidon_hash::<G, _>(cs, &format!("acc{i}"), &[acc, nf, x, y])?;
    }
    Ok(vec![r_in, e, acc])
  }
}

fn main() {
  type C = RlnStep<<E1 as Engine>::GE>;

  let a0 = Fr::from(987654321u64);
  let Qs = Fr::from(100u64);
  let e = Fr::from(424242u64);

  // native group root for the test member (leaf index 0 => all-zero siblings)
  let id_comm = poseidon_native(&[a0]);
  let R = poseidon_native(&[id_comm, Qs]);
  let mut r = R;
  for _ in 0..DEPTH {
    r = poseidon_native(&[r, Fr::ZERO]);
  }

  println!("# Folded proofs — batched RLN relation (k msgs/step) inside Nova; membership once per step");
  println!("total_msgs,K,steps,e2e_prove_ms,e2e_ms_per_msg,fold_loop_ms,compress_ms,proof_bytes,verify_ms");

  let total_msgs: usize = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(8);
  let ks: Vec<usize> = std::env::args().nth(2)
    .map(|a| a.split(',').filter_map(|v| v.parse().ok()).collect())
    .unwrap_or_else(|| vec![1, 4]);

  for k in ks {
    let n = total_msgs / k; // number of fold steps
    let circuits: Vec<C> = (0..n)
      .map(|s_i| RlnStep {
        a0,
        Qs,
        j: (0..k).map(|jj| Fr::from((s_i * k + jj + 1) as u64)).collect(),
        x: (0..k).map(|jj| Fr::from((1000 + s_i * k + jj) as u64)).collect(),
        pathElements: vec![Fr::ZERO; DEPTH],
      })
      .collect();

    let pp = PublicParams::<E1, E2, C>::setup(&circuits[0], &*S1::ck_floor(), &*S2::ck_floor())
      .unwrap();
    let z0 = vec![r, e, Fr::ZERO];
    let mut rs: RecursiveSNARK<E1, E2, C> =
      RecursiveSNARK::<E1, E2, C>::new(&pp, &circuits[0], &z0).unwrap();

    let t = Instant::now();
    for c in &circuits {
      rs.prove_step(&pp, c).unwrap();
    }
    let prove_ms = t.elapsed().as_secs_f64() * 1000.0;
    rs.verify(&pp, rs.num_steps(), &z0).unwrap();

    let (pk, vk) = CompressedSNARK::<_, _, _, S1, S2>::setup(&pp).unwrap();
    let t = Instant::now();
    let comp = CompressedSNARK::<_, _, _, S1, S2>::prove(&pp, &pk, &rs).unwrap();
    let comp_ms = t.elapsed().as_secs_f64() * 1000.0;

    let bytes = {
      let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
      bincode::serde::encode_into_std_write(&comp, &mut enc, bincode::config::legacy()).unwrap();
      enc.finish().unwrap().len()
    };

    let steps = rs.num_steps();
    // median of a few verify runs (verify is fast; folding's claim is that it's FLAT in N)
    let mut vs = vec![];
    for _ in 0..5 {
      let t = Instant::now();
      comp.verify(&vk, steps, &z0).unwrap();
      vs.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    vs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let vrf_ms = vs[2];

    // END-TO-END = fold loop + compression. Nova defers real work: the first prove_step is
    // a base case (~free) and each step's folding happens at the NEXT step or in compression —
    // so the fold-loop column alone under-reports small step counts. e2e is the honest metric.
    let e2e_ms = prove_ms + comp_ms;
    println!(
      "{total_msgs},{k},{n},{e2e_ms:.0},{:.1},{prove_ms:.0},{comp_ms:.0},{bytes},{vrf_ms:.2}",
      e2e_ms / total_msgs as f64
    );
  }
}
