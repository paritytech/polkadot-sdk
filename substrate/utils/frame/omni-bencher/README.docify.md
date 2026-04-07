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

First we need to ensure that there is a runtime available. As example we will build the `cumulus-test-runtime`:

```sh
cargo build -p cumulus-test-runtime --profile production --features runtime-benchmarks
```

Now as an example, we benchmark the `balances` pallet:

<!-- docify::embed!("tests/benchmark_works.rs", benchmarking_example_pallet_balances) -->

The `--steps`, `--repeat`, `--heap-pages` and `--wasm-execution` arguments have sane defaults and do
not need be passed explicitly anymore.

### Benchmark all pallets

To benchmark all pallets of a runtime, pass the wildcard `*`:

<!-- docify::embed!("tests/benchmark_works.rs", benchmarking_example_all_pallets) -->

### Benchmark overhead

To benchmark the overhead of a runtime:

<!-- docify::embed!("tests/benchmark_works.rs", benchmarking_example_overhead) -->

### Generate weights (templates)

To render Rust weight files from benchmark results, pass an output path. Optionally you can pass a
custom header and a Handlebars template (defaults are provided):

<!-- docify::embed!("tests/benchmark_works.rs", benchmarking_example_export_weights) -->

This uses the same flags as the node-integrated benchmarking CLI. The output can be a directory or a
file path; when a directory is given, a file name is generated per pallet/instance.

## Backwards Compatibility

The exposed pallet sub-command is identical as the node-integrated CLI. The only difference is that
it needs to be prefixed with a `v1` to ensure drop-in compatibility.
