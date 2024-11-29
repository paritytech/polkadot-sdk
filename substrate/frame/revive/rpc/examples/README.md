## Pre-requisites

 Build `pallet-revive-fixture`, as we need some compiled contracts to exercise the RPC server.

```bash
cargo build -p pallet-revive-fixtures
```

## Start the node

Start the kitchensink node:

```bash
RUST_LOG="error,evm=debug,sc_rpc_server=info,runtime::revive=debug" cargo run --bin substrate-node -- --dev
```

## Start a zombienet network

Alternatively, you can start a zombienet network with the westend Asset Hub parachain:

Prerequisites for running a local network:
- download latest [zombienet release](https://github.com/paritytech/zombienet/releases);
- build Polkadot binary by running `cargo build -p polkadot --release  --features fast-runtime` command in the
  [`polkadot-sdk`](https://github.com/paritytech/polkadot-sdk) repository clone;
- build Polkadot Parachain binary by running `cargo build -p polkadot-parachain-bin --release` command in the
  [`polkadot-sdk`](https://github.com/paritytech/polkadot-sdk) repository clone;

```bash
zombienet spawn --provider native  westend_local_network.toml
```

## Start the RPC server

This command starts the Ethereum JSON-RPC server, which runs on `localhost:8545` by default:

```bash
RUST_LOG="info,eth-rpc=debug" cargo run -p pallet-revive-eth-rpc -- --dev
```

## Rust examples

Run one of the examples from the `examples` directory to send a transaction to the node:

```bash
RUST_LOG="info,eth-rpc=debug" cargo run -p pallet-revive-eth-rpc --features example --example deploy
```

## JS examples

Interact with the node using MetaMask & Ether.js, by starting the example web app:

```bash

cd substrate/frame/revive/rpc/examples/js
bun install
bun run dev
```

Alternatively, you can run the example script directly:

```bash
cd substrate/frame/revive/rpc/examples/js
bun src/script.ts
```

### Configure MetaMask

See the doc [here](https://contracts.polkadot.io/work-with-a-local-node#metemask-configuration) for more
information on how to configure MetaMask.

