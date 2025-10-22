#!/usr/bin/env -S bash -eux

export RUSTFLAGS="-Cdebug-assertions=y -Dwarnings"
cargo check --release
cargo check --release --features="bandersnatch-experimental"

export RUSTFLAGS="$RUSTFLAGS --cfg substrate_runtime"
T=wasm32-unknown-unknown
cargo check --release --target=$T --no-default-features
