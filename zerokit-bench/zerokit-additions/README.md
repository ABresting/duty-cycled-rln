# Our additions to zerokit (for reproduction)

The zerokit clone itself is not committed (it's large and upstream). To reproduce our
benchmarks, clone zerokit at the recorded base commit and apply our additions:

```sh
git clone https://github.com/vacp2p/zerokit.git
cd zerokit && git checkout <BASE-COMMIT from BASE-COMMIT.txt>
git apply ../zerokit-modifications.patch          # adds the named-input proving helpers
cp ../benches/*.rs rln/benches/                    # benchmark harnesses
cp ../examples/*.rs rln/examples/                  # sustained-proving and envelope examples
cargo bench -p rln --bench three_methods
```

`bench-kit/setup.sh` performs these steps, registers the bench targets, and
points the sources at `zerokit-bench/artifacts/`.

Contents:
- `BASE-COMMIT.txt`          — the zerokit commit our work is based on
- `zerokit-modifications.patch` — diff to rln/src/circuit/mod.rs (proving and verification helpers)
- `benches/three_methods.rs` — single / multi-burn / batched comparison (the paper's headline numbers)
- `benches/batch_vs_single.rs` — batch(4) vs single on the shipped multi-message-identifier circuit
- `benches/variants.rs`      — dual-window, typed, and typed dual-window circuits
- `benches/verify.rs`        — relayer-side verification with a prepared verifying key
- `benches/bursts.rs`        — whole-burst proving at a fixed capacity
- `examples/sustained.rs`    — consecutive proofs, per-proof wall time
- `examples/envelope.rs`     — builds a real batch envelope and validates it as a relayer would,
                               including tampered envelopes and quota-slot reuse (see the top-level README)
