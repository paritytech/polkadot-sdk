#!/usr/bin/env bash
set -e
# Run all the relevant UI tests
# Any new UI tests in different crates need to be added here as well.
$RUSTUP_RUN cargo test -p sp-runtime-interface ui
$RUSTUP_RUN cargo test -p sp-api-test ui
$RUSTUP_RUN cargo test -p frame-election-provider-solution-type ui
$RUSTUP_RUN cargo test -p frame-support-test ui
