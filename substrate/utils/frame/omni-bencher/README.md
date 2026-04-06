# Polkadot Omni Benchmarking CLI

The Polkadot Omni benchmarker allows for benchmarking the extrinsics of any Polkadot runtime. It is intended to
replace the manual integration of the `benchmark pallet` command into every parachain node, reducing duplicate code
and simplifying maintenance for builders.

Currently, the CLI supports benchmarking extrinsics, with plans to extend its capabilities to other areas in the future.

General FRAME runtimes can be used with this benchmarker, provided they do not rely on host functions outside
the Polkadot host specification.

## Installation

You can install `frame-omni-bencher` directly from crates.io:

```sh
cargo install frame-omni-bencher --profile=production --locked
```

Alternatively, install from the GitHub source:

```sh
cargo install --git https://github.com/paritytech/polkadot-sdk frame-omni-bencher --profile=production --locked
```

Or build and install locally:

```sh
cargo install --path substrate/utils/frame/omni-bencher --profile=production
```

Verify the installation and explore the available commands:

```rust,ignore
bash!(
	frame-omni-bencher --help
)
```

## Usage

Before running benchmarks, ensure you have a compiled runtime WASM blob. For example, to build the Westend runtime
with benchmarking enabled:

```sh
cargo build -r -p westend-runtime --features runtime-benchmarks
```

### Benchmarking a Pallet

To benchmark a specific pallet (e.g., `pallet_balances`):

```rust,ignore
bash!(
	frame-omni-bencher v1 benchmark pallet --runtime $runtime_path --pallet "pallet_balances" --extrinsic "*"
)
```

The `--steps`, `--repeat`, `--heap-pages`, and `--wasm-execution` arguments have sane defaults and do not
need to be passed explicitly.

### Generating Weights

To render Rust weight files from benchmark results, specify an output path. You can also provide a custom header
and a Handlebars template (defaults are used if omitted):

```rust,ignore
bash!(
	frame-omni-bencher v1 benchmark pallet --runtime $runtime_path
		--pallet "pallet_balances" --extrinsic "*"
		--output ./weights/ --header ./HEADER.rs --template ./template.hbs
)
```

This uses the same flags as the node-integrated benchmarking CLI. If the output is a directory, a separate
file is generated for each pallet/instance.

## Backwards Compatibility

The `pallet` subcommand is identical to the node-integrated CLI but must be prefixed with `v1` to ensure
compatibility and allow for future versioned updates.
