#!/usr/bin/env -S bash -eux

export RUSTFLAGS="-Cdebug-assertions=y -Dwarnings"
T=wasm32-unknown-unknown

cargo check --release
cargo check --release --features="bandersnatch-experimental" 
cargo check --release --target=$T --no-default-features
