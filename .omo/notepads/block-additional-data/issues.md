# Issues / gotchas - block-additional-data

## [SETUP] Bugs Metis's gap-analysis caught before implementation (do not reintroduce)
- **Network-splitting bug**: `substrate/client/network/common/src/sync/message.rs:68-71` `BlockAttributes::from_be_u32` uses `bitflags::from_bits(...).ok_or(...)` which REJECTS the whole value if ANY unknown bit is set. Verified independently by reading the file. Todo 5 fixes this to `from_bits_truncate`. If any future work touches this function, do NOT regress it back to `from_bits`.
- **Consensus-breaking silent-rejection gap**: original design only registered the replay provider in cumulus's `validate_block`, never on the plain/generic full-node executing-import path. Without todo 13, every full node would panic/reject every block of an opted-in chain. This is now its own explicit todo (13) - do not let it get "simplified away" as redundant with todo 14; they cover different execution contexts (generic client vs. isolated PoV validation).
- **`migrate_3_to_4` is RocksDB-only** (uses `kvdb_rocksdb::Database` directly, `_db_type` param unused, test module gated `#[cfg(all(test, feature = "rocksdb"))]`). ParityDB has NO equivalent in `upgrade.rs` - its versioning lives entirely in `parity_db.rs`'s metadata mechanism. Do not try to add a ParityDB branch inside `upgrade.rs`.
- **Two distinct `BuiltBlock` types** exist in the codebase: `substrate/client/block-builder/src/lib.rs:191-196` (generic, `block`+`storage_changes`) and a LOCAL one in `cumulus/client/consensus/aura/src/collator.rs:343` (`block`+`proof`+`backend_transaction`). Todo 10 extends the former, todo 11 works with the latter (cumulus-specific path). Do not conflate them.
- **`ParachainBlockData::into_inner()`** consumes `self` and returns `(Vec<Block>, CompactProof)` - call any new accessor (`additional_data()`) BEFORE calling `into_inner()`.
- warp.rs line range `:1543-1575` cited during planning turned out to be TEST code, not the production background-import path, per Metis's verification. Read the file fresh before todo 12/15 touch it - do not trust a pre-supplied line range there.
- `cumulus-pallet-parachain-system`'s `validate_block` tests require a full WASM build (`WASM_BINARY.expect(...)` in `tests.rs:73`) - never run those under `SKIP_WASM_BUILD=1`.

## Open / unresolved during implementation (append here as discovered)
(none yet - append findings from Wave 1 here)

## [DONE] Todo 5 - wire format (Wave 1)
- `BlockAttributes::ADDITIONAL_DATA = 0b01000000` added in `substrate/client/network/common/src/sync/message.rs`.
- `from_be_u32` fixed: `from_bits(...).ok_or(...)` -> `Ok(Self::from_bits_truncate(...))`. Unknown bits are now dropped, not rejected. Return type kept as `Result<Self, Error>` for API stability (caller `block_request_handler.rs:243` uses `?` unchanged).
- `bytes additional_data = 10;` added to `BlockData` in `api.v1.proto`; regenerated via `prost_build` on next build.
- 4 tests added in `block_attributes_tests` (roundtrip, unknown-bit dropped, known-only no-regression, all-bits-set truncated). All pass.
- Compile fallout from the pre-existing `IncomingBlock.additional_data` field (prior todo): added `additional_data: None` to 10 initializers across sc-network-sync (chain_sync x5, state x2, warp x3), sc-service import_blocks, sc-network-test block_import, cumulus-pov-recovery lib. `block_request_handler.rs` response builder sets `additional_data: Vec::new()` (plumbing of the wire field into IncomingBlock is todo 9/12).
- Verified: `SKIP_WASM_BUILD=1 cargo check -p sc-network-common -p sc-network-sync --all-targets --all-features` passes; `cargo test -p sc-network-common block_attributes` 4/4 pass.

## [ENV] category="deep" is NOT available in this environment
- Dispatching with `category="deep"` fails immediately with "Category 'deep' requires model 'gpt-5.6-sol' which is not available." No work is performed - the task errors out before touching any files.
- FIX: route every remaining todo whose plan-recommended category is "deep" to `category="unspecified-high"` instead. Affects todos 1, 2, 4 (re-dispatched), and will affect 12, 13, 14, 15 later in the plan - use unspecified-high for those too when we get there.
- Pre-existing dirty-worktree files confirmed NOT touched by any of our agents (verified via `git status --short` cross-checked against `git log`): bridges/primitives/runtime/src/lib.rs, cumulus/parachains/runtimes/bridge-hubs/test-utils/src/test_cases/{helpers,mod}.rs (modified), prdoc/pr_slot_based_relay_proof_cache.prdoc, rtcmp.mjs (untracked). Out of scope - do not touch, do not stage, do not investigate further.

## [TODO-8] Block number off-by-one in additional_data tests

**Symptom:** Both `additional_data_digest_deposited_when_pushed` and `no_additional_data_digest_when_nothing_pushed` panicked with:
```
assertion `left == right` failed: Block number must be strictly increasing.
 left: 1
 right: 2
```

**Root cause:** `frame_system::Pallet::initialize` (substrate/frame/system/src/lib.rs:~1972) asserts
`Self::block_number() + 1 == *number`. In a fresh `new_test_ext()`, `block_number()` is `0`, so the
next valid block number is `1`. Both tests passed `Header::new(2, ...)` — an off-by-one.

**Fix:** Changed `2` to `1` in both `Header::new(...)` calls. Amended commit `87b0495067a`.

**Note:** `new_test_ext()` DOES require the WASM binary in this environment (it's a pre-existing
crate characteristic, not something introduced by this todo). The tests run correctly against the
real block-execution pipeline once the WASM binary is built (without `SKIP_WASM_BUILD=1`).

## [ENV] Recurring cosmetic import-reordering diffs after subagent sessions
- Multiple times, files already committed and verified (digest.rs, block_request_handler.rs, client.rs, warp.rs, chain_sync/test.rs) reappeared as uncommitted "modified" in git status with PURELY import-order diffs (alphabetical vs grouped) - zero functional/logic changes, confirmed by diffing content and re-running cargo check/test successfully both ways.
- Root cause suspected: a subagent's "run cargo +nightly fmt before finishing" step running unscoped (`cargo fmt` across the whole workspace/crate) instead of scoped to just-changed files, reformatting import ordering in unrelated already-committed files per nightly rustfmt's import-granularity rules.
- FIX applied each time: `git checkout -- <file>` to revert to the clean committed state, then re-verify compile+tests still pass (they always did - confirms zero functional impact).
- FUTURE AGENTS: prefer `cargo +nightly fmt -p <specific-crate>` or `rustfmt <specific-file>` over a bare workspace-wide `cargo +nightly fmt` to avoid reformatting unrelated already-committed files.

## [TODO-11] Self-import path analysis

**Finding:** The collator's self-import is NOT a divergent path requiring special handling.

In `cumulus/client/consensus/aura/src/collator.rs`, after `seal()` produces `BlockImportParams`,
`build_block()` sets `sealed_importable.additional_data = additional_data_recorder.take_data()`
**before** returning it. The caller (`build_block_and_import`) passes that same `import_block`
directly to `collator.import_block(import_block)`, which calls `self.block_import.import_block(...)`.

This goes through the standard parachain block import pipeline (the `ParachainBlockImport`
wrapper), which ultimately calls `sc-service`'s `client.rs` commit site
(`operation.op.set_additional_data(additional_data)`  —  added by todo 12 running in parallel).

**Slot-based path:** same pattern. `block_builder_task.rs` receives back `(built_block, mut import_block)`
from `build_block()`, overrides `import_block.additional_data = additional_data_recorder.take_data()`
(the external per-block recorder's blob), then passes `import_block` to `collator.import_block()`.
Same commit site reached — no divergence.

**Conclusion:** No separate self-import persistence route needed for todo 11. Both paths correctly
set `BlockImportParams::additional_data` and flow to the single commit site wired by todo 12.

## [TODO-11] Test block-building required relay-chain sproof — caught by real test run

**Symptom (found by running `cargo test -p cumulus-client-collator --lib` WITHOUT SKIP_WASM_BUILD):**
Both `v3_packing_when_additional_data_provided` and `v2_fallback_when_no_additional_data` panicked:
```
panicked at cumulus/test/client/src/block_builder.rs:226:49:
Pushes inherent: RuntimeApiError(... AbortedDueToTrap(... "wasm `unreachable` instruction executed" ...
 cumulus_pallet_parachain_system::pallet::Pallet::maybe_drop_included_ancestors ...
```

**Root cause:** `build_and_import_block()` called `client.init_block_builder_builder().build()` without
providing a `RelayStateSproofBuilder`. The parachain-system pallet's `set_validation_data` inherent
calls `maybe_drop_included_ancestors`, which panics when no `included_para_head` is set in the
relay sproof — it has no relay-chain context to reference.

**Fix (commit `56455de3a8d`):** Mirror the pattern from
`cumulus/client/consensus/aura/src/collators/mod.rs::sproof_with_parent_by_hash`:
```rust
let mut sproof = RelayStateSproofBuilder::default();
sproof.para_id = cumulus_test_client::runtime::PARACHAIN_ID.into();
sproof.included_para_head = Some(HeadData(genesis_header.encode()));
client.init_block_builder_builder().with_relay_sproof_builder(sproof).build()
```

**FUTURE TESTS using `init_block_builder_builder` MUST always supply a relay sproof with
`included_para_head` set, or the `set_validation_data` inherent will WASM-trap.**

## `additional_data_recorded_when_runtime_pushes` test failure — `Invalid(Call)` on unsigned extrinsic

**Commit where fixed:** 44925c19949 (amend of 098acdb1d52)

**Root cause:** `substrate_test_pallet::validate_unsigned` whitelists only four calls for
bare/unsigned submission: `deposit_log_digest_item`, `storage_change`, `read`,
`read_and_panic`. `push_additional_data` is NOT in that list — submitting it as an unsigned
extrinsic returns `TransactionValidityError::Invalid(InvalidTransaction::Call)`.

**Fix:** Use `ExtrinsicBuilder::new(call)` (signed by Alice, nonce=0) instead of
`ExtrinsicBuilder::new_unsigned(call)`. The signed path goes through `CheckSubstrateCall`
→ `validate_runtime_call`, which returns `Ok(Default::default())` for any call not
explicitly listed — so signed `push_additional_data` is accepted without issue.

**Lesson:** For substrate-test-runtime, prefer signed (`ExtrinsicBuilder::new`) unless the
specific call is in `validate_unsigned`'s whitelist. Do not assume "unsigned" means
"no validation" — the pallet has an explicit signed-only whitelist for unsigned submissions.

## [TODO-14] validate_block additional-data tests: root-cause CONFIRMED + final test names

**Root cause of the digest-count abort CONFIRMED** (`assertion left == right failed: Number of digest items must match that calculated. left: 3 right: 2`):
Pre-digest injection of `DigestItem::AdditionalData` into the block header cannot work. `frame_executive::execute_block` -> `final_checks` (`substrate/frame/executive/src/lib.rs:919-945`) asserts `header.digest().logs().len() == new_header.digest().logs().len()`. During `validate_block`, `verify_and_remove_seal` strips the seal, leaving the input header with `[PreRuntime, AdditionalData]`, but the runtime never deposits `AdditionalData` during execution (the flag is never set because no `push_additional_data` extrinsic ran), so the calculated header has only `[PreRuntime]` -> count mismatch -> abort.

**The fix (implemented):** the runtime must ACTUALLY push during execution. `cumulus-test-runtime`'s `test_pallet` now mirrors todo 8's `substrate_test_pallet` exactly: `AdditionalDataPushed<T>` bool flag, `push_additional_data` dispatchable (pushes `b"additional-data-test"`), and an `on_finalize` hook that calls `additional_data::finalize()` + `deposit_log(DigestItem::AdditionalData(hash))` when the flag is set. Blocks are built with the dispatchable as a real extrinsic, so the digest lands in the header during block building AND during validate_block's re-execution.

**Second bug found & fixed (implementation.rs):** the explicit `"additional data hash does not match header digest"` assertion sat AFTER `E::execute_verified_block`. But `frame_executive::final_checks` does a hard digest-item-equality assert INSIDE execution (`DigestItem must match that calculated.`), and the replayed blob's hash lands in the executed header, so a tampered blob always panicked there FIRST — the pinned explicit message was UNREACHABLE. Fixed by moving the explicit `assert_eq!(hash_blob(blob), expected_hash)` BEFORE execution (fail-fast; pure function of blob+header). This is the "specific bug" justifying the implementation.rs edit.

**Also required:** `cumulus/test/client/src/block_builder.rs` `init_block_builder` must register `AdditionalDataExt(RecordingAdditionalDataProvider::new())` alongside `ProofSizeExt`, otherwise building a block containing `push_additional_data` panics (`AdditionalDataExt extension not registered`). Registered unconditionally — harmless for blocks that never push (flag never set -> `finalize()` never called -> no digest, `take_data()` unused).

**Final 4 test names (all match `-- additional_data` filter):**
1. `validate_block_v3_with_additional_data_succeeds` — happy path (correct blob + runtime-deposited digest) -> SUCCEEDS
2. `validate_block_v3_tampered_additional_data_fails` — blob=`Some(vec![0u8; 32])`, header digest unchanged -> fails with `additional data hash does not match header digest`
3. `validate_block_v3_additional_data_digest_without_data_fails` — digest present, `additional_data: vec![None]` -> fails with `header has AdditionalData digest but no additional data provided`
4. `validate_block_v3_additional_data_without_digest_fails` — data present, no digest (block built without dispatchable) -> fails with `additional data present but header digest missing AdditionalData item`

## [TODO-16] GENUINE PRODUCTION BUG: requesting ADDITIONAL_DATA over sync breaks EVERY import (relay AND parachain)

**Severity:** HIGH. Breaks all syncing for any node in `ChainSyncMode::Full` (or non-storage `LightState`)
or archive mode that requests `BlockAttributes::ADDITIONAL_DATA` (wired by todo 12) - relay chains included.
Discovered by todo 16's e2e test; only a real in-process network could catch this (todo 9/12/13's unit tests
never exercised sync + empty-blob + guard together).

**Symptom (from the e2e test run):** relay validators and the collator's embedded relay repeatedly fail to
import every synced block:
```
sc_service::client::client: Block prepare storage changes error: Import failed:
additional data present but header digest missing or has multiple AdditionalData items
sync: 💔 Error importing block 0xd07e...: consensus error: Import failed: ... additional data present but header digest missing or has multiple AdditionalData items
```
The relay chain never syncs -> the collator never produces -> todo 16's `wait_for_blocks_or_timeout(&collator, 1)` times out (300s).

**Root cause chain (each step verified by reading source):**
1. `substrate/client/network/sync/src/strategy/chain_sync.rs:279-306` - `ChainSyncMode::Full.required_block_attributes`
   (and non-storage `LightState`) now OR in `BlockAttributes::ADDITIONAL_DATA` (todo 12). Applies to ALL full nodes, relay chains included.
2. Server side `substrate/client/network/sync/src/block_request_handler.rs:433-437`:
   `if get_additional_data { self.client.block_additional_data(hash)?.unwrap_or_default() }` - a block with NO
   stored data yields an EMPTY `Vec::new()` (proto field 10 = empty).
3. Client side `block_request_handler.rs:586-590`: `additional_data: if request.fields.contains(ADDITIONAL_DATA) {
   Some(block_data.additional_data) } else { None }` - the empty payload becomes **`Some(vec![])`, NOT `None`**.
   (Todo 9's own note flagged this: "old peer gives Some(vec![]) not None" - harmless for indexed_body because
   indexed_body is never guard-checked; NOT harmless here.)
4. `chain_sync.rs:1937` copies `block_data.block.additional_data` -> `IncomingBlock.additional_data`;
   `substrate/client/consensus/common/src/import_queue.rs` `verify_single_block_metered` copies it ->
   `import_block.additional_data = Some(vec![])`.
5. Todo 13's guard in `substrate/client/service/src/client/client.rs:872-879`:
   `match &import_block.additional_data { Some(blob) => { if additional_data_digest_count != 1 { REJECT } } }` -
   `Some(vec![])` is treated as "data present" -> digest_count == 0 -> REJECT with the pinned message.
   (The todo-15 non-executing-path check would reject `Some(vec![])` the same way.)

**Why empty can never be a real blob:** the canonical blob is `sp_additional_data::encode_items(...)` = a
SCALE-encoded `Vec<Vec<u8>>`, always >= 1 byte (an empty item-list encodes as the single byte `0x00`).
`DigestItem::AdditionalData` is `[u8; 32]`, never empty. So `vec![]` ⟺ "absent" is sound.

**Recommended minimal fix (root, single point):** in `blocks_from_schema` (`block_request_handler.rs:586-590`)
map empty -> None:
```rust
additional_data: if request.fields.contains(BlockAttributes::ADDITIONAL_DATA) {
    (!block_data.additional_data.is_empty()).then_some(block_data.additional_data)
} else {
    None
},
```
This keeps `Some(vec![])` from ever reaching `IncomingBlock`/`BlockImportParams`, so BOTH the todo-13
executing-path guard AND the todo-15 non-executing-path check stop misfiring on every no-data block.
Defense-in-depth alternative: normalize `Some(v) if v.is_empty()` -> `None` at the `verify_single_block_metered`
copy site in `import_queue.rs` or inside the client.rs guard. The boundary fix is preferred - it keeps the
invariant "Some means a real non-empty blob" everywhere downstream.

**Verification after fix:** `cargo test -p cumulus-test-service block_additional_data_end_to_end -- --exact --nocapture`
(worker binaries + WASM built). The e2e test exercises (a)-(e), including a real full-node sync of a no-data
control block (e) - exactly the path that was broken.

**Not fixed here:** per todo 16's guardrail ("If you discover a genuine production bug requiring production
code changes to make the test pass, STOP and document it precisely in the notepad rather than expanding scope -
report it as a separate finding"). The test + test-harness wiring (cumulus-test-service executor HostFunctions,
Cargo.toml dep) are in the working tree but UNCOMMITTED because the test cannot pass yet.

## [TODO-16] FIXED by commit be6b2029200 — sync empty-additional_data bug (previously reported above)

The empty->`Some(vec![])` sync bug is **FIXED** in `substrate/client/network/sync/src/block_request_handler.rs`
`blocks_from_schema`:
```rust
additional_data: if request.fields.contains(BlockAttributes::ADDITIONAL_DATA) &&
    !block_data.additional_data.is_empty()
{
    Some(block_data.additional_data)
} else {
    None
},
```
The companion test `old_peer_compat_field_absent_defaults_to_empty` was renamed to
`old_peer_compat_field_absent_yields_none` and now asserts `None` (it previously pinned the buggy
`Some(vec![])`). Verified: `SKIP_WASM_BUILD=1 SKIP_PALLET_REVIVE_FIXTURES=1 cargo test -p sc-network-sync
--lib block_request_handler::tests:: -- --nocapture` -> 4/4 pass. Commit message: `fix(sc-network-sync): treat
empty additional_data field as None in blocks_from_schema`.

## [TODO-16] SECOND production bug found by the e2e run - FIXED by commit 4d43adc59c1

**Symptom:** after the sync fix, the push block's header carried the correct `DigestItem::AdditionalData`
(the runtime deposited it) but the collator's DB had NO blob (`block_additional_data` == None).

**Root cause:** `sc_basic_authorship::Proposer::propose` (substrate/client/basic-authorship/src/basic_authorship.rs:300-301)
calls `.with_extra_extensions(extra_extensions).enable_additional_data_recording()` - the second call registers its
OWN `AdditionalDataExt(RecordingAdditionalDataProvider)` into the block builder, and `Extensions::register`
OVERWRITES by type-id. So the block builder used the proposer's recorder (todo 10), while the collator's
`additional_data_recorder` (cumulus/client/consensus/aura/src/collator.rs:306-310, registered into `extra_extensions`
with an `is_registered` check that passed because the proposer registers LATER) never accumulated anything ->
`take_data()` returned None -> `sealed_importable.additional_data = None` -> no DB storage.

**Fix:** `let additional_data = proposal.additional_data.or_else(|| additional_data_recorder.take_data());`
(collator.rs, in the shared `build_block` used by both the lookahead and slot-based paths). `Proposal.additional_data`
is populated by the block builder's `build()` from whatever recorder actually served the block. Verified:
`cargo test -p cumulus-client-collator --lib additional -- --nocapture` -> 2/2 pass
(`v3_packing_when_additional_data_provided`, `v2_fallback_when_no_additional_data`).

## [TODO-16] Reference-runtime change to make the push trigger pool-valid (in test commit 0f668b3110b)

`cumulus/test/runtime/src/test_pallet.rs`: `push_additional_data` previously called the `additional_data::push`
host function in its dispatch body, which PANICS on a missing extension; pool (re)validation has no
`AdditionalDataExt`, so the tx was Invalid/Dropped and could never be included by a real collator. Changed so the
dispatchable only sets the `AdditionalDataPushed` flag and `on_finalize` performs the push (`ADDITIONAL_DATA_SAMPLE`)
+ finalize + deposit. The public behavior (a block containing the dispatchable gets the digest) is unchanged -
todo 14's 4 validate_block tests still pass unchanged (`cargo test -p cumulus-pallet-parachain-system additional_data`).
Also added `ADDITIONAL_DATA_SAMPLE: &[u8]` const next to the flag storage.

## [TODO-16] FIXED by test commit 0f668b3110b - e2e test now GREEN (5/5 sub-assertions)

`cargo test -p cumulus-test-service block_additional_data_end_to_end -- --exact --nocapture` -> **exit 0**,
all of (a) collator DB, (b) full-node sync+import+DB, (c) validate_block V3, (d) corruption rejected, (e) control
block unaffected. Full log: /tmp/opencode/e2e_final.log. Requires the PVF worker binaries
(`cargo build --bin polkadot-execute-worker --bin polkadot-prepare-worker`) and real WASM build (no SKIP_WASM_BUILD).

## [FINAL-WAVE F2] FIXED by commit c36317707f7 — two code-quality defects

**Defect A** (`substrate/client/service/src/client/client.rs`, `prepare_block_storage_changes` `None` arm):
The `None` arm's error message was copy-pasted from the `Some(blob)` arm and FACTUALLY INVERTED. In the
`None` arm the guard fires when `additional_data_digest_count > 0` (header HAS a digest, NO blob) but the
message said "additional data present but header digest missing or has multiple AdditionalData items"
(which describes the opposite: blob present, digest absent). Fixed the message to
`"header has AdditionalData digest but no additional_data blob was provided on the executing import path"`.
`Some(blob)` arm left untouched.

**Defect B** (`substrate/client/consensus/common/src/import_queue.rs`, WarpSync early return in
`verify_single_block`): verified `block.additional_data` via `verify_additional_data_non_executing` but
constructed `BlockImportParams::new(block_origin, header)` WITHOUT threading the field — a future
`Some(blob)` would pass verification then be silently dropped (never persisted). Now sets
`import_block.additional_data = block.additional_data` before returning.

**Test pinned:** `additional_data_digest_without_blob_rejected_before_execute` previously asserted only
`msg.contains("additional data")`; strengthened to assert the new accurate message
(`msg.contains("header has AdditionalData digest but no additional_data blob")`). The `Some(blob)`-arm
test (`additional_data_blob_without_digest_rejected_before_execute`) still asserts `"additional data"` and
was NOT touched.

**Verification (all exit 0):**
- `SKIP_WASM_BUILD=1 SKIP_PALLET_REVIVE_FIXTURES=1 cargo check -p sc-service -p sc-consensus --all-targets --all-features`
- `SKIP_WASM_BUILD=1 SKIP_PALLET_REVIVE_FIXTURES=1 cargo test -p sc-service --lib -- additional_data` -> 4/4 pass
- `SKIP_WASM_BUILD=1 SKIP_PALLET_REVIVE_FIXTURES=1 cargo test -p sc-consensus --lib -- warp_ skip_exec_` -> 8/8 pass
- Scoped format via `rustfmt --edition 2021` on exactly the two files (no rustup/`cargo +nightly` in this env; no unrelated files reformatted).
