# Bridges Tests for Local Rococo <> Westend Bridge

This folder contains [zombienet](https://github.com/paritytech/zombienet/) based integration tests for both
onchain and offchain bridges code. Due to some
[technical difficulties](https://github.com/paritytech/parity-bridges-common/pull/2649#issue-1965339051), we
are using native zombienet provider, which means that you need to build some binaries locally.

To start those tests, you need to:

- download latest [zombienet release](https://github.com/paritytech/zombienet/releases);

- build Polkadot binary by running `cargo build -p polkadot --release --features fast-runtime` command in the
  [`polkadot-sdk`](https://github.com/paritytech/polkadot-sdk) repository clone;

- build Polkadot Parachain binary by running `cargo build -p polkadot-parachain-bin --release` command in the
  [`polkadot-sdk`](https://github.com/paritytech/polkadot-sdk) repository clone;

- ensure that you have [`node`](https://nodejs.org/en) installed. Additionally, we'll need globally installed
  `polkadot/api-cli` package (use `yarn global add @polkadot/api-cli` to install it);

- build Substrate relay by running `cargo build -p substrate-relay --release` command in the
  [`parity-bridges-common`](https://github.com/paritytech/parity-bridges-common) repository clone.

- copy fresh `substrate-relay` binary, built in previous point, to the `~/local_bridge_testing/bin/substrate-relay`;

- change the `ZOMBIENET_BINARY_PATH` (and ensure that the nearby variables have correct values) in
  the `./run-new-test.sh`.

Extra steps for the Polkadot<>Kusama test:

- clone the [`polkadot-fellows/runtimes`](https://github.com/polkadot-fellows/runtimes) locally and do the following
  adaptation:
    - Add the `sudo` pallet to the Polkadot and Kusama runtimes and give sudo rights to Alice.

- build the chain spec generator by running `cargo build --release -p chain-spec-generator --features fast-runtime` 
  command in the [`polkadot-fellows/runtimes`](https://github.com/polkadot-fellows/runtimes) repository clone.

- copy fresh `chain-spec-generator` binary, built in previous point to `~/local_bridge_testing/bin/chain-spec-generator`

After that, you could run tests with the `./run-new-test.sh <test>` command. Hopefully, it'll complete successfully.
Otherwise, it'll print paths to zombienet logs and command logs, which can be used for debugging failures.
