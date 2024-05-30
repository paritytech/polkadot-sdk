# The FRAME Benchmarking CLI

This crate contains commands to benchmark various aspects of Substrate and the hardware.  
The goal is to have a comprehensive suite of benchmarks that cover all aspects of Substrate and the hardware that its
running on.  
There exist fundamentally two ways to use this crate. A node-integrated CLI version, and a freestanding CLI. If you are
only interested in pallet benchmarking, then skip ahead to the [Freestanding CLI](#freestanding-cli).

# Node Integrated CLI

Mostly all Substrate nodes will expose some commands for benchmarking. You can refer to the `staging-node-cli` crate as
an example on how to integrate those. Note that for solely benchmarking pallets, the freestanding CLI is more suitable.

## Usage

Here we invoke the root command on the `staging-node-cli`. Most Substrate nodes should have a similar output, depending
on their integration of these commands.

```sh
$ cargo run -p staging-node-cli --profile=production --features=runtime-benchmarks -- benchmark

Sub-commands concerned with benchmarking.

USAGE:
    substrate benchmark <SUBCOMMAND>

OPTIONS:
    -h, --help       Print help information
    -V, --version    Print version information

SUBCOMMANDS:
    block       Benchmark the execution time of historic blocks
    machine     Command to benchmark the hardware.
    overhead    Benchmark the execution overhead per-block and per-extrinsic
    pallet      Benchmark the extrinsic weight of FRAME Pallets
    storage     Benchmark the storage speed of a chain snapshot
```

All examples use the `production` profile for correctness which makes the compilation *very* slow; for testing you can
use `--release`.
For the final results the `production` profile and reference hardware should be used, otherwise the results are not
comparable.

# Freestanding CLI

The freestanding is a standalone CLI that does not rely on any node integration. It can be used to benchmark pallets of
any FRAME runtime that does not utilize 3rd party host functions.  
It currently only supports pallet benchmarking, since the other commands still rely on a node.

## Installation

Installing from local source repository:

```sh
cargo install --locked --path substrate/utils/frame/omni-bencher --profile=production
```

## Usage

The exposed pallet sub-command is identical as the node-integrated CLI. The only difference is that it needs to be prefixed
with a `v1` to ensure drop-in compatibility.

First we need to ensure that there is a runtime available. As example we will build the Westend runtime:

```sh
cargo build -p westend-runtime --profile production --features runtime-benchmarks
```

Now the benchmarking can be started with:

```sh
frame-omni-bencher v1 \
    benchmark pallet \
    --runtime target/release/wbuild/westend-runtime/westend-runtime.compact.compressed.wasm \
    --pallet "pallet_balances" --extrinsic ""
```

For the exact arguments of the `pallet` command, please refer to the [pallet] sub-module.

# Commands

The sub-commands of both CLIs have the same semantics and are documented in their respective sub-modules:

- [block] Compare the weight of a historic block to its actual resource usage
- [machine] Gauges the speed of the hardware
- [overhead] Creates weight files for the *Block*- and *Extrinsic*-base weights
- [pallet] Creates weight files for a Pallet
- [storage] Creates weight files for *Read* and *Write* storage operations

License: Apache-2.0

<!-- LINKS -->

[pallet]: ../../../frame/benchmarking/README.md
[machine]: src/machine/README.md
[storage]: src/storage/README.md
[overhead]: src/overhead/README.md
[block]: src/block/README.md
