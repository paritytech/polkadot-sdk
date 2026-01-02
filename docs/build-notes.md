# Build & Test Notes for V3 Implementation

## Environment Variables
- **SKIP_WASM_BUILD=1**: Skip WASM runtime building. UNSET this when you need embedded WASM runtimes (e.g., for zombienet tests).
- **JEMALLOC_OVERRIDE**: Can cause linker errors with tikv-jemalloc. UNSET this to let cargo build jemalloc from source.

## Zombienet Tests

### Required Feature Flag
Tests in `polkadot/zombienet-sdk-tests/` require the `zombie-ci` feature:
```bash
cargo test --release -p polkadot-zombienet-sdk-tests --features zombie-ci <test_name>
```

### Required Binaries
Zombienet native tests need these binaries in `target/release/`:
- `polkadot` (with `--features fast-runtime` for faster test execution)
- `polkadot-execute-worker`
- `polkadot-prepare-worker`
- `test-parachain`

### Build Commands (with WASM)
```bash
unset SKIP_WASM_BUILD
unset JEMALLOC_OVERRIDE

# Build polkadot with fast-runtime for testing
cargo build --release --features fast-runtime -p polkadot -p polkadot-node-core-pvf-execute-worker -p polkadot-node-core-pvf-prepare-worker

# Build test-parachain
cargo build --release -p cumulus-test-service
```

### Running Zombienet Test
```bash
ZOMBIE_PROVIDER=native cargo test --release -p polkadot-zombienet-sdk-tests --features zombie-ci scheduling_v3_test -- --nocapture
```

## Package Names
- PVF workers are NOT `polkadot-execute-worker` but:
  - `polkadot-node-core-pvf-execute-worker`
  - `polkadot-node-core-pvf-prepare-worker`

## Runtime API Implementation
When adding new runtime APIs like `SchedulingV3EnabledApi`:
1. Define the API in `cumulus/primitives/core/src/lib.rs`
2. Implement in parachain runtimes (e.g., `cumulus/test/runtime/src/lib.rs`)
3. Add trait bounds to collator functions:
   - `basic.rs`: `Client::Api: ... + SchedulingV3EnabledApi<Block>`
   - `lookahead.rs`: same
   - `slot_based/mod.rs`: same
   - `slot_based/block_builder_task.rs`: same

## Collator V3 Integration Points
- `cumulus/client/collator/src/service.rs`: `build_collation_v3()` method
- `cumulus/client/consensus/aura/src/collator.rs`: `collate_v3()` method
- `cumulus/client/consensus/aura/src/collators/basic.rs`: V3 check and scheduling proof
- `cumulus/client/consensus/aura/src/collators/lookahead.rs`: V3 check and scheduling proof
- `cumulus/client/consensus/aura/src/collators/slot_based/`:
  - `mod.rs`: `CollatorMessage` with `scheduling_proof` field
  - `block_builder_task.rs`: V3 check and scheduling proof creation
  - `collation_task.rs`: Use `build_collation_v3` when scheduling_proof present

## Tips
- Always check `cargo check -p <package>` before full builds
- Use `tail -30` or `tail -50` on build output to see errors quickly
- The slot-based collator is what test-parachain uses (not basic or lookahead)

## Shortening Feedback Cycles

### 1. Use `cargo check` before `cargo build`
```bash
cargo check -p cumulus-client-consensus-aura  # Fast type checking, no codegen
```

### 2. Incremental builds - avoid full rebuilds
- Don't use `cargo clean` unless necessary
- Use specific package builds: `-p <package>` instead of workspace-wide

### 3. Debug builds for testing logic (faster compile)
```bash
cargo build -p cumulus-test-service  # Debug mode, much faster
cargo test -p polkadot-zombienet-sdk-tests --features zombie-ci  # Debug test
```
Only use `--release` for final verification.

### 4. Run zombienet with PATH set
```bash
export PATH="$PWD/target/release:$PATH"
# or for debug:
export PATH="$PWD/target/debug:$PATH"
```

### 5. Pre-build binaries once, then iterate on tests
Build all required binaries once:
```bash
cargo build --release -p polkadot -p polkadot-node-core-pvf-execute-worker -p polkadot-node-core-pvf-prepare-worker -p cumulus-test-service
```
Then just run tests without rebuilding.

### 6. Use `cargo watch` for auto-recompile (if installed)
```bash
cargo watch -x 'check -p cumulus-client-consensus-aura'
```

### 7. Split testing: unit tests vs integration tests
- Run unit tests first (fast): `cargo test -p <package> --lib`
- Only run zombienet (slow) after unit tests pass

### 8. Background builds with status checks
```bash
cargo build --release -p polkadot 2>&1 | tee /tmp/build.log &
# Check periodically:
tail -5 /tmp/build.log
```

### 9. Avoid WASM rebuilds when not needed
For node-side changes only:
```bash
SKIP_WASM_BUILD=1 cargo build -p cumulus-client-consensus-aura
```
Only unset when runtime changes are involved.

### 10. Test-specific binaries
If only testing collator changes, you may only need to rebuild:
```bash
cargo build --release -p cumulus-test-service  # Includes collator
```
Not polkadot (if no relay-chain changes).
