# polkadot-bulletin-parachain (PoC)

Custom parachain node for the Polkadot Bulletin Chain, composed from
`polkadot-omni-node-lib` instead of reusing the generic `polkadot-omni-node`
binary.

Tracking: [polkadot-bulletin-chain#479](https://github.com/paritytech/polkadot-bulletin-chain/issues/479).

HOP wiring (data-pool build, `hop-maintenance` task, `HopRpcServer`) lives in
this crate and plugs into the lib via `NodeExtension` /
`NodeExtensionFactory`. HOP CLI flags are exposed by augmenting the lib's
clap parser with `sc_hop::HopParams` and dispatching via
`polkadot_omni_node_lib::run_with_matches`. `polkadot-omni-node` is unaffected.

The lib retains a no-op `HopRuntimeApi` stub in `fake_runtime_api/utils.rs`
so the bulletin's `HopExtension` satisfies trait bounds at compile time. The
stub is unreachable at runtime; the actual runtime is loaded from the chain
spec wasm.

## Build, test, run

```bash
SKIP_WASM_BUILD=1 cargo check -p polkadot-bulletin-parachain
SKIP_WASM_BUILD=1 cargo test  -p polkadot-bulletin-parachain --test cli
cargo build -p polkadot-bulletin-parachain --release
./target/release/polkadot-bulletin-parachain --help | grep -i hop
```

Live dev-node smoke run against `cumulus-test-runtime`:

```bash
cargo build -p cumulus-test-runtime --release
./target/release/polkadot-bulletin-parachain chain-spec-builder \
    -c /tmp/bulletin-spec.json create \
    --runtime target/release/wbuild/cumulus-test-runtime/cumulus_test_runtime.compact.compressed.wasm \
    named-preset development
# patch /tmp/bulletin-spec.json to add `relay_chain` and `para_id`
./target/release/polkadot-bulletin-parachain \
    --chain /tmp/bulletin-spec.json --dev --tmp --rpc-port 9944 --no-hardware-benchmarks \
    --enable-hop --hop-data-dir /tmp/bulletin-hop
```

`cumulus-test-runtime` does not implement `HopRuntimeApi`, so HOP runs in
cleanup-only mode. With a real Bulletin runtime, the maintenance task runs
the full promotion flow.
