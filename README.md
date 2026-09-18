# duty-cycled-rln

Circuits, benchmarks, and measurement data for the paper *Anonymous Rate
Limiting for Duty-Cycled IoT Traffic in Permissionless Messaging Networks*.

The paper adapts Rate-Limiting Nullifiers (RLN), the anonymous spam protection
deployed on the Waku network, to duty-cycled IoT publishers: a batched circuit
that covers up to 64 messages with one membership proof, dual-window and typed
quota policies enforced inside the same proof, and a Nova-folded variant with a
constant-size proof. Everything is implemented on zerokit, the RLN library used
by the production Waku client, and measured natively on five hardware
platforms. This repository contains what is needed to audit that claim and
repeat the measurements: the circuits, the patches to the deployed stack, and
the benchmark pipeline that produced every number in the paper.

## Dependencies and pinned versions

| Dependency | Version / commit | Used for |
|---|---|---|
| Rust | 1.93.0 (pinned by the setup script) | all provers and benchmarks |
| [vacp2p/zerokit](https://github.com/vacp2p/zerokit) | `e077b5bb99ef` | the deployed RLN proving stack; patched by `zerokit-additions/zerokit-modifications.patch` |
| [microsoft/Nova](https://github.com/microsoft/Nova) | `666e3b25bfb9` | the folding prototype; clone into `folding/nova/` |
| [vacp2p/circom-rln](https://github.com/vacp2p/circom-rln) | `94636847fab2` | circuit template tree for artifact rebuilds; clone into `circuits/circom-rln/` |
| nwaku | v0.38.1 | the testbed cluster (`testbed/waku/node.sh` builds it from source) |
| Node.js and npm | Node 18 or later | `model/model.mjs`, `circuits/gen-circuits.mjs`, and `npm install` inside `circuits/circom-rln/` (pulls circomlib and snarkjs 0.7 for the artifact build) |
| [circom](https://github.com/iden3/circom) | 2.1.0 | compiling the circuits (artifact build) |
| [iden3/circom-witnesscalc](https://github.com/iden3/circom-witnesscalc) | `build-circuit/v0.1.1` | witness graphs (`graph.bin`) for zerokit (artifact build) |
| [seemenkina/ark-zkey](https://github.com/seemenkina/ark-zkey) | `029bfe8` | converting snarkjs zkeys to zerokit's arkzkey format (artifact build) |
| powers of tau | BN254, size 2^17 | Groth16 setup for the artifact build; see below |
| Python 3 | 3.9 or later | result collection and testbed experiments |

## Running the benchmarks

Requirements: Rust 1.93.0, and on Debian/Ubuntu the packages
`build-essential git curl python3 rsync libssl-dev pkg-config time`
(macOS: Xcode command-line tools only).

```sh
# one-time setup: clones zerokit at the pinned commit, applies the patch,
# installs the bench sources, points them at the artifacts, builds
bash bench-kit/setup.sh

# full pipeline: circuit sweep, verification, sustained runs, burst families
nohup bash bench-kit/run-benchmarks.sh > campaign.log 2>&1 &
```

Run the setup once and wait for `KIT READY`. Before launching the pipeline,
build the proving artifacts as described under *Proving artifacts* below: the
benches load them from `zerokit-bench/artifacts/` and skip any circuit whose
artifacts are missing. The pipeline stages then run strictly in sequence and
each writes its CSV into `zerokit-bench/results/` before the next begins.

## Repository map

| Directory | Contents |
|---|---|
| `circuits/` | The circom sources: batched (`gen_monotonic_*`), dual-window (`*_dw_*`), typed (`gen_typed_*`) circuits and their generator (`gen-circuits.mjs`) |
| `zerokit-bench/zerokit-additions/` | Benchmark harnesses (criterion), the sustained-proving and envelope examples, and the patch applied to zerokit at the pinned commit (`BASE-COMMIT.txt`) |
| `zerokit-bench/build-artifacts.sh` | Rebuilds the proving artifacts (r1cs, witness graph, arkzkey) for every circuit |
| `bench-kit/` | Turns a fresh Linux or macOS machine into a benchmark runner: one setup script, one pipeline script, and the envelope demonstration |
| `folding/` | The Nova folding prototype (`nova-additions/rln_fold.rs`, built against microsoft/Nova at the commit given there) and its sweep script |
| `testbed/` | The 5-node local nwaku cluster (`waku/node.sh`) and the transport experiments: burst packaging (`experiments/packaging.py`) and the maximum-envelope-size probe (`experiments/envelope_probe.py`) |
| `model/` | The analytical model (`model.mjs`, zero dependencies) that generates the paper's feasibility table from the measured constants |

## Reproducing the paper's measurements

| Paper element | How to reproduce |
|---|---|
| Proving and verification costs, amortization (Sec. 6.3) | `bench-kit/run-benchmarks.sh`, stages 1--3 |
| Constraint counts (Table 2) | compile the circuits with `zerokit-bench/build-artifacts.sh`; counts are deterministic |
| Whole-burst runs (Sec. 6.4) | `bench-kit/run-benchmarks.sh`, stage 4; set `ALL_BURST_CAPACITIES=1` for the k=16/32 families as well |
| Folding (Sec. 6.5) | `folding/run-fold-sweep.sh B K1,K2` after building the Nova example |
| Transport and packaging (Sec. 6.6) | `testbed/waku/node.sh up 5`, then `testbed/experiments/packaging.py` and `envelope_probe.py` |
| Feasibility table (Table 3, Sec. 6.7) | `node model/model.mjs` |

Every benchmark stage writes a CSV with the host, CPU, sample count, and date
in its header, directly comparable to the tables in the paper.

## Building and validating an envelope

The benchmarks measure proof generation and verification separately. The
`envelope` example connects the two: it builds a real batch envelope as a gateway would and
validates it as a relayer would, checking freshness first, recomputing every
message hash from the delivered payloads, verifying every proof, and consulting
the nullifier log. One script runs it on five cases in a few seconds (after
`setup.sh`; needs only the `our_batch_8` artifacts):

```sh
bash bench-kit/run-envelope-demo.sh
```

| Case | Outcome |
|---|---|
| honest burst of 32 messages | accepted, 32 of 32 |
| one proof corrupted | rejected as a whole, 0 of 32 |
| one payload altered after proving | rejected as a whole, since the proofs bind the hash of every payload |
| two envelopes spending the same quota slots | the second is rejected, and its shares reveal the member's identity secret |
| stale envelope | rejected before any proof is checked |

The example validates locally. In a deployment the same checks run in the
relayer's GossipSub topic validator, so a rejected envelope is never forwarded.

## Identifier glossary

The benchmark harnesses and result CSVs use short internal identifiers, kept
verbatim so every CSV matches the code that produced it:

| Identifier | Meaning in the paper |
|---|---|
| `single` | the deployed single-message RLN circuit |
| `our_batch_k` | the batched circuit at capacity k |
| `our_dualwindow_k` / `our_dw_k` | batched + dual-window quotas |
| `our_typed_k`, `our_typed_dw_k` | batched + typed quotas (+ dual-window) |
| `their_multiburn_k` | the shipped multi-message-identifier extension (baseline) |
| `our_partial_pofk` | batched proof with p of k slots occupied |
| `burst_Bn_kk` | a whole burst of n messages proven at capacity k |
| `K` (folding CSVs) | messages per folding step |

## Proving artifacts

The compiled artifacts (about 1.2 GB: witness graphs and arkzkey files for 22
circuit variants) are not stored in git; `zerokit-bench/build-artifacts.sh`
rebuilds them. It expects the following layout, all of it git-ignored:

```sh
# 1. circuit template tree, with our circuits copied into it
git clone https://github.com/vacp2p/circom-rln circuits/circom-rln
git -C circuits/circom-rln checkout 94636847fab2
(cd circuits/circom-rln && npm install)          # circomlib + snarkjs
cp circuits/*.circom circuits/circom-rln/circuits/

# 2. tools, at the paths the script looks for (or export CIRCOM, WC, AZ to point elsewhere)
mkdir -p zerokit-bench/tools
#    circom 2.1.0 binary            -> zerokit-bench/tools/circom210
#    circom-witnesscalc, tag build-circuit/v0.1.1, `cargo build --release`
#                                   -> zerokit-bench/tools/circom-witnesscalc/
#    ark-zkey, commit 029bfe8, `cargo build --release`
#                                   -> zerokit-bench/tools/ark-zkey/

# 3. a BN254 powers-of-tau file of size 2^17 (the k=64 dual-window circuits
#    have 73,000-74,000 constraints, above the 2^16 limit), for example
#    generated with `snarkjs powersoftau`, placed at
#    circuits/circom-rln/build/pot17.ptau (or export PTAU_OVERRIDE=/path/to/file)

bash zerokit-bench/build-artifacts.sh            # all 22 tags; or list tags to build a subset
```

The script checks the prerequisites and stops at the first missing one. Each
tag takes from seconds (k=4) to a few minutes (k=64) on a workstation. The
trusted setup is a local development ceremony; it affects no reported ratio.

## License

MIT (see `LICENSE`).
