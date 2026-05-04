# polkadot-bulletin-parachain (PoC)

Custom parachain node for the Polkadot Bulletin Chain, built by composing
`polkadot-omni-node-lib` instead of reusing the generic `polkadot-omni-node` binary.

Tracking: [polkadot-bulletin-chain#479](https://github.com/paritytech/polkadot-bulletin-chain/issues/479).
Discussion: [polkadot-sdk#11662](https://github.com/paritytech/polkadot-sdk/pull/11662).

## What this delivers

The PR is three commits, layered:

1. **Pass 1 (revert)**: HOP physically removed from `polkadot-omni-node-lib`. The
   `sc-hop` dep, `sp_hop::HopRuntimeApi` supertrait, `HopParams` CLI fields,
   pool building, maintenance task spawn, and `HopRpcServer` registration are
   all gone from the lib.

2. **Pass 2 (extension trait + replicate)**: HOP wiring lives entirely in
   `cumulus/polkadot-bulletin-parachain/`. The lib gains a generic
   `NodeExtension<Block, RuntimeApi>` trait plus an object-safe
   `NodeExtensionFactory` plug-in point on `RunConfig`. The Bulletin crate
   provides a `HopExtension` impl that owns the `HopParams`, builds the
   `HopDataPool` in `on_start`, spawns `hop-maintenance`, and registers
   `HopRpcServer` in `build_rpc_extension`.

3. **Pass 3 (CLI flag exposure)**: The bulletin's `main.rs` builds the clap
   parser itself, layering `sc_hop::HopParams::augment_args` on top of the
   lib's `Cli<CliConfig>`. After parsing, it extracts `HopParams` from the
   `ArgMatches` to construct the `HopExtensionFactory`, then dispatches via
   the new `polkadot_omni_node_lib::run_with_matches` entry point. All HOP
   flags (`--enable-hop`, `--hop-max-pool-size`, `--hop-retention-blocks`,
   `--hop-data-dir`, etc.) appear in `--help` and flow through to the runtime
   wiring.

The `polkadot-omni-node` binary is unaffected. Its `RunConfig::new` defaults
the factory to a no-op `NoNodeExtensionFactory` and its existing
`run_with_custom_cli` entry point continues to work without changes.

The lib retains a no-op `HopRuntimeApi` stub in `fake_runtime_api/utils.rs`,
purely to satisfy compile-time trait bounds. The stub is unreachable at
runtime; the actual runtime is loaded from the chain spec wasm.

## Layout

```text
cumulus/polkadot-bulletin-parachain/
├── Cargo.toml
├── build.rs
├── src/
│   ├── main.rs           # CLI augmentation + RunConfig wiring
│   └── hop_extension.rs  # HopExtension + HopExtensionFactory impls
├── tests/cli.rs          # version + HOP-flag smoke tests
└── README.md
```

Registered as a workspace member, not in `default-members`.

## Build, test, run

```bash
SKIP_WASM_BUILD=1 cargo check -p polkadot-bulletin-parachain
SKIP_WASM_BUILD=1 cargo test  -p polkadot-bulletin-parachain --test cli
cargo build -p polkadot-bulletin-parachain --release
./target/release/polkadot-bulletin-parachain --version
./target/release/polkadot-bulletin-parachain --help | grep -i hop
```

Live dev-node smoke run (cumulus-test-runtime as runtime fixture, with
`--enable-hop` from the CLI):

```bash
cargo build -p cumulus-test-runtime --release
./target/release/polkadot-bulletin-parachain chain-spec-builder \
    -c /tmp/bulletin-spec.json create \
    --runtime target/release/wbuild/cumulus-test-runtime/cumulus_test_runtime.compact.compressed.wasm \
    named-preset development
# patch /tmp/bulletin-spec.json to add `relay_chain` and `para_id` fields
./target/release/polkadot-bulletin-parachain \
    --chain /tmp/bulletin-spec.json --dev --tmp --rpc-port 9944 --no-hardware-benchmarks \
    --enable-hop --hop-data-dir /tmp/bulletin-hop
```

What you should see in the logs:

```text
🪪 Parachain id: 100
Initializing HOP data pool params=HopParams { enable_hop: true, ..., data_dir: Some("/tmp/bulletin-hop") }
HOP data pool initialized, RPC methods will be registered
HOP enabled but runtime does not support HopRuntimeApi — running cleanup only
🎁 Prepared block #1 ... 🏆 Imported #1
🎁 Prepared block #2 ... 🏆 Imported #2
🎁 Prepared block #3 ... 🏆 Imported #3
```

The HOP cleanup-only path is expected: cumulus-test-runtime is the stub
runtime fixture and does not implement `HopRuntimeApi`. With a real Bulletin
parachain runtime that does implement it, the `hop-maintenance` task would run
the full promotion flow.

## Branch layout

- Base branch: `origin/hop-base` (PR #11662)
- This work: `ndk/bulletin-parachain-poc`
- Per @bkontur: if this approach is accepted, the contents of
  `cumulus/polkadot-bulletin-parachain/` move to
  `paritytech/polkadot-bulletin-chain`. The lib-side `NodeExtension` trait,
  `NodeExtensionFactory` plug-in points, and `run_with_matches` entry point
  stay in `polkadot-sdk` so other parachains can use the same hooks.
