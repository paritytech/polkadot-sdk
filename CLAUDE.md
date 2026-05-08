# CLAUDE.md

## Workflow Orchestration

### 1. Plan Mode Default
- Enter plan mode for ANY non-trivial task (3+ steps or architectural decisions)
- If something goes sideways, STOP and re-plan immediately — don't keep pushing
- Use plan mode for verification steps, not just building
- Write detailed specs upfront to reduce ambiguity

### 2. Subagent Strategy
- Use subagents liberally to keep main context window clean
- Offload research, exploration, and parallel analysis to subagents
- For complex problems, throw more compute at it via subagents
- One task per subagent for focused execution

### 3. Self-Improvement Loop
- After ANY correction from the user: update `tasks/lessons.md` with the pattern
- Write rules for yourself that prevent the same mistake
- Ruthlessly iterate on these lessons until mistake rate drops
- Review lessons at session start for relevant project

### 4. Verification Before Done
- Never mark a task complete without proving it works
- Diff behavior between main and your changes when relevant
- Ask yourself: "Would a staff engineer approve this?"
- Run tests, check logs, demonstrate correctness

### 5. Demand Elegance (Balanced)
- For non-trivial changes: pause and ask "is there a more elegant way?"
- If a fix feels hacky: "Knowing everything I know now, implement the elegant solution"
- Skip this for simple, obvious fixes — don't over-engineer
- Challenge your own work before presenting it

### 6. Autonomous Bug Fixing
- When given a bug report: just fix it. Don't ask for hand-holding
- Point at logs, errors, failing tests — then resolve them
- Zero context switching required from the user
- Fix failing CI tests without being told how

## Task Management

1. **Plan First**: Write plan to `tasks/todo.md` with checkable items
2. **Verify Plan**: Check in before starting implementation
3. **Track Progress**: Mark items complete as you go
4. **Explain Changes**: High-level summary at each step
5. **Document Results**: Add review section to `tasks/todo.md`
6. **Capture Lessons**: Update `tasks/lessons.md` after corrections

## Core Principles

- **Simplicity First**: Make every change as simple as possible. Impact minimal code.
- **No Laziness**: Find root causes. No temporary fixes. Senior developer standards.
- **Minimal Impact**: Changes should only touch what's necessary. Avoid introducing bugs.

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository Overview

The Polkadot SDK is a monorepo containing all components needed to build on the Polkadot network. It was formed by
merging three previously separate repositories:

- **Substrate** (`substrate/`): Core blockchain framework providing consensus, networking, storage, and runtime
  execution
- **Polkadot** (`polkadot/`): Relay chain implementation including the validator node
- **Cumulus** (`cumulus/`): SDK for building parachains that connect to Polkadot
- **Bridges** (`bridges/`): Cross-chain bridge infrastructure including Snowbridge (Ethereum bridge)

## Rust Toolchain

This repository is meant to be compiled with a stable Rust toolchain. A nightly toolchain is only required
to run `cargo +nightly fmt`. It should always compile with the latest Rust version. However, the CI will
use the version referenced in `.github/env`. Using features not available in that version will not pass CI.
Additionally, newer versions will likely result in warnings when compiling the repository.

The toolchain requires the `rust-src` component to build for the PolkaVM target.
We also need the `wasm32v1-none` target to compile the WASM runtimes.

## Build Commands

```bash
# Check or clippy the entire workspace
# This skips the time-intensive building of the WASM runtimes
SKIP_WASM_BUILD=1 cargo check --workspace --all-targets --all-features
SKIP_WASM_BUILD=1 cargo clippy --workspace --all-targets --all-features

# Build specific binary
cargo build -p polkadot --release
cargo build -p polkadot-parachain-bin --release

# Build specific runtime
cargo build -p kitchensink-runtime --release --features runtime-benchmarks
```

## Testing

```bash
# Run all tests (testnet is a release profile with debugging)
cargo test --workspace --profile testnet
```

## Formatting and Linting

```bash
# Format Rust code (requires nightly)
cargo +nightly fmt

# Format TOML files
taplo format --config .config/taplo.toml
```

## Architecture

### Runtime vs Node

The SDK separates **runtime** (on-chain logic, compiled to WASM) from **node** (off-chain client):
- Runtime code lives in `*/runtime/` directories and must be `no_std` compatible
- Node/client code lives in `*/client/` and `*/node/` directories

### FRAME Pallets

Pallets are modular runtime components in `substrate/frame/`. Each pallet:
- Has a `Config` trait for configuration
- May have storage items, dispatchables (extrinsics), events, and errors
- Uses macros from `frame_support` (`#[pallet::*]`)

### XCM (Cross-Consensus Messaging)

Located in `polkadot/xcm/`. XCM is the messaging format for cross-chain communication:
- `xcm/` - Core XCM types and versioning
- `xcm-builder/` - Configurable components for XCM execution
- `xcm-executor/` - XCM instruction executor
- `pallet-xcm/` - Runtime pallet for XCM

### Key Directories

- `substrate/primitives/` - Core types shared across the codebase
- `substrate/frame/support/` - FRAME macros and support code
- `polkadot/node/` - Polkadot validator node subsystems
- `cumulus/pallets/parachain-system/` - Core parachain runtime support
- `cumulus/parachains/runtimes/` - System parachain runtimes (Asset Hub, Bridge Hub, etc.)

## Code Style

- **Indentation**: Tabs (not spaces)
- **Line width**: 100 characters max
- **Panickers**: Avoid `unwrap()`; if used, add proof comment ending with `; qed`
- **Unsafe code**: Requires explicit safety justification

## PR Requirements

1. All PRs need a `prdoc` file unless labeled `R0-no-crate-publish-required`
2. Use `/cmd prdoc` in PR comments to generate prdoc (paritytech org members)
3. Use `/cmd fmt` to format code
4. Use `/cmd bench` for weight generation
5. Tag PRs with at least one `T*` label indicating the component changed

## Running Local Networks

```bash
# Using zombienet (recommended)
zombienet --provider native spawn ./zombienet/examples/small_network.toml

# Manual: Start relay chain
./target/release/polkadot --chain rococo-local --alice --tmp

# Manual: Start parachain collator
./target/release/polkadot-parachain --collator --alice --force-authoring --tmp
```

## UI Tests

UI tests verify macro output. Update them with:
```bash
./scripts/update-ui-tests.sh
# Or for a specific Rust version:
./scripts/update-ui-tests.sh 1.70
```
