#!/usr/bin/env bash
set -e

#shellcheck source=../common/lib.sh
source "$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )/common/lib.sh"

# build runtime
WASM_BUILD_NO_COLOR=1 cargo build -q --locked --release -p staging-kusama-runtime -p polkadot-runtime -p westend-runtime
# make checksum
sha256sum target/release/wbuild/*-runtime/target/wasm32-unknown-unknown/release/*.wasm > checksum.sha256

cargo clean

# build again
WASM_BUILD_NO_COLOR=1 cargo build -q --locked --release -p staging-kusama-runtime -p polkadot-runtime -p westend-runtime
# confirm checksum
sha256sum -c checksum.sha256
