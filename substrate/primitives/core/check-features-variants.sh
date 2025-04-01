#!/usr/bin/env -S bash -eux

export RUSTFLAGS="-Cdebug-assertions=y -Dwarnings"
T=wasm32-unknown-unknown

cargo check --target=$T --release --no-default-features  --features="bls-experimental"
cargo check --target=$T --release --no-default-features  --features="full_crypto,bls-experimental"
cargo check --target=$T --release --no-default-features  --features="full_crypto,serde"
cargo check --target=$T --release --no-default-features  --features="full_crypto,serde"
cargo check --target=$T --release --no-default-features  --features="full_crypto"
cargo check --target=$T --release --no-default-features  --features="serde"
cargo check --target=$T --release --no-default-features
