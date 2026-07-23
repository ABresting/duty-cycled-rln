# Our additions to zerokit (for reproduction)

The zerokit clone itself is not committed (it's large and upstream). To reproduce our
benchmarks, clone zerokit at the recorded base commit and apply our additions:

```sh
git clone https://github.com/vacp2p/zerokit.git
cd zerokit && git checkout <BASE-COMMIT from BASE-COMMIT.txt>
git apply ../zerokit-modifications.patch          # adds prove_from_named_inputs + registers benches
cp ../benches/*.rs rln/benches/                    # our two benchmark files
cargo bench -p rln --bench three_methods
```

Contents:
- `BASE-COMMIT.txt`          — the zerokit commit our work is based on
- `zerokit-modifications.patch` — diff to rln/src/circuit/mod.rs (helper) + rln/Cargo.toml (bench registration)
- `benches/three_methods.rs` — single / multi-burn / batched comparison (the paper's headline numbers)
- `benches/batch_vs_single.rs` — batch(4) vs single on the shipped multi_message_id circuit
