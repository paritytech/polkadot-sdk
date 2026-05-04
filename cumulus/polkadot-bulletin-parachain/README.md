# polkadot-bulletin-parachain (PoC)

A proof-of-concept custom parachain node binary for the Polkadot Bulletin Chain, built by
**composing `polkadot-omni-node-lib`** rather than reusing the generic `polkadot-omni-node`
binary.

This crate exists to answer one open question raised on
[paritytech/polkadot-sdk#11662](https://github.com/paritytech/polkadot-sdk/pull/11662):

> @alindima: *"It feels wrong to have this protocol part of the omni node binary, considering
> that the omni node should be a somewhat unopinionated way of running all substrate-based aura
> parachains… the actual wiring of the protocol should be done in the bulletin chain node
> implementation."*
>
> @bkontur (Branislav Kontur): *"Ok, we will do a quick prototype based on this `hop-base`
> branch — custom parachain binary reusing/configuring `polkadot-omni-node-lib`, and we will
> see if it will work and how extensible is `polkadot-omni-node-lib`. … And when this prototype
> works, we don't need to even merge it to the PolkadotSDK and move it to the Bulletin repo."*

This binary is built **on the `hop-base` branch** of `polkadot-sdk`, on a topic branch named
`ndk/bulletin-parachain-poc`. No version bumps, no SDK release dance — exactly the path Brani
described.

## What this PoC delivers

A new workspace member at `cumulus/polkadot-bulletin-parachain/` whose `main.rs` is a thin
wrapper around `polkadot_omni_node_lib::run_with_custom_cli`, mirroring the structure of
`cumulus/polkadot-omni-node/src/main.rs` but with a Bulletin-specific `CliConfig` (binary name,
support URL, copyright year).

```text
cumulus/polkadot-bulletin-parachain/
├── Cargo.toml      # depends on polkadot-omni-node-lib
├── build.rs        # standard substrate build-script
├── src/main.rs     # ~50 LOC wrapper
└── README.md       # this file
```

The crate is registered as a workspace member in the root `Cargo.toml` but **not** added to
`default-members`, so the existing top-level `cargo build` is unaffected.

## How to build and run

```bash
# from the repo root, on branch ndk/bulletin-parachain-poc

# fast compile-only check
SKIP_WASM_BUILD=1 cargo check -p polkadot-bulletin-parachain

# release build
cargo build -p polkadot-bulletin-parachain --release

# verify CLI surface (HOP flags should appear under the same group as in polkadot-omni-node)
./target/release/polkadot-bulletin-parachain --help | grep -i hop
```

## How to test

There are three layers of validation, in increasing fidelity:

1. **Compile gate** — `cargo check -p polkadot-bulletin-parachain` must pass on `hop-base`. This
   is the core extensibility signal: if it compiles, then the lib's public surface (`RunConfig`,
   `CliConfig`, `run_with_custom_cli`, `NoExtraSubcommand`) is sufficient to compose a custom
   binary, modulo the supertrait caveat below.

2. **CLI smoke test** — `--help` must list the HOP CLI group inherited from
   `polkadot_omni_node_lib::cli::Cli` (`--enable-hop`, `--hop-data-dir`, `--hop-retention-blocks`,
   etc.). If they don't appear, the lib isn't surfacing flattened CLI flags through the wrapper
   and that's a finding.

3. **Live dev-node smoke test** — start the binary in `--dev` mode pointing at a chain spec built
   from a runtime that implements `sp_hop::HopRuntimeApi`, submit a HOP blob via the RPC, and
   verify the data pool accepts it:

   ```bash
   # using the chain-spec-builder subcommand bundled into the binary
   ./target/release/polkadot-bulletin-parachain chain-spec-builder \
       create --runtime ./bulletin-runtime.compact.compressed.wasm default

   # run dev mode with HOP enabled
   ./target/release/polkadot-bulletin-parachain \
       --dev --tmp \
       --chain ./chain_spec.json \
       --enable-hop \
       --hop-data-dir /tmp/bulletin-hop \
       --rpc-port 9944

   # submit a blob (pseudocode — exact RPC method names live in sc-hop)
   websocat ws://127.0.0.1:9944
   ```

   The Bulletin runtime in `paritytech/polkadot-bulletin-chain` already has a HOP-promotion
   pallet, but it's currently a **solo chain** runtime, not a parachain runtime, so a real
   live test against the production Bulletin runtime is out of scope for this PoC. To
   actually start a node, today you would need either (a) a parachain-shaped Bulletin runtime
   (separate work), or (b) any cumulus parachain runtime that implements `HopRuntimeApi`
   (even as the panic-stub from `sp_hop` docs) — a minimal test fixture in
   `cumulus/test/runtime/` is the most practical option.

## Findings on lib extensibility

This is the part that matters for answering @alindima's concern.

### ✅ What works out of the box

- The lib's public surface (`run_with_custom_cli<CliConfig, ExtraSubcommand>(RunConfig)`) is
  sufficient to build a custom binary. The Bulletin binary's `main.rs` is ~50 LOC.
- Bulletin-specific identity (binary name, support URL, copyright) is fully customizable via
  the `CliConfig` trait.
- HOP CLI flags are inherited automatically from `polkadot_omni_node_lib::cli::Cli` because
  they are `#[command(flatten)]`'d into the parent CLI struct.
- HOP service wiring (data pool construction, `hop-maintenance` task spawn, RPC registration)
  is all driven by `NodeExtraArgs.hop` inside the lib, which the lib already gates on
  `--enable-hop`. So a Bulletin runtime that wants HOP gets it; one that doesn't, doesn't.

### ⚠️ What does not yet address @alindima's concern

The structural objection is that **`HopRuntimeApi` is a mandatory supertrait of
`NodeRuntimeApi`** (see `cumulus/polkadot-omni-node/lib/src/common/mod.rs`):

```rust
pub trait NodeRuntimeApi<Block: BlockT>:
    ApiExt<Block>
    + Metadata<Block>
    + ...
    + sp_hop::HopRuntimeApi<Block, AccountId32>   // <-- mandatory
    + Sized
{ }
```

This means **every runtime built against `polkadot-omni-node-lib`** — including every existing
Aura parachain that does not care about HOP — has to provide a stub `impl HopRuntimeApi` (the
canonical no-op stub is documented in `sp_hop`'s rustdoc and replicated in
`fake_runtime_api/utils.rs`). That is exactly the "code smell" alindima flagged.

**Building a Bulletin-specific binary on top of the lib does not, by itself, lift this
constraint.** The supertrait bound is in the lib, not in the binary. The Bulletin binary just
inherits it.

### What would actually solve it (next step, beyond this PoC)

To make HOP truly opt-in at the lib level, one of these refactors is needed in the lib:

1. **Split `NodeRuntimeApi` into a base trait + a `HopNodeRuntimeApi` extension trait.** The
   `nodes::aura::new_aura_node_spec` factory becomes generic over which trait the runtime
   satisfies; the HOP wiring in `common::spec` and `common::rpc` is gated on the extension
   trait. The Bulletin binary opts into the HOP-extended trait; everyone else uses the base
   trait and never sees HOP.

2. **Move the HOP wiring out of `polkadot-omni-node-lib` entirely** into either a separate
   `polkadot-omni-node-lib-hop` crate, or directly into the Bulletin binary's `main.rs`,
   exposing builder hooks (`Service::extend_rpc`, `Service::spawn_extra_task`, etc.) from the
   lib so the Bulletin binary can register HOP itself.

Option (1) is a smaller, more localized change and is probably the right next step. Option (2)
matches what alindima literally asked for ("the actual wiring of the protocol should be done
in the bulletin chain node implementation") but is a much bigger refactor of the lib's startup
plumbing.

### Recommended verdict for the discussion thread

> The composition pattern works — a Bulletin-specific binary built on top of
> `polkadot-omni-node-lib` is straightforward and small. **However**, the structural concern
> alindima raised (HOP being a mandatory `NodeRuntimeApi` supertrait that every omni-node
> consumer has to stub) is **not** resolved just by the binary swap. To fully address it, the
> lib needs the supertrait split described above, which we can do as a follow-up PR before the
> work moves to the Bulletin repo.

## Branch layout

- Base branch: `origin/hop-base` (PR #11662)
- This work: `ndk/bulletin-parachain-poc`
- Per @bkontur: if this PoC works out, the contents of `cumulus/polkadot-bulletin-parachain/`
  move to the `paritytech/polkadot-bulletin-chain` repo and the lib-side changes (the
  supertrait split) go into `polkadot-sdk` as a separate PR.
