# Omni Node binary

This is a white labeled implementation based on [`polkadot-omni-node-lib`](https://crates.io/crates/polkadot-omni-node-lib).
It can be used to start a parachain node from a provided chain spec file. It is only compatible with runtimes that use block
number `u32` and `Aura` consensus.

## Installation

We can either download the binary and expose it via the `PATH` environment variable, or compile and install it with `cargo`.

Example download & expose it via `PATH`:

```bash
# Download and set it on PATH.
wget https://github.com/paritytech/polkadot-sdk/releases/download/<stable_release_tag>/polkadot-omni-node
chmod +x polkadot-omni-node
export PATH="$PATH:`pwd`"
```

Example compile + install via `cargo`:

```bash
# Assuming ~/.cargo/bin is on the PATH
cargo install polkadot-omni-node
```

## High-level usage

A basic Omni Node run example can start from a runtime which implements the [sp_genesis_builder::GenesisBuilder](https://docs.rs/sp-genesis-builder/latest/sp_genesis_builder/trait.GenesisBuilder.html).
The interface mandates the runtime to expose a [named-preset](https://docs.rs/staging-chain-spec-builder/6.0.0/staging_chain_spec_builder/#generate-chain-spec-using-runtime-provided-genesis-config-preset).

### Install chain-spec-builder

**Note**: due to a name conflict with a crate that already exists on [crates.io](https://crates.io) the  `chain-spec-builder`
binary is published under [staging-chain-spec-builder](https://crates.io/crates/staging-chain-spec-builder). Install it with
`cargo` like bellow :

```bash
cargo install staging-chain-spec-builder
```

### Generate a chain spec

It is also expected for the chain spec to contain parachains related fields like `relay_chain` and `para_id`.
These fields can be introduced by running [staging-chain-spec-builder](https://crates.io/crates/staging-chain-spec-builder)
with additional flags:

```bash
chain-spec-builder create --relay-chain <relay_chain_id> --para-id <id> -r <runtime.wasm> named-preset <preset_name>
```

### Run Omni Node

And now with the generated chain spec we can start Omni Node like so:

```bash
polkadot-omni-node --chain <chain_spec.json>
```

## Useful links

* [Omni node Polkadot SDK docs](https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/reference_docs/omni_node/index.html)
* [polkadot-parachain-bin](https://crates.io/crates/polkadot-parachain-bin)
* [polkadot-sdk-parachain-template](https://github.com/paritytech/polkadot-sdk-parachain-template)
* [frame-omni-bencher](https://crates.io/crates/frame-omni-bencher)
* [staging-chain-spec-builder](https://crates.io/crates/staging-chain-spec-builder)
