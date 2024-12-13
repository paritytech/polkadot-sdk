# Bridges Tests for Local Rococo <> Westend Bridge

This folder contains [zombienet](https://github.com/paritytech/zombienet/) based integration tests for both
onchain and offchain bridges code.

Prerequisites for running the tests locally:

- download latest [zombienet release](https://github.com/paritytech/zombienet/releases);

- build Polkadot binary by running `cargo build -p polkadot --release  --features fast-runtime` command in the
  [`polkadot-sdk`](https://github.com/paritytech/polkadot-sdk) repository clone;

- build Polkadot Parachain binary by running `cargo build -p polkadot-parachain-bin --release` command in the
  [`polkadot-sdk`](https://github.com/paritytech/polkadot-sdk) repository clone;

- ensure that you have [`node`](https://nodejs.org/en) installed. Additionally, we'll need globally installed
  `polkadot/api-cli` package (use `npm install -g @polkadot/api-cli@beta` to install it);

- build Substrate relay by running `cargo build -p substrate-relay --release` command in the
  [`parity-bridges-common`](https://github.com/paritytech/parity-bridges-common) repository clone;

- copy the `substrate-relay` binary, built in the previous step, to `~/local_bridge_testing/bin/substrate-relay`;

After that, any test can be run using the `run-test.sh` command.
Example: `./run-test.sh 0001-asset-transfer`

Hopefully, it'll show the
"All tests have completed successfully" message in the end. Otherwise, it'll print paths to zombienet
process logs, which, in turn, may be used to track locations of all spinned relay and parachain nodes.
