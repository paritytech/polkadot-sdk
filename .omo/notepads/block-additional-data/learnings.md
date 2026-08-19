# Learnings - block-additional-data

## [SETUP] Canonical encoding (MUST be identical everywhere - read before ANY todo)
- `items: Vec<Vec<u8>>` pushed by the runtime, in insertion order.
- `blob: Vec<u8> := items.encode()` (ONE SCALE encoding of the whole Vec<Vec<u8>>). Computed by `sp-additional-data::encode_items()`.
- `digest_hash: [u8; 32] := blake2_256(&blob)`. Computed by `sp-additional-data::hash_blob()`.
- From here on, ONLY `blob` (raw bytes) travels through DB column / proto field / IncomingBlock / BlockImportParams / ParachainBlockData::V3. No layer re-encodes it or decodes it back into items. Verification only ever needs `hash_blob(&blob)`.
- `DigestItem::AdditionalData` is `[u8; 32]` (fixed-size), NOT `Vec<u8>`.
- At most ONE `AdditionalData` digest item per header. Invariant everywhere: `header_has_digest == additional_data.is_some()`; mismatch in either direction is rejected.

## [SETUP] Repo precedents to mirror (do not reinvent)
- Host-fn + extension pattern: `cumulus/primitives/proof-size-hostfunction/src/lib.rs:35-42` (trait `StorageProofSize`).
- Extension type: `substrate/primitives/trie/src/proof_size_extension.rs` - `ProofSizeExt` decl_extension (~L26-56), `RecordingProofSizeProvider` (~L86), `ReplayProofSizeProvider` (~L177).
- Extension registration during block building (cumulus): `cumulus/client/consensus/aura/src/collator.rs:282-296` (is_registered check ~L285).
- Extension registration during import execution (the pattern todo 13 mirrors): `cumulus/client/consensus/aura/src/collators/slot_based/block_import.rs:253-255`.
- Generic (non-cumulus) extension threading ALREADY EXISTS - do not rebuild: `substrate/client/block-builder/src/lib.rs:139,165-168,253-255` (`with_extra_extensions`), `substrate/client/basic-authorship/src/basic_authorship.rs:300`.
- Indexed-body DB column precedent: `substrate/client/db/src/lib.rs` columns module, `apply_indexed_body`, `block_indexed_body`.
- DB migration precedent: `substrate/client/db/src/upgrade.rs` `migrate_3_to_4` (RocksDB-only!) + `substrate/client/db/src/parity_db.rs` (separate metadata-based mechanism for ParityDB - NOT the same code path).

## [SETUP] Known repo-wide CI gates that silently break if skipped
- New crate MUST be added to root `Cargo.toml` workspace `members` (explicit list, not glob).
- After adding a crate, run `python3 scripts/generate-umbrella.py --sdk . --version "$UMBRELLA_VERSION"` - must produce no diff (`check-umbrella` CI job).
- `cargo +nightly fmt` before every commit.
- `docs/design/` does NOT exist in this repo - use `docs/contributor/` for the soundness doc.
- markdownlint enforces MD013 line_length: 120 chars.

## [TODO 6 REVIEWER PASS] block-additional-data-soundness.md - APPROVED

Reviewer pass completed on docs/contributor/block-additional-data-soundness.md:

1. **Data Availability** - APPROVE: Cites ADDITIONAL_DATA column (lib.rs:466-490), BlockAttributes bit (message.rs:42-72), Backend trait default impl, and ParachainBlockData::V3 field. Correctly states chain-level policy responsibility.

2. **PoV/Block Size Budget** - APPROVE: Cites ParachainBlockData::V3 (parachain_block_data.rs:72-87), RecordingAdditionalDataProvider (additional-data/src/lib.rs), and ParachainBlockData::new (collator/src/service.rs:279-311). Correctly distinguishes parachain PoV limit (relay-enforced) from solochain (chain responsibility).

3. **Pruning and Archive Interaction** - APPROVE: Cites ADDITIONAL_DATA column (lib.rs:466-490), prune_block function (lib.rs:2183-2237), and indexed_body precedent (BODY_INDEX column). Correctly states pruning alongside block body, archive retention.

4. **Non-Executing Import Paths** - APPROVE: Cites execute_block (import_queue.rs), host functions (sp_additional_data::push/finalize), ReplayAdditionalDataProvider, and hash_blob helper. Correctly contrasts executing path (header-equality backstop) with non-executing paths (explicit hash check necessary). Justification for explicit check is precise.

5. **Host-Function and Runtime-Upgrade Compatibility** - APPROVE: Cites sp-additional-data host functions (additional-data/src/lib.rs), #[runtime_interface] trait, panic-on-missing-extension behavior, and substrate/test-utils/runtime integration. Correctly states loud failure (not silent), contrasts with storage_proof_size graceful fallback, explains consensus-critical rationale.

6. **Malicious-Peer Handling** - APPROVE: Cites sync protocol (todo 9), BlockImportParams::additional_data (todo 7), explicit hash check (todo 15), and indexed_body precedent (apply_indexed_body function). Correctly states hash check before import, rejection before DB storage.

7. **Header-Decode Compatibility for Un-Upgraded Nodes** - APPROVE: Cites DigestItem enum (digest.rs:75-109), discriminant index 3, Decode impl (digest.rs:301-316), exhaustive pattern matching, and decode error on unknown discriminant. Correctly states hard incompatibility (not soft), coordinated node upgrade requirement, and that opting in is NOT a silent change.

**FINAL VERDICT: 7/7 APPROVE**

All seven points present with concrete mechanism citations. No hand-waving. Closing "Verified Sound Because" section summarizes each backing mechanism. markdownlint passes (0 issues).

## [WAVE1-TODO7] IncomingBlock/BlockImportParams field addition
- `IncomingBlock` is NOT `#[non_exhaustive]` - adding a field breaks every struct-literal construction in the workspace (network/sync strategy files, warp.rs, service/chain_ops, network/test, basic_queue tests). All were fixed with `additional_data: None` (by this todo for sc-consensus tests; other todos fixed their own call sites).
- `BlockImportParams` IS `#[non_exhaustive]` - only `new()` needed the `additional_data: None` initializer; external constructors unaffected.
- Test lives in `block_import.rs` `mod tests` using `sp-test-primitives` (dev-dep). `Header::new` takes 5 args (number, extrinsics_root, state_root, parent_hash, digest). `BlockImportParams::new` needs explicit type annotation `BlockImportParams<Block>` for inference.
- `cargo test -p sc-consensus additional_data_field_constructs -- --exact` does NOT match (module path prefix); run without `--exact` or use full path `block_import::tests::additional_data_field_constructs`.
- rustup binary not on PATH in this env; use `~/.rustup/toolchains/nightly-x86_64-unknown-linux-gnu/bin/cargo fmt`.

## [WAVE1-TODO1] DigestItem::AdditionalData variant (commit ad3179206f7)
- Discriminant 3 confirmed free: `git log -S 'ChangesTrieRoot' -- substrate/primitives/runtime` shows commit `4cbbf0cf436` retired indices 2 (`ChangesTrieRoot`) and 7 (`ChangesTrieSignal`). Index 1 also not present in current enum. 3 is lowest free slot.
- `DigestItemType` encodes discriminant as single u8 byte (not u32 despite `#[repr(u32)]`). SCALE derive reads 1 byte and matches discriminant values. Unknown byte → `Err` immediately.
- `[u8; 32]` encodes as 32 raw bytes (no length prefix) in SCALE - fixed-size arrays are zero-cost in headers.
- ALL external workspace files with `DigestItem::` matches use wildcard arms (`_ => ...`) or specific-variant-with-guard patterns. Zero non-wildcard exhaustive matches outside `digest.rs` needed updating.
- Serde impls (`Serialize`/`Deserialize`) delegate to `Encode`/`Decode` via `using_encoded` - no changes needed for new variants.
- `DigestItemRef<'a>` must mirror `DigestItem` 1:1; `AdditionalData(&'a [u8; 32])` borrows the fixed array.
- Workspace-wide `cargo check --workspace` fails on `sp-additional-data` (untracked dir `??`) due to `RIType` trait errors from another in-progress agent - pre-existing, unrelated to digest.rs changes. `cargo check -p sp-runtime` is clean.

## [WAVE1-TODO2] sp-additional-data crate (commit bb3bfd52bc6)

- Umbrella version: 0.1.0 (cargo metadata returned empty for umbrella/Cargo.toml; CI fallback used).
- `#[runtime_interface]` requires explicit pass_by wrapper types — `Vec<u8>` as a parameter needs `PassFatPointerAndRead<Vec<u8>>` and `Option<[u8; 32]>` as a return type needs `AllocateAndReturnByCodec<Option<[u8; 32]>>`. Plain `Vec<u8>` / `Option<[u8;32]>` cause `RIType` / `IntoFFIValue` compile errors. Import from `sp_runtime_interface::pass_by::{AllocateAndReturnByCodec, PassFatPointerAndRead}`.
- `codec::Decode` import appears unused (clippy warns) even though `AllocateAndReturnByCodec` conceptually needs it — the proc-macro generates code that resolves Decode through the trait bounds, not a direct call site. Remove `Decode` from the use statement.
- `sp_externalities::decl_extension! { pub struct Foo(...); }` does NOT support a doc comment placed BEFORE the macro call (rustdoc warns "unused doc comment"). Put the `///` doc inside the macro invocation instead.
- The `#[cfg(feature = "std")]` guard on `AdditionalDataProvider`, `AdditionalDataExt`, and the provider impls is correct and mirrors `cumulus-primitives-proof-size-hostfunction`. The `#[runtime_interface]` proc macro wraps the host function body in `#[cfg(not(substrate_runtime))]` automatically.
- Umbrella regen: `cargo-workspace>=1.2.4` installed in a local venv (newer version, 2026-08-18) generates a completely different umbrella format (spaces vs tabs, entirely different feature structure) from the current HEAD which was generated with an older version. Running the script locally DESTROYS the committed umbrella. Workaround: make manual edits in the existing HEAD format (std feature, runtime-full feature, runtime feature, [dependencies.sp-additional-data] section, umbrella/src/lib.rs re-export). The CI docker image (bullseye-1.93.0-2026-01-27) uses the old cargo-workspace version and will see no diff with the manual edits.
- Clippy pre-existing failures: `sp-runtime-interface-proc-macro` has 3 clippy lints (`iter_kv_map` x2, `explicit_counter_loop`) that trigger as errors with `-D warnings` on Rust 1.97.1 (locally installed). CI uses Rust 1.93.0 where these lints don't fire. The `sp-additional-data` code itself has zero clippy issues.

## [WAVE2-TODO9] block_request_handler sync handler (bundled into commit 534aef9340e)

- `block_additional_data` is on `sp_blockchain::Backend<Block>` (NOT on `sc_client_api::BlockBackend`). To make it callable from `BlockRequestHandler<B, Client>` whose bound is `HeaderBackend<B> + BlockBackend<B>`, the method was added to `sc_client_api::BlockBackend` with `default Ok(None)`. Then `sc_service::client::Client`'s `BlockBackend` impl delegates to `self.backend.blockchain().block_additional_data(hash)` — same pattern as `block_indexed_body`.
- Server side mirrors `indexed_body` exactly: `let get_additional_data = attributes.contains(BlockAttributes::ADDITIONAL_DATA);` + `self.client.block_additional_data(hash)?.unwrap_or_default()` + proto field name `additional_data` (not a variable rename since prost struct field == attribute name).
- Client side (`blocks_from_schema`): `additional_data: if request.fields.contains(BlockAttributes::ADDITIONAL_DATA) { Some(block_data.additional_data) } else { None }` — matches `indexed_body` pattern; old peer (field 10 absent) gives `Some(vec![])` not `None`.
- `sp_runtime::generic::Header<N, H>` has NO `new()` method (unlike `sp_runtime::testing::Header`). Use struct-literal initialization with public fields: `Header { parent_hash: Default::default(), number: 0u64, ... }`.
- `blocks_from_schema` is `fn` (private) on `FullBlockDownloader` but accessible from `#[cfg(test)] mod tests` within the same file — Rust child modules can read private items of their parent.
- `FullBlockDownloader` private fields are also readable from `mod tests` in the same file: `FullBlockDownloader { protocol_name: ProtocolName::Static("test"), network: NetworkServiceProvider::new().handle() }`.
- `MockClient` for handler tests: manual struct (not mockall) with `HashMap<Hash, Header>` and `HashMap<Hash, Vec<u8>>`. `hash()` returns `Ok(None)` to stop the ascending-direction loop after one block. `status()` and `number()` can be `unimplemented!()` since they aren't called by `get_block_response`.
- Files changed beyond the two stated in the plan: `sc_client_api/src/client.rs` and `sc_service/src/client/client.rs` were required to expose `block_additional_data` through the `BlockBackend` bound. The struct-literal fixes in `warp.rs`, `blocks.rs`, `chain_sync/test.rs` were required by the new `additional_data` field on `generic::BlockData`.
- QA test names must be prefixed with full module path for `--exact`: `block_request_handler::tests::additional_data_fetched_when_requested`.

## [WAVE3-TODO12] import-queue/sync-strategy wiring

### client.rs exact DB commit site
- File: `substrate/client/service/src/client/client.rs`
- Function: `apply_block` (private fn)
- `operation.op.set_additional_data(additional_data)?;` is at **line 519** (after `set_create_gap` at line 518, before `execute_and_import_block` call at line 521)
- `additional_data` is extracted by adding it to the `BlockImportParams` destructuring at line ~481

### warp.rs production "background block import" path
- There is NO separate background block import path in warp.rs production code.
- The only production block request in warp.rs is `target_block_request()` at **warp.rs:618** (fields at line 648-651 now include `BlockAttributes::ADDITIONAL_DATA`).
- After warp sync completes, the background historic block download is handled by `ChainSync` via the **gap sync** mechanism: `peer_gap_block_request` in chain_sync.rs (called at line ~2065-2085), using `mode.required_block_attributes(true, is_archive)` at **chain_sync.rs:2070**.
- Previously-cited warp.rs L1543-1575 is test code (tests the `target_block_request` function). Do not confuse with production.
- The gap-sync exclusion: when `is_gap=true && !is_archive`, both `BODY` and `ADDITIONAL_DATA` are excluded (single `attrs & !(BODY | ADDITIONAL_DATA)` expression in `required_block_attributes`).

### Scoped nightly fmt
- Run `cargo +nightly fmt -p sc-consensus -p sc-network-sync -p sc-service` (not workspace-wide).
- `block_request_handler.rs` was cosmetically reformatted by the scoped fmt even though not changed by this todo; reverted with `git checkout -- <file>` per the established pattern.

## TODO-10 outcomes (feat(sc-block-builder): surface collected additional data)

**Commit:** 098acdb1d52

**Files changed:**
- `substrate/client/block-builder/Cargo.toml` — added `sp-additional-data` workspace dep
- `substrate/client/block-builder/src/lib.rs` — `BuiltBlock::additional_data`, `BlockBuilderBuilderStage2::recording_additional_data` field + `enable_additional_data_recording()`, `BlockBuilder::recording_additional_data` field, `build()` population via `take_data()`
- `substrate/primitives/consensus/common/src/lib.rs` — `Proposal::additional_data: Option<Vec<u8>>`
- `substrate/client/basic-authorship/src/basic_authorship.rs` — unconditional `enable_additional_data_recording()` + `additional_data` propagation to `Proposal`
- `substrate/client/consensus/babe/src/tests.rs` — `additional_data: None` in mock `Proposal {}`
- `substrate/client/consensus/aura/src/lib.rs` — `additional_data: None` in mock `Proposal {}`

**Tests added:**
- `sc_block_builder::tests::additional_data_is_none_without_recording`
- `sc_block_builder::tests::additional_data_recorded_when_runtime_pushes`

**Surprises / lessons:**
- Partial-move error: `built_block.additional_data` (non-Copy `Option<Vec<u8>>`) cannot be field-moved then `into_inner()` called. Fix: `mut built_block` + `.take()` on the field.
- LSP errors during editing showed stale line numbers after each Edit; relied on `cargo check` as ground truth rather than LSP.
- `sc-rpc-spec-v2` and `polkadot-zombienet-sdk-tests` have pre-existing workspace check failures unrelated to this todo (confirmed by stash test).
- `cargo +nightly fmt` is just `cargo fmt` in this nix environment (nightly rustfmt is the default binary).
- `treasury::Proposal` is a completely different struct — not the consensus one; no change needed there.

## [WAVE4-TODO13] execute_block call-site (sc-service client.rs)

- After todo 13's edits, `runtime_api.execute_block(...)` is at **line 896** in
  `substrate/client/service/src/client/client.rs` (inside `prepare_block_storage_changes`,
  `(true, None, Some(ref body))` arm).
- The digest/additional_data consistency guard + `AdditionalDataExt` registration is inserted
  at lines 862-895 (immediately before `execute_block`).
- `sp-additional-data` added as a regular dep of sc-service; `sc-block-builder` added as
  dev-dep for the happy-path test.
- Error returned on mismatch: `sp_blockchain::Error::Consensus(ConsensusError::ClientImport("additional data present but header digest missing or has multiple AdditionalData items"))`.
  Callers receive `ConsensusError::ClientImport(e.to_string())` (maps in `import_block`'s `map_err`).

## [TODO-14] validate_block additional data implementation

### Host function names (generated by #[runtime_interface] on trait AdditionalData)
- Push: `sp_additional_data::additional_data::host_push` — replacement fn takes `Vec<u8>`
- Finalize: `sp_additional_data::additional_data::host_finalize` — replacement fn returns `Option<[u8; 32]>`

### environmental! gotcha: two trait-variant macros in the same module scope
Both generate `static GLOBAL` → E0428 name conflict. Fix: put the second one in a private
submodule with `pub(super)` wrapper fns, then alias the module with `use`:
```rust
mod _additional_data_replay_env {
    use super::AdditionalDataReplay;
    environmental::environmental!(env: trait AdditionalDataReplay);
    pub(super) fn using<R, F: FnOnce() -> R>(t: &mut dyn AdditionalDataReplay, f: F) -> R { env::using(t, f) }
    pub(super) fn with<R, F: for<'a> FnOnce(&'a mut (dyn AdditionalDataReplay + 'a)) -> R>(f: F) -> Option<R> { env::with(f) }
}
use _additional_data_replay_env as additional_data_replay;
```

### Pre-digest test-construction pattern
`build_block_with_witness` accepts `pre_digests: Vec<DigestItem>`. Pass
`vec![DigestItem::AdditionalData(hash_blob(&blob))]` — the digest is included in the sealed
header. After build, call `block.into_inner()` to get `(blocks, proof)` then manually construct:
```rust
ParachainBlockData::V3 { blocks, proof, scheduling_proof: dummy_scheduling_proof(), additional_data: vec![Some(blob)] }
```
With V3_SCHEDULING_ENABLED = false (default test runtime, no `v3-descriptor` feature), the
scheduling_proof is not validated — a dummy with empty header_chain passes.

## [TODO-15] Non-executing import path enumeration (confirmed)

### Confirmed non-executing paths (re-verified by reading current source)

**Check point A — WarpSync early return** (`import_queue.rs`, guarded by `BlockOrigin::WarpSync`):
- `substrate/client/network/sync/src/strategy/warp.rs:406` — `proof_to_incoming_block` closure:
  warp-proof authority-set headers, `BlockOrigin::WarpSync, skip_execution: true`.
  Always `additional_data: None` in production; check is a no-op but present for soundness.
  Bypasses the verifier entirely via the early-return guard at import_queue.rs:338.

**Check point B — `StateAction::Skip` branch** (`import_queue.rs`, guarded by `skip_execution`):
- `substrate/client/network/sync/src/strategy/chain_sync.rs:1360` —
  `PeerSyncState::DownloadingGap`: gap-sync (post-warp historic fill),
  `import_existing: true, skip_execution: true, BlockOrigin::GapSync`.
  **This is the only production "background block download" path after warp sync.**
  There is no separate warp.rs background-import path (confirmed by todo 12 and re-verified
  by reading current warp.rs — lines 1366/1470 with `skip_execution: true` are test code,
  not production).
- `substrate/client/network/sync/src/strategy/chain_sync.rs:1411` —
  `PeerSyncState::DownloadingBlocks` / regular sync in `ChainSyncMode::LightState`:
  `skip_execution: self.skip_execution()` returns `true`; historic blocks are not executed.
- `substrate/client/network/sync/src/strategy/chain_sync.rs:1554` —
  `PeerSyncState::DownloadingStale`: stale blocks, `skip_execution: true`.

**NOT a non-executing path (false positive):**
- `chain_sync.rs:2184-2185` — has `skip_execution: self.skip_execution()` but ALSO has
  `state: Some(state)`, so it hits the `StateAction::ApplyChanges` branch, not `Skip`.

### Implementation details

- Helper function: `verify_additional_data_non_executing<B: BlockT>` in import_queue.rs
- Handles all 4 cases: (both absent → OK), (both present + hash match → OK),
  (hash mismatch → ClientImport error), (data-only → ClientImport error),
  (digest-only → ClientImport error)
- Call at check point A: uses `block.additional_data.as_deref()` (not yet moved)
- Call at check point B: uses `import_block.additional_data.as_deref()` (moved from block at line ~381)
- 8 tests total (4 per check point): correct-match, tampered, digest-only, data-only
- `block_request_handler.rs` was cosmetically reformatted by `cargo +nightly fmt -p sc-network-sync`;
  reverted with `git checkout --` per the established pattern.

## [TODO-14] cumulus-test-runtime additional-data wiring + sample bytes

**Exact sample bytes used (MUST match across layers):**
- `ADDITIONAL_DATA_SAMPLE: &[u8] = b"additional-data-test"` (same as todo 8's substrate-test-runtime).
- Hard-coded in `cumulus/test/runtime/src/test_pallet.rs` `push_additional_data` dispatchable as `b"additional-data-test".to_vec()`.
- Tests compute the blob as `sp_additional_data::encode_items(&[b"additional-data-test".to_vec()])` and the expected digest as `hash_blob(&blob)`.
- todo 16's e2e test needs the SAME runtime support (cumulus-test-runtime now pushes when `push_additional_data` is called) and the SAME sample bytes.

**Runtime wiring (mirrors todo 8 exactly):**
- `cumulus/test/runtime/src/test_pallet.rs`: `AdditionalDataPushed<T>` (bool, ValueQuery) storage; `on_finalize` hook calling `sp_additional_data::additional_data::finalize()` + `frame_system::Pallet::<T>::deposit_log(sp_runtime::generic::DigestItem::AdditionalData(hash))` when flag set; `push_additional_data` dispatchable (`#[pallet::weight(0)]`, `b"additional-data-test"`).
- `cumulus/test/runtime/Cargo.toml`: added `sp-additional-data = { workspace = true }` + `"sp-additional-data/std"` in the std feature.
- The runtime's executor host functions live in `cumulus-test-client` (`cumulus/test/client/src/lib.rs`, executor tuple lines 59-64 and 210-215) — the uncommitted fix adding `sp_additional_data::additional_data::HostFunctions` is correct and required.
- `cumulus/test/client/src/block_builder.rs` `init_block_builder` registers `AdditionalDataExt(Box::new(RecordingAdditionalDataProvider::new()))` unconditionally next to `ProofSizeExt`.

**Key gotcha (why the explicit assertion moved in implementation.rs):** with the assertion AFTER `execute_verified_block`, a tampered blob panics inside `frame_executive::final_checks` with "Digest item must match that calculated." (the replayed blob's hash lands in the executed header and mismatches the input header's digest) BEFORE the explicit "additional data hash does not match header digest" assertion runs — so the pinned message was unreachable. The assertion now runs BEFORE execution (fail-fast).

**Test names (renamed to match the `additional_data` filter):** `validate_block_v3_with_additional_data_succeeds`, `validate_block_v3_tampered_additional_data_fails`, `validate_block_v3_additional_data_digest_without_data_fails`, `validate_block_v3_additional_data_without_digest_fails`.

## [WAVE5-TODO16] End-to-end integration test (BLOCKED by a production bug - see issues.md)

**Test file:** `cumulus/test/service/tests/additional_data.rs`
**Test name:** `block_additional_data_end_to_end` (integration test; run via
`cargo test -p cumulus-test-service block_additional_data_end_to_end -- --exact --nocapture`)
**Runtime:** cumulus-test-runtime (the reference runtime); needs real WASM build (NO SKIP_WASM_BUILD)
plus the PVF worker binaries built first: `cargo build --bin polkadot-execute-worker --bin polkadot-prepare-worker`.

**Topology** (mirrors the `transaction_throughput` bench, no zombienet):
- 2 in-process relay validators via `run_relay_chain_validator_node` (Alice/Bob)
- 1 collator: `TestNodeBuilder::new(para_id, handle, Charlie).enable_collator().connect_to_relay_chain_nodes([&alice,&bob]).build()`
- `alice.register_parachain(100, WASM_BINARY, RelayHeadData(get_raw_genesis_header(collator.client)))`
- 1 full node: `TestNodeBuilder::new(para_id, handle, Dave).connect_to_relay_chain_nodes([&alice,&bob]).connect_to_parachain_node(&collator).build()`

**Key harness gotchas:**
- `run_relay_chain_validator_node` BLOCKS internally (spawns via `tokio_handle.block_on`), so it MUST be
  called OUTSIDE the test's `runtime.block_on` - calling it inside panics
  "Cannot start a runtime from within a runtime".
- cumulus-test-service's executor `HostFunctions` tuple did NOT include
  `sp_additional_data::additional_data::HostFunctions`; without it the TestNode cannot even construct the
  cumulus-test-runtime WASM (`runtime requires function imports which are not present on the host:
  'env:ext_additional_data_push_version_1', 'env:ext_additional_data_finalize_version_1'`). Fixed by adding
  the host functions to `cumulus/test/service/src/lib.rs` `HostFunctions` + `sp-additional-data` as a
  regular (std, `default-features = true`) dep of cumulus-test-service (mirrors cumulus-test-client).
- `Backend::block_additional_data` on a TestNode backend is reached via `backend.blockchain().block_additional_data(hash)`
  (needs `sc_client_api::backend::Backend` in scope for `.blockchain()` and `sp_blockchain::Backend` for
  `.block_additional_data()`).
- `TransactionStatus::InBlock` is a 2-tuple `(hash, index)` - pattern `InBlock((hash, _))`.
- Submit extrinsics via the tx pool (`submit_and_watch` + wait for `InBlock`) to get the exact block hash;
  auto-fetch nonce per tx and wait for InBlock between txs (the nonce then accounts for the prior inclusion).

**Sub-assertions:**
- (a) collator `blockchain().block_additional_data(push_block)` == Some(canonical blob); header carries
  exactly one `DigestItem::AdditionalData(hash_blob(&blob))`.
- (b) full node syncs to the push block; its DB has the same blob (proves ADDITIONAL_DATA transferred over
  sync + accepted on the generic executing import path + persisted).
- (c) direct `validate_block` on a V3 candidate built via cumulus_test_client's block builder WITH
  `push_additional_data` (mirrors todo 14's `build_v3_with_runtime_pushed_additional_data`) returns the same header.
- (d) corruption: blob replaced with `vec![0xABu8; 16]`, header digest unchanged -> `validate_block` returns Err;
  logs what was corrupted and where rejection occurred.
- (e) control block that never calls `push_additional_data`: no AdditionalData digest in header,
  `block_additional_data` is None on collator AND full node; a no-push V1 candidate validates normally.

**Control-case approach for (e):** a regular `BalancesCall::transfer_allow_death` extrinsic (NOT
`push_additional_data`) submitted to the collator's tx pool and included by the REAL collator. The control
block is therefore a genuine non-opted-in block produced by the real chain, synced by the full node, with
no digest and no db entry on either node.

**STATUS: the test cannot pass yet - blocked by a genuine production bug (see issues.md "[TODO-16] ...").**
Do NOT treat this test as green until that bug is fixed. The test file + test-harness wiring are in the
working tree but UNCOMMITTED.

## [WAVE5-TODO16] DONE - test is GREEN (supersedes the earlier BLOCKED status note)

The e2e test now passes end-to-end (`cargo test -p cumulus-test-service block_additional_data_end_to_end
-- --exact --nocapture` -> exit 0, 5/5 sub-assertions). Two production bugs and one runtime change were required;
see issues.md for details and commits.

**Final control-case approach for (e):** the control block is parachain block **#1** - the first block the real
collator produces after registration, which contains only inherents (no user extrinsic, so no `push_additional_data`).
It is located via `client.hash(1)` and asserted to have no `AdditionalData` digest and `block_additional_data == None`
on both the collator and the synced full node. This needs no extra tx submission and needs only ~2-3 canonical
parachain blocks total (the in-process harness reliably produces only a few before its candidates stop being included).

**Other harness reality (hard-won):**
- A SINGLE relay validator stalls the collator after ~2 blocks; TWO are required. But even with two, the collator
  only reliably produces ~3-4 parachain blocks (relay keeps going) and occasionally retracts early candidates - so
  the test must need as few blocks as possible and must POLL for the digest block (the tx-pool watcher is unreliable:
  an included-then-retracted tx becomes `Pool(TemporarilyBanned)`/`AlreadyImported` and the watcher emits `Invalid`).
- `push_additional_data` must be pool-valid (see the runtime change in issues.md) or the tx is Invalid on pool
  revalidation and never included.
- `run_relay_chain_validator_node` blocks internally and must be created OUTSIDE `runtime.block_on`.

**Final test file:** `cumulus/test/service/tests/additional_data.rs`, test `block_additional_data_end_to_end`.
**Commits:** be6b2029200 (sync fix), 4d43adc59c1 (collator fix), 0f668b3110b (test + runtime + harness wiring).
