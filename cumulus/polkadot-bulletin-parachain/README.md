# polkadot-bulletin-parachain (PoC)

Custom parachain node for the Polkadot Bulletin Chain, built by composing
`polkadot-omni-node-lib` instead of reusing the generic `polkadot-omni-node` binary.

The point is to answer one question raised on
[paritytech/polkadot-sdk#11662](https://github.com/paritytech/polkadot-sdk/pull/11662):
can the lib be composed cleanly enough that HOP wiring lives in a Bulletin-specific
binary, instead of being baked into the generic omni-node?

This crate is the prototype. Built on top of `hop-base`, on branch
`ndk/bulletin-parachain-poc`. Tracking: [polkadot-bulletin-chain#479](https://github.com/paritytech/polkadot-bulletin-chain/issues/479).

## Layout

```text
cumulus/polkadot-bulletin-parachain/
├── Cargo.toml      # depends on polkadot-omni-node-lib
├── build.rs
├── src/main.rs     # ~50 LOC wrapper around run_with_custom_cli
└── README.md
```

Registered as a workspace member but not in `default-members`, so existing
top-level builds are unaffected.

## Build and run

```bash
SKIP_WASM_BUILD=1 cargo check -p polkadot-bulletin-parachain
cargo build -p polkadot-bulletin-parachain --release
./target/release/polkadot-bulletin-parachain --help | grep -i hop
```

## Test plan

1. **Compile gate**: `cargo check` passing means the lib's public surface is
   sufficient to compose a custom binary.
2. **CLI smoke test**: `--help` should list HOP flags (`--enable-hop`,
   `--hop-max-pool-size`, `--hop-retention-blocks`, etc.) inherited through
   `#[command(flatten)]`. `--version` should carry the Bulletin name.
3. **Live dev-node test (deferred)**: starting `--dev` against a chain spec
   needs a parachain-shaped runtime that implements `sp_hop::HopRuntimeApi`.
   The current Bulletin runtime is solo-chain-shaped, so this step is out of
   scope for the PoC. A cumulus test runtime with the documented stub impl
   from `sp_hop` is the most practical fixture.

## Findings

- The composition works. Compile passes, the binary is ~50 LOC of glue, HOP
  CLI flags inherit cleanly, `--version` reflects Bulletin identity.
- It does not by itself address @alindima's concern. `sp_hop::HopRuntimeApi`
  is still a mandatory `NodeRuntimeApi` supertrait inside the lib (see
  `cumulus/polkadot-omni-node/lib/src/common/mod.rs`), so every runtime built
  against the lib must provide a stub impl. That is exactly the "code smell"
  flagged in the PR thread.
- Closing that gap needs a small lib refactor: split `NodeRuntimeApi` into a
  base trait plus a `HopNodeRuntimeApi` extension, and gate the HOP wiring in
  `common::spec` and `common::rpc` on the extension. Bulletin opts in,
  everyone else stays unaware of HOP.

## Next steps

If this direction is approved, the contents of this crate move to
`paritytech/polkadot-bulletin-chain` and the supertrait split lands as a
separate PR against `polkadot-sdk`.
