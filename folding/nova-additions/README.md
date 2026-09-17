# Nova folding prototype

`rln_fold.rs` folds a burst of RLN messages into a single constant-size proof:
each folding step runs the batched RLN relation (per-message Shamir share and
nullifier, accumulated through a Poseidon sponge) over K messages, and a final
Spartan compression yields one proof of roughly 12 KB whose verification cost
is independent of the burst length. Section 5.4 of the paper specifies the
construction; Section 6.5 reports the measurements.

## Building

    git clone https://github.com/microsoft/Nova.git ../nova
    cd ../nova && git checkout 666e3b25bfb9f8b2106f8b4d8057010f28b1ee79
    cp ../nova-additions/rln_fold.rs examples/
    cargo build --release --example rln_fold --features test-utils

## Running

From `folding/`:

    bash run-fold-sweep.sh 128 16,32   # burst of 128 messages, step sizes 16 and 32

Each run prints one CSV row: total and per-message proving time, folding and
compression split, proof size in bytes, and verification time.
