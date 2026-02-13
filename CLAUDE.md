# CLAUDE.md

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

## Optimizing Zombienet Test Build Times

Building all binaries for zombienet tests can take 30-60 minutes. Here are optimizations to reduce this to 10-15 minutes (first build) or 2-5 minutes (subsequent runs).

### Quick Start - Two-Step Workflow

**Step 1: Initial build (run once, ~10-15 minutes)**
```bash
./scripts/zombienet-quick-build.sh
```

**Step 2: Run tests with fast iteration (~2-5 minutes per run)**
```bash
./scripts/zombienet-dev-test.sh cumulus          # Run cumulus tests
./scripts/zombienet-dev-test.sh substrate        # Run substrate tests
./scripts/zombienet-dev-test.sh polkadot         # Run polkadot tests
```

The dev test script automatically sets `SKIP_WASM_BUILD=1` to skip WASM rebuilds between test runs.

### What Gets Built

The quick-build script builds:
- `polkadot` with **only rococo runtime** (skips westend, saves ~5 minutes)
- `polkadot-parachain` (still needs all 10 runtimes - ~20-30 min)
- `test-parachain` (9 WASM variants for elastic scaling tests)

**Total savings: ~5 minutes from selective relay chain runtime build**

### Manual Selective Builds

Build polkadot with only the runtime you need (for local development):
```bash
# For rococo-only tests (most common - saves ~5 min):
cargo build --profile testnet --no-default-features --features rococo-native,fast-runtime \
  --bin polkadot --bin polkadot-prepare-worker --bin polkadot-execute-worker

# For westend-only tests:
cargo build --profile testnet --no-default-features --features westend-native,fast-runtime \
  --bin polkadot --bin polkadot-prepare-worker --bin polkadot-execute-worker

# For both runtimes (default - needed for full test suite):
cargo build --profile testnet --features fast-runtime \
  --bin polkadot --bin polkadot-prepare-worker --bin polkadot-execute-worker
```

**Note:** Some tests use westend (e.g., 0004-coretime-smoke-test), so selective builds
are for local development iteration only. Run full build before running complete test suite.

### Skip WASM Rebuilds for Fast Iteration

After initial build, skip WASM rebuilds if you haven't changed runtime code:
```bash
# Method 1: Use the dev test script (recommended)
./scripts/zombienet-dev-test.sh cumulus test_name

# Method 2: Manual with SKIP_WASM_BUILD
SKIP_WASM_BUILD=1 cargo test -p cumulus-zombienet-sdk-tests --features zombie-ci -- test_name

# Method 3: Using the run.sh scripts
SKIP_WASM_BUILD=1 ./cumulus/zombienet/zombienet-sdk/run.sh test_name
```

### Available Runtime Features

The `polkadot` binary supports these features:
- `rococo-native` - Rococo relay chain runtime (used by 25+ tests)
- `westend-native` - Westend relay chain runtime (used by ~5 tests)
- Default: Both runtimes enabled

Most zombienet tests use `rococo-local`, so building only rococo saves significant time.

### Understanding Build Times

**Full build (30-60 minutes):**
- polkadot + 2 runtimes: ~10-15 min
- polkadot-parachain + 10 runtimes: ~20-30 min
- test-parachain + 9 WASM variants: ~5-10 min

**Optimized build (10-15 minutes):**
- polkadot + rococo only: ~5-8 min (saved ~5 min!)
- polkadot-parachain + 10 runtimes: ~20-30 min (future: omninode migration)
- test-parachain + 9 variants: ~5-10 min (needed for tests)

**Fast iteration (2-5 minutes):**
- SKIP_WASM_BUILD=1: Only recompiles changed Rust code
- No WASM builds, no runtime builds
- Perfect for test-only changes or node logic changes

### Future Optimizations (In Progress)

#### Omninode Migration (Phase 2 - In Progress)

The `polkadot-omni-node` is a universal parachain binary that loads runtimes from chain spec files
instead of embedding them at compile time. This eliminates the need to build `polkadot-parachain`
which embeds 10+ runtimes (~20-30 min build time).

**Using omninode for zombienet tests:**

1. **Build omninode (once)**:
```bash
cargo build --release -p polkadot-omni-node
```

2. **Generate chain specs** (see `scripts/generate-glutton-chain-specs.sh` for example):
```bash
# Build the runtime
cargo build --release -p glutton-westend-runtime

# Generate chain spec with custom configuration
chain-spec-builder create \
  --relay-chain "rococo-local" \
  --para-id 2000 \
  -r target/release/wbuild/glutton-westend-runtime/glutton_westend_runtime.compact.compressed.wasm \
  patch glutton-config.json \
  > glutton-westend-local-2000-spec.json
```

3. **Use in zombienet config**:
```toml
[[parachains]]
id = 2000
chain_spec_path = "./zombienet-chain-specs/glutton-westend-local-2000-spec.json"

[parachains.collator]
command = "polkadot-omni-node"  # Instead of polkadot-parachain
```

**Example migration**: See `polkadot/zombienet_tests/functional/0013-systematic-chunk-recovery-omninode.toml`
for a complete example of migrating a glutton test to use omninode.

**Compatible tests** (can use omninode):
- Tests using glutton runtime (0013, 0014, 0015, 0019)
- Tests using asset-hub, bridge-hub, coretime, people runtimes
- Any test using real parachain runtimes with GenesisBuilder support

**Incompatible tests** (cannot use omninode):
- Tests using test-parachain (needs custom CLI flags like `--fail-pov-recovery`)
- Tests using undying/adder collators (custom genesis generation)

#### Other Planned Optimizations

- **Feature-gate polkadot-parachain runtimes**: Build only needed runtimes
- **Metadata-only testing**: Use pre-built metadata files for subxt tests

## UI Tests

UI tests verify macro output. Update them with:
```bash
./scripts/update-ui-tests.sh
# Or for a specific Rust version:
./scripts/update-ui-tests.sh 1.70
```
