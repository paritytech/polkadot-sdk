# Polkadot Omni Benchmarking CLI

The Polkadot Omni benchmarker allows to benchmark the extrinsics of any Polkadot runtime. It is
meant to replace the current manual integration of the `benchmark pallet` into every parachain node.
This reduces duplicate code and makes maintenance for builders easier. The CLI is currently only
able to benchmark extrinsics. In the future it is planned to extend this to some other areas.

General FRAME runtimes could also be used with this benchmarker, as long as they don't utilize any
host functions that are not part of the Polkadot host specification.

## Installation

Directly via crates.io:

```sh
cargo install frame-omni-bencher --profile=production --locked
```

from GitHub:

```sh
cargo install --git https://github.com/paritytech/polkadot-sdk frame-omni-bencher --profile=production --locked
```

or locally from the sources:

```sh
cargo install --path substrate/utils/frame/omni-bencher --profile=production
```

Check the installed version and print the docs:

```sh
frame-omni-bencher --help
```

## Usage

First we need to ensure that there is a runtime available. As example we will build the Westend
runtime:

```sh
cargo build -p westend-runtime --profile production --features runtime-benchmarks
```

Now as an example, we benchmark the `balances` pallet:

```sh
frame-omni-bencher v1 benchmark pallet \
--runtime target/release/wbuild/westend-runtime/westend-runtime.compact.compressed.wasm \
--pallet "pallet_balances" --extrinsic ""
```

The `--steps`, `--repeat`, `--heap-pages` and `--wasm-execution` arguments have sane defaults and do
not need be passed explicitly anymore.

## Backwards Compatibility

The exposed pallet sub-command is identical as the node-integrated CLI. The only difference is that
it needs to be prefixed with a `v1` to ensure drop-in compatibility.
