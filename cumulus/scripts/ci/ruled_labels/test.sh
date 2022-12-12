#!/usr/bin/env bash

DIR=$(dirname -- "$0")
unset RUST_LOG

pushd "$DIR" > /dev/null

ruled-labels --version
ruled-labels test

popd > /dev/null
