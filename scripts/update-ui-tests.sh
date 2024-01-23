#!/usr/bin/env bash
# Script for updating the UI tests for a new rust stable version.
# Exit on error
set -e

# by default current rust stable will be used
RUSTUP_RUN=""
# check if we have a parameter
# ./scripts/update-ui-tests.sh 1.70
if [ ! -z "$1" ]; then
 echo "RUST_VERSION: $1"
  # This will run all UI tests with the rust stable 1.70.
  # The script requires that rustup is installed.
  RUST_VERSION=$1
  RUSTUP_RUN="rustup run $RUST_VERSION"


  echo "installing rustup $RUST_VERSION"
  if ! command -v rustup &> /dev/null
  then
    echo "rustup needs to be installed"
    exit
  fi
  
  rustup install $RUST_VERSION
  rustup component add rust-src --toolchain $RUST_VERSION
fi

# Ensure we run the ui tests
export RUN_UI_TESTS=1
# We don't need any wasm files for ui tests
export SKIP_WASM_BUILD=1
# Let trybuild overwrite the .stderr files
export TRYBUILD=overwrite

# ./substrate
$RUSTUP_RUN cargo test --manifest-path substrate/primitives/runtime-interface/Cargo.toml ui
$RUSTUP_RUN cargo test -p sp-api-test ui
$RUSTUP_RUN cargo test -p frame-election-provider-solution-type ui
$RUSTUP_RUN cargo test -p frame-support-test --features=no-metadata-docs,try-runtime,experimental ui
