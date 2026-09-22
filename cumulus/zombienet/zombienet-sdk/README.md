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

## JAM network tests

The test suites can run against a JAM network instead of a relay chain by enabling the `jam` feature:

```
ZOMBIE_PROVIDER=native cargo test --release -p cumulus-zombienet-sdk-tests --features jam,zombie-ci
```

The `jam` feature gates the `elastic_scaling` and `block_bundling` test suites to run on a JAM network. Tests requiring
more than two cores are excluded: `elastic_scaling::asset_hub_westend`, `elastic_scaling::slot_based_authoring`,
`elastic_scaling::upgrade_to_3_cores`, and `block_bundling::basic`. A tiny JAM network has exactly two cores (six
validators, three per core), so these tests have no JAM counterpart.

Some of these suites upgrade a para to a `cumulus-test-runtime` feature flavor. Those flavors are
built as PolkaVM blobs by building the test crate **without** `SKIP_WASM_BUILD` and with
`SUBSTRATE_RUNTIME_TARGET=riscv WASM_BUILD_RUSTFLAGS="--cfg jam"`: under the riscv target the
wasm-builder emits each flavor's PVM blob under the same `WASM_BINARY` const the test reads, so no
separate `.polkavm` path is needed. This is a heavy build — every flavor is compiled for riscv — and
it dominates the test run.

JAM networks are supported on the native provider only.
