# Parachain modules

## Configuration

Note: This section is not necessary for local development, as there are scripts to auto-configure the parachain in the
[test directory](https://github.com/Snowfork/snowbridge/blob/main/web/packages/test).

For a fully operational chain, further configuration of the initial chain spec is required. The specific configuration will
depend heavily on your environment, so this guide will remain high-level.

After completing a release build of the parachain, build an initial spec for the snowbase runtime:

```bash
target/release/snowbridge build-spec --chain snowbase --disable-default-bootnode > spec.json
```

Now edit the spec and configure the following:
1. Recently finalized ethereum header and difficulty for the ethereum light client
2. Contract addresses for the Ether, Erc20, and Dot apps.
3. Authorized principal for the basic channel

For an example configuration, consult the [setup script](https://github.com/Snowfork/snowbridge/blob/main/web/packages/test/scripts/start-services.sh)
for our local development stack. Specifically the `start_polkadot_launch` bash function.

## Tests

To run the parachain tests locally, use `cargo test --workspace`. For the full suite of tests, use
`cargo test --workspace --features runtime-benchmarks`.

Optionally exclude the top-level and runtime crates:

```bash
cargo test --workspace \
        --features runtime-benchmarks \
        --exclude snowbridge \
        --exclude snowbridge-runtime \
        --exclude snowblink-runtime \
        --exclude snowbase-runtime
```

### Updating test data for inbound channel unit tests

To regenerate the test data, use a test with multiple `submit` calls in `ethereum/test/test_basic_outbound_channel.js`, eg.
"should increment nonces correctly".

Add the following preamble:

```javascript
const rlp = require("rlp");
const contract = BasicOutboundChannel;
const signature = 'Message(address,address,uint64,uint64,bytes)';
```

For each encoded log you want to create, find a transaction object `tx` returned from a `submit` call and run this:

```javascript
const rawLog = tx.receipt.rawLogs[0];
const encodedLog = rlp.encode([rawLog.address, rawLog.topics, rawLog.data]).toString("hex");
console.log(`encodedLog: ${encodedLog}`);
const iface = new ethers.utils.Interface(contract.abi);
const decodedEventLog = iface.decodeEventLog(
  signature,
  rawLog.data,
  rawLog.topics,
);
console.log(`decoded rawLog.data: ${JSON.stringify(decodedEventLog)}`);
```

Place the `encodedLog` string in the `message.data` field in the test data. Use the `decoded rawLog.data` field to
update the comments with the decoded log data.

## Generating pallet weights from benchmarks

Build the parachain with the runtime benchmark flags for the chosen runtime:

```bash
runtime=snowbase
cargo build \
    --release \
    --no-default-features \
    --features "$runtime-native,rococo-native,runtime-benchmarks,$runtime-runtime-benchmarks" \
    --bin snowbridge
```

List available pallets and their benchmarks:

```bash
./target/release/snowbridge benchmark pallet --chain $runtime --list
```

Run a benchmark for a pallet, generating weights:

```bash
target/release/snowbridge benchmark pallet \
  --chain=$runtime \
  --execution=wasm \
  --wasm-execution=compiled \
  --pallet=basic_channel_inbound \
  --extra \
  --extrinsic=* \
  --repeat=20 \
  --steps=50 \
  --output=pallets/basic-channel/src/inbound/weights.rs \
  --template=templates/module-weight-template.hbs
```

## Generating beacon test fixtures and benchmarking data

### Minimal Spec

To generate `minimal` test data and benchmarking data, make sure to start the local E2E setup to spin up a local beacon
node instance to connect to:

```bash
cd web/packages/test
./scripts/start-services.sh
```

Wait for output `Testnet has been initialized`.

In a separate terminal, from the `snowbridge` directory, run:

```bash
mage -d relayer build && relayer/build/snowbridge-relay generate-beacon-data --spec "minimal" && cd parachain &&
cargo +nightly fmt -- --config-path rustfmt.toml && cd -
```

### Mainnet Spec

We only use the mainnet spec for generating fixtures for pallet weight benchmarks.

To generate the data we can connect to the Lodestar Goerli public node. The script already connects to the Lodestar node,
so no need to start up additional services. In the event of the Lodestar node not being available, you can start up your
own stack with these commands:

```bash
cd web/packages/test
./scripts/start-goerli.sh
```

From the `snowbridge` directory, run:

```bash
mage -d relayer build && relayer/build/snowbridge-relay generate-beacon-data --spec "mainnet" && cd parachain &&
cargo +nightly fmt -- --config-path rustfmt.toml && cd -
```

### Benchmarking tests

To run the benchmark tests

```bash
cd parachain/pallets/ethereum-beacon-client
cargo test --release --features runtime-benchmarks
```
