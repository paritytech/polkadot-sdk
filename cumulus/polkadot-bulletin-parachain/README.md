# polkadot-bulletin-parachain (PoC)

Custom parachain node for the Polkadot Bulletin Chain, built by composing
`polkadot-omni-node-lib` instead of reusing the generic `polkadot-omni-node` binary.

Tracking: [polkadot-bulletin-chain#479](https://github.com/paritytech/polkadot-bulletin-chain/issues/479).
Discussion: [polkadot-sdk#11662](https://github.com/paritytech/polkadot-sdk/pull/11662).

## What this delivers (Pass 1 + Pass 2)

The PR is two commits, layered:

1. **Pass 1 (revert)**: HOP physically removed from `polkadot-omni-node-lib`. The
   `sc-hop` dep, `sp_hop::HopRuntimeApi` supertrait, `HopParams` CLI fields,
   pool building, maintenance task spawn, and `HopRpcServer` registration are
   all gone from the lib.

2. **Pass 2 (replicate)**: HOP wiring lives entirely in
   `cumulus/polkadot-bulletin-parachain/`. The lib gains a generic
   `NodeExtension<Block, RuntimeApi>` trait plus an object-safe
   `NodeExtensionFactory` plug-in point on `RunConfig`. The Bulletin crate
   provides a `HopExtension` impl that owns the `HopParams`, builds the
   `HopDataPool` in `on_start`, spawns `hop-maintenance`, and registers
   `HopRpcServer` in `build_rpc_extension`. The `polkadot-omni-node` binary is
   unaffected (its `RunConfig::new` defaults the factory to a no-op
   `NoNodeExtensionFactory`).

The lib retains a no-op `HopRuntimeApi` stub in `fake_runtime_api/utils.rs`,
purely to satisfy compile-time trait bounds. The stub is unreachable at
runtime; the actual runtime is loaded from the chain spec wasm.

## Layout

```text
cumulus/polkadot-bulletin-parachain/
├── Cargo.toml
├── build.rs
├── src/
│   ├── main.rs           # entry point: CliConfig + RunConfig wiring
│   └── hop_extension.rs  # HopExtension + HopExtensionFactory impls
├── tests/cli.rs          # version smoke test
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

Live dev-node smoke run (cumulus-test-runtime as runtime fixture):

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

What you should see in the logs:

```text
🪪 Parachain id: 100
hop: Initializing HOP data pool params=HopParams { enable_hop: true, ... }
hop: HOP data pool initialized, RPC methods will be registered
hop: HOP enabled but runtime does not support HopRuntimeApi — running cleanup only
🎁 Prepared block #1 ... 🏆 Imported #1
🎁 Prepared block #2 ... 🏆 Imported #2
🎁 Prepared block #3 ... 🏆 Imported #3
```

The HOP cleanup-only path is expected: cumulus-test-runtime is the stub
runtime fixture and does not implement `HopRuntimeApi`. With a real Bulletin
parachain runtime that does implement it, the `hop-maintenance` task would run
the full promotion flow.

## Pass 3 (follow-up)

The HOP CLI flags are not exposed through `--help` yet. The bulletin's
`main.rs` constructs `HopParams` via `HopParams::parse_from(["bulletin",
"--enable-hop"])`, which uses defaults. To expose `--enable-hop`,
`--hop-max-pool-size`, etc. as user-controllable flags, the Bulletin crate
needs a `BulletinCli` that flattens `polkadot_omni_node_lib::Cli<CliConfig>`
together with `sc_hop::HopParams`, plus a thin fork of
`run_with_custom_cli` that uses the bulletin's `BulletinCli` instead of the
lib's `Cli<Config>`. That work is a follow-up commit; the Pass 2 wiring is
otherwise complete.

## Branch layout

- Base branch: `origin/hop-base` (PR #11662)
- This work: `ndk/bulletin-parachain-poc`
- Per @bkontur: if this approach is accepted, the contents of
  `cumulus/polkadot-bulletin-parachain/` move to
  `paritytech/polkadot-bulletin-chain`. The lib-side `NodeExtension` trait
  + `NodeExtensionFactory` plug-in points stay in `polkadot-sdk` so other
  parachains can use the same hook.
