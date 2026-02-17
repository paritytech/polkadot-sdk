# How to run locally

As a prerequisite, the `test-parachain` and `polkadot` binaries need to be installed or available under `$PATH`.

The following commands need to be run from the repository root:
```
# install test-parachain
cargo install --path ./cumulus/test/service --locked
# install polkadot
cargo install --path ./polkadot --locked
```

The following command launches the tests:

```
ZOMBIE_PROVIDER=native cargo test --release -p cumulus-zombienet-sdk-tests --features zombie-ci
```

You can also just use `run.sh` that setups everything for you and runs the tests.

In addition, you can specify a base directory with `ZOMBIENET_SDK_BASE_DIR=/my/dir/of/choice`. All chain files and logs
will be placed in that directory.
