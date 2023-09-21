#!/usr/bin/env bash
# Script for updating the UI tests for a new rust stable version.
# Exit on error
set -e

# by default current rust stable will be used
RUSTUP_RUN=""
# check if we have a parameter
# ./substrate/.maintain/update-rust-stable.sh 1.70
if [ "$#" -e 1 ]; then
  # This will run all UI tests with the rust stable 1.70.
  # The script requires that rustup is installed.
  RUST_VERSION=$1
  RUSTUP_RUN=rustup run $RUST_VERSION

  rustup install $RUST_VERSION
  rustup component add rust-src --toolchain $RUST_VERSION


  if ! command -v rustup &> /dev/null
  then
    echo "rustup needs to be installed"
    exit
  fi
fi


# ./substrate
$RUSTUP_RUN cargo test -p sp-runtime-interface ui
$RUSTUP_RUN cargo test -p sp-api-test ui
$RUSTUP_RUN cargo test -p frame-election-provider-solution-type ui
$RUSTUP_RUN cargo test -p frame-support-test ui

# ./polkadot
$RUSTUP_RUN cargo test -p orchestra ui
