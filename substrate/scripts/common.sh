#!/usr/bin/env bash

ROOT=`dirname "$0"`

# A list of directories which contain wasm projects.
SRCS=(
	"substrate/executor/wasm"
)

DEMOS=(
	"demo/runtime/wasm"
	"substrate/test-runtime/wasm"
)

# Make pushd/popd silent.

pushd () {
	command pushd "$@" > /dev/null
}

popd () {
	command popd "$@" > /dev/null
}
