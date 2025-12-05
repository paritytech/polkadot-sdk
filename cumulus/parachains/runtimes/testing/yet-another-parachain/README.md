# Yet Another Parachain (YAP) Runtime

A parachain runtime used for testing purposes.

## Features

- `dev` - Development mode enables running without a relay chain
- `fast-runtime` - Faster block times for testing
- `runtime-benchmarks` - Enable runtime benchmarking

## Running with polkadot-omni-node locally and 500ms blocks

### Build

```bash
# From the polkadot-sdk root directory

# 1. Build the YAP runtime with the dev feature
cargo build --release -p yet-another-parachain-runtime --features dev

# 2. Build polkadot-omni-node
cargo build --release -p polkadot-omni-node
```

### Generate Chain Spec

Available presets: `development`, `local_testnet`

```bash
# Generate chain spec using the development preset
./target/release/polkadot-omni-node chain-spec-builder \
  --chain-spec-path yap-dev-spec.json \
  create \
  --chain-name "YAP Development" \
  --chain-id yap-dev \
  -t development \
  --runtime ./target/release/wbuild/yet-another-parachain-runtime/yet_another_parachain_runtime.wasm \
  named-preset development

# Patch chain spec to add relay_chain for parachain mode
jq '. + {"relay_chain": "rococo-local"}' yap-dev-spec.json > tmp.json && mv tmp.json yap-dev-spec.json
```

### Run the Node

```bash
./target/release/polkadot-omni-node \
  --chain yap-dev-spec.json \
  --dev --dev-block-time 500\
  --tmp
```

## Connecting to the Node

Once running, you can connect to the node using:

- **Polkadot.js Apps**: https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:9944
- **RPC endpoint**: `ws://127.0.0.1:9944`
