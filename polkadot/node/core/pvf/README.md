# PVF Host

## Running basic tests

The first step is to build the worker binaries:

```sh
SKIP_WASM_BUILD=1 cargo build --bin polkadot-execute-worker --bin polkadot-prepare-worker
```

and then to run `cargo test` in the `pvf/` directory.

## Observing Logs

To verify expected behavior it's often useful to observe logs. To avoid too many logs at once, run one test at a time:

1. Add `sp_tracing::try_init_simple();` to the beginning of a test
2. Specify `RUST_LOG=parachain::pvf=trace` before the cargo command.

For example:

```sh
RUST_LOG=parachain::pvf=trace cargo test execute_can_run_serially
```

For more info on how our logs work, check [the docs](https://github.com/paritytech/polkadot-sdk/blob/master/polkadot/node/gum/src/lib.rs).

## Running a test-network with zombienet

For major changes it is highly recommended to run a test-network. Zombienet allows you to run a mini test-network locally on your own machine.

First, make sure you have [zombienet](https://github.com/paritytech/zombienet) installed.

Now, all the required binaries must be installed in your $PATH. You must run the following (not `zombienet setup`!) from the `polkadot/` directory in order to test your changes.

```sh
cargo install --path . --locked
```

You will also need to install `undying-collator`. From `polkadot/`, run:

```sh
cargo install --path ./parachain/test-parachains/undying/collator --locked
```

Finally, run the zombienet test from the `polkadot` directory:

```sh
RUST_LOG=parachain::pvf=trace zombienet --provider=native spawn zombienet_tests/functional/0001-parachains-pvf.toml
```

You can pick a validator node like `alice` from the output and view its logs (`tail -f <log_file>`) or metrics. Make sure there is nothing funny in the logs (try `grep WARN <log_file>`).

## Testing on Linux

Much of the PVF functionality, especially related to security, is Linux-only. If you touch anything security-related, make sure to test on Linux! If you're on a Mac, you can either run a VM or you can hire a VPS and use [EternalTerminal](https://github.com/MisterTea/EternalTerminal) to ssh into it. (ET preserves your session across disconnects, and unlike mosh it allows scrollback.)
