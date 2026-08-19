# Decisions - block-additional-data

## [SETUP] Owner decisions (from planning interview)
- Q1 digest anchoring: NEW dedicated `DigestItem` variant (not Consensus engine-id trick). Index 3 (2 and 7 are retired).
- Q2 scope: mechanism + reference integration ONLY. `jam_chain_read` and `storage_proof_size` replay migration are OUT OF SCOPE (follow-up issues).
- Q3 test strategy: tests-with-implementation (every todo lands its own tests in the same commit).

## [SETUP] Design decisions locked during planning (Metis-reviewed)
- `finalize()` returns `Option<[u8;32]>`: `None` only when extension registered but zero items pushed (legitimate). Missing extension entirely -> PANIC (both push and finalize), never silent fallback - this value is consensus-critical (deposited in header), unlike storage_proof_size's graceful degradation.
- Persistence route unified: BOTH self-authored (todo 10/11) and network-received (todo 12) blocks set `BlockImportParams::additional_data` and flow through the SAME commit-site call to `set_additional_data` (`substrate/client/service/src/client/client.rs:728-735`). No second/divergent persistence route.
- `into_inner()` on `ParachainBlockData` keeps its existing 2-tuple return shape UNCHANGED. `validate_block` must call `additional_data()` BEFORE `into_inner()` (which consumes self).
- `BlockImportOperation::set_block_data` signature is NEVER changed. New data goes through a separate `set_additional_data` method.
- Wire-compat: `BlockAttributes::from_be_u32` changes from `from_bits().ok_or(Error)` to `from_bits_truncate()` (returns Ok, drops unknown bits) - REQUIRED fix, not optional, or the network splits the instant any node requests the new attribute (see issues.md).
- RocksDB and ParityDB migrations are TWO SEPARATE MECHANISMS (`upgrade.rs` vs `parity_db.rs` metadata versioning) - both required, neither substitutes for the other.
- Replay-provider registration required on BOTH: (a) generic executing import path (todo 13, NEW - registers in `sc-service`'s client.rs before `execute_block`) and (b) cumulus `validate_block` (todo 14). Missing (a) was the original design's most severe gap.

## [TODO-3] ParachainBlockData::V3 constructor strategy
- Chose **separate constructor** (`new_with_additional_data`) over modifying `new()`'s signature.
- `new(blocks, proof, scheduling_proof)` is unchanged → zero call-site churn at the 4 existing construction sites.
- `new_with_additional_data(blocks, proof, scheduling_proof, additional_data)` selects V3 when `additional_data.any(Some) && scheduling_proof.is_some()`; falls back to V2/V1 otherwise (V3 requires a scheduling proof since the field is non-optional in the variant).
- Rationale: least invasive; existing collator/test callers continue to compile unmodified; the new constructor is the entry point for todos 11/12 once they produce actual digest data.

## [TODO-8] Reference runtime: pallet hook and ordering guarantee

- Hook used: `SubstrateTest::on_finalize` (`substrate_test_pallet::pallet`).
- `on_finalize` ordering in this runtime (declaration order in `construct_runtime!`):
  `System → Babe → SubstrateTest → Utility → Balances`.
  SubstrateTest is 3rd; `frame_system::Pallet::finalize()` (which consumes the digest) runs
  AFTER all `on_finalize` hooks, so `deposit_log` called from SubstrateTest is included in the
  header. No other pallet in this runtime calls `additional_data::push`, so ordering within the
  hook sequence does not matter for correctness here.
- Storage-flag guard: `AdditionalDataPushed<T>` (bool, ValueQuery) is set to `true` by
  `push_additional_data` and consumed by `on_finalize::take()`. This means blocks that never call
  `push_additional_data` never invoke `additional_data::finalize()`, so `AdditionalDataExt` need
  not be registered for those blocks. This prevents panics in existing block-execution tests that
  use the test runtime without registering the extension.
- `TestRuntimeHostFunctions` type alias defined in `substrate-test-runtime::lib.rs` (gated by
  `#[cfg(feature = "std")]`):
  `(sp_additional_data::additional_data::HostFunctions, sp_io::SubstrateHostFunctions)`.
  Used in `substrate-test-runtime-client`'s `ExecutorDispatch`, `Client<B>`, and the
  `TestClientBuilderExt` impl so any `WasmExecutor` built for this runtime resolves the new host
  function imports. The `genesis_builder_tests` inline executor also switched to this type.
- Tests use `TestExternalities` (native execution path), registering `AdditionalDataExt` only
  in the test that calls `push_additional_data`; the "nothing pushed" test deliberately omits it
  to exercise the storage-flag guard.
