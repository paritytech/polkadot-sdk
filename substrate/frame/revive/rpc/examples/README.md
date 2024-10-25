## Pre-requisites

 Build `pallet-revive-fixture`, as we need some compiled contracts to exercise the RPC server.

```bash
cargo build -p pallet-revive-fixtures --features riscv
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
RUST_LOG="info,eth-rpc=debug" cargo run -p pallet-revive-eth-rpc --features dev
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

You can use the following instructions to setup [MetaMask] with the local chain.

> **Note**: When you interact with MetaMask and restart the chain, you need to clear the activity tab (Settings >
Advanced > Clear activity tab data), and in some cases lock/unlock MetaMask to reset the nonce.
See [this guide][reset-account] for more info on how to reset the account activity.

#### Add a new network

To interact with the local chain, add a new network in [MetaMask].
See [this guide][add-network] for more info on how to add a custom network.

Make sure the node and the RPC server are started, and use the following settings to configure the network
(MetaMask > Networks > Add a network manually):

- Network name: KitchenSink
- RPC URL: <http://localhost:8545>
- Chain ID: 420420420
- Currency Symbol: `DEV`

#### Import Dev account

You will need to import the following account, endowed with some balance at genesis, to interact with the chain.
See [this guide][import-account] for more info on how to import an account.

- Account: `0xf24FF3a9CF04c71Dbc94D0b566f7A27B94566cac`
- Private Key: `5fb92d6e98884f76de468fa3f6278f8807c48bebc13595d45af5bdc4da702133`

[MetaMask]: https://metamask.io
[add-network]: https://support.metamask.io/networks-and-sidechains/managing-networks/how-to-add-a-custom-network-rpc/#adding-a-network-manually
[import-account]: https://support.metamask.io/managing-my-wallet/accounts-and-addresses/how-to-import-an-account/
[reset-account]: https://support.metamask.io/managing-my-wallet/resetting-deleting-and-restoring/how-to-clear-your-account-activity-reset-account

