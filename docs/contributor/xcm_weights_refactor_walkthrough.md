# XCM Weights Refactor Walkthrough

This document explains the current XCM weight refactor status and the runtime behavior model now in place.

The reusable XCM weight composition helpers now live with the benchmark infrastructure, and both relay and Asset Hub runtimes use that shared surface. The important point is that they share infrastructure but not identical asset semantics.

## Goal

Reduce repeated handwritten XCM weight wiring by centralizing shared helper logic in the benchmark layer while preserving runtime-specific policy where behavior genuinely differs.

Current status:
[x] Shared helper infrastructure is centralized (inside xcm-benchmarks)
[ ] Runtime-level instruction mapping is still explicit, but asset-weighing policy is now factored and reusable.

## Why This Lives In The Benchmark Layer

This logic is benchmark-composition logic, not relay-chain-only policy logic. Placing it beside XCM benchmarks gives one reusable source for all runtimes that consume benchmark weight outputs.

This separation keeps responsibilities clear:
- Shared mechanics: benchmark crate helpers.
- Runtime policy: local matcher/count/per-asset override choices.
- Runtime exceptions: explicit overrides or unsupported instructions.

## Implemented Shared Helper Surface

The shared helper layer now exposes two distinct asset-weighing models, plus common instruction combinators.

### 1. Relay-style classification model

Relay-style helpers classify assets into Known (Balances) vs Unknown classes and then price by class.

Key properties:
- Asset identity is part of policy.
- Known class uses benchmark-derived weight.
- Unknown class escalates to `Weight::MAX`.
- Wild filters are bounded via configured max-asset caps.

This matches relay-chain behavior where accepted assets are intentionally narrow.

### 2. Asset Hub style count-based model

Asset Hub style helpers treat assets as broadly supported and scale cost primarily by count, not by class rejection.

Key properties:
- No known/unknown hard split in normal path.
- `AssetFilter` and `Assets` weighing scales with bounded counts.
- Per-asset hook allows special pricing when an asset class has a cost model that differs from the runtime's default benchmark unit cost.
- Default behavior remains uniform per-asset cost when no special case is configured.

This matches Asset Hub semantics where many asset types are valid and pricing is dominated by cardinality and selected special policies.

When special pricing is needed:
- the asset executes through a different metering domain than the runtime default (for example, external gas-derived pricing)
- the asset path has materially different execution characteristics that would be mispriced by a single flat per-asset benchmark weight
- the runtime intentionally caps or normalizes a class to a fixed charge for safety or policy reasons

### Asset Hub Westend ERC20 Special Case

Asset Hub Westend includes a concrete per-asset override for ERC20-style assets. In that case, the runtime uses an ERC20 transfer gas-limit-derived charge instead of the default fungible benchmark unit weight.

Why this matters:
- it prevents undercharging or overcharging when ERC20 handling cost does not track the default Substrate benchmark unit
- it keeps the general count-based model while allowing targeted policy-correct pricing for a known special class

All non-ERC20 assets in that path continue to use the default per-asset benchmark-based weight.

### 3. Shared instruction combinators

Common helper functions are used by both models for recurring instruction patterns:
- transfer initiation logic that combines remote fee filters and transfer filters with saturating addition
- hint processing logic that applies bounded additive charging for supported hint variants

## Relay Runtime Interpretation Model

Relay runtimes interpret asset-bearing XCM instructions through classification:
- If an instruction references recognized local asset classes, benchmark weights are applied.
- If an instruction references unsupported classes, weighing can escalate to `Weight::MAX`.
- Filter/list operations therefore encode both workload and support policy in one path.

In short: relay runtimes treat asset support as selective, and that selectivity is reflected directly in weighting behavior.

## Asset Hub Runtime Interpretation Model

Asset Hub runtimes interpret asset-bearing instructions through count-based support with optional per-asset adjustment:
- Asset types are generally considered supported.
- Weight scales with how many assets may be touched.
- Runtime-specific hooks can override per-asset cost for special classes (for example, assets whose cost model follows external gas semantics).
- Unsupported instruction families are still explicitly marked as unsupported where applicable.

In short: Asset Hub runtimes treat support as broad, then encode cost through count bounds plus targeted overrides.

## What Is Automated Today vs What Remains Manual

Automated/shared today:
- core asset weighing algorithms for both relay and Asset Hub semantics
- common transfer/hint composition helpers
- reusable benchmark-composition primitives available to all participating runtimes

Still manual today:
- full per-instruction `XcmWeightInfo` method mapping in each runtime wrapper
- explicit runtime exceptions and unsupported instruction declarations

This is an intentional intermediate state: shared policy mechanics are centralized first, and wrapper boilerplate removal can build on that stable base.

## Behavior Compatibility Notes

Current behavior is intended to stay equivalent to prior runtime semantics:
- relay runtimes keep classification-driven support behavior
- Asset Hub runtimes keep count-driven behavior and per-asset override behavior
- transfer and hint charging remains saturating and additive under the same policy inputs
- unsupported instruction families still resolve to `Weight::MAX` where defined by runtime policy

## Why This Refactor Matters

The major gain is architectural correctness and reuse:
- one shared home for XCM benchmark composition logic
- clear separation between shared mechanics and runtime policy
- a practical foundation for the next step: reducing large runtime wrapper boilerplate while keeping explicit override control

## Next Improvement Direction

The natural follow-up is to introduce a higher-level reusable wrapper that auto-maps common fungible and generic instruction paths, so runtime modules mostly declare:
- asset policy (classification or count/per-asset hooks)
- explicit overrides
- unsupported/default behaviors

That would preserve policy flexibility while removing most repetitive method glue.

## Using This Model Outside SDK Runtimes

External chains are independent runtimes, so they do not automatically inherit these weight implementations. They have two practical options:
- copy the same modeling pattern (relay-style classification or Asset Hub-style count/per-asset hooks) and wire it to their own benchmark outputs
- depend on the same benchmark/helper crate surface and provide local runtime policy types (matcher/count bounds/per-asset overrides/unsupported rules)

In both cases, the key principle is the same: benchmark outputs are runtime-local, and the helper model is reusable. So non-SDK chains can use the same architecture, but they still define their own policy and regenerate weights for their own runtime configuration.
