# XCM Weights Refactor Walkthrough

This document explains the refactoring used to reduce manual XCM weight glue code by moving the reusable helper logic into `pallet-xcm-benchmarks`.

## Goal

Reduce per-runtime handwritten logic in XCM `weights/xcm/mod.rs` by extracting reusable helper logic into `pallet-xcm-benchmarks`, where it can be shared by relay runtimes today and parachain runtimes later.

This remains an incremental step:
- It does not yet auto-generate `mod.rs`.
- It preserves current Rococo, Westend, and rc runtime behavior.
- It places the reusable logic next to the XCM benchmark infrastructure rather than in a relay-runtime-specific common crate.

## Why Move It Out Of `runtime/common`

`polkadot/runtime/common` works for relay-chain reuse, but it is the wrong abstraction boundary for broader XCM runtime reuse:

- The helper logic is about benchmarking-oriented XCM weight composition, not relay runtime policy.
- Asset Hub and other Cumulus runtimes should be able to adopt the same helpers without depending on relay runtime common code.
- `pallet-xcm-benchmarks` is already the natural home for the generated XCM benchmark weight building blocks.

Putting the helper logic there makes the layering clearer:

- benchmark primitives and reusable XCM weight helpers live in `pallet-xcm-benchmarks`
- each runtime keeps only its local asset-classification and instruction-policy mapping

## Scope Of This Change

Files changed:
- `polkadot/xcm/pallet-xcm-benchmarks/src/lib.rs`
- `polkadot/xcm/pallet-xcm-benchmarks/src/xcm_weights.rs` (new)
- `polkadot/runtime/rococo/src/weights/xcm/mod.rs`
- `polkadot/runtime/westend/src/weights/xcm/mod.rs`
- `substrate/frame/staking-async/runtimes/rc/src/weights/xcm/mod.rs`
- `polkadot/runtime/rococo/Cargo.toml`
- `polkadot/runtime/westend/Cargo.toml`
- `substrate/frame/staking-async/runtimes/rc/Cargo.toml`

Removed:
- `polkadot/runtime/common/src/xcm_weights.rs`

## Step 1: Expose A Shared XCM Weight Helper Module In `pallet-xcm-benchmarks`

### What was done

Added a new public module export in `pallet-xcm-benchmarks`:

- In `polkadot/xcm/pallet-xcm-benchmarks/src/lib.rs`, added:
  - `pub mod xcm_weights;`

### Why

The helper logic is tightly coupled to how benchmark-generated XCM weights are composed. Keeping it in the same crate as the benchmark primitives makes it reusable for any runtime that consumes those benchmarks.

## Step 2: Move Shared Helper Primitives

### What was done

Created:

- `polkadot/xcm/pallet-xcm-benchmarks/src/xcm_weights.rs`

This file contains the reusable helper surface:

1. `AssetTypes`
- `Balances`
- `Unknown`

2. `AssetMatcher`
- `fn classify(asset: &Asset) -> AssetTypes`
- `fn max_assets() -> u64`

3. `WeighAssets`
- shared trait for applying runtime asset policy to `Assets` and `AssetFilter`

4. `weigh_assets_list`
- shared logic for `Assets`

5. `weigh_assets_filter`
- shared logic for `AssetFilter`

6. `weigh_initiate_transfer`
- shared logic for combining `remote_fees` and transfer asset filters

7. `weigh_hints`
- shared logic for `SetHints`

### Why

These are generic XCM weight-composition algorithms, not relay-specific runtime helpers. By moving them into `pallet-xcm-benchmarks`, they become available to relay runtimes and future Cumulus runtimes through the same crate boundary.

## Step 3: Repoint Relay Runtime Imports

### What was done

Updated these runtime modules:

- `polkadot/runtime/rococo/src/weights/xcm/mod.rs`
- `polkadot/runtime/westend/src/weights/xcm/mod.rs`
- `substrate/frame/staking-async/runtimes/rc/src/weights/xcm/mod.rs`

Each now imports from:

- `pallet_xcm_benchmarks::xcm_weights`

instead of:

- `polkadot_runtime_common::xcm_weights`

### Why

This preserves the current runtime-specific policy structure while moving the reusable mechanics to the benchmark crate.

## Step 4: Normalize Runtime Dependency Wiring

### What was done

For the runtimes that now directly import `pallet-xcm-benchmarks`, changed the dependency from optional to normal:

- `polkadot/runtime/rococo/Cargo.toml`
- `polkadot/runtime/westend/Cargo.toml`
- `substrate/frame/staking-async/runtimes/rc/Cargo.toml`

Also updated their `std` feature wiring from:

- `pallet-xcm-benchmarks?/std`

to:

- `pallet-xcm-benchmarks/std`

### Why

Once the helpers are imported in normal runtime code, the crate can no longer remain benchmark-only and optional for those runtimes.

## Step 5: Remove The Old Relay-Common Copy

### What was done

Removed:

- `polkadot/runtime/common/src/xcm_weights.rs`

### Why

Keeping the old copy would make the new crate boundary redundant and create two competing homes for the same helper logic.

## Behavior Compatibility Notes

This refactor is intended to be behavior-preserving for the runtimes already migrated:

1. `Balances` vs `Unknown` asset handling is unchanged.
2. `MAX_ASSETS` cap logic is unchanged.
3. `initiate_transfer` total calculation remains saturating and additive over the same elements.
4. `set_hints` still only counts `AssetClaimer` the same way.
5. Instruction-level mapping in each runtime `impl XcmWeightInfo` is unchanged except for the helper import path.

## Why This Is A Better Home For Future Migrations

This layout is more useful for Asset Hub and other Cumulus runtimes because:

- they can adopt the helper crate without depending on relay `runtime/common`
- the helper APIs now sit next to the benchmark inputs they are meant to compose
- future refactors can target a single reusable crate for both relay and parachain XCM weight wrappers

## What This Enables Next

Follow-up work can now proceed on a better boundary:

1. Migrate Asset Hub and other Cumulus XCM wrappers onto the same helper module where appropriate.
2. Extend the helper APIs for richer asset models beyond the simple relay-chain `Balances`/`Unknown` split.
3. Introduce optional override hooks for runtimes with more specialized asset policies.
4. Generate runtime wrapper skeletons around benchmark-produced weights.
5. Add broader validation for helper adoption across relay and parachain runtimes.

## Summary

The main improvement in this step is not just extraction, but extraction to the correct crate boundary. The reusable XCM weight helper logic now lives in `pallet-xcm-benchmarks`, which better matches both its purpose and its intended future consumers. Rococo, Westend, and rc keep their local policy definitions, while the common mechanics now live where relay and parachain runtimes can both build on them.