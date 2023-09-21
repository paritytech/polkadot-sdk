#!/usr/bin/env bash
set -e
# Run all the relevant UI tests
# Any new UI tests in different crates need to be added here as well.
$RUSTUP_RUN cargo test -p orchestra ui
