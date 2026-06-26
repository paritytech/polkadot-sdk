<!-- markdownlint-disable MD013 MD001 -->

### Changelog for `Node Dev`

**ℹ️ These changes are relevant to:**  Those who build around the client side code. Alternative client builders, SMOLDOT, those who consume RPCs. These are people who are oblivious to the runtime changes. They only care about the meta-protocol, not the protocol itself.


#### [#11701]: statement-store: Test flooding detection
Fixed a bug with double peer state cleanup. Added two zombienet integration tests that verify statement-store flooding detection

#### [#12082]: ParityDB: Do not skip storing on store + ref in same tx
Fixes a bug in the ParityDB adapter where a `store` + `reference` on the same
unknown key within a single transaction would skip storing the value entirely.

ParityDB internally transformed the reference into a dereference, making the value
disappear from the database after commit. The fix maps changes directly to
`parity_db::Operation` variants so the `Set` is always emitted regardless of other
operations in the same transaction.


#### [#11611]: statement-store: add concurrent and multi-peer propagation tests
Implements unit tests for "Propagation under normal load"

#### [#11918]: Invalidate storage transaction cache on state version change
Fix a cache-bleed bug. The cache stored the pre-computed storage root and transaction without
recording which `StateVersion` was used to build them.

The cache now stores the `state_version` it was built with and is invalidated and recomputed
whenever a different version is requested.

#### [#11456]: Collation generation: pass validation data from caller, fetch session from scheduling parent
Makes `handle_submit_collation` robust for V3 candidate descriptors where
the relay parent can be old/ancient (pruned state). Instead of fetching
`PersistedValidationData` and `session_index` from the relay parent's
runtime state (which may be unavailable), the caller now provides
`PersistedValidationData` directly in `SubmitCollationParams`, and the
session index is now fetched correctly from the scheduling parent (which
always has available state).

The `parent_head` field is removed from `SubmitCollationParams` — callers should set
`validation_data.parent_head` directly instead.

All runtime API calls in the submit collation path now correctly target the scheduling
parent rather than the relay parent.

`CandidateDescriptorV2::new_v3` now takes an explicit `scheduling_session_index`
parameter instead of hardcoding the offset to 0.


#### [#12232]: litep2p: bitswap metrics added
Adds four Prometheus metrics to the litep2p bitswap inbound handler so we can observe bitswap-driven load:

- `substrate_sub_libp2p_bitswap_entries_total` — counter, label `outcome` one of: `block_served`, `have`, `dont_have`, `unsupported_cid`.
- `substrate_sub_libp2p_bitswap_request_errors_total` — counter, label `reason` one of: `too_many_entries`, `client`.
- `substrate_sub_libp2p_bitswap_inbound_request_duration_seconds` — histogram.
- `substrate_sub_libp2p_bitswap_response_bytes_total` — counter.

`NetworkBackend::bitswap_server` gains an `Option<Registry>` parameter (mirroring `peer_store`). The libp2p backend impl accepts and ignores it — bitswap isn't wired into its request-response framework. Bitswap remains gated by `--ipfs-server`.

Labels are pruned vs paritytech/Polkadot-sdk#12083: `missing`/`bad_cid` outcomes and `decode`/`encode`/`invalid_wantlist` errors are absorbed by litep2p before reaching the shim.


#### [#11967]: Experimental collator protocol — leaf-authoritative capacity tracking
Reworks the experimental collator protocol's validator side around a leaf-authoritative
capacity model. The leaf's per-core claim queue is the single source of truth; capacity at
every scheduling parent (SP) on a path to that leaf is derived via offset arithmetic.

Behavior changes vs. master:

- **Rotation-boundary bug fixed.** After a group rotation, candidates for both the
  pre-rotation core's para (advertised at the pre-rotation leaf) and the post-rotation
  core's para (at the post-rotation leaf) are accepted. Previously, rotation caused
  claim-queue state for the old core to be overwritten and ancestor-rooted advertisements
  to be wrongly rejected.

- **Fork-aware view cleanup.** When a leaf is dropped, SPs are removed from
  `per_scheduling_parent` (with their in-flight fetches cancelled) only when no remaining
  leaf still references them. Previously, sibling-fork retention in implicit-view storage
  could leave SPs orphaned with leaked in-flight fetches.

- **Free-slot accounting tightened to the current leaf.** Assignments and free CQ
  positions now reflect only what the current leaf's CQ actually predicts. Previously,
  claim-queue state for already-produced ancestor blocks lingered until those blocks were
  pruned, so paras and slots that had already been backed on-chain were still reported as
  open — keeping peers connected past their relevance and offering capacity for slots that
  could no longer be filled.

Internals: the previous `ClaimQueueState` / `PerLeafClaimQueueState` machinery is gone
(~2200 LoC deleted). Capacity is now computed per-(leaf, core) by `build_leaf_core_cqs`
as a `Vec<LeafCoreCq>`, where each `LeafCoreCq` holds the leaf's CQ as
`Vec<Option<ParaId>>` (consumed positions cleared to `None`) and an
`sps_by_depth: Vec<Option<Hash>>` masking cross-core ancestors. The fetch planner walks
free CQ positions back-to-front and picks the best advertisement among same-core SPs whose
window reaches that position.

Test coverage in `validator_side_experimental/tests.rs`:
- `core_rotation_accepts_candidates_for_both_cores` — regression for the original
  rotation-boundary bug.
- Five active-fork tests covering: assignment union across leaves, longest-window capacity
  at a common ancestor across different-length forks, no double-counting of in-flight
  candidates across sibling forks, and reclamation of capacity / peer disconnection when a
  fork is dropped.
- Two linear multi-SP tests pinning over-fetch and under-fetch invariants when ancestor
  and leaf SPs share the same per-core CQ.
- Tests for cross-SP reputation arbitration and cross-core slot-reservation correctness.
- Three previously-existing tests rewritten to assert the same intent against the new
  model.

No public API changes; behavior change is contained to the experimental validator side.

#### [#11793]: Updated wasmtime to 36.0.7
Wasmtime 35.0.0 → 36.0.7


#### [#11673]: statement-store: Add malformed input unit tests
Add unit tests for malformed input handling in statement-store crates.


#### [#12086]: sc-client-db: add prefetched indexed transactions support
Adds `PrefetchedIndexedTransactions` to `BlockImportParams`, letting upstream
block-import wrappers feed indexed-transaction data into the backend out-of-band:

- `renew_payloads`: `(content_hash, bytes)` pairs consumed by `apply_index_ops`
  when a runtime-produced `IndexOperation::Renew` references a hash not yet
  present in the local `TRANSACTION` column.

- `ops`: synthetic `IndexOperation`s used when the runtime produced no index ops
  (e.g. gap-sync backfill). Runtime-produced ops always override when non-empty.


#### [#11641]: Fix runtime upgrade zombienet tests exceeding max_code_size
Zombienet runtime upgrade tests now use compact (`WASM_BINARY`) runtimes
instead of `WASM_BINARY_BLOATY`, which had grown beyond the relay chain's
`max_code_size` limit. A new `submit_sudo_runtime_upgrade` helper verifies
that the inner sudo dispatch succeeded and renders any dispatch errors using
runtime metadata (e.g. `ParachainSystem::TooBig` instead of raw byte indices).

#### [#11557]: Double max memory on block import
`CallContext::Onchain` now carries an `import: bool` field.
When `import: true` (block import), the WASM executor allocates double
the configured heap pages. When `import: false` (block building), behavior
is unchanged.

This is a breaking change for any code that constructs or matches on
`CallContext::Onchain`.

The `WasmModule::new_instance` trait method now requires a
`HeapAllocStrategy` parameter. The `WasmInstance` trait gains a new
`set_heap_alloc_strategy` method with a default no-op implementation.
WASM memory limits are no longer baked into the compiled artifact;
they are enforced per-instance at instantiation time via wasmtime
`StoreLimits`.


#### [#11962]: client/db, sp-transaction-storage-proof: preserve `MultiRenew` submit order
PR #11474 changed `DbExtrinsic::MultiRenew::hashes` from `Vec<DbHash>` to `BTreeSet<DbHash>`,
which reordered hashes by sort order. The runtime indexes `TransactionInfo` in dispatch
order, so `build_proof` and the runtime resolved `selected_chunk_index` to different
chunks, causing `InvalidProof` and chain halt on the first proof block after a multi-renew.

Reverted `MultiRenew.hashes` to `Vec<DbHash>`, preserving submission order and duplicates
so column refcount inc/dec stays symmetric.


#### [#11600]: collator-proto/metrics: Fix blindspot in collation fetch latency metrics
The current implementation of the `collation_fetch_latency` metric contains a critical observability blindspot due to insufficient histogram bucket resolution.


Currently, the the collation fetching is capped at an upper bound of 5 seconds. This effectively creates a black box for investigating latency events. Any fetch operation exceeding the 5s threshold is aggregated into the final bucket, regardless if the fetching took 30s or 1h. This obscures the true distribution of network delays and prevents accurate performance profiling for high-latency scenarios.

The discrepancy was identified with https://github.com/lexnv/block-confidence-monitor and confirmed via manual analysis of the logs. Without granular visibility into these outliers, we cannot effectively measure the success of our block confidence / debug bottlenecks in validator-side protocols.

While at it, have increased the granularity of other buckets which might be relevant.

Part of the block confidence work:
- https://github.com/paritytech/polkadot-sdk/issues/11377


cc @sandreim @skunert

#### [#11691]: statement-store: test crash mid-sync
Adds a zombienet integration test that verifies statement store recovery after a node crash mid-sync. The test submits statements concurrently to multiple nodes, kills one mid-gossip, submits more while it's down, then asserts all recoverable statements converge after restart.

#### [#11521]: Implement `bitswap_v1_get` RPC method
Implement `bitswap_v1_get` RPC method according to the [spec](https://github.com/paritytech/json-rpc-interface-spec/blob/main/src/api/bitswap_v1_get.md).

#### [#12219]: Bump jsonrpsee to 0.24.11
Bumps `jsonrpsee` and `jsonrpsee-core` to 0.24.11 to pick up the new `subscription_id()` getter on
  `PendingSubscriptionSink` (paritytech/jsonrpsee#1634).

#### [#11900]: Polkadot: Increase block size limit on the node side
This pull request increases the block size limit allowed while building a block. This block size limit is checked on the node side. The ultimate authority about the actual block size limit is the runtime.

#### [#10477]: Block Bundling Node Side
Implements the node-side logic for block bundling (aka 500ms blocks) in parachains.
The main changes are in the slot-based collator: instead of building one block per core,
blocks are built as requested and distributed over the available cores.


#### [#12038]: Network: Add bitswap client
Adds a Bitswap client API to `sc-network` for fetching CID-addressed blocks
from a peer over the native litep2p Bitswap protocol. The public API is
block-fetch only and reports per-CID outcomes as either delivered bytes or
"missing"; on-wire presence (`HAVE` / `DONT_HAVE`) is handled internally
and is not part of the public surface.


#### [#11237]: Add statement store e2e integration tests
Add 2 integration tests and 2 bench tests for the statement store subsystem:
  - **statement_store_basic_propagation**: basic submission and cross-node propagation via genesis-injected allowances
  - **statement_store_check_propagation_and_quota_invariants**: concurrent multi-account propagation, quota enforcement, and priority eviction via sudo allowances
  - **statement_store_memory_stress_bench**: memory usage under extreme load
  - **statement_store_latency_bench**: submission and propagation latency measurement
Add `spawn_network_with_injected_allowances()` helper for per-participant quota configuration.

#### [#12084]: Allow calling `runtime_api` with supplied storage overlay
This change allows users of `client.runtime_api()` to provide a custom overlay with changes before calling further runtime APIs.

#### [#11415]: Statement store e2e integration tests lite person setup
## Summary

- Add a lite person registration test that exercises the full PeopleLite::attest flow on a live parachain, including ring-VRF key generation, attestation, and subsequent statement submission.

#### [#11480]: Refactor statement store index locking: stage 1
Optimize statement store index operations by splitting the index into separate read and write indexes

#### [#12282]: Logging improvements for the collator revamp
Some logging updates:

- Decrease log levels in `wait_for_first_leaf` to DEBUG, to avoid startup spam
- Use `warn_if_frequent` for fetch errors
- Log assignment changes on view change
- pick_best_advertisement: trace logs for each outcome
- update_view: log scheduling parent <-> assigned core mapping
- update_view: log sp removal
- handle_seconded_collation: logs for each error case
- PeerManager: log reputation updates

Partially addresses https://github.com/paritytech/polkadot-sdk/issues/10402

#### [#12314]: approval-voting: cleanup coalescing logic
Refactor the approval coalescing logic so that the runtime `ApprovalVotingParams` are read
through `ExtendedSessionInfo` (cached per session) rather than via a separate runtime API
call. The change is fully backwards compatible with the previous behaviour and was validated
with a mixed deployment of old/new nodes against old/new runtimes.


#### [#12207]: statement-store: remove unused `Proof::OnChain` variant
Removes the `Proof::OnChain` variant from `sp_statement_store::Proof` because it
was unused.


#### [#11381]: Fix slot-based collator panic during warp sync (#11072)
When a parachain collator starts with `--authoring=slot-based` and performs warp sync, the `slot-based-block-builder` essential task immediately calls `slot_duration()` which requires `AuraApi_slot_duration`. During warp sync the runtime isn't ready, so this fails and the task returns, shutting down the node.

The lookahead collator avoids this by calling `wait_for_aura()` before starting. This PR adds an equivalent guard to the slot-based collator.

### Manual test
Before the fix the collator panicked after the relay chain warp sync with AuraApi_slot_duration not available, which does not occur anymore now.
```
 ./target/release/polkadot-parachain \
    --chain asset-hub-polkadot \
    --sync warp \
    --authoring=slot-based \
    --tmp -- --sync warp
```
Closes #11072.

#### [#12163]: RPC: support keccak-256 in `bitswap_v1_get`
Support Keccak-256 hash for Parity with transaction storage pallet in Bulletin chain.

#### [#11820]: Fix statement-distribution request cleanup on leaf deactivation
`handle_deactivate_leaves` in statement-distribution v2 was cleaning up outgoing
attested-candidate requests using the wrong key: it called
`remove_by_scheduling_parent(*leaf)` once per pruned ancestor, instead of
`remove_by_scheduling_parent(pruned_rp)` once per pruned relay parent. As a result,
requests tied to ancestors pruned alongside a deactivated leaf were never cleaned up,
leaking entries in the `RequestManager`.

The bug was introduced in #1436 (vstaging rework, 2023) and was latent because the
stale entries only caused gradual memory growth and wasted retry cycles, not
correctness failures.

Also adds unit tests covering the full cleanup contract of `handle_deactivate_leaves`
(implicit view pruning, `per_scheduling_parent`, request manager, `per_session`,
and the "last session's topology is retained" edge case).

#### [#12453]: Fix duplicate statement notifications for multi-topic MatchAll subscriptions
Fixes a bug in the statement store subscription matcher where a multi-topic `MatchAll`
subscription could be notified of the same statement multiple times. Because such a
subscription is registered under each of its topics, the topic-combination loop in
`notify_match_all_subscribers_best` could select it across several combinations and
deliver the same statement once per combination. The matcher now tracks already-notified
subscriptions per statement (mirroring the `MatchAny` path), delivering each statement at
most once per subscription. This also avoids needlessly consuming a subscriber's bounded
channel capacity, which could otherwise lead to premature auto-unsubscription.

#### [#11474]: Support multiple `IndexOperation::Renew` calls within a single extrinsic index in `sc-client-db`
Previously `apply_index_ops` used `HashMap<u32, DbHash>` so multiple `Renew`
operations at the same extrinsic index silently overwrote each other, blocking
batch-renewal inherents (e.g. the Bulletin chain's `process_auto_renewals`).

A new `DbExtrinsic::MultiRenew` variant carries all renewed hashes for an
extrinsic; single-renewal extrinsics still produce `DbExtrinsic::Indexed`
(backwards-compatible). Downstream consumers reading `BODY_INDEX` directly
must handle the new variant; users of the standard `BlockchainDb` APIs do not.

#### [#11274]: Statement store: Latency bench cli sudo
Extend the statement-latency-bench CLI tool to automatically set up per-account statement allowances via sudo extrinsics.

Added a separate `setup-allowances` binary that issues subxt-based `Sudo(batch_all(set_storage(...)))` calls to write `:statement_allowance:` keys for each benchmark account. Extracted shared subxt configuration and test utilities into `subxt_client` and `test_utils` modules gated behind the `test-helpers` feature.

#### [#11387]: docs/async_back: Align docs to latest behavior
Tiny PR to update the async backing documentation / guidelines:
- older params are no longer used `async_backing_params`
- UNINCLUDED_SEGMENT_CAPACITY is now 3
- no need for `experimental` feature flags


Closes: https://github.com/paritytech/polkadot-sdk/issues/8804

#### [#11745]: statement-store: fix unbounded growth of the evicted-hashes map
Fix unbounded growth of the evicted-statement map used for re-gossip suppression.
Naturally-expired statements are no longer inserted into the map, and the map is
now bounded to `max_total_statements` entries with deadline-ordered purging.

#### [#12113]: Aura: Fetch slot duration at parent and not best block
Fetch the slot duration from the parent we are building on top of and not from the best block.

#### [#12212]: node: introduce shared `Clock` abstraction and apply to four subsystems
Introduces a new `polkadot-node-clock` crate exporting a unified `Clock`
trait (`now` / `delay` / `duration_since_epoch`) plus a production
`SystemClock` implementation and a feature-gated `MockClock` for tests.
Migrates four subsystems off their ad-hoc per-crate clock traits and
onto the shared abstraction:

- `polkadot-collator-protocol`: full migration of all time reads
  (production paths use the shared `SystemClock`; the crate-level
  `clippy.toml` forbids direct `Instant::now`, `SystemTime::now`,
  `tokio::time::sleep`, `futures_timer::Delay::new`, and
  `sp_timestamp::Timestamp::current`).
- `polkadot-node-core-chain-selection`: replaces the local `Clock` /
  `SystemClock` with the shared crate.
- `polkadot-node-core-av-store`: replaces the local `Clock` /
  `SystemClock` with the shared crate. Drops the `Error::Time`
  variant; reads via the shared trait are infallible.
- `polkadot-node-core-dispute-coordinator`: replaces the local
  `Clock` / `SystemClock` (in `status.rs`) with the shared crate.

Known gap (documented in `polkadot-node-clock`'s module docs): the
chain-selection, av-store, and dispute-coordinator subsystems still
call `futures_timer::Delay::new` directly for their internal timers
(their previous `Clock` traits did not cover delays either). Those
sites are not yet routed through the shared trait and must be
migrated before those subsystems can run under a deterministic test
harness. The collator-protocol subsystem routes all its time reads
through the shared `Clock`.

The approval-voting / approval-distribution stack has its own
tick-aligned clock layer; that migration is deferred to a follow-up
PR.

No production behaviour change.


#### [#11329]: statement-store: Allow light clients to specify topic affinity
Adds explicit topic affinity to the statement protocol via a new "statement/2" protocol.
Peers can advertise which topics they care about using a bloom filter. Only matching
statements are forwarded to peers with an active affinity filter. When affinity changes,
relevant statements are re-sent. Light clients must advertise affinity before receiving
statements. Includes rate limiting on affinity advertisements.

#### [#12049]: net: Update litep2p to v0.14.0
This PR updates litep2p to the latest release.

- multihash needs a new const generic
- multihash erorr is opaque without public variants
- litep2p re exports no longer have the `Identity` variant, which is handled via a local constant

Litep2pProtocol and Libp2pProtocol now are deduced to the same type, therefore the LiteP2pProtocol to/from Protocol is no longer needed.  Small other changes revolve around the fact that Protocol::p2p now contains a peerID rather than a multihash.

#### [#10742]: Add V3 scheduling validation for parachains
Collators now build V3 scheduling proofs and respect runtime-configured `MaxClaimQueueOffset`.
Backwards compatible: falls back to default offset of 1 if runtime doesn't implement the API.


#### [#12432]: cargo: Bump litep2p to 0.14.3
This patch release is dedicated entirely to strengthening the WebRTC transport layer, specifically focusing on connection resilience and build stability.

### Fixed

- fix(webrtc): decode errors during opening phase are not recoverable  ([#622](https://github.com/paritytech/litep2p/pull/622))
- fix(webrtc): build vendored OpenSSL for str0m  ([#620](https://github.com/paritytech/litep2p/pull/620))
- fix(webrtc): time out inbound and outbound opening data channels  ([#617](https://github.com/paritytech/litep2p/pull/617))

#### [#11838]: statement-store: fix metrics accuracy
# Description

Fixes accuracy gaps and clarify help text in statement store and network metrics.

## Integration

No integration needed.

#### [#11650]: Update PolkaVM to latest version (0.31 → 0.33)
Updates all PolkaVM dependencies from 0.31.0 to 0.33.0.


#### [#11893]: Raise relay-chain BlockLength to 10 MiB on test runtimes; decouple AttestedCandidate response cap
`polkadot-node-network-protocol` decouples the libp2p response-size cap for the
`AttestedCandidateV2` request-response protocol from `polkadot_primitives::MAX_CODE_SIZE`
and pins it at a flat 8 MiB.

Adds an end-to-end test in `polkadot-test-service`.


#### [#11107]: Add runtime metadata detection for Aura authority ID
Adds optional runtime metadata-based detection for Aura authority ID types in `polkadot-omni-node-lib`.

When metadata is available, the node detects whether Aura uses `sr25519` or `ed25519`
directly from the runtime. If detection fails or metadata is unavailable, it falls
back to chain spec ID heuristics.

A warning is emitted when the Aura authority ID type is not found in metadata.

#### [#11628]: fix: slot-based collator node exits immediately on startup
Regression from #11381: the `wait_for_aura` init wrapper was spawned as an essential task, but it
completes immediately after spawning the actual long-running collator tasks, causing the node to
shut down. Use `spawn_handle()` instead. Only affects `--authoring=slot-based` collators.

#### [#11685]: Pass NodeExtraArgs through Dev Chain
Enables the statement store when running `polkadot-omni-node` in dev mode with
`--enable-statement-store`.

The dev Aura node path now wires the statement store the same way as the regular node path:
it registers the statement network protocol, builds the store, exposes the RPCs, and passes
the statement store extension into offchain workers.

Also wires up the storage monitor in dev mode and emits warnings when
collation-specific flags (`--authoring-policy`, `--export-pov`,
`--max-pov-percentage`) are passed in dev mode where they have no effect.

`NodeExtraArgs` is now destructured in `start_dev_node` so the compiler
will enforce that any newly added fields are explicitly handled.

#### [#11662]: HOP: ephemeral data pool service for Substrate nodes
Adds the `sc-hop` crate implementing the Hand-Off Protocol (HOP)
node service. HOP enables peer-to-peer data sharing when recipients are
offline by holding data in a temporary node-side pool until it is either
claimed or promoted to on-chain storage via the `HopRuntimeApi` runtime
API defined in `sp-hop`.

**`sc-hop`** provides:
- Disk-backed data pool (`HopDataPool`) with 256-way sharded blob/meta storage
  and crash-recovery from disk on restart.
- RPC interface: `hop_submit`, `hop_claim`, `hop_ack`, `hop_poolStatus`.
- Submission gated by two runtime API calls: `HopRuntimeApi::max_promotion_size`
  sets the per-submission size cap (authoritative; no separate node-side
  ceiling), and `HopRuntimeApi::can_account_promote(who, data_len)` defines
  the per-account authorization policy.
- Recipient fan-out capped at `MAX_RECIPIENTS` (256) via a type-level
  `BoundedVec` (`RecipientVec`) so oversized lists are rejected before
  SCALE decoding and signature verification. Duplicate recipients are also
  rejected.
- Per-user quota plus bounded per-recipient metadata overhead charged
  against both pool capacity and per-user usage to close the
  metadata-amplification DoS.
- Domain-separated signing contexts for submit / claim / ack
  (`hop-submit-v1:`, `hop-claim-v1:`, `hop-ack-v1:`) so a signature issued
  for one operation cannot be replayed as another.
- Per-account submit rate limiter (`RateLimiter`): two token buckets per
  sender (requests + bandwidth), generous defaults sized for chat media.
- Background maintenance task (`HopMaintenanceTask`) that promotes near-expiry
  entries to on-chain storage, garbage-collects expired ones, and evicts
  stale per-sender rate-limit state.

Integration requires flattening `HopParams` into your CLI, constructing a
`HopDataPool`, and merging `HopRpcServer` into the RPC module. See `sc-hop`
crate docs for a step-by-step guide.

No runtime changes are required to deploy an HOP-enabled node:
`HopRuntimeApi` support is detected dynamically at startup via
`ApiExt::has_api_with`, and runtimes that do not implement it cause the
maintenance task to fall back to cleanup-only (no promotion). This lets
operators roll out HOP nodes ahead of the runtime upgrade that adds the API.


#### [#12080]: fix(statement-network): split peer metric by kind
Impl #12060

## Summary

- Change `substrate_sync_statement_peers_connected` from a plain gauge to a `GaugeVec` labelled by `kind`
- Report statement protocol peers as `kind="full"` or `kind="light"`

#### [#11617]: statement-store: add channel replacement logic tests
# Description
Implement unit tests for "Channel replacement: verify only higher-priority statements replace existing entries, corner cases of replacement logic." #11534

## Summary
- `channel_replacement_only_higher_priority_succeeds` -> verifies lower/equal priority rejected with `ChannelPriorityTooLow`, higher priority replaces, one-per-channel invariant preserved

- `channel_replacement_with_size_increase_evicts_others` -> verifies that replacing a channel message with a larger one triggers  additional eviction of lowest-priority non-channel statements to satisfy size constraints

#### [#11892]: statement-store: reduce sync burst interval
Reduces `INITIAL_SYNC_BURST_INTERVAL` from 100 ms to 10 ms. With many light clients connected, this interval determines how quickly a light client syncs and observes statements of interest. Other flows are unaffected because the polling branch is already at the end of a `select!` biased toward higher-priority work.

#### [#11615]: statement-store: add eviction priority ordering tests
# Description
  Implement unit tests for "Eviction: verify the lowest-priority statements are evicted first, corner cases of priority ordering" #11534

## Summary
  - Extend the existing `constraints()` test with eviction priority ordering corner cases:
    - Verify that equal priority statements are rejected with `AccountFull` when the account is full
    - Verify that specific evicted statement hashes appear in the expired map

#### [#10468]: Publish indexed transactions with BLAKE2b hashes to IPFS DHT
Add `--ipfs-bootnodes` flag for specifying IPFS bootnodes. If passed along with `--ipfs-server`, the node will register as a content provider in IPFS DHT of indexed transactions with BLAKE2b hashes of the last two weeks (or pruning depth if smaller).

#### [#10757]: rpc-server: Use own thread pool for RPC functionality
Right now the RPC is using the same thread pool as the rest of the node. When there is high usage and the node is running out of threads for blocking futures, RPC calls start to take very long time. This may also results in problems with other node functionality that would also be blocked by waiting for new threads. This pull request assigns the rpc server its own thread pool that gets the same number as threads as `max_connections`. These threads are only started on demand, but should allow any RPC connection to have at least one thread to run blocking tasks.

In a next step we should finally look into the performance metering of RPC calls and ensure that we have some proper rate limit in place to give every connection a fair share.


Hopefully helps with: https://github.com/paritytech/polkadot-sdk/issues/10719


### Changelog for `Runtime Dev`

**ℹ️ These changes are relevant to:**  All of those who rely on the runtime. A parachain team that is using a pallet. A DApp that is using a pallet. These are people who care about the protocol (WASM, not the meta-protocol (client).)


#### [#12225]: pallet-revive: map account in prepare_dry_run
## Summary
- Map the origin account in `prepare_dry_run` when it is not already mapped, so dry-running contract calls/instantiations does not fail with `AccountUnmapped` for callers that never registered a mapping.

## Test plan
- [ ] CI passes

#### [#11761]: asset-conversion-pallet: quote respects minimum balance
Quote functions (`quote_price_exact_tokens_for_tokens`, `quote_price_tokens_for_exact_tokens`)
now return `None` when the computed output exceeds what the pool can actually withdraw while
staying alive.

Swap execution withdraws from pools with `Preserve` preservation, meaning the pool must retain
at least `min_balance`. The quote functions did not account for this, so they could return a
price for a swap that would fail at execution.

Both quote functions now check the output against `reducible_balance(Preserve, Polite)` — the
same preservation level the swap uses.


#### [#11700]: Redirect XCM delivery fees to DAP / DAP satellite on Westend chains
Route XCM delivery fees to DAP satellite (or DAP on Asset Hub) instead of treasury on all Westend system chains.

#### [#12144]: allow Root-originated nested CREATE
# Allow Root-originated nested CREATE in pallet-revive

Closes paritytech/contract-issues#279.

## Motivation

`pallet-revive`'s exec stack rejects `Origin::Root` at any constructor frame, which means `bare_call(RuntimeOrigin::root(), contract_addr, ...)` errors with `RootNotAllowed` the moment the called contract reaches a `CREATE`/`CREATE2` opcode — even though the contract itself is the semantic instantiator.

The historical reason for the block was that the origin had to fund the new contract's ED. Since the PGAS rework, the ED is freshly minted by `T::Deposit::init_contract` and immediately deactivated for issuance accounting, so the origin no longer needs to pay it.

## Change

- Remove the explicit `RootNotAllowed` check at the start of the constructor frame in `exec.rs`.

Root is still **not** allowed to instantiate directly: `instantiate`/`bare_instantiate` continue to gate on `T::InstantiateOrigin::ensure_origin` (default `EnsureSigned` → `BadOrigin`). The change only unblocks the case where another contract sits between Root and the new contract and acts as the instantiator. Giving Root its own contract-address attribution is intentionally out of scope.

## Test plan

- Existing `root_cannot_instantiate{,_with_code}` and `root_can_call` continue to pass — direct Root instantiation is still rejected at the dispatchable layer, and Root-originated calls remain functional.
- Full `pallet-revive` test suite is green.

#### [#11860]: pallet-revive: align eth_Substrate_call origin check with other eth dispatchables
### Summary
Adds `ensure_non_contract_if_signed` to `eth_substrate_call`, matching `eth_call` and `eth_instantiate_with_code`.

#### [#11806]: [Staking] Refactor reward mode selection to use storage
Refactor of the payout path in `pallet-staking-async` to select the reward mode
(DAP pot vs. legacy minting) based on the `DisableMintingGuard` storage value
instead of checking for the existence of the era's reward pot account.

#### [#11807]: PSM init: skip assets with mismatched decimals instead of panicking
Replaces the `assert!` in the PSM `InitializePsm` migration with a log and skip
when an asset's decimals don't match the stable asset. Panicking in a runtime
upgrade bricks the chain. Migrations must be infallible.

#### [#11507]: [asset-hub-westend] Add revive_debug cfg for DebugEnabled
`debug_trace*` RPCs (`debug_traceTransaction`, `debug_traceBlockByNumber`, `debug_traceCall`)
only work when pallet-revive's `DebugEnabled` config is set to `true`. Currently only the
dev-node has this enabled, but the dev-node is a simplified environment that doesn't fully
replicate parachain runtime behavior (e.g. no PoV deduplication). To get accurate debug
tracing data, it needs to run on actual parachain runtimes.

This PR uses a plain `cfg` flag so debug mode can be toggled at build
time without code changes.
Build with:
```bash
RUSTFLAGS="--cfg revive_debug" cargo build -p asset-hub-westend-runtime --release
```

Other runtimes can adopt the same pattern by using
`ConstBool<{ cfg!(revive_debug) }>` for `DebugEnabled`.

#### [#11354]: Snowbridge: API to Check Inbound Nonce Consumption
Adds a runtime API so off-chain callers can check whether an inbound message from Ethereum (by nonce) has already been relayed or consumed on Bridge Hub.

#### [#11581]: Support State Overrides in Tracing
# Description

Adds support for [Geth-compatible state overrides](https://geth.ethereum.org/docs/interacting-with-geth/rpc/objects#state-override-set) in `debug_traceCall`, extending the state override support introduced in #11545 (which added them to `eth_call`).

Per the [Geth specification](https://geth.ethereum.org/docs/interacting-with-geth/rpc/ns-debug#debugtracecall), `debug_traceCall` accepts a config object that is a superset of the base tracer config, adding `stateOverrides` for ephemerally modifying account state during traced execution.

## Changes

### Pallet (`pallet-revive`)

- **`TracingConfig` type** — New backwards-compatible config type following the same pattern as `DryRunConfig` from #11545 (custom `Decode` impl, append-only fields, must be the last runtime API argument).
- **`trace_call_with_config` runtime API** — New method that applies state overrides then delegates to the existing `trace_call`. Implemented in the macro so it can call `Self::trace_call` directly.
- **`state_overrides` module** made `#[doc(hidden)] pub` so the macro-generated code can access it from downstream runtime crates.

### ETH-RPC (`pallet-revive-eth-rpc`)

- **`TraceCallConfig` type** — Extends `TracerConfig` (flattened) with an optional `stateOverrides` field, matching Geth's `TraceCallConfig` schema.
- **`debug_traceCall`** signature updated to accept `Option<TraceCallConfig>`. When state overrides are present, the RPC uses `trace_call_with_config`; otherwise it falls back to `trace_call` for backwards compatibility with older runtimes.

## Integration

Existing `debug_traceCall` callers are unaffected — the config parameter remains optional, and omitting `stateOverrides` uses the original code path. Callers wanting state overrides pass them in the config object alongside the tracer settings:

```json
{
  "tracer": "callTracer",
  "stateOverrides": {
    "0x1234...": {
      "balance": "0xDE0B6B3A7640000",
      "code": "0x6080..."
    }
  }
}
```

## Review Notes

- `TracingConfig` mirrors `DryRunConfig`'s backwards compatibility strategy documented in #11545. The custom `Decode` impl defaults missing fields, and `sp_api`'s `Decode::decode` (not `decode_all`) discards trailing bytes from newer encodings.
- The macro impl applies overrides before delegating to `Self::trace_call`, keeping the tracing logic in one place.
- A single integration test (`test_state_override_trace_call`) verifies end-to-end functionality using alloy's `DebugApi::debug_trace_call_callframe` with state overrides.

#### [#11804]: Runtime safety and fee precision fixes
This PR applies a set of small runtime-facing fixes across FRAME pallets.

## `pallet-asset-conversion`
- Changes `Config::LPFee` from `Get<u32>` (tenths-of-a-percent encoding) to `Get<Permill>`.
- Reworks swap math to use `Permill::ACCURACY` and `left_from_one()` so fee handling is explicit and avoids underflow-prone arithmetic.
- Updates runtimes and mocks to configure LP fee via `Permill`.

## `frame-system`
- Extends `Event::CodeUpdated` to include the updated runtime code hash: `CodeUpdated { hash }`.
- Emits this hash from `update_code_in_storage` when scheduling or applying a runtime code update.

## `pallet-psm`
- Adds a `try-runtime` `pre_upgrade` guard that checks configured asset decimals before initialization.

## `pallet-balances`
- Removes a tautological internal assertion in account mutation flow.

## `pallet-tips`
- Hardens tip payout by returning `NoActiveTippers` when all recorded tippers become inactive instead of indexing an empty tip set.
- Adds a regression test for the `NoActiveTippers` close-path.

#### [#11716]: Add legacy NegativeImbalance support to DAP and DAP satellite
Add `DapLegacyAdapter` and `DapSatelliteLegacyAdapter` wrapper structs that implement `OnUnbalanced<NegativeImbalance>` from the legacy `Currency` trait, bridging pallets not yet migrated to fungible traits.

Wire Westend runtimes: AH referenda slash to DAP, collectives (fellowship, ambassador, alliance) and people identity slash to DAP satellite.

Closes #11704.

#### [#12196]: [pallet-assets-precompiles] saturate permit/approval allowance
## Summary

`approve()` / `permit()` previously reverted with `"Balance conversion failed"` on `call.value > Balance::MAX` — including `type(uint256).max`, the universal "infinite allowance" idiom hard-coded by MetaMask, Uniswap, and every mainstream DEX router. The U256 → Balance conversion now saturates at `Balance::MAX`. `transfer` / `transferFrom` keep the existing revert-on-overflow behavior — they move exact amounts, so silently clamping would produce partial transfers the caller never asked for. The emitted `Approval` event still carries the raw `call.value`, matching the ERC-20 / OpenZeppelin convention.

## Non-goals

No `transferFrom` sentinel branch — saturated allowances decrement on every spend; on a `u128` runtime `Balance::MAX` is large enough to be operationally indistinguishable from infinite. No `allowance()` readback lift — returns the stored value as-is.

## Test plan

- [x] `cargo test -p pallet-assets-precompiles`

#### [#12012]: Allow Emergency origin to set PSM max debt
- `PsmManagerLevel::can_set_max_psm_debt` now returns `true` for both `Full` and
  `Emergency`.
- Updates the `ManagerOrigin` doc comment to reflect the expanded Emergency
  capabilities.
- Flips the `emergency_origin_cannot_set_max_psm_debt` unit test to
  `emergency_origin_can_set_max_psm_debt`.

When the max PSM debt is reached, minting is blocked and the internal stablecoin
can depeg to the upside. Arbitrageurs can no longer deposit external stablecoins
to mint and sell internal above peg, so demand pressure has nowhere to relieve.

Allowing the Emergency origin to raise the ratio restores the arbitrage path.

#### [#12125]: ec-utils: handle twisted Edwards z=0 results via IntoAffineSafe instead of panicking
On incomplete twisted Edwards curves such as Bandersnatch, HWCD arithmetic
fed non-subgroup inputs can produce projective points with `z = 0`. These
have no affine representative, and arkworks' standard
`From<Projective> for Affine` panics on `z.inverse().unwrap()` for the
F-exception shapes that miss the `is_zero()` short-circuit.

The `mul_te` and `msm_te` helpers in `sp-crypto-ec-utils` previously
called `.into_affine()` unconditionally and would crash on such inputs.
They now go through a new `IntoAffineSafe` trait that returns `None` on
`z = 0`, and surface the degenerate case across the FFI boundary as a
new `Error::DegeneratePoint` (numeric code `4`) rather than substituting
a sentinel inside the shared helper. The wire format on the FFI boundary
is unchanged: still `ArkScale<TEAffine>`, no sentinel bit, no new codec.

The fallback policy is per-curve, decided in the runtime-side hook:

  - **Bandersnatch** (incomplete TE form): the `HostHooks` impl catches
    `Err(DegeneratePoint)` for both `msm_te` and `mul_projective_te` and
    substitutes the all-zero projective point `(0, 0, 0, 0)`. This is
    not a valid curve point: it has no affine representative (`z = 0`)
    and any downstream validity or `is_in_correct_subgroup_*` check on
    the result rejects it rather than silently accepting an
    identity-like value. The Bandersnatch `mul_projective_te` hook also
    short-circuits locally with the same all-zero projective fallback
    when handed a `z = 0` projective *input* that can't be serialized
    for the host at all.
  - **Other TE curves** (e.g. `ed_on_bls12_377`, which is complete by
    construction so `z = 0` cannot occur on subgroup-valid arithmetic):
    the existing `.expect(FAIL_MSG)` propagates the error as a panic,
    which is correct since it should never fire.

Honest callers that subgroup-validate inputs upstream never reach the
degenerate branch in the first place.

#### [#11460]: [pallet-assets-precompiles] add foreign assets instance to kitchensink
## Summary

- Set `CallbackHandle = (pallet_assets_precompiles::ForeignAssetId<Runtime, Instance1>,)`
  in `pallet_assets::Config<Instance1>` for the kitchensink runtime.
- Asset creation (`create`, `force_create`) now automatically populates a sequential
  foreign asset index mapping. Asset destruction cleans it up.

## Test plan

- [x] Run [end-to-end tests](https://github.com/paritytech/evm-test-suite/pull/142) (requires Substrate-node, eth-rpc, node, cast)
- [x] Revert CallbackHandle to `()` and confirm end-to-end tests fail

#### [#11573]: Fix can_inc_consumer check blocking session key rotation in pallet_session
Check consumer capacity only when we  actually increment the consumer count (first-time local registration or external-to-local transition), not on key rotation.

#### [#11796]: Removed OpenGov pallets from Westend relay chain post-AHM
Removed pallet_conviction_voting, pallet_referenda, pallet_custom_origins
and pallet_whitelist from the Westend relay chain runtime. Post-AHM, governance
lives on AssetHub.

#### [#11755]: Bump ark-vrf to 0.5.0
Bumps `ark-vrf` from 0.2.2 to 0.5.0. The Bandersnatch VRF primitives in `sp-core` are updated
to use the new API. Plain sign/verify now uses thin VRF proofs instead of IETF proofs with a
dummy input. VRF sign and ring VRF sign/verify use the new `VrfIo`-based interface. No changes
to the public `sp-core` traits (`VrfSecret`, `VrfPublic`, `RingVrfSign`, `RingVrfVerify`).

#### [#11522]: eth-rpc: Add trace logging for receipt lookup debugging
Add trace logging to receipt handling to help diagnose intermittent receipt retrieval failures for finalized transactions, observed while running revive differential test benchmarks.

#### [#3520]: Add virtualization host functions
This PR adds experimental support for the virtualization host functions. Those allow the runtime to spawn and run PolkaVM instances. It is experimental because the behaviour is subject to change until PolkaVM and the host functions have a spec. However, we need to merge the code to go on with development. Docs and tests are all there and hence I argue it is good enough to be merged. I added a note that users should not use those functions in production.

This PR adds or changes the following components:

- `sc-executor-wasmtime`: Just exposing our virtualization manager to host functions. Needs to be added here to be available for the whole lifetime of a runtime call.
- `sp-virtualization`: New crate that abstracts away the host functions. Meaning that a user (like pallet-contracts) will interface only with this crate and not with the host functions directly. This is necessary so that the natively running test code still works. The host functions also depend on this crate. Those also contain all the tests. Everything PolkaVM is neatly organized into one crate. It also contains the definition of the new host functions.
- `sp-wasm-interface`: We  added an interface mirroring the host functions here. This is necessary in order for the host functions to be able to call into the executor.

#### [#11999]: pallet-staking-async: Use offence era for proportional slash distribution
Fixes the proportional slash split between active stake and unlocking chunks.
The ledger now uses the offence era (not the slash application era) to decide
which unlocking chunks are still in range, restoring the intended proportional
distribution. No funds previously escaped slashing — the active balance was just
taking a disproportionate share.

#### [#11694]: Make pallet_xcm_bridge_hub_router exporter configurable for paid/unpaid
Add `type Exporter: SendXcm` to the pallet's Config trait so runtimes can choose between `SovereignPaidRemoteExporter` (paid) and `UnpaidRemoteExporter` (unpaid) bridging. Provide convenience type aliases `PaidRemoteExporter` and `UnpaidRemoteExporterAdapter`.
Asset Hub runtimes are configured to now use `UnpaidRemoteExporter` to reduce deployment complexity
(no more need to manage/top-up AH sov account on BH).

#### [#11590]: Add asset-conversion precompile
Adds a precompile that exposes pallet-asset-conversion (Asset Hub DEX) to Solidity contracts running on pallet-revive. This enables smart contracts to swap tokens through the on-chain DEX and query swap prices.

The primary use case is W3S products (e.g. ticketing app) where contracts accept payment in one asset (e.g. USDC) and convert it to DOT/PUSD via the Asset Hub DEX, rather than holding arbitrary tokens directly.

#### [#11545]: Support State Overrides in ethCall
# Description

Follow-up to [#11075](https://github.com/paritytech/polkadot-sdk/pull/11075), scoped down to just `eth_call` state overrides per the feedback there. The original PR bundled state overrides together with `eth_simulateV1` (multi-block simulation, block overrides, transfer tracing, validation), which reviewers felt added too much surface area at once. This PR extracts the state override piece on its own, which is the most immediately useful part for the ecosystem and for our team.

Adds state override support to the `eth_call` JSON-RPC method, matching the [Geth state override specification](https://geth.ethereum.org/docs/interacting-with-geth/rpc/objects#state-override-set). This lets callers temporarily modify account balance, nonce, code, and storage during `eth_call` simulations without touching on-chain state.

Tools like Foundry, Hardhat, Tenderly, and really any dApp doing pre-flight simulation rely on state overrides heavily. Without them, our `eth_call` was strictly less capable than what the ecosystem expects.

### Why apply overrides in the runtime, not at the node level?

During the review of #11075, there was a suggestion to apply state overrides at the node level using `OverlayedChanges`, bypassing the runtime entirely. That approach works well if you control the node (Anvil does exactly this with a [custom executor](https://github.com/paritytech/foundry-polkadot/blob/24b2973b170779eb399b3fc1f393d7d900281ce5/crates/anvil-polkadot/src/substrate_node/service/executor.rs#L39)), but pallet-revive's eth-rpc is a standalone process that talks to the node over WebSocket via `state_call`. It has no access to `OverlayedChanges` or `StateMachine`. Adding a custom `state_call_with_overrides` RPC to Substrate core would need buy-in from the SDK team and would be a much larger change. Applying overrides inside the runtime, within the dry-run's transactional context that always rolls back, keeps everything self-contained and works with **any node out of the box**.

### Why extend DryRunConfig instead of adding a new runtime API method?

The codebase already had the pattern of `eth_transact` → `eth_transact_with_config` (a new method added when `DryRunConfig` was introduced). Adding `eth_transact_with_state_overrides` as yet another method felt like it would keep accumulating, and the fallback chain in the RPC layer was already two levels deep. Instead, we append `state_overrides` as a trailing field on `DryRunConfig` and rely on `sp_api`'s existing behavior: runtime API argument decoding uses `Decode::decode` (not `decode_all`), so trailing bytes from a newer RPC are silently ignored by an older runtime. For the reverse direction, a custom `Decode` impl defaults the missing field. No new method, no new fallback layer, backwards compatible in both directions. The [doc comments on `DryRunConfig`](https://github.com/paritytech/polkadot-sdk/blob/0xOmarA/eth-call-state-overrides/substrate/frame/revive/src/evm/api/rpc_types.rs#L24-L63) lay out the full reasoning and constraints.

## Integration

This PR introduces a third optional parameter to `eth_call`:

```jsonc
// Before
eth_call(transaction, block)

// After
eth_call(transaction, block, stateOverrides?)
```

The `stateOverrides` parameter is an address-keyed map where each entry can override:

- `balance`: fake balance before executing the call
- `nonce`: fake nonce
- `code`: inject EVM or PVM bytecode (promotes EOAs to contracts if needed)
- `state`: full storage replacement (wipes everything, then writes the provided slots)
- `stateDiff`: partial storage patch (only touches specified slots)
- `movePrecompileToAddress`: not yet implemented, silently ignored

Existing callers that don't pass the third parameter are completely unaffected. The parameter is `Option`-typed and defaults to `None`.

If you're a downstream consumer of pallet-revive's runtime API: the `DryRunConfig` struct has a new `state_overrides` field. It's `Option<StateOverrideSet>` and defaults to `None`, so existing code that constructs `DryRunConfig` via `Default` or the builder pattern will continue to work without changes. The SCALE encoding is backwards-compatible in both directions as described above.

## Review Notes

I'd suggest starting with the type definitions, then moving to the application logic, and finally the RPC wiring. The tests are worth skimming at the end to see the coverage.

### Type definitions

The core types live in [`rpc_types_gen.rs`](https://github.com/paritytech/polkadot-sdk/blob/0xOmarA/eth-call-state-overrides/substrate/frame/revive/src/evm/api/rpc_types_gen.rs#L1338-L1547):

- [`StateOverrideSet`](https://github.com/paritytech/polkadot-sdk/blob/0xOmarA/eth-call-state-overrides/substrate/frame/revive/src/evm/api/rpc_types_gen.rs#L1338): newtype around `BTreeMap<Address, StateOverride>` with builder methods and iterator impls. Both the set-level and per-account builder patterns are provided (e.g. `StateOverrideSet::new().with_balance(addr, val)`).
- [`StorageOverride`](https://github.com/paritytech/polkadot-sdk/blob/0xOmarA/eth-call-state-overrides/substrate/frame/revive/src/evm/api/rpc_types_gen.rs#L1455): enum encoding the mutual exclusivity of `state` vs `stateDiff` at the type level. Uses `#[serde(flatten)]` with externally-tagged enum representation so the JSON stays flat (`{"state": {...}}` or `{"stateDiff": {...}}`), matching exactly what Geth produces.
- [`StateOverride`](https://github.com/paritytech/polkadot-sdk/blob/0xOmarA/eth-call-state-overrides/substrate/frame/revive/src/evm/api/rpc_types_gen.rs#L1474): the per-account override struct. All fields optional.

### DryRunConfig extension

[`DryRunConfig` in `rpc_types.rs`](https://github.com/paritytech/polkadot-sdk/blob/0xOmarA/eth-call-state-overrides/substrate/frame/revive/src/evm/api/rpc_types.rs#L64-L74) has a new `state_overrides: Option<StateOverrideSet>` trailing field. The struct now has a manual `Default` (because `perform_balance_checks` defaults to `Some(true)`) and a [custom `Decode`](https://github.com/paritytech/polkadot-sdk/blob/0xOmarA/eth-call-state-overrides/substrate/frame/revive/src/evm/api/rpc_types.rs#L82-L89) that tolerates missing bytes for the new field. The backwards compatibility story is documented extensively in the [doc comments](https://github.com/paritytech/polkadot-sdk/blob/0xOmarA/eth-call-state-overrides/substrate/frame/revive/src/evm/api/rpc_types.rs#L24-L63).

### State override application logic

This is where most of the actual logic lives: [`state_overrides.rs`](https://github.com/paritytech/polkadot-sdk/blob/0xOmarA/eth-call-state-overrides/substrate/frame/revive/src/state_overrides.rs). The [entry point](https://github.com/paritytech/polkadot-sdk/blob/0xOmarA/eth-call-state-overrides/substrate/frame/revive/src/state_overrides.rs#L57) iterates the override set and dispatches to per-field handlers. The ordering (balance, nonce, code, storage) is intentional, because code overrides can promote an EOA to a contract, which needs to happen before storage overrides can write to the contract's child trie.

Key things to look at:
- [Code override](https://github.com/paritytech/polkadot-sdk/blob/0xOmarA/eth-call-state-overrides/substrate/frame/revive/src/state_overrides.rs#L158): detects PVM vs EVM via `BLOB_MAGIC`, creates a `ContractBlob`, stores code + metadata, promotes EOA to contract if needed. Checks `AllowEVMBytecode` for EVM code.
- [Storage override](https://github.com/paritytech/polkadot-sdk/blob/0xOmarA/eth-call-state-overrides/substrate/frame/revive/src/state_overrides.rs#L214): `State` clears the child trie then writes; `StateDiff` writes without clearing.

All of this runs inside the dry-run's transactional context, so mutations are rolled back after the call.

### RPC wiring

- [`execution_apis.rs`](https://github.com/paritytech/polkadot-sdk/blob/0xOmarA/eth-call-state-overrides/substrate/frame/revive/rpc/src/apis/execution_apis.rs#L36-L41): added `state_overrides` as the third parameter to the `eth_call` trait method
- [`lib.rs`](https://github.com/paritytech/polkadot-sdk/blob/0xOmarA/eth-call-state-overrides/substrate/frame/revive/rpc/src/lib.rs#L200-L211): forwards the overrides to `runtime_api.dry_run()`
- [`client/runtime_api.rs`](https://github.com/paritytech/polkadot-sdk/blob/0xOmarA/eth-call-state-overrides/substrate/frame/revive/rpc/src/client/runtime_api.rs#L142-L160): `dry_run()` now packs state overrides into `DryRunConfig` and sends them through `eth_transact_with_config`

### Call site ordering

In [`dry_run_eth_transact`](https://github.com/paritytech/polkadot-sdk/blob/0xOmarA/eth-call-state-overrides/substrate/frame/revive/src/lib.rs#L1937-L1953), state overrides are applied **after** `prepare_dry_run` (which only bumps the nonce). This ordering matters: if a user overrides the nonce of the `from` account, their override takes precedence over the nonce bump.

### Tests

24 integration tests [starting here](https://github.com/paritytech/polkadot-sdk/blob/0xOmarA/eth-call-state-overrides/substrate/frame/revive/rpc/src/tests.rs#L2121), all going through alloy-provider's `Provider::call().overrides()`. This is deliberate, by going through alloy we're not just testing our logic but also verifying that the JSON-RPC wire format is compatible with what the wider Ethereum ecosystem actually sends. If alloy can talk to us, so can ethers.js, viem, Foundry, etc.

Coverage at a glance:

**Balance:** override unfunded account, override to zero, override on `from` account (interaction with `prepare_dry_run`), nonce override.

**Code (full transition matrix):** empty→EVM, empty→PVM, EOA→EVM, EOA→PVM, EVM→EVM, EVM→PVM, PVM→EVM. All call `Callee.echo(N)` and assert the return value, including PVM variants (Resolc-compiled Callee).

**Storage:** `stateDiff` patches a slot, `state` replaces a slot, `state` clears unspecified slots (new [`TwoSlots.sol`](https://github.com/paritytech/polkadot-sdk/blob/0xOmarA/eth-call-state-overrides/substrate/frame/revive/fixtures/contracts/TwoSlots.sol) fixture), `stateDiff` preserves unspecified slots, multiple slots, empty `state` map clears all, zero-value diff write, combined code+storage, storage on EOA fails.

**Edge cases:** overrides don't persist, empty override set, multiple accounts, combined balance+code.

### New dependencies (dev-only)

`alloy-network`, `alloy-primitives`, `alloy-provider`, `alloy-rpc-types`, `alloy-signer-local` added to the workspace and inherited by the eth-rpc crate as dev-dependencies. Only compiled during `cargo test`.

### Known limitations

- `movePrecompileToAddress` is accepted but silently ignored.

#### [#12214]: pallet-beefy-mmr: align ECDSA→ETH failure sentinel between converter and consumer
BeefyEcdsaToEthereum returned an empty Vec<u8> on conversion failure, while compute_authority_set counted failures by matching [0u8; 20].
Extract the sentinel into a shared FAILED_BEEFY_TO_ETH_ADDRESS constant referenced by both sites.
Fix mock_beefy_id to derive valid ECDSA keys so tests exercise the happy path as well as the failure branch.

#### [#11939]: Add TransactionStorageApi v2 with `indexed_transactions` function
This bumps `TransactionStorageApi` to v2 and offers the option to query data referenced in the blocks via `indexed_transactions`.

#### [#11538]: [eth-rpc] Detect and backfill gaps in finalized block subscriptions
Temporary connection drops can result in missed blocks, leaving gaps in the local database and causing incomplete results for RPC methods such as eth_getLogs and eth_getBlockByNumber. This PR introduces automatic gap detection in the finalized block subscription and backfills missing ranges via a background worker.

- Gap detection: When a newly finalized block arrives with a number higher than expected, the skipped range is queued for backfill.
- Gap-fill queue: A bounded in-memory channel (capacity: 32), non-blocking; a separate atomic counter tracks queued and in-flight requests.
- Gap-filler task: A background worker processes requests sequentially, reusing sync_backward_range; it does not update Head/Tail sync labels or the first_evm_block boundary.
- Sync state (Head advancement): The sync head does not advance while gap fills are in flight, ensuring continuity of the synced block range.

#### [#11793]: Updated wasmtime to 36.0.7
Wasmtime 35.0.0 → 36.0.7


#### [#12176]: xcmp-queue: Store the bytes in the channel status
This improves the performance of xcmp-queue by not requiring to check all pages individually.

#### [#11641]: Fix runtime upgrade zombienet tests exceeding max_code_size
The wasm-builder's compaction and compression decision now uses the actual
blob build profile rather than the outer cargo profile. Since debug builds
already compile WASM blobs with the Release profile, they now also get
compacted and compressed, producing a properly sized `WASM_BINARY`.
Previously `WASM_BINARY` in debug builds was identical to `WASM_BINARY_BLOATY`.

#### [#12209]: Remove deprecated AssetsToBlockAuthor from parachains-common
Removes deprecated `parachains_common::AssetsToBlockAuthor`, which forwarded asset
transaction fees to the block author via `HandleCredit`. Use
`MaybeResolveAssetTo<BlockAuthor<Runtime>, ...>` instead (already used by system
parachain runtimes in this repo).


#### [#12246]: pallet-dap: re-export WeightInfo at crate root
The trait was only reachable at `pallet_dap::weights::WeightInfo`, so the benchmark template's generated `pallet_dap::WeightInfo` path failed to resolve and required a manual fixup in every consumer. Add the standard FRAME re-export (`pub use weights::WeightInfo;`) and switch the Asset Hub Westend weights file and the staking-async integration test back to the canonical `pallet_dap::WeightInfo` path.

#### [#12149]: Remove deprecated frame_support::error module
# Description

Removes deprecated `frame_support::error` as part of #11561.

The module was a re-export of `sp_runtime::traits::{BadOrigin, LookupError}` (deprecated July 2023). Updates the only in-repo caller, `pallet-revive`.

Does not close #11561.

#### [#12027]: Extend PGAS filter to allow batches
# Description

Just as the title says.

#### [#12059]: pallet-transaction-storage: mark publish = false and drop from umbrella
## Summary
The pallet remains a normal workspace member and can still be depended on directly via its path, but it is excluded from the umbrella crate and the published release set.

#### [#12169]: Remove deprecated EnsureOneOf type alias
Removes deprecated `frame_support::EnsureOneOf`, which was an alias for
`EitherOfDiverse`. Use `EitherOfDiverse` directly.

```diff
- use frame_support::traits::EnsureOneOf;
+ use frame_support::traits::EitherOfDiverse;
```


#### [#11798]: pallet-asset-conversion: distinguish `PoolEmpty` from `PoolNotFound`
## Summary

- Adds a new `PoolEmpty` error variant to `pallet-asset-conversion`
- `PoolNotFound` now only means the pool does not exist in storage
- `PoolEmpty` is returned when the pool exists but has zero reserves

## Motivation

When a pool exists but has no liquidity, `get_reserves()` returned `PoolNotFound`. This is misleading — the pool is in storage, it just has empty reserves. Users and frontends cannot distinguish between "you need to create a pool" and "you need to add liquidity."

## Changes

- `substrate/frame/asset-conversion/src/lib.rs`: Added `PoolEmpty` error variant, changed the zero-reserves check in `get_reserves()` to use it
- `substrate/frame/asset-conversion/src/tests.rs`: Updated `can_not_swap_in_pool_with_no_liquidity_added_yet` to expect `PoolEmpty`

#### [#11816]: pallet-beefy: Allow unsigned execution of an unsigned method
Allow unsigned execution of `report_future_block_voting_unsigned` in beefy.

#### [#11068]: FRAME: Add Peg Stability Module (PSM) pallet
Introduces `pallet-psm`, a Peg Stability Module that enables 1:1 swaps between a native
stablecoin (pUSD) and pre-approved external stablecoins (e.g. USDC, USDT). The PSM
strengthens the stablecoin peg by creating arbitrage opportunities bounded by configurable
minting and redemption fees.

Key features:
- Minting: deposit an external stablecoin to receive pUSD
- Redemption: burn pUSD to receive an external stablecoin
- Per-asset circuit breakers to disable minting or all swaps
- Configurable ceiling weights and maximum PSM debt ratio

#### [#11817]: asset-conversion precompile: expose getReserves
## Summary
- Add `getReserves(bytes asset1, bytes asset2)` view function to the asset-conversion precompile, returning the reserve balances of both tokens in the pool
- This exposes `pallet_asset_conversion::Pallet::get_reserves()` to EVM/PVM contracts and frontends via the precompile interface

## Motivation
The precompile already exposes `quoteExactTokensForTokens` and `quoteTokensForExactTokens`, which allow contracts to estimate swap outputs. However, there is no way to query the raw pool reserves directly. This forces frontends and contracts to probe with arbitrary amounts to infer pool state. Exposing `getReserves` gives direct access to pool balances, enabling:
- DEX UIs to display pool composition and depth
- Contracts to make routing decisions based on actual liquidity
- Parity with Uniswap V2's `getReserves` interface that Solidity developers expect

## Test plan
- [x] `get_reserves_works` — verifies correct reserve values for an existing pool
- [x] `get_reserves_fails_for_nonexistent_pool` — verifies revert for missing pool
- [x] All 23 existing tests continue to pass

#### [#11215]: try state hook for pallet authorship
This PR introduces the try_state hook to pallet-authorship to verify a key storage invariant.

closes part of https://github.com/paritytech/polkadot-sdk/issues/239

#### [#12179]: pallet-revive: skip non-existent accounts in batch_map_accounts
## Summary

`batch_map_accounts` now skips non-existing accounts in addition to eth-derived accounts. Without this, any caller could permanently insert `OriginalAccount` entries for arbitrary 32-byte values with no `frame_system::Account` backing them, without paying any fees, and with no way to clean them up. The proportion check still uses the original batch length, so padding with skipped entries cannot reach the 90% free-tx threshold.

`map_no_deposit` is renamed to `map_no_deposit_unchecked` with a doc spelling out the precondition.

#### [#11529]: Westend Asset Hub: Integrate PSM pallet and add remote tests
Integrates the PSM pallet into the Asset Hub Westend runtime and adds remote
integration tests that run against live chain state.

Runtime changes:
- Configures `pallet-psm` on Asset Hub Westend with pUSD (asset ID 50000342)
- Adds `pallet-parameters` for governance-configurable maximum issuance (default 50 million pUSD)
- Fee destination is the pUSD insurance fund account (`PalletId(*b"pusd/ins")`)
- Adds V1 migration to initialize USDT (1984) as the first external asset
  with 0% minting fee and 0.01% redemption fee
- Adds weights for the PSM pallet

Remote tests:
- Adds `pallet-psm-remote-tests` library with reusable test functions
- Adds `remote-ext-tests-psm` binary that fetches live Asset Hub state via RPC
- Tests mint/redeem flow and circuit breaker mechanism against real asset data
- Uses snapshot caching to avoid redundant RPC requests

#### [#12171]: WAH: wire pallet-recovery benchmarks
Wired `pallet-recovery` into the asset-hub-westend benchmark list.
Fixed the benchmark setup: `finish_attempt` / `cancel_attempt` advance `frame_system`'s block number, which does not move `RelaychainDataProvider`, causing `NotYetInheritable` / `NotYetCancelable`.
Under `runtime-benchmarks`, use `frame_system` as the `BlockNumberProvider` so the time-delay guards can be satisfied.

#### [#11930]: pallet-staking-async: Rotate era reward pots through a fixed-size pool
Era reward pot accounts are now drawn from a fixed pool of `POT_POOL_SIZE = 200`
accounts, indexed by `era % POT_POOL_SIZE`, instead of one fresh account per era.
This ensure we only use a fixed size of pot accounts for the lifetime of the
chain rather than growing per era.

An `integrity_test` enforces `POT_POOL_SIZE > HistoryDepth` so a slot is only
reused after its previous era has been pruned.

#### [#11676]: [pallet-assets] Reject delegatecall into pallet-assets ERC20 precompile
There is no legitimate use case for delegatecalling into the asset precompile. This matches the precedent set by the Storage precompile, which already enforces a delegatecall check (in the opposite direction — it *requires* delegatecall).

## Changes

- `lib.rs`: Add `ERR_DELEGATECALL_DENIED` const and `is_delegate_call()` guard before any dispatch logic
- `tests.rs`: Add `delegatecall_is_rejected` test using the `Caller.sol` fixture

## Test plan

- [x] `cargo test -p pallet-assets-precompiles` — all 67 tests pass
- [x] `delegatecall_is_rejected` verifies the guard rejects delegatecall via the `Caller` fixture contract

#### [#11052]: update multi asset bounties pallet account derivation logic
Bounty and child-bounty account derivation now uses raw-byte `[u8; 3]` prefixes `b"mbt"`
(multi-asset bounty) and `b"mcb"` (multi-asset child bounty) instead of the `&str` literals
`"bt"` and `"cb"` used by the legacy pallets. This avoids collisions with the old bounties
pallet (same account for both pallets).

**Encoding note:** The prefixes are SCALE-encoded as three raw bytes with no length prefix
(e.g. `b"mbt"` → `[0x6d, 0x62, 0x74]`). This differs from the legacy `&str` encoding which
includes a compact length prefix (e.g. `"bt"` → `[0x08, 0x62, 0x74]`).

`BountySourceFromPalletId` and `ChildBountySourceFromPalletId` now take a `Prefix` type
parameter (`Get<[u8; 3]>`). This is a **breaking API change** — downstream runtimes must
update their configuration to supply the prefix type.

Module and type docs were updated to document the derivation.

#### [#12069]: pallet-revive: fix execution tracer reporting zero gas for plain transfers
Fixes [paritytech/contract-issues#278](https://github.com/paritytech/contract-issues/issues/278).

`Stack::run_call`'s no-code branch was passing `Default::default()` to
`exit_child_span`, so the execution tracer reported `gas: 0` for any
transaction whose destination has no contract code (plain EOA transfers),
even when the meter had charged an existential deposit. The contract-call
branch already reads `frame_meter.total_consumed_gas()` /
`frame_meter.weight_consumed()` and forwards them — this PR does the same
at the transaction-meter level for the no-code branch.

## Tests

- New regression test `execution_tracing_records_consumption_for_plain_transfer`
  in `tests/pvm.rs`: transfers to an unfunded account, asserts
  `trace.gas > 0`. Fails on `master`, passes here.
- Updated `tracing_works_for_transfers`, which previously asserted the
  buggy zero via `..Default::default()`, to compare against the `bare_call`
  result's `gas_consumed`.

End-to-end: 1500 transfers via `manual-seal-6000` then
`debug_traceBlockByNumber` → before: every trace `gas: 0`; after: every
trace `gas: 200000` (the ED charge).

#### [#11604]: Refactor: candidate-validation fetches executor_params itself
# Description

  Remove `executor_params` from `CandidateValidationMessage::ValidateFromExhaustive`
  and have `candidate-validation` derive the session index from the candidate
  descriptor and fetch `executor_params` via the runtime API internally.

  This simplifies backing, approval-voting, and dispute-coordinator by removing
  executor_params threading through `Action::LaunchApproval`, `RetryApprovalInfo`,
  `ParticipationRequest`, `BackgroundValidationParams`, and `PerSessionCache`.

This PR is a follow up of this [comment](https://github.com/paritytech/polkadot-sdk/pull/11566#discussion_r3015664660)

#### [#11823]: Refactor asset-conversion tx payment fee correction
Fixes a bug where the `AssetTxFeePaid` event reported an incorrect `actual_fee` when paying
in the native asset via the asset-conversion extension (`asset_id == A::get()`). The returned
fee amount was double-subtracting the refund, under-reporting the fee in the event.

Refactors `SwapAssetAdapter::correct_and_deposit_fee` in `pallet-asset-conversion-tx-payment`
to handle all edge cases gracefully during post-dispatch fee correction. Adds comprehensive
test coverage for fee correction paths including account killed, account blocked, pool
drained, and native account with no free balance scenarios.

#### [#11690]: asset-conversion-precompiles expose pool management
Add createPool, addLiquidity, and removeLiquidity functions to the asset-conversion precompile, enabling EVM contracts to manage liquidity pools directly. Also refactors common helpers (caller lookup, path validation) for reuse across swap and liquidity operations.

#### [#11921]: pallet-psm: switch Westend Asset Hub to Location AssetId backed by LocalAndForeignAssets
On Asset Hub Westend, `pallet-psm` is switched from `u32` (trust-backed
asset id) to `xcm::v5::Location` as its `AssetId`, and from `Assets` to
`LocalAndForeignAssets` as its `Fungibles`. PSM can now mint and redeem
against both trust-backed and foreign-registered external stablecoins,
addressed uniformly by `Location`.

Storage migration: every `AssetId`-keyed PSM storage item is encoded under
the old `u32` key. The runtime wipes the existing PSM storage with
`frame_support::migrations::RemovePallet` and re-seeds the pallet from
`PsmInitialConfig` via `InitializePsm`. USDT (trust-backed asset `1984`)
is reseeded under its `Location` representation as the first external
asset.

`pallet-psm`'s `BenchmarkHelper` trait gains a `get_asset_id(index: u32)`
method so benchmark scenarios can derive a runtime-specific `AssetId`
(e.g. a `Location`) from a `u32` index. Existing impls need to add this
method.

#### [#11655]: [eth-rpc] Handle event decode errors across runtime upgrades
### Motivation
During backward sync, eth-rpc processes historical blocks using the current runtime's metadata. If the event layout differs between the current runtime and the runtime that produced those blocks (e.g., the Balances pallet gained new event variants in #7250), event decoding fails and receipts are lost.

### Changes
Replace events.has::<EthExtrinsicRevert>() with an event iterator that logs and skips decode errors, checks revert status by pallet/variant name, and collects ContractEmitted logs in a single pass — so that undecodable events no longer cause the entire receipt to be lost.

Behavior change: Previously, any undecodable event in a block caused the entire receipt to be lost. Now, decode errors are logged and skipped — the receipt is stored with best-effort revert status and logs.

#### [#11715]: Reject delegatecall into precompiles via PrecompileDelegateDenied
## Summary

- Add delegatecall guard to the ERC20 assets precompile and XCM precompile, matching the existing pattern in the vesting and asset-conversion precompiles
- Converge asset-conversion precompile from `Error::Revert(string)` to `Error::Error(PrecompileDelegateDenied)` for consistency across all precompiles
- Add delegatecall rejection test for the XCM precompile

## Motivation

Delegatecall to precompiles allows a malicious contract to execute precompile logic in a misleading caller context. The precompiles derive caller identity from `env.caller()`, which during delegatecall returns the original caller — letting the intermediary contract act on the caller's assets or send XCM on their behalf. There is no legitimate use case for delegatecalling into these precompiles.

## Changes

- `substrate/frame/assets/precompiles/src/lib.rs` — add `PrecompileDelegateDenied` guard
- `substrate/frame/asset-conversion/precompiles/src/lib.rs` — replace `Error::Revert(ERR_DELEGATE_CALL)` with `PrecompileDelegateDenied`, remove unused const
- `polkadot/xcm/pallet-xcm/precompiles/src/lib.rs` — add `PrecompileDelegateDenied` guard
- `polkadot/xcm/pallet-xcm/precompiles/src/tests.rs` — add `delegatecall_is_rejected` test
- `polkadot/xcm/pallet-xcm/precompiles/Cargo.toml` — add `pallet-revive-fixtures` dev-dependency

## Test plan

- [x] `cargo test -p pallet-xcm-precompiles` — 13 tests pass, including new `delegatecall_is_rejected`
- [x] `cargo test -p pallet-asset-conversion-precompiles` — 18 tests pass
- [x] `cargo test -p pallet-assets-precompiles` — 66 tests pass

#### [#11902]: pallet-psm: rename pUSD/stable to internal
Renames pallet-psm's "pUSD" / "stable" vocabulary to a generic "internal"
role, paired with the existing "external" terminology, so the pallet reads
as a generic peg stability module rather than one tied to a specific
stablecoin.

Public API changes:
- `Config::StableAsset` -> `Config::InternalAsset`
- `StableDecimals` storage -> `InternalDecimals`
- `AssetDecimals` storage -> `ExternalDecimals`
- `Event::Minted.pusd_received` -> `received`
- `Event::Redeemed.pusd_paid` -> `paid`
- `redeem(.., pusd_amount)` parameter -> `amount`

Runtime impls of `pallet_psm::Config` need to update the associated type
name; the storage rename is reflected directly in the v1→v2
`PopulateDecimals` migration (no separate migration needed).

All internal helpers, locals, comments, mock fixtures and prose are also
updated. The module docs and README gain a Terminology section explaining
"Internal" vs "External".

#### [#11847]: [revive] pgas as storage deposit
## Storage deposits backed by PGAS

> PGAS is a protocol-level gas token or gas allowance mechanism for users verified through Polkadot's Proof of Personhood ecosystem.

This PR adds a second payment backend for pallet-revive storage deposits: instead of always charging the user in native currency (DOT), a runtime can opt in to having deposits denominated in **PGAS**.

### New trait: `Deposit`

`substrate/frame/revive/src/deposit_payment.rs`

A new sealed `Deposit<T: Config>` trait abstracts over how storage deposits are charged, held, refunded. It has two implementations in-crate:

- **`()`**: the default, charges and refunds the native currency. Identical to the existing pre-PR behavior.
- **`PGasDeposit<Mutator, Holder, Freezer, Id, RefundPercent>`**: the PGAS-backed backend.

The trait is wired into `Config::Deposit` and called from `charge_deposit` / `refund_deposit` in place of the direct `T::Currency` calls that used to live there.

### Account lifecycle: `init_account` / `deinit_account`

The trait includes `init_account(to)` and `deinit_account(contract)` methods. Rather than transferring the ED from the origin at contract creation (and back to origin on destruction), the EDs are **minted** on init and **burned** on deinit:

### New storage: `NativeDepositOf`

`substrate/frame/revive/src/lib.rs`

```rust
pub(crate) type NativeDepositOf<T: Config> = StorageDoubleMap<
    _, Identity, T::AccountId,
    Blake2_128Concat, T::AccountId,
    BalanceOf<T>, ValueQuery,
>;
```

Keyed `(holder_account, user) -> native_amount`. It records how much **native currency** a user has contributed to a given account's hold. The holder is either a contract (for storage deposits on contract accounts) or the pallet account (for code-upload deposits).

It exists because in the mixed PGAS/DOT world, a user's refund cap needs to be tracked explicitly. The map caps how much of a refund can come back as DOT ; anything beyond that is settled in PGAS.

### `PGasDeposit<Mutator, Holder, Freezer, Id, RefundPercent>`

`substrate/frame/revive/src/deposit_payment.rs`

Parameterized by five type parameters that the runtime wires up:

- `Mutator: fungibles::Mutate` — the fungibles impl backing PGAS (e.g. `pallet-assets`).
- `Holder: fungibles::MutateHold` — the holds backend (e.g. `pallet-assets-holder`).
- `Freezer: fungibles::freeze::Mutate` — the freezes backend (e.g. `pallet-assets-freezer`), used to pin each contract's PGAS ED.
- `Id: Get<AssetId>` — the PGAS asset id on that fungibles instance.
- `RefundPercent: Get<Perbill>` — the fraction of PGAS returned on refund/collect; the rest is burned.

Charge semantics:
- If the user has enough reducible PGAS, the full amount is paid in PGAS via `fungibles::MutateHold::transfer_and_hold`, which emits the `TransferOnHold` event. No DOT is touched.
- Otherwise the charge falls through to DOT, and the contribution is recorded in `NativeDepositOf` so it can be refunded as DOT later.

Refund / collect semantics:
- DOT is returned first, capped by `NativeDepositOf[holder][user]` (and by `Precision::BestEffort` on the actual DOT hold).
- Any shortfall is taken from the PGAS hold. `RefundPercent` of that PGAS is transferred to the user's free balance; the remainder is burned.
- **Sub-ED refunds**: if the `RefundPercent` portion would land below PGAS's ED on the user's account (e.g. the user has no PGAS account and the refund is too small to create one), that portion is folded into the burn rather than aborting the whole refund.

The `RefundPercent` burn is what prevents free-PGAS harvesting: a user can't deposit storage, release it, and walk away with an allowance they can spend on execution.

### Migration (v4)

`substrate/frame/revive/src/migrations/v4.rs`

A three-phase multi-block migration brings live chains over:

- **Phase 1**: record each existing code-upload deposit under `NativeDepositOf[pallet_account][owner]` so it can still be refunded in DOT.
- **Phase 2**: flip each contract's storage deposit from DOT to PGAS via `Deposit::migrate_native_to_pgas` — mint + freeze the PGAS ED under `FreezeReason::PGasMinBalance`, burn the native `StorageDepositReserve` hold, re-hold the same amount in PGAS. Needed because pre-PR DOT deposits weren't tracked per-contributor.
- **Phase 3**: rewrite `DeletionQueue` from `TrieId` to `DeletionQueueItem { trie_id, account_id }` so the on-idle sweep can also clear the contract's `NativeDepositOf` rows. Runs on every runtime.


#### [#11475]: revive: Skip redundant eth_block_hash RPC call in block subscription
Skip a redundant eth_block_hash RPC call during block subscription by reusing the block hash
already available in the subscription context. Adds extra logging for receipt extraction.

#### [#11510]: frame-omni-bencher: better diagnostic on insufficient data points
When a benchmark is run with not enough steps and too many points are skipped then it can make the analysis panic. This PR improves the panic message and gives precise information about which benchmark is at fault.

#### [#12115]: pallet-revive: recalibrate `contains_storage` benchmark
The `contains_storage` benchmark now exercises the persistent storage path, matching how `WeightInfo::contains_storage` is dispatched from the precompile. The transient path remains benchmarked separately as `seal_contains_transient_storage`.
Also the `clear_storage`, `contains_storage`, `take_storage` and the transient `clear/contains` benchmarks now size their pre-existing value by n.

#### [#11818]: Add pallet-pgas-allowance with ChargePGAS transaction extension
- Introduces `pallet-pgas-allowance` providing a new `ChargePGAS<T, S>` transaction
  extension that wraps an inner fee extension `S`.
- When a signed transaction dispatches a call matching `Config::CallFilter` and the
  signer holds enough PGAS (a trusted asset on Asset Hub), the fee is withdrawn as
  a `fungibles::Credit`. In `post_dispatch` the `actual_fee`
  portion is dropped and burned and the unused remainder refunded to the payer.
- A `PGASFeePaid { who, actual_fee }` event is emitted when the fee is paid in PGAS.
- Wires the extension into `asset-hub-westend`.


#### [#11858]: pallet-revive: reserve on_finalize per-tx weight for eth extrinsics
### Summary
Eth extrinsics using `with_ethereum_context` add per transaction work to `on_finalize` (closing out the Ethereum block). That cost should be reserved at dispatch via `on_finalize_block_per_tx`.

This PR adds the reservation to `eth_substrate_call` and `eth_instantiate_with_code`, matching `eth_call`.

#### [#11795]: Harden asset-conversion quote functions against zero amounts
Hardens `quote_price_exact_tokens_for_tokens` and `quote_price_tokens_for_exact_tokens` in
`pallet-asset-conversion` to return `None` for zero input amounts and when integer rounding
produces a zero output. Previously, zero inputs could propagate through the AMM math and
zero outputs from small-input rounding were returned as `Some(0)`.

#### [#11434]: pallet-dap-satellite: Add token transfers via XCM to DAP from the satellites
Adds XCM-based transfer support to `pallet-dap-satellite`, enabling system parachains
that accumulate native token burns (fees, dust, coretime revenue) to periodically
teleport those funds to the central DAP buffer account on AssetHub.

## New components

**`sp-dap`**: Contains the `SendToDap` trait, to be implemented when funds need to be
  sent to the central DAP buffer, as well as the DAP and DAP satellite pallet IDs.

**`xcm-builder`**: Two new adapters are added:
- `SendToDapViaTeleport` — implements `SendToDap` and wraps it in a storage transaction
  so that any failure rolls back all local state changes.

## Integration (Westend system parachains)

All five Westend system parachains (AssetHub, BridgeHub, Collectives, Coretime, People)
and the Westend relay chain are configured with `SendToDapViaTeleport`.

## Testing

Integration tests covering the full round-trip (satellite accumulates → `on_idle` fires →
XCM teleport → DAP buffer receives) are provided for the Westend relay chain and all
system parachains (via `xcm-emulator`). Additional unit tests are also provided.

#### [#12216]: [revive] test: dry-run max_storage_deposit from an unfunded account
## Summary

Adds a regression test that verifies a `bare_call` dispatched with the
runtime-api dry-run `ExecConfig` from an account with no balance still
reports the same `max_storage_deposit` as a funded run on the same call.

#### [#11616]: Move era reward minting from staking to DAP
Introduces dual-mode era rewards in `pallet-staking-async`, controlled by
`Config::DisableMinting`:
- `true` (non-minting): staking expects an external source to fund a general reward
  pot. At era boundary, the balance is snapshotted into an era-specific pot. Payouts
  transfer from the pot. Unclaimed rewards returned via `UnclaimedRewardHandler`.
- `false` (legacy minting): `EraPayout` computes inflation, tokens minted on payout,
  remainder sent to `RewardRemainder`. Kept for Kusama compatibility.

Switching from legacy to non-minting is irreversible.

New config: `DisableMinting`, `UnclaimedRewardHandler`, `GeneralPots`, `EraPots`,
`StakerRewardCalculator`. New storage: `MaxCommission`, `DisableMintingGuard`.
New extrinsic: `set_max_commission`.

Runtimes using non-minting mode provide an `IssuanceCurve` impl to DAP and
register `StakerRewardRecipient` as a budget recipient. DAP storage version
bumped to V2 with migration.

#### [#11801]: eth-rpc: skip receipt extraction for finalized blocks already processed as best
### Motivation
Both the best and finalized block subscriptions extract receipts independently, so every block is processed twice. This skips redundant extraction on the finalized path when the block was already handled by the best block subscription.

### Summary
- Skip redundant receipt extraction on finalized blocks already processed by the best block subscription
- Read logs from DB for skipped blocks only when log subscribers exist
- Refactor: extract process_block helper, parse_log_row shared function, advance_sync_head
- Add unit tests for get_processed_eth_block_hash and logs_by_block_number

#### [#10952]: Fix `claim_rewards_to` benchmark to enable Snowbridge reward claims
The `prepare_rewards_account` benchmark helper was returning `None`, causing `claim_rewards_to` to be assigned `Weight::MAX` and effectively disabling the extrinsic. This fix returns a valid beneficiary account, enabling Snowbridge relayers to claim rewards to AssetHub as intended.

#### [#11908]: UncheckedExtrinsic: Improve memory usage
Improves the memory usage of the unchecked extrinsic by pre-allocating some buffers and preventing e.g. printing huge calls.

#### [#12297]: nomination-pools: allow permissionless full unbond of depositor in destroying state
Previously, any attempt to permissionlessly unbond the pool depositor was unconditionally
rejected with `DoesNotHavePermission`, even in the valid case where the pool is in the
`Destroying` state and the depositor is the sole remaining member.

This fix allows a permissionless full unbond of the depositor when both conditions hold:
1. The unbond is a full unbond (all remaining active points).
2. The pool is in the `Destroying` state and the depositor is the only member left
   (`is_destroying_and_only_depositor`).

Partial permissionless unbonds of the depositor continue to return
`PartialUnbondNotAllowedPermissionlessly`, and permissionless unbond attempts when the
depositor is not the sole member continue to return `DoesNotHavePermission`.


#### [#11809]: [DAP] Catch-up drip on V1->V2 migration
The DAP V2 migration seeded `LastIssuanceTimestamp` to a point in the past
(typically the active era start) so the next regular drip would credit
elapsed time back to that point. That elapsed is then clamped by
`MaxElapsedPerDrip`, so only up to one cap's worth of inflation is actually
credited on the first drip, and the rest is silently dropped.

This migration now performs a one-shot catch-up drip for the full
`[last_inflation, now]` window and seeds `LastIssuanceTimestamp` to `now`, so
regular drips start a fresh cadence from this point.

#### [#11949]: Additional improvements for the DAP satellite pallet generalization
Additional fixes / improvements for the DAP satellite pallet generalization (https://github.com/paritytech/polkadot-sdk/pull/11881):
- Rename `TeleportForwarder` to `TeleportForwarderForAccountId32` since it only works on `AccountId32`-type accounts
  (used in all system parachains), but future users with different account types will need different trait implementations
- Improved account migration testing
- Additional comments to clarify important corner-cases

#### [#11791]: Make block producer overridable for non-Aura chains
Introduces a `BlockProducer` trait used by `xcm-emulator` to drive slot duration
and pre-runtime digest construction when emulating a parachain block. The default
`AuraBlockProducer<T>` impl preserves the previous behaviour (slot derived from
`pallet_aura::Pallet::<T>::slot_duration()` and an Aura `PreRuntime` digest under
`AURA_ENGINE_ID`), so existing Aura-based `decl_test_parachains!` invocations are
unaffected.

The `decl_test_parachains!` macro gains an optional `BlockProducer:` field. Nimbus-based
parachains (e.g. Moonbeam) can now plug in a custom producer that emits a Nimbus
pre-runtime digest and a bespoke slot duration instead of being forced to implement
`pallet_aura::Config` just to participate in xcm-emulator integration tests.

Example:

```rust
decl_test_parachains! {
    pub struct MyPara {
        // ...
        core = {
            XcmpMessageHandler: my_runtime::XcmpQueue,
            LocationToAccountId: my_runtime::LocationToAccountId,
            ParachainInfo: my_runtime::ParachainInfo,
            MessageOrigin: cumulus_primitives_core::AggregateMessageOrigin,
            BlockProducer: MyNimbusBlockProducer,
        },
        // ...
    }
}
```

Downstream crates that implement `Parachain` directly (rather than via the macro)
must now also provide a `type BlockProducer: BlockProducer` associated type.


#### [#11822]: Remove deprecated CurrencyAdapter from pallet-transaction-payment
Removes the deprecated `CurrencyAdapter` from `pallet-transaction-payment`. This adapter was
deprecated since March 2024 in favor of `FungibleAdapter`. Runtimes still using
`CurrencyAdapter` must migrate to `FungibleAdapter`.

#### [#12131]: Remove deprecated ToStakingPot from parachains-common
Removes `parachains_common::ToStakingPot`, deprecated since March 2024 with no remaining
in-repo usage. `DealWithFees` already routes fees via `ResolveTo<StakingPotAccountId, Balances>`.

Migration:

```diff
- type OnChargeTransaction = ToStakingPot<Runtime>;
+ type OnChargeTransaction = ResolveTo<StakingPotAccountId<Runtime>, Balances>;
```

#### [#11416]: revive: Automatic address mapping via OnNewAccount/OnKilledAccount
## Summary

- Add `AutoMapper<T>` struct that implements `OnNewAccount`/`OnKilledAccount` to automatically map accounts when created and unmap when killed
- Add `AutoMap` config constant to enable/disable the feature per-runtime
- Guard `map_account`/`unmap_account` dispatchables with `AutoMappingEnabled` error when auto-mapping is active
- Wire up `AutoMapper` in Asset Hub Westend and dev-node runtimes with `AutoMap = true`
- Add v3 multi-block migration to auto-map all existing accounts and release deposits for already-mapped accounts

#### [#11398]: [pallet-revive] Add vesting precompile
## Summary

- Add a new built-in precompile exposing Substrate's vesting pallet to EVM contracts
- Implement `IVesting.sol` Solidity interface with methods for `vest`, `vestOther`, and `vestingBalance`
- Wire up the precompile in pallet-revive's builtin precompile registry and execution context

## Changed files
- **`IVesting.sol`** / **`precompiles/vesting.rs`**: Solidity interface and Rust implementation for vesting operations
- **`precompiles/builtin.rs`** / **`precompiles.rs`**: Register the new vesting precompile
- **`exec.rs`**: Expose vesting functionality to the execution context
- **`tests.rs`**: Add tests for the vesting precompile

## Test plan
- [ ] New vesting precompile tests pass (`vest`, `vestOther`, `vestingBalance`)
- [ ] Existing pallet-revive tests unaffected

#### [#11770]: Fix PSM migration to run on first deployment
Replaces the versioned `MigrateToV1` with an idempotent `InitializePsm` migration.

The previous `MigrateToV1` used `VersionedMigration<0, 1>` which never ran on fresh
deployments because `BeforeAllRuntimeMigrations` initializes the on-chain storage
version to the in-code version (1) before migrations execute, causing the version
check to skip.

`InitializePsm` instead checks whether each external asset already exists and skips
it if so. This makes it safe to run multiple times with no storage version dependency.

#### [#11734]: Try state check for pallet beefy mmr
This PR introduces try state hook into the Beefy MMR Pallet. It also defines the invariants that holds for the pallet.

Part of: https://github.com/paritytech/polkadot-sdk/issues/239

#### [#12158]: Remove deprecated sp_core hashing re-export
Removes the deprecated re-export of `sp-crypto-hashing` from `sp-core`. Hashing
helpers (`keccak_256`, `blake2_256`, `twox_*`, etc.) are no longer available at
the `sp_core` crate root or via `sp_core::hashing`.

Use `sp_crypto_hashing` directly. Types such as `H160`, `H256`, and `U256` remain
on `sp_core`. `polkadot-sdk-frame::hashing` continues to expose the same helpers,
now sourced from `sp_crypto_hashing` rather than `sp_core`.


#### [#10150]: Deprecate `ValidateUnsigned` trait and `#[pallet::validate_unsigned]` attribute
Deprecate the `ValidateUnsigned` trait and `#[pallet::validate_unsigned]` attribute as part of phase 2 of Extrinsic Horizon.


#### [#11563]: [pallet-broker] introduce Market trait for a generic coretime market
Introduce a Market trait to decouple the sale mechanism from the rest of the broker logic.
This allows alternative market implementations (e.g. RFC-17) to be swapped in without
modifying the broker pallet itself.

The purpose of this trait is to be implemented by a pallet containing the
logic for RFC-17 market and be exposed as a configuration item in pallet-broker.

#### [#11901]: UnionOf: implement metadata traits
Adds `fungibles::metadata::Inspect` and `fungibles::metadata::Mutate` impls
to both `fungibles::UnionOf` and `fungible::UnionOf`. Each dispatches to the
`Left` or `Right` backend via the existing `Criterion`.

Unblocks runtimes that need a metadata-aware fungibles surface across two
pallet instances, e.g., a pallet consuming
`UnionOf<Assets, ForeignAssets, ...>` that needs to read decimals/name/symbol.

`metadata::MetadataDeposit` is intentionally not implemented: its method
signature has no `AssetId` parameter and can't be dispatched via `Criterion`.


#### [#12005]: Make `TargetBlockRate` runtime API match `BLOCK_PROCESSING_VELOCITY` on test parachains

This PR aligns the `TargetBlockRate` implementation on every affected runtime with
`BLOCK_PROCESSING_VELOCITY`.

#### [#11922]: Update merkle mountain lib
Updates the merkle mountain crate to its latest version.

#### [#11081]: Implement `eth_subscribe`
# Description

Implemented `eth_subscribe` in the eth-rpc. The subscription kinds implemented is `newHeads` and `logs`.

#### [#11839]: pallet_revive:  Fix dispatch_as_fallback_account
Without this fix we stripped any call filters existing on the origin.

#### [#11992]: Pass -Zjson-target-spec when building with a .json target spec
Recent rustc requires `-Z json-target-spec` to opt into the JSON target spec format whenever `--target=*.json` is used. Without this, builds that go through `polkavm-linker::target_json_path` fail with:

  error: `.json` target specs require -Zjson-target-spec

Fix the two places in the workspace that invoke cargo with a JSON target spec for the Riscv runtime:

- Substrate-wasm-builder (`wasm_project.rs`): pass the flag for `RuntimeTarget::Riscv`. `RUSTC_BOOTSTRAP=1` is already set by the preceding `-Z build-std` block (Riscv always opts into build-std).
- pallet-revive-fixtures (`builder.rs`): refactor the inline rustc version detection to expose major/minor and derive both `new_immediate_abort` (1.92+) and `needs_json_target_spec` (1.95+) from them.

The flag is gated on rustc 1.95+ where it was introduced. Older rustc doesn't recognize it; later rustc requires it.

#### [#11649]: Consolidate try-state warnings into summary counts
Aggregates repetitive try-state warnings (min bond violations, pool ED imbalance, depositor
insufficient stake) into single summary lines with counts and up to 10 example accounts.
Reduces log noise from hundreds of expected per-item warnings on production runtimes.

Closes #11646.

#### [#11279]: [pallet-assets] Fix ERC-20 approve semantics in precompile
The ERC-20 approve(spender, amount) spec sets the allowance to amount. The precompile was calling do_approve_transfer, which adds to the existing allowance — breaking ERC-20 compliance.

This PR fixes the precompile's approve to use set semantics by composing existing pallet-assets primitives: when overwriting a non-zero allowance, the existing approval is cancelled first so the new value replaces (not accumulates with) the old one.

Also extracts do_cancel_approval from pallet-assets for reuse by the precompile.

#### [#11767]: Rework sp-virtualization API
## Summary

Redesign `sp-virtualization` around a **typestate API** (`Module -> Instance -> Execution`) that enforces correct usage at compile time. Split host-side polkavm ownership into a new **`sc-virtualization`** crate behind the `VirtManagerBackend` trait, decouple virtualization from the WASM executor, and replace the original `compile_from_hash` with two opaque-identifier entry points: `compile_from_storage_key` (auto-reload on cache miss) and a pure `lookup`. Cache hit/miss status is reported back to the runtime so callers can weight the call.

## Typestate API

Replace the trait-based `VirtT` / `MemoryT` abstraction with three concrete types that encode the lifecycle via Rust's move semantics:

- **`Module`** — a compiled PolkaVM program. Created via `Module::from_bytes(program, identifier)`, `Module::from_storage_key(storage_key, child_trie)`, or `Module::lookup(identifier)`.
- **`Instance`** — an idle instance ready to run. Created from `module.instantiate()`. Calling `instance.prepare(function)` consumes it and produces an `Execution`.
- **`Execution`** — a running (or suspended-at-syscall) instance. `execution.run(gas, a0)` drives one step and returns an `ExecResult` enum that carries ownership back through the transitions.

This makes invalid state transitions (e.g. calling `run` on an idle instance, or accessing memory outside a syscall) unrepresentable.

### `ExecResult` enum

```rust
enum ExecResult<V, I> {
    Finished { instance: V, gas_left },                            // V = idle Instance
    Syscall  { execution: I, gas_left, syscall_symbol, a0..a5 },   // I = running Execution
    Error    { instance: V, error },
}
```

Replaces the old `ExecOutcome` + separate error path. Ownership of the instance/execution is always returned, preventing resource leaks.

### Memory access restricted to `Execution`

`read_memory` / `write_memory` are now methods on `Execution`, not on a separate `Memory` type. This enforces at the type level that guest memory can only be accessed while the instance is actively running (i.e. suspended at a syscall).

## Crate split: `sp-virtualization` + `sc-virtualization`

`sp-virtualization` (runtime-facing) houses the forwarder API (`Module`/`Instance`/`Execution`), the `#[runtime_interface]` declaration, the `VirtManagerBackend` trait, and the `VirtManagerExt` externalities extension. No PolkaVM dependency. `#![cfg_attr(substrate_runtime, no_std)]` so it compiles into runtimes.

**New `sc-virtualization` crate** (host-side): the concrete `VirtManager` implementation of `VirtManagerBackend`, owning all PolkaVM types and the compiled-module cache. The runtime side stays storage-aware; the backend trait does not. The runtime-interface call site is the only layer that touches `self.storage` / `self.child_storage`.

## Syscall symbols instead of numeric IDs

Syscalls are now identified by **string symbols** (up to 32 bytes via `SyscallSymbol`) instead of `u32` syscall numbers. The test fixture imports change from `#[polkavm_import(symbol = 1u32)]` to `#[polkavm_import(symbol = "read_counter")]`, etc.

## Compile entry points

Three host functions, each with a clear contract:

| Host function | Behavior |
|---|---|
| `compile_from_bytes(program, identifier)` | If `Some(identifier)` is already cached, return that module without recompiling. Otherwise compile `program` and cache under `identifier` if supplied. |
| `compile_from_storage_key(storage_key, child_trie)` | Cache lookup keyed by `storage_key`. On miss, load program bytes from storage at `storage_key` (main trie or `child_trie`), compile, cache. |
| `lookup(identifier)` | Pure cache lookup. Never reads storage. Returns `Err(NotCached)` on miss. |

All three return `Result<_, ModuleError>`. The two compile entry points return a `CompiledModule { id, status }` packing both the new `ModuleId` and a `CompileStatus { Cached, Compiled }`, so the runtime can tell a cheap cache hit from a fresh compile (which, for `compile_from_storage_key`, includes a storage read) and charge weight accordingly.

### Wire encoding

`CompiledModule` packs into the `i64` FFI return value: low 32 bits hold the `ModuleId`; bits 32-39 hold the `CompileStatus` discriminant (`u8`); bits 40-62 are reserved for future fields; negative values stay reserved for `ModuleError` discriminants. `Module::lookup` returns `Result<ModuleId, ModuleError>` directly (no status — a successful lookup is always cached by definition).

## Per-extension cache, keyed by opaque identifier

The compiled-module cache lives on the `VirtManager` instance itself, not on a process-global static. Its lifetime tracks the externalities extension — one block of authoring or one `execute_block` call — which eliminates the author/importer asymmetry where cache hits depended on host process history rather than chain state.

The cache is keyed by an **opaque caller-supplied identifier** (the runtime passes a storage key for `from_storage_key`, or any byte slice for `from_bytes`). The host no longer re-hashes loaded bytes to verify them — storage tries are merkle-verified by their own infrastructure — and `sp-crypto-hashing` is no longer a dependency of either crate.

## `VirtManagerExt` externalities extension

`VirtManager` is no longer embedded in the wasmtime executor's `HostState` or accessed via `FunctionContext::virtualization()`. Instead, it is registered as an **externalities extension** (`VirtManagerExt`), making it accessible to both the WASM executor and native test code through the standard externalities mechanism.

## New `PassFatPointerAndReadOption<T>` in `sp-runtime-interface`

Added to ferry `Option<&[u8]>` across the FFI without SCALE overhead, using `(ptr = 0, len = 0)` as the `None` sentinel (Rust's `&[]::as_ptr()` is documented non-null, so a real `Some(&[])` always carries a non-zero pointer). Used by `compile_from_bytes` for the optional cache identifier.

## Removed abstractions

| Removed | Replacement |
|---|---|
| `VirtT` trait | Concrete `Module` / `Instance` / `Execution` types |
| `MemoryT` trait + `Memory` type | `Execution::read_memory` / `write_memory` |
| `native.rs` (205 lines) | `VirtManager` (in `sc-virtualization`) directly owns PolkaVM types |
| `ExecAction::Execute` / `Resume` | `Instance::prepare` + `Execution::run(gas, a0)` |
| `ExecOutcome` (in `sp-wasm-interface`) | `ExecResult` enum in `sp-virtualization` |
| `Virtualization` trait (in `sp-wasm-interface`) | `VirtManagerExt` externalities extension |
| `FunctionContext::virtualization()` | Extension access via `self.extension::<VirtManagerExt>()` |
| `InstanceId` (in `sp-wasm-interface`) | `InstanceId` + `ModuleId` (in `sp-virtualization`) |
| `RIInstantiateError`, `RIExecError`, etc. | `impl_ri_error_encoding!` macro |
| `DestroyError` (public) | Moved to `host_functions` (internal) |
| `compile_from_hash` + hash/prefix split | `compile_from_storage_key` + `lookup` (opaque identifier) |
| `ModuleError::HashMismatch` | — (no longer relevant; storage proves authenticity) |
| Process-global `MODULE_CACHE` | Per-`VirtManager` cache, lifetime = externalities extension |

## Error type changes

All error enums are `repr(u32)`. `CompileStatus` is `repr(u8)` (packed alongside `ModuleId` in the wire `i64`). `ModuleError` variants are `InvalidImage`, `NotCached`, `NotFound`. Wire encoding (negated discriminants for errors) is generated by the `impl_ri_error_encoding!` macro instead of manual `From`/`TryFrom` impls per error type.

## Engine configuration

The PolkaVM engine configures `worker_count(10)` so contract calls always have a worker ready, and `default_cost_model(CostModelKind::Full(CacheModel::L2Hit))` to match the cost model used in production. PolkaVM compilation/runtime errors panic (they indicate a host bug, not a guest error); only validation failures and missing storage map to `ModuleError`.

## Dependency changes

| Removed from | Added to |
|---|---|
| `sp-wasm-interface` dep from `sp-virtualization` | `sp-externalities` dep in `sp-virtualization` |
| `sp-virtualization` dep from `sc-executor-wasmtime` | `sp-storage` (std-only) in `sp-virtualization` |
| `sp-crypto-hashing` dep (was used for hash verification) | New crate `sc-virtualization` (host-side `VirtManager`) |
| `sp-std` dep from `sp-virtualization` | `sp-io` (dev-dependency) in `sp-virtualization` |
| — | `sp-virtualization` dep in `frame-benchmarking-cli` |

## Test changes

Tests run inside `sp_io::TestExternalities` with `VirtManagerExt::new(VirtManager::default())` registered. Each test has both a shared `run()` entry point (callable from WASM integration tests) and a standalone `#[test]` wrapper. New tests cover `from_storage_key` (cache hit reports `Cached`; cache miss reports `Compiled`; main trie + child trie reload paths; invalid image; not found), `from_bytes` (cache-lookup-first behavior — second call with the same identifier returns `Cached`), and `lookup` (proves it never falls back to storage even when the identifier exists at a storage key). Syscall handlers pattern-match on byte-string symbols (`b"read_counter"`) instead of numeric IDs. `run_out_of_gas_works` counter value reflects the new cost model configuration. Benchmarking CLI and executor integration tests register `VirtManagerExt`.

#### [#11647]: Fix try-state warning for LastValidatorEra in staking-async
Fixes a false try-state warning where `LastValidatorEra` was flagged as incorrect for active
validators. After the election for the next era completes but before that era becomes active,
`LastValidatorEra` is correctly set to `active_era + 1`. The previous check only accepted
`active_era`, causing spurious warnings.

Adds a test verifying `LastValidatorEra` transitions from `active_era` to `active_era + 1`
once the next era's election results are stored.

#### [#11912]: [pallet-assets-precompiles] Charge DepositEvent by data length, not topic count
## Summary

`deposit_event` in the assets ERC-20 precompile passed `topics.len()` for both the `num_topic` and `len` fields of `RuntimeCosts::DepositEvent`. The `len` field is the byte length of the event data payload, so the per-byte data cost was charged against the topic count (always 3 for the ERC-20 events emitted here) instead of the actual payload size — undercharging every `Transfer` and `Approval` emitted via this precompile by 7,640,746 ref_time and making its metering inconsistent with the EVM `LOG_n` path in `pallet-revive`, which correctly passes the data byte length.

## Changes

- `substrate/frame/assets/precompiles/src/lib.rs`: pass `data.len()` to `RuntimeCosts::DepositEvent { len }`.
- `substrate/frame/assets/precompiles/src/tests.rs`: add `deposit_event_charges_data_byte_length` regression test that asserts a precompile `transfer`'s `weight_consumed` equals `WeightInfo::transfer() + DepositEvent{num_topic: 3, len: 32}.weight()`. Verified to pass with the fix and fail without it (off by exactly the per-byte event-charge delta).

## Test plan
- [x] Verified the new regression test fails when the bug is reintroduced and passes when the fix is in place

#### [#7035]: Allow declaration and usage of multiple transaction extension versions in FRAME and primitives
This PR enhance `UncheckedExtrinsic` type with a new optional generic: `ExtensionOtherVersions`.
This generic defaults to `InvalidVersion` meaning there is not other version than the regular version 0. This is the same behavior as before this PR.

# Breaking change

The types `Preamble`, `CheckedExtrinsic` and `ExtrinsicFormat` also have this new optional generic. Their type definitions also have changed a bit: the `General` variant was 2 fields, the version and the extension, it is now only one field, the extension, and the version can be retrieve by calling `extension.version()`

Some trait such as `ExtrinsicMetadata` and `EthExtraImpl` changed their associated type named `Extension` to `ExtensionV0` and have new associated type `ExtensionOtherVersions`. This is because multiple version are now supported.
You can always use `InvalidVersion` for `ExtensionOtherVersions` and keep the old `Extension` for `ExtensionV0` to keep the same behavior as before this PR.

The type inference for those types may fail because of this PR, to update the code by writing the concrete types.

# New feature

To use this new feature, you can use the new types `PipelineAtVers` and `MultiVersion` to define a transaction extension with multiple version:

```rust
pub type TransactionExtensionV0 = ();
pub type TransactionExtensionV4 = ();
pub type TransactionExtensionV7 = ();

pub type OtherVersions = MultiVersion<
    PipelineAtVers<4, TransactionExtensionV4>;
    PipelineAtVers<7, TransactionExtensionV7>;
>;

pub type UncheckedExtrinsic = generic::UncheckedExtrinsic<
    AccountId,
    RuntimeCall,
    UintAuthorityId,
    TransactionExtensionV0, // The version 0, same as before
    OtherVersions, // The other versions.
>;
```


#### [#12135]: revive automap: batch map accounts for free
Changes:
- Add TX to pallet revive to allow anyone to map accounts

The new TX `batch_map_accounts`:
- Maps a batch of accounts without taking a deposit
- Releases existing mapping deposits of accounts
- Is free to send when at least 90% of accounts were newly mapped or had an existing deposit released

#### [#12092]: [acf] Move py/trsry drain migration from pallet to Westend RC
`pallet-accumulate-and-forward` is meant to be generic and reusable. Hard-coding a one-shot drain of the legacy `py/trsry` from RC to AH account belongs in a runtime, not the generic pallet.
To achieve the above:
- Removed `DrainLegacyTreasuryToAccumulationAccount` and its tests from `pallet-accumulate-and-forward`.
- Added a runtime-local equivalent inside `westend-runtime`'s migrations module, with the same semantics: drain into the local `ACF` accumulation account, where the existing `Forwarder` teleports to AssetHub's central DAP on the next forwarding interval.
- Dropped the migration from BridgeHub / Collectives / Coretime / People Westend: `py/trsry` is empty there, so no migration is needed.

#### [#12123]: Fix migrations for recovery & parachain-system pallets
Changes:
- pallet-recovery: be more lenient towards broken accounts to keep their recovery config
- parachain-system: clear old storage value to prevent decoding error

#### [#11513]: Add issuance/budget traits
Moves `EraPayout` trait from `pallet-staking` and `pallet-staking-async` to `sp-staking`,
eliminating duplicate definitions. Adds a new `budget` module with stake independent
traits for issuance computation (`IssuanceCurve`) and budget distribution
(`BudgetRecipient`, `BudgetRecipientList`). Also adds `StakerRewardCalculator` trait for
customizing validator incentive weights and staker reward splits.

No behavior changes. Existing `EraPayout` re-exports from both staking pallets are preserved.

#### [#11527]: Add issuance drip and budget distribution to pallet-dap
Adds issuance drip and budget distribution to `pallet-dap`. DAP mints new tokens on a
configurable cadence via `IssuanceCurve` and distributes them to registered
`BudgetRecipient`s according to a governance-updatable allocation map.

Includes `set_budget_allocation` extrinsic, `OnUnbalanced` slash handling with buffer
deactivation, safety ceiling on elapsed time, and a V1→V2 migration struct (not yet applied).

All runtimes are configured with a noop `IssuanceCurve` (`()` impl that returns 0) so there
is no behavior change. Minting will be enabled when staking is integrated with DAP.

#### [#11897]: Reorder `VerifySignature` extension variants so `Disabled` encodes to `0x00`
Swap the order of the `Disabled` and `Signed { .. }` variants in
`pallet_verify_signature::VerifySignature` so that `Disabled` is now the first variant
and encodes as the SCALE byte `0x00`, while `Signed { .. }` is the second variant and
encodes with tag `0x01`.

The motivation is signer compatibility. Generic signers can default an extension to its
passthrough state when that state encodes to a single zero byte — the same convention used
by other simple, defaultable transaction extensions (`CheckMetadataHash`'s `Mode::Disabled`,
`Option::None`, `bool::false`). Under the previous variant order, the disabled state of
`VerifySignature` encoded as `0x01`, which a signer cannot produce without knowing the
enum's specific variant layout.

**On-chain encoding change.** This is a breaking change to the SCALE encoding of the
extension: the variant tags for `Disabled` and `Signed` are flipped. To my knowledge there
is no production runtime using this extension right now, but the breaking change is
reflected in the major bump of the pallet.


#### [#12146]: Remove deprecated WeightMeter::defensive_saturating_accrue
Removes `WeightMeter::defensive_saturating_accrue`, deprecated in December 2023 in favor
of `consume`. No remaining in-repo usage.

```diff
- meter.defensive_saturating_accrue(weight);
+ meter.consume(weight);
```

Other `defensive_saturating_accrue` in the repo are on unrelated traits in
`frame_support::traits::misc`, not `WeightMeter`.

#### [#10742]: Add V3 scheduling validation for parachains
Adds V3 scheduling validation with `SchedulingV3Enabled` config and `MaxClaimQueueOffset`
to parachain-system pallet. Parachains must enable V3 explicitly after all collators are updated.
Do not enable it unless instructed to, otherwise, your chain will stall.


#### [#11512]: Vested Payout trait and implementation
Introduces a new `VestedPayout` trait in `frame-support` for transferring funds with a linear
vesting schedule. Unlike the existing `VestedTransfer` trait, callers only specify the total
amount and duration, and the implementor handles per-block computation internally.

`pallet-vesting` provides the implementation. The per-block unlock rate is rounded up so that
vesting always completes within the specified duration, never longer.

#### [#11905]: pallet-psm: relax AssetId from Copy to Clone
Relaxes the `T::AssetId` bound on `pallet-psm`'s `Config` from `Copy` to
`Clone`. Lets runtimes wire `pallet-psm` with a non-`Copy` `AssetId`.
The most relevant example is using XCM `Location` as `AssetId`.

No semantic changes; only the `Config` bound and ownership at use sites.
For `AssetId`s that are `Copy` (e.g. `u32`), the added `.clone()` calls
are free since `Copy` types implement `Clone` trivially.


#### [#11811]: Wired pallet_dap weights on Asset Hub Westend
Replaced type WeightInfo = () with the generated weights::pallet_dap::WeightInfo<Runtime>

#### [#11777]: eth-rpc: single-pass event processing for receipt extraction
## Summary

- Process block events in a single pass instead of re-scanning per extrinsic, reducing O(N×E) to O(E)
- Merge two integration tests into one that validates revert and logs are correctly attributed within the same block

#### [#12075]: frame-benchmarking: allow 2-point slope fits in `min_squares_iqr`
# Description

a33b7c2e36 short-circuits `min_squares_iqr` to `median_value` whenever `r.len() <= 2` to avoid OLS panics on `--steps=1 --repeats=1`. That fallback also catches the case where two samples sit at different x-values, where the slope is exactly determined. However, benchmarks whose valid sample set is narrowed to two values (e.g. by `BenchmarkError::Skip` filtering a `Linear<lo, hi>` parameter) lose their linear component.

Route the `r.len() <= 2` fallback through `median_slopes` instead, which fits a slope exactly from two samples at different x-values. The degenerate "all samples share one x" case now surfaces an explicit error from `median_slopes`'s own check.

## Integration

No API or storage changes. Downstream runtimes do not need to update code.

Regenerating weights may produce different values for benchmarks whose valid sample set is exactly 2 points at distinct x-values. The resulting weight expression will now include a linear component fitted via `median_slopes` instead of falling back to the constant median.

Benchmarks whose 2 remaining samples share an x-value will now fail with an explicit `median_slopes` error rather than returning the median. If you see this, broaden the parameter range or increase `--steps`/`--repeats` so more than one distinct x-value is sampled.

## Review Notes

`min_squares_iqr` had a single guard combining two unrelated short-circuits:

```rust
if r[0].components.is_empty() || r.len() <= 2 {
    return Self::median_value(r, selector);
}
```

The `components.is_empty()` branch is correct: a benchmark with no parameters has no slope to fit, so a constant median should be returned. The `r.len() <= 2` branch was a workaround for `linregress`'s OLS fit, which is under-determined with two samples and one intercept + one slope variable. Both cases fell through to the same `median_value` call, which drops slopes entirely.

The no-components fallback is kept on `median_value`, while `r.len() <= 2` is routed to `median_slopes` instead. `median_slopes` is well-suited: it forms the pairwise slope list `(y_i - y_j) / (x_i - x_j)` over samples with distinct x-values, takes the median, then derives the intercept from per-sample offsets. With exactly two distinct-x samples the slope list has length 1, so the median is the exact slope. No regression is needed.

#### [#11877]: pallet-dap: expose budget recipients and staging account as view functions
Adds two view functions to `pallet-dap`:
- `budget_recipients()` returns `Vec<(BudgetKey, AccountId, Perbill)>`: every registered recipient joined with its
  current allocation share.
- `staging()` returns the sub-account that collects burns/slashes before `on_idle` drains them into the buffer.

Needed by off-chain clients (e.g. pjs app) that would otherwise re-derive sub-accounts and join recipient lists
  client-side.

#### [#10726]: [Penpal] cleanup XCM config setup regarding assets
Closes #7314 by implementing all the subtasks mentioned in https://github.com/paritytech/polkadot-sdk/issues/7314#issuecomment-2792437373.

## Changes
Essentially, the main driver of all changes is that we adjust the Penpal runtime as follows:
- Make the native token the base token for buying weight (before it was a hybrid set up, probably not 100% intentional).
- Merge the `Assets` and the `ForeignAssets` pallet into one pallet called `Assets`, as the local assets can also be identified with a location starting with `parents: 0`.
- Give the pallet-asset-conversion a genesis config so that we can easily set up pools at genesis instead of redundantly calling the setup macro with the same args.


### Test Changes
I tried to keep the changes minimal in the tests in order to not harm any previously established invariants. Hence, in most cases I just did:

- Add a PEN<>WND pool in order to be able to pay xcm execution fees in WND
- Replaced the Penpal's teleportable asset with it's new location based version.
- In very few cases, I switched from WND to PEN to make the tests easier, when I was sure that no invariants would be harmed.
- The rest should only be renamings.

#### [#11395]: bandersnatch: extract vrf_sign_io/vrf_verify_io helpers
Extract vrf_sign_io and vrf_verify_io helper functions in the bandersnatch
VRF module and reuse them in both the VRF trait impls and the TraitPair
sign/verify methods (with zero input). This removes duplicated
proving/verifying logic and fixes the TraitPair::sign implementation which
was previously using a manual Schnorr scheme inconsistent with verification.

#### [#11432]: [frame-support] Add fungible metadata and lifetime traits with ItemOf support
Adds `fungible::metadata` and `fungible::lifetime` modules mirroring the corresponding
`fungibles` APIs:

- **metadata**: `Inspect` and `Mutate` traits for fungible token metadata (name, symbol,
  decimals).
- **lifetime**: `Create` trait for creating a new fungible asset.

These modules initially exist to support functionality in the `ItemOf` adapter for `fungibles`.
When a `fungibles` implementation provides the corresponding traits
(`fungibles::metadata::Inspect`, `fungibles::metadata::Mutate`, `fungibles::lifetime::Create`),
the `ItemOf` wrapper implements the fungible equivalents, allowing a single asset from a
fungibles set to be used where fungible metadata or create is expected.

#### [#11650]: Update PolkaVM to latest version (0.31 → 0.33)
Updates all PolkaVM dependencies from 0.31.0 to 0.33.0.


#### [#11893]: Raise relay-chain BlockLength to 10 MiB on test runtimes; decouple AttestedCandidate response cap
Raises the relay-chain block-length cap from 5 MiB to 10 MiB on `westend-runtime`,
`rococo-runtime`, and `polkadot-test-runtime`.


#### [#12145]: Remove deprecated NegativeImbalance from Polkadot-runtime-common
Removes `polkadot_runtime_common::NegativeImbalance`, deprecated in March 2024
in favor of `fungible::Credit`. No remaining in-repo usage.

Downstream: use `fungible::Credit` from `frame_support::traits` instead.

#### [#11651]: Validator self-stake incentive curve (non-vested)
Adds a separate validator incentive reward track funded from a second DAP budget pot.
Each validator's share is determined by a sqrt-based piecewise weight function of their
self-stake, with governance-configurable parameters (optimum, cap, slope factor).
Payout is a direct liquid transfer from the era incentive pot.

New extrinsic: `set_validator_self_stake_incentive_config` (AdminOrigin).

#### [#11942]: pallet-assets-precompiles: add EIP-2612 permit integration tests
## Summary

Adds integration tests for the `permit()` precompile in a new `mod precompile` submodule of `permit_tests.rs`. The existing tests in that file exercise `permit::Pallet` in isolation; these drive the same logic through the precompile dispatcher via `bare_call`, signing each digest at runtime with Hardhat account #0.

Covers the precompile-level concerns the pallet tests cannot reach:

- **Four-branch allowance update** in `ERC20::permit`, with `Approval` event emission and approval-deposit reservation.
- **`with_transaction` rollback** of nonce, allowance, deposit, and contract events, exercised across three failure surfaces (frozen asset, `to_balance` overflow, insufficient native balance for the approval deposit).
- **Rollback preserves a prior allowance** — a failed permit must not destroy an existing approval.
- **Domain separator integration**: cross-prefix replay rejection and `force_set_metadata` invalidating outstanding permits.
- **Dispatcher invariants**: `STATICCALL` to `permit()` is rejected (mirrors the existing `delegatecall_is_rejected` test); `nonces()` round-trips through the dispatcher.
- **Edge cases**: `deadline == now` boundary, zero-address owner/spender, and the `secp256k1_ecdsa_recover` failure path.

Tests-only — no production code changes. Four helpers in `tests.rs` were widened to `pub(crate)` so the submodule can reuse them.

## Test plan

- [ ] `cargo test -p pallet-assets-precompiles permit_tests` passes
- [ ] `cargo test -p pallet-assets-precompiles` passes

#### [#11710]: pallet-revive: expose pre-dispatch weight runtime API
# Description

This PR adds a new `pallet-revive` runtime API for computing the booked pre-dispatch weight of an
Ethereum transaction from its signed payload bytes.

The new API decodes the signed Ethereum transaction payload, converts it into the inner revive
call, and returns the same per-extrinsic weight contribution that `frame_system::CheckWeight`
books:

- `dispatch_info.total_weight()` including extension weight
- the dispatch class `base_extrinsic`
- the length-based proof-size charge

This is intended to expose the actual booked pre-dispatch weight for benchmarking and analysis,
without reconstructing the outer transaction from a `GenericTransaction`.

## Integration

This changes the `ReviveApi` runtime API surface by adding:

```rust
fn eth_pre_dispatch_weight(tx: Vec<u8>) -> Result<Weight, EthTransactError>;
```

Downstream consumers of `ReviveApi` will need regenerated metadata or updated runtime API bindings
to call the new method.

The method expects the signed Ethereum transaction payload bytes, not a `GenericTransaction`. This
is important because the outer transaction length contributes to proof-size booking and should come
from the real signed payload.

There is no storage migration and no change to dispatch behavior.

## Review Notes

Implementation details:

- the new pallet helper decodes `TransactionSigned` from the provided payload
- it recovers the signer address and builds the `GenericTransaction` from the signed tx
- it computes the actual outer `eth_transact` encoded length from the provided payload
- it reuses `into_call(CreateCallMode::ExtrinsicExecution(...))` to construct the inner revive call
- it returns:

```rust
dispatch_info.total_weight()
+ base_extrinsic
+ Weight::from_parts(0, encoded_len)
```

The PR also adds a regression test, `eth_pre_dispatch_weight_matches_check_weight_booking`, which
checks that the new API returns the same booked weight that `CheckWeight` would account for.

Example usage:

```rust
let weight = runtime_api.eth_pre_dispatch_weight(signed_tx_bytes)?;
```

#### [#10535]: Set a proper proof size block limit
The current block limit of the revive dev node defined its `proof_size` as `u64::MAX`.

This is a reasonable setting for a standalone chain as the PoV as a limiting resource is only relevant for parachains.

However, this gives some confusing gas mapping calculations: they are correct and consistent but the resulting `proof_size` weights are unexpectedly high.

This PR sets the `proof_size` of the block limit to the same value as the Polkadot Asset Hub.

#### [#12140]: WAH: benchmark revive with AH runtime and not kitchensink
For asset-hub-westend, benchmark pallet-revive using proper AH weights and not `SubstrateWeights` (benchmarked against kitchensink runtime).

#### [#11819]: pallet-psm: support external assets with different decimal precision
Previously pallet-psm rejected any external stablecoin whose decimals did
not match pUSD. This change normalizes to pUSD units internally so the PSM
can approve assets with arbitrary decimal precision within a safe range.

Core changes:
- New storage: per-asset `AssetDecimals` snapshot and pallet-wide
  `StableDecimals` snapshot. Storage version bumped to 2.
- Conversion helpers `external_to_pusd` / `pusd_to_external` with checked
  arithmetic and `MAX_DECIMALS_DIFF = 24` to prevent overflow.
- `mint` and `redeem` use round-trip rounding. Truncation dust stays in the
  caller's wallet on both paths (symmetric behavior), no value is trapped
  in the reserve and no hidden dust is routed to the fee destination.
- `PsmDebt` now denominates in pUSD units so aggregate ceilings and issuance
  checks are meaningful across mixed-decimal assets.
- Runtime drift guard: `mint`/`redeem` return `DecimalsMismatch` if live
  metadata diverges from the registration snapshot; that asset halts until
  governance intervenes.
- New errors: `DecimalsRangeExceeded`, `ConversionOverflow`,
  `AmountTooSmallAfterConversion`.

Migrations:
- `InitializePsm` now also seeds `StableDecimals` from live metadata if
  missing, and snapshots `AssetDecimals` for any new assets it adds.
- New one-shot `PopulateDecimals` migration backfills `StableDecimals` and
  `AssetDecimals` for chains that approved external assets before this
  upgrade. Out-of-range assets are auto-disabled (migration does not fail);
  `try-runtime` `post_upgrade` surfaces the anomaly to operators.

#### [#12100]: staking-async / WAH: use div_ceil for VoterSnapshotPerBlock
Floor division could undersize the paged voter snapshot relative to
`MaxElectingVoters`. Switch to `div_ceil` so
`VoterSnapshotPerBlock * Pages >= MaxElectingVoters` holds for any
configured values, and add invariant tests for both fake presets.

Aligns AH-Westend and the fake DOT/KSM presets with PAH and KAH in the
runtimes repo, which already use `div_ceil`.


#### [#12203]: pallet-referenda: Check before decrement deciding count
This ensures when a referendum is canceled or killed that we check if it was deciding before decrementing the deciding counter.

#### [#10992]: benchmarking: Support child trie key whitelisting
## Summary

Extends the benchmarking framework to support whitelisting child trie storage keys, enabling accurate PoV measurement for pallets that use child tries (e.g. `pallet-revive` for contract storage).

- Add `child_trie_key: Option<Vec<u8>>` field to `TrackedStorageKey` with `new_child()` constructor
- Add `add_to_whitelist_child()` helper function for benchmarks
- Update whitelist pre-read in both v1 and v2 benchmark macros to handle child trie keys via `child::get_raw()`
- **Fix stale whitelist bug**: `on_before_start` now reads from the global whitelist (`get_whitelist()`) instead of a captured local copy, so keys added during benchmark setup are properly pre-loaded
- Export `ChildInfo` from `frame_support::storage`

### Problem

Calling `add_to_whitelist()` or `add_to_whitelist_child()` during benchmark setup had no effect on PoV measurement because:
1. The pre-read only used `unhashed::get_raw()` which doesn't work for child tries
2. The pre-read closure captured a local whitelist copy that missed keys added during benchmark setup

#### [#11881]: Make `pallet-dap-satellite` more generic
The DAP satellite pallet is being converted into a generic Accumulate-and-Forward pallet, with
the purpose of pooling tokens of a given type into an accumulation account, which is then
periodically sent (forwarded) to a specified destination.

Notable changes:
- Crate renamed from `pallet-dap-satellite` to `pallet-accumulate-and-forward`
- The `SendToDap` trait is replaced by the `Forwarder` trait
- The pallet IDs value changes from `*b"dap/satl"` to `*b"acf/dott"` (at this point we
  are only forwarding DOT tokens, but new IDs should be added for other future tokens)
- Multiple other events and constants get renamed accordingly
- The old treasury is now drained via `DrainLegacyTreasuryToAccumulationAccount`


#### [#11960]: sp-hop: HOP runtime API primitives crate
Adds the `sp-hop` crate, which defines the `HopRuntimeApi` runtime API
trait for the Hand-Off Protocol (HOP). HOP is an ephemeral, peer-to-peer
data pool service that holds data node-side until it is claimed or
promoted to on-chain storage.

`HopRuntimeApi` is the contract between the HOP node service (`sc-hop`,
added in a follow-up PR) and the runtime:

- `can_account_promote(who, data_len)` — authorization check.
- `create_promotion_extrinsic(data, signer, signature, submit_timestamp)` —
  constructs the unsigned promotion extrinsic carrying the user's
  submit-time signer, signature, and timestamp so the runtime pallet can
  re-verify consent on-chain.
- `max_promotion_size()` — runtime-defined upper bound on a single
  promotion blob.
- `is_promoted_on_chain(hash)` — used by the node's maintenance task to
  confirm on-chain inclusion before flagging an entry as promoted.

This PR introduces the crate only; it has no runtime or node consumers
yet and is therefore a no-op at runtime. The follow-up PR adds `sc-hop`
and the `polkadot-omni-node` integration that depend on this trait.


#### [#11481]: [pallet-revive] Add PVM fuel tracing
Add **`pvm_fuel`** trace steps to PVM execution traces, recording pvm fuel consumption between syscalls and after the execution loop exits.

Separate synthetic trace steps from the real syscall list: `list_syscalls()` now only contains syscalls contracts can actually import, while new `list_trace_ops()` / `lookup_trace_op_index()` include both real syscalls and synthetic steps like `pvm_fuel`.

## Integration

Code using `list_syscalls()` for trace serialization should switch to `list_trace_ops()` / `lookup_trace_op_index()`. `list_syscalls()` and `lookup_syscall_index()` are unchanged for real syscalls.

## Review Notes
  - Proc-macro wraps `sync_from_executor` with `enter_ecall` / `exit_step` tracing hooks for `pvm_fuel`
  - PreparedCall::call adds a final `pvm_fuel` trace after the execution loop exits
  - PVM JSON trace fixtures updated to include `pvm_fuel` steps

#### [#10165]: YAP runtime: tune elastic scaling parameters and add local-run README
Update the YAP testing runtime:

- Support 12 cores / 500ms blocks
- Add README with build/run instructions for the local omni-node setup.
- Bumps `spec_version` to `1_003_002`.


#### [#12199]: pallet-staking-async: gate reap_stash and withdraw_unbonded kill by existential deposit
Gated `reap_stash` strictly by ED and not by `min(MinValidatorBond, MinNominatorBond).max(ED)`. Apply the same fix for `withdraw_unbonded` to kill stash if ledger.active < ED.


#### [#11763]: [westend] Remove pallet_treasury from RC and clean up satellite matchers
Remove pallet_treasury entirely from Westend relay chain.
Drain residual balances from the legacy `py/trsry`-derived account into the local
DAP satellite buffer on the relay and on each Westend system parachain
(bridge-hub, collectives, coretime, people).
Remove RelayTreasuryLocation matchers from all Westend system parachains.

Closes #11705.

#### [#12048]: [pallet-assets-precompiles] replace balance type u64 with u128 in tests
Replace the test runtime's `Balance` type from `u64` to `u128` in
`pallet-assets-precompiles` to align with `pallet_revive`'s test
convention. Test-only change with no production impact.


#### [#11843]: Remove DDayBodyId from Westend relay chain
Post-AHM cleanup following #11796. The DDay plurality origin in `AuthorizeCurrentCodeOrigin` is no longer needed since governance lives on AssetHub. Simplified to `EnsureRoot`.

#### [#10195]: Added
# Description

This PR introduces a new `#[stored]` attribute macro that simplifies the definition of storage types in FRAME pallets.
By automatically generating consistent field-based trait bounds for `Encode`, `Decode`, `MaxEncodedLen`, `Clone`, `Eq`, `PartialEq`, `Debug`, and `TypeInfo`, it reduces boilerplate and ensures robust trait implementations for generic storage structures.


#### [#11575]: [Penpal] fix genesis presets - assign proper ED to accounts'
Penpal had values below the ED for initializing asset balances for some accounts. This has not been detected as no unit tests actually use the presets. This PR fixes the invalid values, and it also adds some unit tests for validating that the presets build at least.

Closes #11558.

#### [#12402]: nomination-pools: benchmark against staking-async, enable on Asset Hub Westend
Make `pallet-nomination-pools-benchmarking` depend on `pallet-staking-async` instead of the deprecated `pallet-staking`, so nomination pools can be benchmarked on Asset Hub.


#### [#11778]: Set Aura slot duration to 24s for all Westend parachains
Increases the Aura slot duration from 6s to 24s for all Westend system
parachains (asset-hub, bridge-hub, coretime, people, collectives) via the
shared testnet constants, and for glutton-westend and YAP individually.

Additionally, glutton-westend is updated to use named elastic scaling
constants (RELAY_PARENT_OFFSET, BLOCK_PROCESSING_VELOCITY,
UNINCLUDED_SEGMENT_CAPACITY) instead of hardcoded values

#### [#11726]: eth-rpc: Bulk INSERT/DELETE and commit per-block writes atomically
### Summary

- Query SQLite's max variable limit at startup and use it to chunk bulk INSERTs and DELETEs, avoiding bind-parameter overflows
- Replace per-row individual inserts with batched bulk inserts
- Commit transaction hashes, logs, and block mapping in a single SQLite transaction per block, same for deletes
- Use `INSERT OR REPLACE` for logs (previously plain INSERT) to match transaction_hashes and prevent UNIQUE constraint failures if the EXISTS dedup guard is bypassed

#### [#11736]: Asset Hub Westend: Add MonetaryGuard governance track for PSM emergency actions
Adds a `MonetaryGuard` custom origin and governance track (ID 16) to Asset Hub
Westend for PSM emergency actions.

Changes:
- Adds `MonetaryGuard` origin to `pallet_custom_origins`
- Adds `monetary_guard` track with fast confirm/enactment periods
- Updates `EnsurePsmManager` origin mapping:
  - Root -> Full (all PSM operations)
  - MonetaryGuard -> Emergency (circuit breaker only)

Track parameters are relaxed for testnet purposes (low deposit, short periods,
0% support floor). Production values will differ.

#### [#10482]: Recovery pallet modernization
Revamps the recovery pallet to support multiple recovery groups and many new features.

#### [#11894]: Raise MAX_CODE_SIZE governance ceiling to 5 MiB
Raises `polkadot_primitives::MAX_CODE_SIZE` from 3 MiB to 5 MiB. Governance can now
set `HostConfiguration.max_code_size` up to 5 MiB;

Adds tests for previously untested rejection paths against the on-chain
`max_code_size`: `inclusion::verify_backed_candidate`,
`inclusion::check_validation_outputs_for_runtime_api`, `paras::schedule_code_upgrade_external`,
and `paras_inherent` sanitization.


#### [#11815]: Parachain disputes: Add some checks and tests
Extend the parachain disputes logic with some extra plus some tests.

#### [#12003]: emulated integration tests cleanup
This PR mostly consists of minor refactoring like introducing/centralize some helpers and replacing some inlined functions with these helpers.

#### [#11630]: [pallet-revive] Add vestedTransfer to vesting precompile
## Summary

Add `vestedTransfer(address, uint256, uint256, uint256)` to the vesting precompile, allowing Solidity contracts to create vesting schedules for target accounts via `pallet_vesting::vested_transfer`. Updates the `IVesting.sol` interface with the new function signature and NatSpec docs. Includes tests covering success, below-minimum revert, insufficient balance revert, and read-only/delegate-call guards.

## Test plan

- [x] `vested_transfer_succeeds` — verifies schedule creation on target
- [x] `vested_transfer_reverts_below_min` — reverts when locked < `MinVestedTransfer`
- [x] `vested_transfer_reverts_insufficient_balance` — reverts when caller lacks funds
- [x] Guard test cases — rejects in read-only and delegate-call contexts

#### [#11594]: Bags-list on_idle: per-item weight consumption via WeightMeter
Replaces the bulk `on_idle` benchmark with a per-item `on_idle_rebag` benchmark that
measures the worst-case cost of a single rebag. `on_idle` now consumes weight per
iteration via `WeightMeter` instead of reserving a single bulk weight upfront.
This decouples the benchmark from `MaxAutoRebagPerBlock` — changing the config no
longer requires re-running benchmarks.


### Changelog for `Node Operator`

**ℹ️ These changes are relevant to:**  Those who don't write any code and only run code.


#### [#12082]: ParityDB: Do not skip storing on store + ref in same tx
Fixes a bug in the ParityDB adapter where a `store` + `reference` on the same
unknown key within a single transaction would skip storing the value entirely.

ParityDB internally transformed the reference into a dereference, making the value
disappear from the database after commit. The fix maps changes directly to
`parity_db::Operation` variants so the `Set` is always emitted regardless of other
operations in the same transaction.


#### [#12358]: net: Fix bitswap silently stopping to serve a peer after a substream open failure
Bumps litep2p to 0.14.2. Fixes a bug where a failed outbound bitswap substream open left responses queued on a dead queue, silently halting bitswap service to that peer until node restart. Only affects nodes run with `--ipfs-server`.

#### [#11662]: HOP: ephemeral data pool service for Substrate nodes
New `--enable-hop` flag activates the HOP data pool for ephemeral
peer-to-peer data exchange. Related flags:
- `--hop-max-pool-size <MiB>` — pool capacity (default: 10240 MiB / 10 GiB)
- `--hop-max-user-size <MiB>` — per-user hard cap (default: 256 MiB)
- `--hop-retention-secs <seconds>` — data lifetime (default: 86400, 24h)
- `--hop-check-interval <seconds>` — maintenance cycle (default: 300, 5 minutes)
- `--hop-promotion-buffer-secs <seconds>` — start promoting this many seconds
  before expiry (default: 7200, 2h)
- `--hop-submit-rate-per-min <requests>` — sustained submit rate (default: 60)
- `--hop-submit-burst <requests>` — submit burst (default: 120)
- `--hop-bandwidth-per-min-mib <MiB>` — sustained bandwidth (default: 128)
- `--hop-bandwidth-burst-mib <MiB>` — bandwidth burst (default: 256)
- `--hop-disable-rate-limit` — turn off rate limiting (dev / tests)
- `--hop-data-dir <path>` — storage directory (default: `<chain-data>/hop`)

Pool state (blobs + metadata) persists across restarts. Orphan and corrupt
files are cleaned up automatically on startup.


#### [#10468]: Publish indexed transactions with BLAKE2b hashes to IPFS DHT
Add `--ipfs-bootnodes` flag for specifying IPFS bootnodes. If passed along with `--ipfs-server`, the node will register as a content provider in IPFS DHT of indexed transactions with BLAKE2b hashes of the last two weeks (or pruning depth if smaller).


### Changelog for `Runtime User`

**ℹ️ These changes are relevant to:**  Anyone using the runtime. This can be a token holder or a dev writing a front end for a chain.


#### [#11761]: asset-conversion-pallet: quote respects minimum balance
Quote functions (`quote_price_exact_tokens_for_tokens`, `quote_price_tokens_for_exact_tokens`)
now return `None` when the computed output exceeds what the pool can actually withdraw while
staying alive.

Swap execution withdraws from pools with `Preserve` preservation, meaning the pool must retain
at least `min_balance`. The quote functions did not account for this, so they could return a
price for a swap that would fail at execution.

Both quote functions now check the output against `reducible_balance(Preserve, Polite)` — the
same preservation level the swap uses.


#### [#11529]: Westend Asset Hub: Integrate PSM pallet and add remote tests
Adds the PSM pallet to Asset Hub Westend, enabling 1:1 minting and redemption
of pUSD against USDT.

#### [#11052]: update multi asset bounties pallet account derivation logic
The account IDs for bounty and child-bounty funding accounts have changed. Off-chain code that
derives or hardcodes these addresses must use the new derivation: raw-byte prefix `b"mbt"` for
bounties and `b"mcb"` for child bounties, SCALE-encoded as fixed `[u8; 3]` arrays (no length
prefix), with the same pallet ID and index encoding as before.

#### [#11823]: Refactor asset-conversion tx payment fee correction
The `AssetTxFeePaid` event now reports the correct fee amount when paying transaction fees
in the native asset through the asset-conversion extension.

#### [#11847]: [revive] pgas as storage deposit
## Storage deposits backed by PGAS

> PGAS is a protocol-level gas token or gas allowance mechanism for users verified through Polkadot's Proof of Personhood ecosystem.

This PR adds a second payment backend for pallet-revive storage deposits: instead of always charging the user in native currency (DOT), a runtime can opt in to having deposits denominated in **PGAS**.

### New trait: `Deposit`

`substrate/frame/revive/src/deposit_payment.rs`

A new sealed `Deposit<T: Config>` trait abstracts over how storage deposits are charged, held, refunded. It has two implementations in-crate:

- **`()`**: the default, charges and refunds the native currency. Identical to the existing pre-PR behavior.
- **`PGasDeposit<Mutator, Holder, Freezer, Id, RefundPercent>`**: the PGAS-backed backend.

The trait is wired into `Config::Deposit` and called from `charge_deposit` / `refund_deposit` in place of the direct `T::Currency` calls that used to live there.

### Account lifecycle: `init_account` / `deinit_account`

The trait includes `init_account(to)` and `deinit_account(contract)` methods. Rather than transferring the ED from the origin at contract creation (and back to origin on destruction), the EDs are **minted** on init and **burned** on deinit:

### New storage: `NativeDepositOf`

`substrate/frame/revive/src/lib.rs`

```rust
pub(crate) type NativeDepositOf<T: Config> = StorageDoubleMap<
    _, Identity, T::AccountId,
    Blake2_128Concat, T::AccountId,
    BalanceOf<T>, ValueQuery,
>;
```

Keyed `(holder_account, user) -> native_amount`. It records how much **native currency** a user has contributed to a given account's hold. The holder is either a contract (for storage deposits on contract accounts) or the pallet account (for code-upload deposits).

It exists because in the mixed PGAS/DOT world, a user's refund cap needs to be tracked explicitly. The map caps how much of a refund can come back as DOT ; anything beyond that is settled in PGAS.

### `PGasDeposit<Mutator, Holder, Freezer, Id, RefundPercent>`

`substrate/frame/revive/src/deposit_payment.rs`

Parameterized by five type parameters that the runtime wires up:

- `Mutator: fungibles::Mutate` — the fungibles impl backing PGAS (e.g. `pallet-assets`).
- `Holder: fungibles::MutateHold` — the holds backend (e.g. `pallet-assets-holder`).
- `Freezer: fungibles::freeze::Mutate` — the freezes backend (e.g. `pallet-assets-freezer`), used to pin each contract's PGAS ED.
- `Id: Get<AssetId>` — the PGAS asset id on that fungibles instance.
- `RefundPercent: Get<Perbill>` — the fraction of PGAS returned on refund/collect; the rest is burned.

Charge semantics:
- If the user has enough reducible PGAS, the full amount is paid in PGAS via `fungibles::MutateHold::transfer_and_hold`, which emits the `TransferOnHold` event. No DOT is touched.
- Otherwise the charge falls through to DOT, and the contribution is recorded in `NativeDepositOf` so it can be refunded as DOT later.

Refund / collect semantics:
- DOT is returned first, capped by `NativeDepositOf[holder][user]` (and by `Precision::BestEffort` on the actual DOT hold).
- Any shortfall is taken from the PGAS hold. `RefundPercent` of that PGAS is transferred to the user's free balance; the remainder is burned.
- **Sub-ED refunds**: if the `RefundPercent` portion would land below PGAS's ED on the user's account (e.g. the user has no PGAS account and the refund is too small to create one), that portion is folded into the burn rather than aborting the whole refund.

The `RefundPercent` burn is what prevents free-PGAS harvesting: a user can't deposit storage, release it, and walk away with an allowance they can spend on execution.

### Migration (v4)

`substrate/frame/revive/src/migrations/v4.rs`

A three-phase multi-block migration brings live chains over:

- **Phase 1**: record each existing code-upload deposit under `NativeDepositOf[pallet_account][owner]` so it can still be refunded in DOT.
- **Phase 2**: flip each contract's storage deposit from DOT to PGAS via `Deposit::migrate_native_to_pgas` — mint + freeze the PGAS ED under `FreezeReason::PGasMinBalance`, burn the native `StorageDepositReserve` hold, re-hold the same amount in PGAS. Needed because pre-PR DOT deposits weren't tracked per-contributor.
- **Phase 3**: rewrite `DeletionQueue` from `TrieId` to `DeletionQueueItem { trie_id, account_id }` so the on-idle sweep can also clear the contract's `NativeDepositOf` rows. Runs on every runtime.


#### [#11822]: Remove deprecated CurrencyAdapter from pallet-transaction-payment
The deprecated `CurrencyAdapter` type has been removed from `pallet-transaction-payment`.
Use `FungibleAdapter` instead.

#### [#10150]: Deprecate `ValidateUnsigned` trait and `#[pallet::validate_unsigned]` attribute
Deprecate the `ValidateUnsigned` trait and `#[pallet::validate_unsigned]` attribute as part of phase 2 of Extrinsic Horizon.


#### [#11736]: Asset Hub Westend: Add MonetaryGuard governance track for PSM emergency actions
Adds a new MonetaryGuard governance track to Asset Hub Westend for fast PSM
circuit breaker activation in case of a depeg or exploit.
