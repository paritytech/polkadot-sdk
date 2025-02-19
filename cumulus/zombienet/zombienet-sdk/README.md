# How to run locally

As a prerequisite, the `test-parachain` and `polkadot` binaries need to be installed or available under `$PATH`.

```
# install test-parachain
cargo install --path ./cumulus/test/service --locked --release
# install polkadot
cargo install --path ./polkadot --locked --release
```

The following command launches the tests:

```
ZOMBIE_PROVIDER=native cargo test --release -p cumulus-zombienet-sdk-tests
```

In addition, you can specify a base directory with `ZOMBIENET_SDK_BASE_DIR=/my/dir/of/choice`. All chain files and logs
will be placed in that directory.
