# polkadot-bulletin-parachain (PoC)

Custom parachain node for the Polkadot Bulletin Chain, built by composing
`polkadot-omni-node-lib` instead of reusing the generic `polkadot-omni-node` binary.

Tracking: [polkadot-bulletin-chain#479](https://github.com/paritytech/polkadot-bulletin-chain/issues/479).
Discussion: [polkadot-sdk#11662](https://github.com/paritytech/polkadot-sdk/pull/11662).

## Status: Pass 1 of 2

This PR delivers Pass 1: HOP is fully reverted out of `polkadot-omni-node-lib`,
addressing the structural objection from @alindima and @sandreim that the
generic omni-node should not carry Bulletin-specific protocol code.

The Bulletin-side HOP wiring (Pass 2) is a follow-up commit on this same PR.
It adds a `NodeExtension` trait to the lib (a generic plug-in point for extra
service tasks and RPC modules) and a `HopExtension` impl in this crate that
builds the data pool, spawns the maintenance task, and registers the HOP RPC.
That trait was already added in Pass 1; the threading through `AuraNode`,
`NodeSpec`, `RunConfig`, and `run_with_custom_cli` is what Pass 2 finishes.

| | Pass 1 (this commit) | Pass 2 (follow-up) |
|---|---|---|
| HOP code in `polkadot-omni-node-lib` | removed | still removed |
| `NodeExtension` trait | added (no consumers yet) | threaded through lib startup |
| Bulletin binary builds | yes | yes |
| Bulletin `--version` | works | works |
| Bulletin `--help` lists HOP flags | no (they followed the lib) | yes (from a Bulletin-owned `Cli` that flattens `HopParams`) |
| HOP runtime wired | no | yes |
| Live `--dev` block production | yes (no HOP) | yes (with HOP) |

## Layout

```text
cumulus/polkadot-bulletin-parachain/
├── Cargo.toml      # depends on polkadot-omni-node-lib
├── build.rs
├── src/main.rs     # ~50 LOC wrapper around run_with_custom_cli
├── tests/cli.rs    # version smoke test
└── README.md
```

Registered as a workspace member, not in `default-members`.

## Build, test, run

```bash
SKIP_WASM_BUILD=1 cargo check -p polkadot-bulletin-parachain
SKIP_WASM_BUILD=1 cargo test  -p polkadot-bulletin-parachain --test cli
cargo build -p polkadot-bulletin-parachain --release
./target/release/polkadot-bulletin-parachain --version
```

Live `--dev` smoke run (cumulus-test-runtime as the runtime fixture, no HOP
since cumulus-test-runtime does not implement `HopRuntimeApi`):

```bash
cargo build -p cumulus-test-runtime --release
./target/release/polkadot-bulletin-parachain chain-spec-builder \
    -c /tmp/bulletin-spec.json create \
    --runtime target/release/wbuild/cumulus-test-runtime/cumulus_test_runtime.compact.compressed.wasm \
    named-preset development
# patch /tmp/bulletin-spec.json to add `relay_chain` and `para_id` fields
./target/release/polkadot-bulletin-parachain \
    --chain /tmp/bulletin-spec.json --dev --tmp --rpc-port 9944 --no-hardware-benchmarks
```

This was verified locally: genesis initialized, runtime metadata V15 detected,
JSON-RPC up, blocks #1, #2, #3 produced at the 3-second manual-seal cadence.

## Pass 2 design preview

The lib gains a `NodeExtension<Block, RuntimeApi>` trait (already present after
Pass 1) with default no-op `on_start` and `build_rpc_extension` methods. Pass 2
threads an `Ext` generic parameter through `AuraNode`, `NodeSpec` impls,
`new_aura_node_spec`, `command::new_node_spec`, `RunConfig`, and
`run_with_custom_cli`, defaulting to `NoNodeExtension` so `polkadot-omni-node`
is unaffected. The Bulletin binary supplies a `HopExtension` that owns the
`HopParams` (parsed via a Bulletin-owned `Cli` that flattens them) and uses
interior mutability (`Arc<OnceLock<Arc<HopDataPool>>>`) to share state between
`on_start` (builds pool, spawns maintenance task) and `build_rpc_extension`
(registers `HopRpcServer`).
