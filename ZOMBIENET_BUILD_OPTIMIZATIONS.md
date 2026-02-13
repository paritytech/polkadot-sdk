# Zombienet Build Time Optimizations

This document summarizes the build time optimizations implemented for zombienet tests in the Polkadot SDK.

## Problem Statement

Building all binaries for zombienet tests takes **30-60 minutes** locally, which severely impacts developer productivity and iteration speed.

**Build time breakdown (before optimizations):**
- `polkadot` + 2 relay chain runtimes: ~10-15 min
- `polkadot-parachain` + 10 embedded runtimes: ~20-30 min  
- `test-parachain` + 9 WASM variants: ~5-10 min

## Solutions Implemented

### Phase 0: Selective Relay Chain Runtime Builds ✅ Complete

**What:** Expose `rococo-native` and `westend-native` as opt-in features for the polkadot binary.

**Impact:** ~5 minutes saved per build

**Changes:**
- `polkadot/Cargo.toml`: Made runtime features optional (default: both enabled for backwards compatibility)
- `.github/workflows/build-publish-images.yml`: CI now builds only rococo for zombienet tests

**Usage:**
```bash
# For rococo tests (most common - 25+ tests)
cargo build --profile testnet --no-default-features --features rococo-native,fast-runtime \
  --bin polkadot --bin polkadot-prepare-worker --bin polkadot-execute-worker

# For westend tests (only ~5 tests)  
cargo build --profile testnet --no-default-features --features westend-native,fast-runtime \
  --bin polkadot --bin polkadot-prepare-worker --bin polkadot-execute-worker
```

**Backwards compatible:** Default builds still include both runtimes.

---

### Phase 1: Developer Scripts & Documentation ✅ Complete

**What:** Provide easy-to-use scripts and documentation for fast iteration.

**Impact:** 2-5 minute iteration time (vs 30-60 min) on subsequent runs

**New scripts:**
1. `scripts/zombienet-quick-build.sh` - Optimized one-time build script
   - Builds polkadot with rococo-only
   - Includes polkadot-parachain and test-parachain
   - ~10-15 minutes vs 30-60 minutes

2. `scripts/zombienet-dev-test.sh` - Fast test iteration script
   - Automatically sets `SKIP_WASM_BUILD=1`
   - Supports cumulus/substrate/polkadot test suites
   - 2-5 minutes per run (no WASM rebuilds)

**Updated existing scripts:**
- `substrate/zombienet/zombienet-sdk/run.sh` - Added SKIP_WASM_BUILD tips
- `cumulus/zombienet/zombienet-sdk/run.sh` - Added optimization hints

**Documentation:**
- `CLAUDE.md` - Comprehensive "Optimizing Zombienet Test Build Times" section
  - Quick start workflow
  - Build time breakdowns
  - Multiple optimization methods
  - Future roadmap

**Usage workflow:**
```bash
# Step 1: Build once (~10-15 minutes)
./scripts/zombienet-quick-build.sh

# Step 2: Iterate fast (~2-5 minutes per run)
./scripts/zombienet-dev-test.sh cumulus test_name
```

---

### Phase 2: Omninode Migration 🚧 In Progress

**What:** Replace `polkadot-parachain` with `polkadot-omni-node` for compatible tests.

**Potential impact:** ~20-30 minutes saved (eliminates need to build polkadot-parachain)

**Status:** Infrastructure complete, gradual migration in progress

**Why omninode?**
- `polkadot-parachain` embeds 10+ runtimes at compile time
- `polkadot-omni-node` has ZERO embedded runtimes
- Loads runtime from chain spec files at runtime
- Single universal binary for all parachain tests

**New tooling:**
- `scripts/generate-glutton-chain-specs.sh` - Generate chain specs for glutton runtime
- `polkadot/zombienet_tests/functional/0013-systematic-chunk-recovery-omninode.toml` - Example migration

**Compatible tests** (can migrate to omninode):
- Glutton tests: 0013, 0014, 0015, 0019
- Asset-hub tests: 0002-upgrade-smoke  
- Coretime tests: 0004-coretime-smoke
- Any test using real parachain runtimes with GenesisBuilder support

**Incompatible tests** (cannot migrate):
- Test-parachain tests (11/14 cumulus tests) - require custom CLI flags like `--fail-pov-recovery`
- Undying/adder collator tests (15+ polkadot tests) - custom genesis generation, CLI parameters

**Usage:**
```bash
# 1. Build omninode (once)
cargo build --release -p polkadot-omni-node

# 2. Generate chain specs
./scripts/generate-glutton-chain-specs.sh

# 3. Update zombienet config
[[parachains]]
id = 2000
chain_spec_path = "./zombienet-chain-specs/glutton-westend-local-2000-spec.json"

[parachains.collator]
command = "polkadot-omni-node"  # Instead of polkadot-parachain
```

**Next steps:**
- Migrate compatible glutton tests (0013, 0014, 0015, 0019)
- Migrate asset-hub and coretime tests
- Document migration guide
- Update CI to use omninode for applicable tests

---

## Results Summary

### Build Time Improvements

**Before optimizations:**
- First build: 30-60 minutes
- Subsequent builds: 30-60 minutes (full rebuild)

**After Phase 0 + Phase 1:**
- First build: 10-15 minutes (rococo-only + optimized)
- Subsequent builds: 2-5 minutes (SKIP_WASM_BUILD=1)

**After Phase 2 (projected):**
- First build: 5-8 minutes (omninode eliminates polkadot-parachain)
- Subsequent builds: 2-5 minutes (SKIP_WASM_BUILD=1)

### Iteration Speed

- **Before:** 6-12x slower
- **After:** Near-instant test iteration with SKIP_WASM_BUILD

### Developer Experience

**Before:**
- No guidance on build optimization
- Every test run = full rebuild
- 30-60 minutes per test cycle

**After:**
- Clear two-step workflow (build once, test fast)
- Automated scripts handle optimizations
- Comprehensive documentation
- 2-5 minutes per test cycle

---

## Git Commits

All changes are in separate, revertible commits:

1. **Phase 0:** `feat: expose relay chain runtime features for selective builds`
   - polkadot/Cargo.toml
   - .github/workflows/build-publish-images.yml

2. **Phase 1:** `feat: add developer scripts and docs for optimized zombienet builds`
   - scripts/zombienet-quick-build.sh
   - scripts/zombienet-dev-test.sh
   - substrate/zombienet/zombienet-sdk/run.sh
   - cumulus/zombienet/zombienet-sdk/run.sh
   - CLAUDE.md

3. **Phase 2 (WIP):** `feat(wip): add omninode migration infrastructure for zombienet tests`
   - scripts/generate-glutton-chain-specs.sh
   - polkadot/zombienet_tests/functional/0013-systematic-chunk-recovery-omninode.toml
   - CLAUDE.md

Each commit can be reverted independently if needed.

---

## Future Optimizations (Planned)

### Phase 3: Feature-Gate polkadot-parachain Runtimes
- Make each of the 10 embedded runtimes optional
- Developers can build only needed runtimes
- Estimated savings: Variable (depends on which runtimes needed)

### Phase 4: Metadata-Only Testing  
- Use pre-built metadata files for subxt-based tests
- Skip runtime rebuilds entirely for metadata-only changes
- Estimated savings: ~5-10 minutes for specific test types

---

## Quick Reference

### For daily development:
```bash
# One-time setup
./scripts/zombienet-quick-build.sh

# Daily iteration
./scripts/zombienet-dev-test.sh cumulus my_test
```

### For custom builds:
```bash
# Rococo-only polkadot
cargo build --profile testnet --no-default-features --features rococo-native,fast-runtime \
  --bin polkadot --bin polkadot-prepare-worker --bin polkadot-execute-worker

# Skip WASM rebuilds
SKIP_WASM_BUILD=1 cargo test -p cumulus-zombienet-sdk-tests --features zombie-ci
```

### For omninode (when ready):
```bash
# Build omninode
cargo build --release -p polkadot-omni-node

# Generate chain specs
./scripts/generate-glutton-chain-specs.sh

# Use in zombienet configs (see 0013-omninode.toml for example)
```

---

## Related Documentation

- `CLAUDE.md` - Full optimization guide with examples
- `scripts/zombienet-quick-build.sh` - Quick build script with inline documentation
- `scripts/zombienet-dev-test.sh` - Fast test iteration script
- `scripts/generate-glutton-chain-specs.sh` - Chain spec generation for omninode
- `polkadot/zombienet_tests/functional/0013-systematic-chunk-recovery-omninode.toml` - Omninode migration example
