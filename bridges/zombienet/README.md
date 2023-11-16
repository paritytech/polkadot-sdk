# Bridges Tests for Local Rococo <> Westend Bridge

This folder contains [zombienet](https://github.com/paritytech/zombienet/) based integration tests for both
onchain and offchain bridges code. Due to some
[technical difficulties](https://github.com/paritytech/parity-bridges-common/pull/2649#issue-1965339051), we
are using native zombienet provider, which means that you need to build some binaries locally.

To start those tests, you need to:

- download latest [zombienet release](https://github.com/paritytech/zombienet/releases);

- build Polkadot binary by running `cargo build -p polkadot --release  --features fast-runtime` command in the
[`polkadot-sdk`](https://github.com/paritytech/polkadot-sdk) repository clone;

- build Polkadot Parachain binary by running `cargo build -p polkadot-parachain-bin --release` command in the
[`polkadot-sdk`](https://github.com/paritytech/polkadot-sdk) repository clone;

- ensure that you have [`node`](https://nodejs.org/en) installed. Additionally, we'll need globally installed
`polkadot/api-cli` package (use `npm install -g @polkadot/api-cli@beta` to install it);

- build Substrate relay by running `cargo build -p substrate-relay --release` command in the
[`parity-bridges-common`](https://github.com/paritytech/parity-bridges-common) repository clone.

- copy fresh `substrate-relay` binary, built in previous point, to the `~/local_bridge_testing/bin/substrate-relay`;

- change the `POLKADOT_SDK_FOLDER` and `ZOMBIENET_BINARY_PATH` (and ensure that the nearby variables
have correct values) in the `./run-tests.sh`.

After that, you could run tests with the `./run-tests.sh` command. Hopefully, it'll show the
"All tests have completed successfully" message in the end. Otherwise, it'll print paths to zombienet
process logs, which, in turn, may be used to track locations of all spinned relay and parachain nodes.
