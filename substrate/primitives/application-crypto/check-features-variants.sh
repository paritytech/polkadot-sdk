#!/usr/bin/env -S bash -eux

export RUSTFLAGS="-Cdebug-assertions=y -Dwarnings"
cargo check --release

export RUSTFLAGS="$RUSTFLAGS --cfg substrate_runtime"
T=wasm32-unknown-unknown
cargo check --release --target=$T --no-default-features
cargo check --release --target=$T --no-default-features  --features="full_crypto"
cargo check --release --target=$T --no-default-features  --features="serde"
cargo check --release --target=$T --no-default-features  --features="serde,full_crypto"
cargo check --release --target=$T --no-default-features  --features="bandersnatch-experimental"
cargo check --release --target=$T --no-default-features  --features="bls-experimental"
cargo check --release --target=$T --no-default-features  --features="bls-experimental,full_crypto"
