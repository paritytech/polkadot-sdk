# Bridges Tests for Local Rococo <> Westend Bridge

This folder contains [zombienet](https://github.com/paritytech/zombienet/) based integration tests for both
onchain and offchain bridges code.

## zombienet-sdk tests (recommended)

The `0001-asset-transfer` and `0002-free-headers-synced-while-idle` tests have been migrated to the Rust based
[`zombienet-sdk`](https://github.com/paritytech/zombienet-sdk) and now live in the
[`bridges/testing/zombienet-sdk-tests`](./zombienet-sdk-tests) crate (mirroring
`polkadot/zombienet-sdk-tests`). They spawn both relay-chain networks together with their Bridge Hub and Asset
Hub parachains, drive the `substrate-relay` binary as subprocesses and assert on-chain state via `subxt`.

Prerequisites:

- build the `polkadot` and `polkadot-parachain` binaries and put them on your `PATH`:

  ```bash
  cargo build -p polkadot --release --features fast-runtime
  cargo build -p polkadot-parachain-bin --release
  export PATH="$PWD/target/release:$PATH"
  ```

- make the `substrate-relay` binary available and point `SUBSTRATE_RELAY_BINARY` at it (it defaults to
  `~/local_bridge_testing/bin/substrate-relay`):

  ```bash
  export SUBSTRATE_RELAY_BINARY=~/local_bridge_testing/bin/substrate-relay
  ```

Running the tests (the subxt metadata `.scale` files are generated from locally built runtime WASM blobs via the
`zombie-metadata` feature on the first build):

```bash
cargo test -p bridges-zombienet-sdk-tests --features zombie-ci,zombie-metadata -- --nocapture
```

The legacy bash + `zndsl` runner described below is kept for the not-yet-migrated
`0003-required-headers-synced-while-active` test.

Prerequisites for running the tests locally:

- download latest [zombienet release](https://github.com/paritytech/zombienet/releases) and place it at
`~/local_bridge_testing/bin/zombienet`;

- build Polkadot binary by running `cargo build -p polkadot --release  --features fast-runtime` command in the
  [`polkadot-sdk`](https://github.com/paritytech/polkadot-sdk) repository clone;

- build Polkadot Parachain binary by running `cargo build -p polkadot-parachain-bin --release` command in the
  [`polkadot-sdk`](https://github.com/paritytech/polkadot-sdk) repository clone;

- ensure that you have [`node`](https://nodejs.org/en) installed. Additionally, we'll need the globally installed
  `polkadot/api-cli` package. Use `yarn global add @polkadot/api-cli` to install it.

- build Substrate relay by running `cargo build -p substrate-relay --release` command in the
  [`parity-bridges-common`](https://github.com/paritytech/parity-bridges-common) repository clone;

- copy the `substrate-relay` binary, built in the previous step, to `~/local_bridge_testing/bin/substrate-relay`;

On Mac, you'll also need to do the following:

- Install an updated version of bash by installing homebrew and running `brew install bash`;

- Install jq with `brew install jq`;

After that, any test can be run using the `run-test.sh` command.
Example: `./run-test.sh 0003-required-headers-synced-while-active`

Hopefully, it'll show the
"All tests have completed successfully" message in the end. Otherwise, it'll print paths to zombienet
process logs, which, in turn, may be used to track locations of all spinned relay and parachain nodes.
