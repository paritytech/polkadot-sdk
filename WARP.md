# WARP.md

This file provides guidance to WARP (warp.dev) when working with code in this repository.

## Project Overview

The **Polkadot SDK** is a comprehensive framework for building blockchains and parachains. This mono-repository contains three main components:
- **Substrate**: Core blockchain framework
- **Polkadot**: Relay chain implementation
- **Cumulus**: Parachain development framework

## Quick Start Development Commands

### Environment Setup
```bash
# Use the automated setup script (recommended for new users)
curl --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/paritytech/polkadot-sdk/master/scripts/getting-started.sh | bash

# Or manually install dependencies
rustup default stable
rustup update
rustup target add wasm32-unknown-unknown
rustup component add rust-src
```

### Building
```bash
# Build the main project (release mode recommended for testing)
cargo build --release

# Build specific binaries
cargo build --release --bin polkadot
cargo build --release --bin polkadot-parachain
cargo build --release --bin polkadot-omni-node

# Quick development build (debug mode)
cargo build
```

### Testing
```bash
# Run all tests (uses special testnet profile)
cargo test --workspace --profile testnet

# Run tests for a specific crate
cargo test -p <crate-name> --profile testnet

# Run a single test
cargo test --package <package-name> --test <test-name> <specific_test>

# Run benchmarks
cargo bench
```

### Code Quality
```bash
# Format code (uses project-specific .rustfmt.toml)
cargo fmt --all

# Lint code (CI enforces this)
cargo clippy --all-targets --locked --workspace --quiet
cargo clippy --all-targets --all-features --locked --workspace --quiet

# Check try-runtime features
cargo check --locked --all --features try-runtime --quiet

# Update UI tests when needed
./scripts/update-ui-tests.sh
```

### Running Nodes
```bash
# Run Polkadot development node
cargo run --bin polkadot -- --dev

# Run with detailed logging
RUST_LOG=debug RUST_BACKTRACE=1 cargo run --bin polkadot -- --dev

# Connect to different networks
./target/release/polkadot --chain=polkadot     # Mainnet
./target/release/polkadot --chain=kusama      # Kusama canary
./target/release/polkadot --chain=westend     # Testnet
```

### Templates and Scaffolding
```bash
# Available templates in templates/ directory:
# - minimal: Basic Substrate runtime
# - parachain: Parachain template  
# - solochain: Standalone blockchain template
# - zombienet: Network testing configuration

# Generate new template (automated in getting-started script)
git clone https://github.com/paritytech/polkadot-sdk-<template>-template.git
```

## Architecture Overview

### Directory Structure
- **`substrate/`**: Core blockchain framework
  - `substrate/frame/`: Runtime development framework (FRAME)
  - `substrate/client/`: Node implementation components
  - `substrate/primitives/`: Core types and traits
  - `substrate/bin/node/`: Reference node implementation
- **`polkadot/`**: Relay chain implementation
  - `polkadot/runtime/`: Polkadot relay chain runtimes
  - `polkadot/node/`: Node-side parachain validation logic
  - `polkadot/xcm/`: Cross-Consensus Messaging format
- **`cumulus/`**: Parachain development framework
  - `cumulus/client/`: Parachain node components
  - `cumulus/pallets/`: Parachain-specific FRAME pallets
  - `cumulus/parachains/`: System parachains (Asset Hub, Bridge Hub, etc.)
- **`bridges/`**: Blockchain bridging infrastructure
- **`templates/`**: Project templates for quick starts
- **`docs/sdk/`**: Comprehensive SDK documentation

### Core Concepts

**Runtime vs Node**: Substrate separates blockchain logic into:
- **Runtime**: State transition function (the "business logic"), built with FRAME
- **Node**: Infrastructure (networking, consensus, RPC), built with Substrate

**FRAME**: Framework for Runtime Aggregation of Modularized Entities
- Pallets are modular components that define specific functionality
- Runtimes compose multiple pallets together
- Provides macros and utilities for blockchain development

**Parachains**: Specialized blockchains that connect to Polkadot
- Benefit from Polkadot's shared security
- Communicate via XCM (Cross-Consensus Messaging)
- Built using Cumulus on top of Substrate

### Key Components

1. **Substrate**: Modular blockchain framework
   - Consensus algorithms (BABE, GRANDPA, Aura)
   - P2P networking and database layers  
   - Transaction pool and RPC systems

2. **FRAME**: Runtime development framework
   - System pallets (Balances, System, Timestamp)
   - Governance pallets (Democracy, Council, Treasury)
   - Consensus pallets (Babe, Grandpa, Im-Online)

3. **Polkadot**: Multi-chain platform
   - Relay chain coordinates parachains
   - Shared security model
   - Cross-chain communication via XCM

4. **Cumulus**: Parachain framework
   - Connects Substrate chains to Polkadot
   - Parachain consensus integration
   - Collator node implementation

## Development Workflows

### Working with Parachains
```bash
# Build parachain collator
cargo build --release -p polkadot-parachain-bin

# Export parachain genesis
./target/release/polkadot-parachain export-genesis-state > genesis-state
./target/release/polkadot-parachain export-genesis-wasm > genesis-wasm

# Run collator (example)
./target/release/polkadot-parachain --collator --alice --force-authoring \
  --tmp --port 40335 --rpc-port 9946 -- --chain rococo-local.json --port 30335
```

### Testing Networks
```bash
# Use Zombienet for integration testing (recommended)
# Install: https://github.com/paritytech/zombienet#requirements-by-provider
zombienet --provider native spawn ./zombienet/examples/small_network.toml

# Manual relay chain setup
./target/release/polkadot build-spec --chain rococo-local --disable-default-bootnode --raw > rococo-local.json
./target/release/polkadot --chain rococo-local.json --alice --tmp
```

### Benchmarking
```bash
# Run runtime benchmarks
cargo run --release --bin polkadot -- benchmark pallet \
  --chain dev --pallet pallet_balances --extrinsic "*" --steps 50 --repeat 20

# Use frame-omni-bencher for more sophisticated benchmarking
frame-omni-bencher --runtime <runtime-name>
```

## Important Notes

### Cargo Workspace
This is a massive Cargo workspace with 500+ crates. Key workspace features:
- Shared dependencies defined in root `Cargo.toml`
- Default members include main binaries: `polkadot`, `polkadot-parachain`, `substrate-node`
- Build profiles optimized for different use cases (testnet profile for testing)

### Build System
- Uses Rust standard `cargo` build system
- WASM compilation required for runtimes (substrate-wasm-builder)
- Forklift utility used in CI for optimized builds
- No Make, CMake, or other build systems

### Testing Strategy
- Unit tests embedded in individual crates  
- Integration tests using Zombienet
- UI tests for proc-macro outputs
- Benchmarking tests for runtime performance
- Special `testnet` profile for faster test compilation

### Code Standards
- Rustfmt configuration in `.rustfmt.toml` (hard tabs, 100 char width)
- Clippy enforced in CI with zero warnings tolerance
- try-runtime feature checking required
- Documentation requirements for public APIs

## Common Issues & Solutions

### Build Issues
- Ensure WASM target: `rustup target add wasm32-unknown-unknown`
- Use `cargo clean` for mysterious build failures
- Check disk space (builds are large)
- Use `--release` flag for actual testing

### Runtime Development
- Always implement both `Config` trait and pallet integration
- Use `#[pallet::weight]` annotations for all extrinsics  
- Test with different runtime configurations
- Consider storage migration for upgrades

### Parachain Development  
- Understand the relay-parachain communication model
- Test parachain registration process
- Consider XCM integration for cross-chain functionality
- Use proper collator configurations

## External Resources

- [Polkadot SDK Documentation](https://docs.polkadot.com)
- [Substrate Documentation](https://docs.substrate.io) 
- [Polkadot Wiki](https://wiki.polkadot.network/)
- [Substrate Stack Exchange](https://substrate.stackexchange.com/)
- [SDK API Documentation](https://paritytech.github.io/polkadot-sdk/)

This repository represents one of the most sophisticated blockchain development frameworks available, with deep architectural considerations around modularity, upgradeability, and interoperability.
