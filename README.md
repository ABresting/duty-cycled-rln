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
| circom, circom-witnesscalc, ark-zkey | 2.1.x / see `build-artifacts.sh` header | artifact regeneration only |
| Node.js | 18 or later | `model/model.mjs` and `circuits/gen-circuits.mjs` |
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

Run the setup once and wait for `KIT READY` before launching anything else;
the pipeline stages run strictly in sequence and each writes its CSV into
`zerokit-bench/results/` before the next begins.

## Repository map

| Directory | Contents |
|---|---|
| `circuits/` | The circom sources: batched (`gen_monotonic_*`), dual-window (`*_dw_*`), typed (`gen_typed_*`) circuits and their generator (`gen-circuits.mjs`) |
| `zerokit-bench/zerokit-additions/` | Benchmark harnesses (criterion), the sustained-proving example, and the patch applied to zerokit at the pinned commit (`BASE-COMMIT.txt`) |
| `zerokit-bench/build-artifacts.sh` | Rebuilds the proving artifacts (r1cs, witness graph, arkzkey) for every circuit |
| `bench-kit/` | Turns a fresh Linux or macOS machine into a benchmark runner: one setup script, one pipeline script |
| `folding/` | The Nova folding prototype (`nova-additions/rln_fold.rs`, built against microsoft/Nova at the commit given there) and its sweep script |
| `testbed/` | The 5-node local nwaku cluster (`waku/node.sh`) and the transport experiments: burst packaging (`experiments/packaging.py`) and the maximum-envelope-size probe (`experiments/envelope_probe.py`) |
| `model/` | The analytical model (`model.mjs`, zero dependencies) that generates the paper's feasibility table from the measured constants |

## Reproducing the paper's measurements

| Paper element | How to reproduce |
|---|---|
| Proving and verification costs, amortization (Sec. 7.1) | `bench-kit/run-benchmarks.sh`, stages 1--3 |
| Constraint counts (Table 2) | compile the circuits with `zerokit-bench/build-artifacts.sh`; counts are deterministic |
| Whole-burst runs (Sec. 7.2) | `bench-kit/run-benchmarks.sh`, stage 4; set `ALL_BURST_CAPACITIES=1` for the k=16/32 families as well |
| Folding (Sec. 7.3) | `folding/run-fold-sweep.sh B K1,K2` after building the Nova example |
| Transport and packaging (Sec. 7.4) | `testbed/waku/node.sh up 5`, then `testbed/experiments/packaging.py` and `envelope_probe.py` |
| Feasibility table (Table 3) | `node model/model.mjs` |

Every benchmark stage writes a CSV with the host, CPU, sample count, and date
in its header, directly comparable to the tables in the paper.

## Identifier glossary

The benchmark harnesses and result CSVs use short internal identifiers, kept
verbatim so every CSV matches the code that produced it:

| Identifier | Meaning in the paper |
|---|---|
| `single` | the deployed single-message RLN circuit |
| `our_routeA_k` / artifact tag `our_ra_k` | the batched circuit at capacity k |
| `our_dualwindow_k` / `our_dw_k` | batched + dual-window quotas |
| `our_typed_k`, `our_typed_dw_k` | batched + typed quotas (+ dual-window) |
| `their_multiburn_k` | the shipped multi-message-identifier extension (baseline) |
| `our_partial_pofk` | batched proof with p of k slots occupied |
| `burst_Bn_kk` | a whole burst of n messages proven at capacity k |
| `K` (folding CSVs) | messages per folding step |

## Proving artifacts

The compiled artifacts (~1.2 GB: witness graphs and arkzkey files for 22
circuit variants) are not stored in git. Either download them from this
repository's release assets and unpack into `zerokit-bench/artifacts/`, or
rebuild them with `zerokit-bench/build-artifacts.sh` (requires circom 2.1,
vacp2p/circom-witnesscalc, and ark-zkey; see the script header). The trusted
setup used for benchmarking is a local development ceremony; it affects no
reported ratio.

## License

MIT (see `LICENSE`).
