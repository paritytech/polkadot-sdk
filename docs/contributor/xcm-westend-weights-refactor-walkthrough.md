# Westend And Rococo XCM Weights Refactor Walkthrough

This document explains every step taken to start reducing manual XCM weight glue code in both Westend and Rococo, while keeping runtime behavior unchanged.

## Goal

Reduce the amount of per-runtime handwritten logic in XCM `weights/xcm/mod.rs` by extracting reusable logic into `polkadot-runtime-common`.

This is intentionally a first step:
- It does not yet auto-generate `mod.rs`.
- It preserves current Westend and Rococo semantics.
- It introduces reusable primitives for follow-up migrations.

## Scope Of This Change

Files changed:
- `polkadot/runtime/common/src/lib.rs`
- `polkadot/runtime/common/src/xcm_weights.rs` (new)
- `polkadot/runtime/westend/src/weights/xcm/mod.rs`
- `polkadot/runtime/rococo/src/weights/xcm/mod.rs`

## Step 1: Expose A Shared XCM Weight Helper Module

### What was done

Added a new public module export in `polkadot-runtime-common`:

- In `polkadot/runtime/common/src/lib.rs`, added:
  - `pub mod xcm_weights;`

### Why

Westend and Rococo currently duplicate almost identical XCM weight helper code. Runtime-common is already used by both relay runtimes and is the correct shared location for reusable runtime logic.

## Step 2: Create Shared Helper Primitives

### What was done

Created a new file:

- `polkadot/runtime/common/src/xcm_weights.rs`

This file adds reusable helpers:

1. `AssetTypes` enum
- `Balances`
- `Unknown`

2. `AssetMatcher` trait
- `fn classify(asset: &Asset) -> AssetTypes`
- `fn max_assets() -> u64`

3. `weigh_assets_list`
- Shared logic for `Assets` to sum balances/unknown per-asset weights.

4. `weigh_assets_filter`
- Shared logic for `AssetFilter` (`Definite`, `AllOf`, `AllCounted`, `All`, etc.).

5. `weigh_initiate_transfer`
- Shared logic for combining `remote_fees` and `assets` in `initiate_transfer`.

6. `weigh_hints`
- Shared logic for summing `set_hints` weights (currently `AssetClaimer`).

### Why

These exact algorithmic patterns are repeated across runtimes. Moving them to runtime-common means each runtime now mainly defines policy and mapping, not full boilerplate loops and match trees.

## Step 3: Refactor Westend To Use Shared Helpers

### What was done

Updated:

- `polkadot/runtime/westend/src/weights/xcm/mod.rs`

#### 3.1 Added shared helper imports

Imported from `polkadot_runtime_common::xcm_weights`:
- `AssetTypes`
- `AssetMatcher`
- `weigh_assets_filter`
- `weigh_assets_list`
- `weigh_initiate_transfer`
- `weigh_hints`

#### 3.2 Replaced local asset classification enum with a matcher policy

Removed:
- `AssetTypes` enum
- `impl From<&Asset> for AssetTypes`

Added:
- `WestendAssetMatcher` implementing `AssetMatcher`.

Policy remains the same:
- `Here` asset is `Balances`
- all others are `Unknown`
- `max_assets()` returns existing `MAX_ASSETS` (`1`)

#### 3.3 Replaced local `AssetFilter` and `Assets` implementations with shared logic

`impl WeighAssets for AssetFilter` now delegates to:
- `weigh_assets_filter::<WestendAssetMatcher>(...)`

`impl WeighAssets for Assets` now delegates to:
- `weigh_assets_list::<WestendAssetMatcher>(...)`

#### 3.4 Replaced local `initiate_transfer` accumulation loop

Delegated to:
- `weigh_initiate_transfer(...)`

The supplied closure keeps the same local policy:
- `|asset_filter, weight| asset_filter.weigh_assets(weight)`

#### 3.5 Replaced local `set_hints` loop

Delegated to:
- `weigh_hints(hints, XcmGeneric::<Runtime>::asset_claimer())`

### Why

This keeps Westend-specific policy local, but removes repetitive mechanics.

## Step 4: Refactor Rococo To Use The Same Shared Helpers

### What was done

Updated:

- `polkadot/runtime/rococo/src/weights/xcm/mod.rs`

#### 4.1 Added shared helper imports

Imported from `polkadot_runtime_common::xcm_weights`:
- `AssetTypes`
- `AssetMatcher`
- `weigh_assets_filter`
- `weigh_assets_list`
- `weigh_initiate_transfer`
- `weigh_hints`

#### 4.2 Replaced local asset classification enum with a matcher policy

Removed:
- `AssetTypes` enum
- `impl From<&Asset> for AssetTypes`

Added:
- `RococoAssetMatcher` implementing `AssetMatcher`

Policy remains the same:
- `Here` asset is `Balances`
- all others are `Unknown`
- `max_assets()` returns existing `MAX_ASSETS` (`1`)

#### 4.3 Replaced local `AssetFilter` and `Assets` implementations with shared logic

`impl WeighAssets for AssetFilter` now delegates to:
- `weigh_assets_filter::<RococoAssetMatcher>(...)`

`impl WeighAssets for Assets` now delegates to:
- `weigh_assets_list::<RococoAssetMatcher>(...)`

#### 4.4 Replaced local `initiate_transfer` accumulation loop

Delegated to:
- `weigh_initiate_transfer(...)`

The supplied closure keeps the same local policy:
- `|asset_filter, weight| asset_filter.weigh_assets(weight)`

#### 4.5 Replaced local `set_hints` loop

Delegated to:
- `weigh_hints(hints, XcmGeneric::<Runtime>::asset_claimer())`

### Why

Rococo had the same hand-written loop patterns as Westend. Moving it onto the same shared helpers confirms that the extracted code is reusable across relay runtimes without changing their instruction policy.

## Behavior Compatibility Notes

This refactor is designed to be behavior-preserving for Westend and Rococo:

1. `Balances` vs `Unknown` asset handling is unchanged.
2. `MAX_ASSETS` cap logic is unchanged.
3. `initiate_transfer` total calculation remains saturating and additive over the same elements.
4. `set_hints` still only counts `AssetClaimer` the same way.
5. Instruction-level mapping in each `impl XcmWeightInfo` is unchanged except for delegating internal loops.

## Why This Is A Good Starting Point

This delivers immediate wins without changing benchmark data flow:

- Benchmark-generated files remain untouched.
- Each runtime still owns policy decisions.
- Common algorithms now live in one place.
- Future runtime migrations can be incremental.

## What This Enables Next

Follow-up work can now proceed with smaller deltas:

1. Migrate the remaining XCM weight wrappers that still use the old hand-written pattern.
2. Introduce optional override hooks in helper-based wrappers.
3. Add a generator for runtime wrapper skeletons around these helpers.
4. Add docs for custom policy points (unsupported instructions, asset models, exchange/alias behavior).
5. Add CI check that generated wrappers are up to date once generation is introduced.

## Summary

This first shared-helper migration does not remove all duplication, but it reduces mandatory handwritten boilerplate and isolates runtime-specific policy from repeated algorithmic code. Westend and Rococo now both use the same extracted helper flow, which is a concrete first step toward fuller automation.