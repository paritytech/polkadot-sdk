# Polkadot Omni Node

This is a white labeled implementation based on [`polkadot-omni-node-lib`](https://crates.io/crates/polkadot-omni-node-lib).
It can be used to start a parachain node from a provided chain spec file. It is only compatible with runtimes that use block
number `u32` and `Aura` consensus.

## Installation

Download & expose it via `PATH`:

```bash
# Download and set it on PATH.
wget https://github.com/paritytech/polkadot-sdk/releases/download/<stable_release_tag>/polkadot-omni-node
chmod +x polkadot-omni-node
export PATH="$PATH:`pwd`"
```

Compile & install via `cargo`:

```bash
# Assuming ~/.cargo/bin is on the PATH
cargo install polkadot-omni-node --locked
```

## Usage

A basic example for an Omni Node run starts from a runtime which implements the [`sp_genesis_builder::GenesisBuilder`](https://docs.rs/sp-genesis-builder/latest/sp_genesis_builder/trait.GenesisBuilder.html).
The interface mandates the runtime to expose a [`named-preset`](https://docs.rs/staging-chain-spec-builder/latest/staging_chain_spec_builder/#generate-chain-spec-using-runtime-provided-genesis-config-preset).

### 1. Install chain-spec-builder

**Note**: `chain-spec-builder` binary is published on [`crates.io`](https://crates.io) under
[`staging-chain-spec-builder`](https://crates.io/crates/staging-chain-spec-builder) due to a name conflict.
Install it with `cargo` like bellow :

```bash
cargo install staging-chain-spec-builder --locked
```

### 2. Generate a chain spec

Omni Node expects for the chain spec to contain parachains related fields like `relay_chain` and `para_id`.
These fields can be introduced by running [`staging-chain-spec-builder`](https://crates.io/crates/staging-chain-spec-builder)
with additional flags:

```bash
chain-spec-builder create --relay-chain <relay_chain_id> --para-id <id> -r <runtime.wasm> named-preset <preset_name>
```

### 3. Run Omni Node

And now with the generated chain spec we can start the node in development mode like so:

```bash
polkadot-omni-node --dev --chain <chain_spec.json>
```

## Useful links

* [`Omni Node Polkadot SDK Docs`](https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/reference_docs/omni_node/index.html)
* [`Chain Spec Genesis Reference Docs`](https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/reference_docs/chain_spec_genesis/index.html)
* [`polkadot-parachain-bin`](https://crates.io/crates/polkadot-parachain-bin)
* [`polkadot-sdk-parachain-template`](https://github.com/paritytech/polkadot-sdk-parachain-template)
* [`frame-omni-bencher`](https://crates.io/crates/frame-omni-bencher)
* [`staging-chain-spec-builder`](https://crates.io/crates/staging-chain-spec-builder)
