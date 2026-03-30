### Changelog for `Node Dev`

**ℹ️ These changes are relevant to:**  Those who build around the client side code. Alternative client builders,
SMOLDOT, those who consume RPCs. These are people who are oblivious to the runtime changes. They only care about the
meta-protocol, not the protocol itself.


#### [#10763]: Make some BEEFY keystore logic more generic
This PR:
1. makes some BEEFY keystore methods more generic:
- `sign()`
- `public_keys()`
   This is done by implementing the specific logic in the `BeefyAuthorityId`.
1. Removes the `BeefyAuthorityId::SignatureHasher` since for some algorithms it doesn't make sense to have a hasher.

Also since now the `BeefyAuthorityId` implements both the signing and the verification logic, we should have better
consistency.

Related to https://github.com/paritytech/polkadot-sdk/pull/8707#discussion_r2673377834

#### [#11463]: Report back when candidate is rejected
When the backing subsystem rejects a candidate due to descriptor version
acceptance failure, it now sends `CollatorProtocolMessage::Invalid` back to
the collator protocol so the collator's reputation is penalized. Previously
this case silently dropped the candidate with no feedback.


#### [#10472]: V3 Candidate Descriptor Support with Explicit Scheduling Parent
Introduces V3 candidate descriptors with an explicit `scheduling_parent` field, decoupling
scheduling context (validator group assignment) from execution context (relay_parent).
This is a foundation for low-latency parachain block production via lookahead scheduling.

V3 candidates are gated behind the `CandidateReceiptV3` node feature and require mandatory
UMP signals. Backward compatibility is maintained: V1/V2 candidates continue to work unchanged,
and old nodes see V3 candidates as invalid V1 (no slashing risk during transition).

Key changes:
- New `CandidateDescriptorV2::new_v3()` constructor with explicit scheduling_parent
- `ValidationParamsExtension::V3` passes both relay_parent and scheduling_parent to PVFs
- Subsystem messages now thread scheduling_parent explicitly
- Collator protocol, backing, and statement distribution use scheduling_parent for group lookups
- CollationVersion::V3 network protocol for advertising V3 collations


#### [#10891]: collator-protocol: Re-advertise collations when peer authority IDs are updated
The collator protocol contained a race-condition which could manifest as "Collation wasn't advertised".

A given peer ("A") can connect before the new authority keys are received via `UpdatedAuthorityIds` (nk -- new key).

- T0: peer A connects`PeerConnected`
- T1: peer A sends its current view `PeerViewChange`
  - Peer A wants the block N
- T2: `validator_group.should_advertise_to`: checks peer A for key nK (the new key)
  - We don't have this key stored and therefore return `ShouldAdvertiseTo::NotAuthority`
- T3: `UpdatedAuthorityIds` arrives with (peer A, [nK])

At this point, we have the collation, peer A wants to collation, we know peer A is an authority but we never send the
collation back. Then, the collation will expire with "Collation wasn't advertised".

To close the gap, the `UpdatedAuthorityIds` events will trigger a re-advertisement of collations
- note: if the advertisement was already sent, the logic does not resend it (achieved in should_advertise_to).

Part of the stabilization of:
- https://github.com/paritytech/polkadot-sdk/issues/10425

#### [#10658]: Omninode instant seal: Support relay parent offset
This brings support for relay parent offset to the omni-node instant seal consensus engine. Before instant seal was not
working with relay parent offsets bigger than `0`.

#### [#10770]: Statement-store: Follow-up improvements from PR #10718 review
This follow-up PR addresses review comments from PR #10718:
- Removed unnecessary Result wrapper from statement_hashes() - method is infallible
- Added debug assertion to validate sent count matches prepared count


#### [#10573]: Bump trie-db to 0.31.0
Bumps `trie-db` to 0.31.0 and `trie-bench` to 0.42.1.


#### [#10998]: Cumulus: Simplify parent search for block-building
While reviewing #10973 I found once more that our parent search is totally overengineered:
- It offers the option to search branches that do not contain the pending block -> These branches can never be taken
- It returns a list of potential parents -> Nobody uses the list, we only care about the latest block that we should
  build on

By eliminating these two annoyances, the code is a lot more simple and easier to follow. There are still some defensive
checks that are not strictly necessary, but does not hurt to keep them.

#### [#11004]: Statementstore: Forward statements to light clients
Forward statements to light clients. For now only done for testing purposes. Later on this needs to be improved to not
overwhelm light clients.

#### [#10755]: Fix Polkadot-omni-node dev mode slot mismatch panic
Fixes a panic when running Polkadot-omni-node in dev mode with parachains
that have slot durations different from the relay chain (e.g., 12s vs 6s).
The mock relay chain data now correctly accounts for the slot duration ratio.


#### [#10650]: Prospective parachains cleanup
This PR removes redundant code and simplifies the prospective parachains subsystem in preparation
for upcoming scheduling_parent design changes. Key improvements include:

1. Removed duplicate ImplicitView from prospective-parachains subsystem - the subsystem was
   maintaining its own relay chain ancestry while also feeding it to ImplicitView and querying
   it back, which was redundant.

2. Separated relay chain scope from para-specific scope - split the `Scope` structure into
   `RelayChainScope` (shared relay parent ancestry) and para-specific `Scope` (pending
   availability and constraints) for better clarity.

3. Removed GetMinimumRelayParents message - this unused inter-subsystem message is no longer
   needed as minimum relay parents are now calculated directly from the `scheduling_lookahead`
   parameter rather than queried per parachain.

4. Simplified ImplicitView - removed per-parachain tracking since all parachains share identical
   allowed relay parent windows at any relay block, reducing code complexity by ~150 lines.

This refactoring reduces the codebase by ~170 lines while maintaining the
same functionality and adding documentation.


#### [#10464]: collator-protocol: Readvertise collations after peer disconnects
There's a possible race case between peer connectivity and collation advertisement:
- The advertisement was generated
- peer disconnected before receiving the advertisement

As a result of that, when the peer reconnects, the previous collation (C0) is not sent.
This happens when the collator has produced another collation (C1).
However, from the logs it looks like the collation C1 is advertising, but C0 is skipped.

- T0: peer disconnects without receiving C0
- T1: peer reconnects
- T2: collator advertises C1, but not C0

This PR aims to resubmit collations on `PeerConect` events to mitigate these cases

Closes https://github.com/paritytech/polkadot-sdk/issues/10463

#### [#11191]: Cumulus: Improve logging
Use correct target and ensure we log when we did not find any parent.

#### [#11008]: Collator protocol revamp - update `calculate_delay`
A followup from https://github.com/paritytech/polkadot-sdk/pull/8541 with changes requested by @eskimor:
- Adjust the protocol parameters and add comments about the picked values
- Simpler fetch mechanism - advertisements from unknown collators (those with 0 reputation) are delayed. Everything else
  is fetched immediately.

#### [#10678]: Add relay chain state proof API for parachains
Collators now query the runtime for additional relay chain keys to prove. Adds
`prove_child_read()` to relay chain interfaces for child trie proofs.

`additional_relay_state_keys` parameter removed from parachain inherent data creation.
If needed, extend `RelayProofRequest.keys` directly at collator level, for example:
```rust
relay_proof_request.keys.extend(
    my_additional_keys.into_iter().map(RelayStorageKey::Top)
);
```


#### [#10785]: Fix inefficient do_propagate_statements
Fixes an O(n^2) complexity issue in `send_statements_in_chunks`. The loop in
`find_sendable_chunk` was inefficient because it was passing `to_send[offset..]`
after each chunk and then calling `chunk.encoded_size()` on the entire slice.

The fix uses an incremental approach that adds statements one by one until the
size limit is reached, only computing sizes for statements that will actually
be sent in each chunk.

For 300,000 statements, this reduces the processing time from ~2.5 seconds to ~0.33s.


#### [#11332]: client/db: Close missing body gaps for non archive nodes
This PR closes missing body gaps in the database for non-archive nodes.


Effectively, a missing body gap cannot be closed on the DB side if the node is non-archive. Since execution is already
skipped, the node will close the memory gap in the sync engine; however, the gap remains open in the db.

This leads to wasting resources at every startup:
- client info contains a gap that cannot be filled (since we don't have the state around for execution)
- blocks are fetched from the connected peers
- gap is filled by ignoring blocks in the sync engine

Further, for collators on origin master this causes an infinite loop of sync engine restarts that get punished via
banning and disconnecting. For more details and root cause check:
- https://github.com/paritytech/polkadot-sdk/pull/11330

Part of:
- https://github.com/paritytech/polkadot-sdk/issues/11299

#### [#11095]: `prefix_logs_with`: Ensure the macro works correctly for futures
When setting up a tracing span in an async future, it may gets invalidated by any `await` point. The problem is that
after continuing a future, it may runs on a different thread where the `span` isn't active anymore. The solution for
this is to `instrument` the future properly.

#### [#11139]: make subscription return statement event instead of bytes
Changes the statement subscription RPC to return `StatementEvent` instead of raw `Bytes`.
When a subscription is initiated, the endpoint now sends NewStatements event batches
with matching statements already in the store. If there are not statements in the store
an empty batch is sent.


#### [#10906]: collator-protocol: Remove stale pending collations from the waiting queue
This PR removes the stale pending collations from the waiting queue when the peer that advertised the collation
disconnects.

When the peer reconnects, the peer data is freshly created without any prior information about advertised collations.
Then the state-pending collation is picked from the queue. The network request will not be emitted since the `fn
fetch_collation`  sees no prior advertisement via `peer_data.has_advertised` and returns
`Err(FetchError::NotAdvertised)`.

To avoid this, remove the stale entries immediately when the peer disconnects.

Part of the stabilization of:
- https://github.com/paritytech/polkadot-sdk/issues/10425

#### [#11233]: Updates PolkaVM to the latest version
At the same time this prepares Substrate runtimes for PolkaVM 64bit support.

#### [#10542]: statement-store: Add latency bench
Adds a latency benchmark for the statement store to measure propagation performance across distributed nodes.


#### [#10954]: auth-discovery: Ensure DHT published addresses have ports
We have seen instances in production where validators will propagate multiaddresses without ports.
These addresses are effectively unreachable from the networking layer perspective.
They might be discovered via:
- identify protocol
- or simply a wrongly configured CLI for public addresses

To close the gap on this issue, this PR checks that the published addresses will always contain a port.

Closes:
- https://github.com/paritytech/polkadot-sdk/issues/10466

Part of:
- https://github.com/paritytech/polkadot-sdk/issues/10425

#### [#11046]: Collator protocol revamp: Change collation hold-off timing to start at leaf activation
https://github.com/paritytech/polkadot-sdk/issues/11022

The hold-off delay should be measured from when the relay parent (leaf) is activated, not when the advertisement message
arrives. This prevents artificially delaying messages that already arrived late.


 Changes
  - Calculate remaining hold-off time from leaf activation, not message arrival
  - Process immediately if hold-off window has already elapsed
  - Add test to ensure late-arriving collations skip artificial delay

#### [#10779]: remote-externalities: Support downloading from multiple RPC servers in parallel + major refactoring
Major refactoring of frame-remote-externalities to support downloading state from multiple
RPC servers in parallel. This improves reliability and performance when fetching remote state.

Breaking changes:
- `OnlineConfig::transports` field renamed to `transport_uris`
- Various internal API changes


#### [#10617]: statement-store: use many workers for network statements processing
Adds --statement-network-workers CLI parameter to enable concurrent statement validation from the network.
Previously, statements were validated sequentially by a single worker. This change allows multiple workers
o process statements in parallel, improving throughput when statement store is enabled


#### [#10816]: Update to Rust 1.93
Updates the Rust toolchain to version 1.93. This includes fixes for new compiler warnings,
updated UI test expectations, and fixes for broken rustdoc intra-doc links that are now
detected by the stricter rustdoc in Rust 1.93.

#### [#10846]: net/metrics: Add metrics for inbound/outbound traffic
This PR adds a new metric for inbound / outbound traffic for individual request-response protocols.

- the PR is motivated by https://github.com/paritytech/polkadot-sdk/issues/10765 which shows a significant number of
  bytes as downloaded (4-5 MiB/s). This is suspicious for a fully synced validator, 1-2 blocks to the tip of the chain.
- It suggests a protocol is internally consuming too much bandwidth leading to network inefficiencies, wasted CPU, and
  in the case of the issue to OOM kills

cc @paritytech/sdk-node

#### [#10787]: statement-store: validation without runtime
This removes slow runtime validation from statement-submission hot path.
Validation now happens on the node side via direct signature verification
and storage reads for account quotas.

#### [#10662]: Bulletin as parachain missing features
- Node developers/operators could enable the transaction storage inherent data provider setup by using
  --enable-tx-storage-idp flag. This is especially useful in the context of bulletin chain.
- Node developers will set up the network `idle_connection_timeout` to 1h when using `--ipfs-server` flag, again, useful
  in the context of bulletin chain.


#### [#10223]: Removed dependency to `sp-consensus-grandpa` from `sc-network-sync`
Refactored warp sync to remove the dependency on GRANDPA consensus primitives from the network sync module.
Instead of directly verifying proofs with GRANDPA-specific parameters, warp sync now uses a more generic
verifier pattern. This makes the warp sync implementation independent of any specific consensus mechanism,
allowing it to work with different consensus algorithms in the future.

#### [#11290]: Fix issue
Removes the `v3_enabled: bool` parameter from `CandidateDescriptorV2::version()` and all
accessor methods (`core_index`, `session_index`, `scheduling_parent`, `scheduling_session`),
making version detection self-contained. Previously, version detection depended on a node
feature lookup from the relay parent, which could produce mismatches between backers (who
derive the feature from a recent leaf) and approval checkers / dispute participants (who may
not be able to determine the feature from an old relay parent). This mismatch could lead to
disputes and backer slashing.

The fix moves version gating responsibility to two places:
- **Runtime**: `check_descriptor_version_and_signals()` rejects ambiguous candidates
  (where old-style and new-style version detection disagree) and V3 candidates before the
  feature is enabled.
- **Backing subsystem**: Adds defense-in-depth `check_version_acceptance()` checks at the
  signing boundary, covering both the seconding path and the statement-import path.

Downstream consumers (dispute coordinator, dispute distribution, statement distribution,
collator protocol) no longer need to look up node features for version detection, since any
candidate that reaches them on-chain was already validated by the runtime.

Additional changes:
- **Candidate validation**: V3 feature detection is based on session changes via
  `handle_active_leaves_update`. Uses `scheduling_parent_for_candidate_validation(v3_ever_seen)`
  and `version_for_candidate_validation(v3_ever_seen)` to transition safely.
- **Dispute coordinator**: V3 feature detection on both first and subsequent leaves.
  Uses `scheduling_parent_for_candidate_validation(v3_ever_seen)` in dispute participation
  queue ordering (`CandidateComparator`), so V3 candidates are ordered by their scheduling
  parent's block number rather than relay parent.
- **Prospective parachains**: Removes the `HypotheticalOrConcreteCandidate` trait. Splits
  `check_potential` into a lightweight check (using only scheduling_parent, no relay_parent
  needed) and a full constraint check (for concrete `CandidateEntry` with relay_parent).
  `HypotheticalCandidate::Incomplete` no longer pretends to have a relay_parent.
- **Subsystem util**: Removes the `HypotheticalOrConcreteCandidate` trait definition and its
  `HypotheticalCandidate` impl from the inclusion emulator.
- **Approval voting**: Fixes executor params fetching to use the candidate descriptor's
  `session_index()` (the relay_parent's session) but use a recent block for the fetch itself.
- **Statement distribution**: Fixes remaining `relay_parent` → `scheduling_parent` variable
  references in request dispatch and response handling.


#### [#10661]: statement-store: Add networking benchmark
Adds a benchmark for the statement store networking to measure performance
of statement propagation and validation under various conditions.


#### [#11108]: Polkadot-runtime-api-cache: Only cache validation code that exists
Otherwise there is the possibility that nodes cache `None` and some later `force_set_code` enacts the code. Then the
nodes that have cached `None` do not know that the validation code now actually exists on chain.

#### [#10960]: Warn when dropping an out of view candidate
This changes a debug log to a warning. The other log messages around the candidate state are also partially warnings. A
candidate that is directly out of view counts clearly as a warning. Besides that this pull request also increases
 the lookahead for Westend + Rococo to `5` to align it with Kusama.

#### [#11027]: Add functions to control statement store per-account allowances
The functions currently residing in `individuality` are moved to Substrate to unify storage allowance control.

#### [#11117]: statement-store: do not populate recent on restart
Previously, on node restart all statements loaded from the database were added to the
`recent` set in the statement-store index. This caused unnecessary network traffic as
the node would attempt to re-propagate all persisted statements to peers. Now, only
newly submitted statements are marked as recent, avoiding redundant gossip after restart.

#### [#10823]: Enforce statement allowances
Add statement allowance enforcement to statement store client

#### [#10965]: fatxpool: Do not remove listener for finalized view
The transaction pool is now able to handle events for active but finalized views. This improves transaction handling for
manual-seal nodes which immediately finalize.

#### [#10796]: Fix size limit mismatch in process_initial_sync_burst
Fixes a debug assertion failure in `process_initial_sync_burst` where the size filter
used `MAX_STATEMENT_NOTIFICATION_SIZE` while `find_sendable_chunk` reserved additional
space for `Compact<u32>` vector length encoding (5 bytes).

This mismatch caused `debug_assert_eq!(to_send.len(), sent)` to fail when statements
were sized to fit the filter's larger limit but exceeded `find_sendable_chunk`'s
stricter limit.

The fix extracts the size calculation into a shared `max_statement_payload_size()`
function that both locations now use, ensuring consistent size limits.


#### [#11051]: Add rate-limiter for statement networking
Adds a rate limiter for the statement networking protocol to prevent peers from bypassing the known-statements LRU cache
by continuously sending valid statements, which would hit the slower statement-store index.

#### [#10807]: Omni-node: Move timestamps closer to now
Blocks produced with Omni-nodes dev-mode are now closer to the current time. Previously they were starting at
`UNIX_EPOCH`.

#### [#11239]: collator-protocol: check v3 candidate against last finished slot block
This PR achieves the following:
1) removes the assumption that the scheduling parent sent with a candidate descriptor is an active leaf on the
collator-side
2) removes the reputation penalty for collators sending a candidate with a scheduling parent that's corresponding to an
in progress slot
3) checks on the validator_side that the candidate descriptor's scheduling parent is the rc block corresponding to the
last finished rc slot's block.
4) improves the testing coverage for V3 candidates descriptor throughout collator-protocol and paras_inherent.

#### [#9880]: ah-westend: Elastic Scaling with 3 cores on AssetHub Westend
This PR enables elastic scaling on AssetHubWestend with 3 bulk cores.

Guideline for enablement:
https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/guides/enable_elastic_scaling/index.html

### Next Steps
- [x] Ensure collators are running with 2509 or newer
- [x] Double check the changes locally
- [ ] If AH Westend looks good, we'll enable ES to AHPaseo

cc @paritytech/sdk-node @sandreim

#### [#10973]: Cumulus: Remove `max_depth` for the parent search
We were just incrementing this number all the time and there is actually no need to have it, as the search is already
automatically bounded. For chains with 500ms blocks and relay offset of 1 we easily go above this limit and this then
leads to forks.

So, let's remove the value.

#### [#11306]: Add CandidateDescriptorV3 support to experimental validator
Adds CandidateDescriptorV3 support to the experimental validator-side collator protocol.

Fixes: #11084

#### [#10690]: statement-store: implement new rpc api
Implements the new simplified RPC API for the statement store as proposed in PR #10452.
The API surface has been reduced to two main functions: `submit` and `subscribe_statement`.

**Submit changes:**
- Added support for the new `expiry` field where statements with an expiration timestamp
  lower than the current time are rejected.

**Subscribe changes:**
- Implemented a configurable worker pool that manages all subscriptions.
- New subscriptions are assigned to workers via a round-robin protocol.
- When a new statement is accepted by the statement-store, all workers are notified and
  evaluate their assigned subscription filters, notifying each subscriber accordingly.
- Existing statements matching the filter are sent on subscription.

**Additional improvements:**
- Added periodical scanning and removal of expired statements.
- Removed the old API methods (broadcast, posts, networkState, etc.) in favor of the
  simplified submit/subscribe interface.


#### [#11102]: Polkadot-omni-node-lib: emit warnings for aura authority id type assumptions
closes https://github.com/paritytech/polkadot-sdk/issues/11026

This PR adds explicit warnings at node startup to surface these assumptions:

- When the chain spec id starts with `asset-hub-polkadot` or `statemint`,
  the node assumes `ed25519` as the Aura authority id type and now emits a
  warning documenting this specific assumption.
- For all other chains, the node assumes `sr25519` by default and now emits
  a warning noting that `ed25519` runtimes  are not yet
  supported.

#### [#11407]: Statement Store: Introduce new CLI args
Introduce new CLI args to control statement store parameters.
Rework the statement store configuration, consolidating all parameters into a single structure.

#### [#11251]: people: Enable Elastic Scaling with 2 block times for people chain
This PR enables elastic scaling:
- 3 cores for producing blocks
- expected block times for 2s

Part of:
- https://github.com/paritytech/polkadot-sdk/issues/10425

#### [#10917]: Implement persistent reputation database for collator protocol (#7751)
Implements persistent storage for the experimental collator protocol's reputation database.

Changes:

  - Adds `PersistentDb` wrapper that persists the in-memory reputation DB to disk
  - Periodic persistence every 10 minutes
  - Adds `--collator-reputation-persist-interval` CLI to specify the persistence interval in seconds.
  - Immediate persistence on slashes and parachain deregistration
  - Loads existing state on startup with lookback for missed blocks

Implementation:

  `PersistentDb` wraps the existing `Db` and adds persistence on top:

```
- All reputation logic (scoring, decay, LRU) stays in `Db`
- Persistence layer handles disk I/O and serialization
- Per-para data stored in parachains_db
```

Tests:

  - `basic_persistence.rs`: Validates persistence across restarts and startup lookback
  - `pruning.rs`: Validates automatic cleanup on parachain deregistration

#### [#10974]: slot_timer: Downgrade spammy log to debug
The log is quite spammy with 12core setup since the last ~2 blocks will be skipped in the last second of block
production.


#### [#11496]: Don't bubble up errors during collator score parsing in collator protocol
When starting the node with a warp sync and we hit a period near the `WARP_SYNC_TARGET_BLOCK` (each 512 blocks) we might
not be able to call a runtime apis for some blocks, which will yield an error in the collator protocol revamp.

Don't bubble up such errors to prevent the subsystem from exiting.

#### [#10827]: Add more buckets to histogram for bitfields sent
**Description**

<img width="858" height="387" alt="image"
src="https://github.com/user-attachments/assets/c48ed21e-71dd-42ef-84ef-c21a7305a95a" />
On Kusama the chart already goes to infinity, so we need to adjust it to the desired value.

**Integration**

Should not affect downstream projects.

#### [#10893]: Do not prune blocks with GrandPa justifications
Warp sync requires GRANDPA justifications at authority set change boundaries to construct proofs. When block pruning is
enabled, all block bodies are removed regardless of whether they contain important justifications. The pruned nodes can
then not be used to fetch warp proofs.
We now have the capability to filter which blocks can be safely pruned. For parachain nodes, everything can be pruned,
solochain nodes using grandpa keep blocks with justifications. This ensures warp sync ability within the network.

#### [#11152]: Warp sync: Warp proof block import should not mark as leaf
While warp syncing a node, I saw some huge stalls. What happens:

- During warp sync we store warp proofs
- After warp sync we start gap sync
- Problem: Once gap sync finishes, node starts looking for displaced leaves from all the warp proof blocks, which was
  over 2500 in my observed case. This led to a 30minutes stall.

In this PR I propose to import the blocks during warp sync with a Disconnected state, which does not add them as leaves.
This fixes the downtime.

#### [#11316]: cargo: Update litep2p to v0.13.3
Update litep2p to latest 0.13.3 version

#### [#10847]: net: Spawn network backend as essential task
This PR spawns the network backends as essential (libp2p / litep2p).

When the network future exits, it will bring down the whole process.
- there's no point in running a node without the core network backend as it will not be able to communicate with peers
- while at it, have changed some logs from debug to warn
- the network backend can be brought down unintentionally by the `import_notif_stream`


Discovered during:
- https://github.com/paritytech/polkadot-sdk/issues/10821

#### [#11283]: Implement retry in bridges equivocation loop
Implement retry mechanism in bridges equivocation loop

#### [#10513]: Extract parachain types into a dedicated crate
Closes https://github.com/paritytech/polkadot-sdk/issues/10512.

Moves the common parachain primitives (accounts, balances, hashes, opaque block types) into a new
`parachains-common-types` crate. The existing `parachains-common` crate re-exports these definitions, and
`polkadot-omni-node-lib` now depends on the lightweight types crate to avoid pulling runtime pallets into omni-node
builds.

#### [#11020]: Block Response Handler: Take protocol overhead better into account
We now take the protobuf overhead into account.

#### [#11085]: Sync: Gracefully handle blocks from an unknown fork
There is the possibility that node A connects to node B. Both are at the same best block (20). Shortly after this, node
B announces a block 21 that is from a completely different fork (started at e.g. block 15). Right now this leads to node
A downloading this block 21 and then failing to import it because it doesn't have the parent block.

This pull request solves this situation by putting the peer into ancestry search when it detects a fork that is
"unknown".

#### [#10718]: Statement-store: Propagate all statements to newly connected peers
When a new node connects, we now propagate all statements in our store to them. This happens in bursts of ~1MiB messages
over time to not completley use up all resources. If multiple peers are connecting, round robin between them.


#### [#11393]: Add relay parent to V3 collation protocol advertisement
Adds a `relay_parent` field to the V3 collation protocol `AdvertiseCollation` message.

Also stores the relay parent and advertised descriptor version in pending collations
and held-off AssetHub advertisements, so that all sanity checks (including descriptor
version mismatch and relay parent mismatch) are correctly applied after fetching.


#### [#11273]: grandpa: Ensure to send `Commit` message before rebuilding the voter
When there is an authority change, grandpa internally rebuilds the `voter`. This leads to the node not sending the
`Commit` message for the finalized block. Light clients that follow these commit messages then need to wait for a
justification.

This pull request fixes the issue by directly sending a commit message, when an authority set change is detected. The
message is send before the voter is rebuild.

Closes: https://github.com/paritytech/polkadot-sdk/issues/9300

#### [#11231]: relay_chain: add max_relay_parent_session_age runtime API and HostConfiguration field
Adds a new `max_relay_parent_session_age` field to `HostConfiguration` and runtime API,
representing the maximum age of the session that a parachain block can build upon,
in terms of relay parent of the candidate.

Includes a storage migration (v12 -> v13) that initialises the new field to zero.


#### [#6029]: Implementation of RFC-123
Store a runtime upgrade in `:pending_code` before moving it to `:code` in the next block. It's gated by `system_version`
of the runtime
and is activated for runtimes with `system_version` >= 3.

#### [#11417]: Decrease the log level for claim queue inconsistency in `ClaimQueueState`
Decrease the log level of "Inconsistency while adding a leaf to the `ClaimQueueState`..." log
message from WARN to DEBUG.

On group rotations the backing groups are assigned to different cores. In this case we start
fetching the claim queue for the newly assigned core and the future assignments in
`ClaimQueueState` are no longer valid so overwriting them is the right
thing to do.

#### [#10421]: statement-store: New RPC result types
Moved submission failures from JSON-RPC errors into structured result types:
- Internal submission result type changed to hold more information for clients.
- The "statement_submit" method now returns enum with clear status variants (New, Known, Invalid, Rejected).
- NetworkPriority removed as we never used it.
- Updated and simplified the reputation system.
- Runtime API wasn't changed.

#### [#10493]: add external transient_storage to pallet_revive::ExecConfig
**Description**

This PR adds the ability to supply external copy of `TransientStorage` to `pallet_revive::ExecConfig` to be used during
execution.
This is required by testing in foundry as we only enter `pallet_revive` during a `CALL` or `CREATE` instruction and we
need to carryover
the transient storage to other following calls as they happen within an external tx.
For example this is required to support more testing scenarious inside `foundry-polkadot` because only a subset of
execution happens on `pallet-revive`.
e.g:
```solidity
fn example_test() { // this entrypoint is executed on the side of `foundry-polkadot`
Example contract = new Example(); // happens on pallet-revive
contract.setTransientStorage(5); // happens on pallet-revive

uint256 result = contract.getTransientStorage(); // happens on pallet-revive and returns `0` aka the default value 
// `result` above is `0` because `transient_storage` is reset after every call to `pallet-revive` and in `foundry-polkadot`
// only `CALL` and `CREATE` within function calls(already executed on foundry's revm) are forwarded to `pallet-revive`,
// so for assertion below to pass we would need to have our external `transient_storage` instance to be manually supplied
// to pallet-revive and be persistent within the wrapping call
assertEq(5, result); // fails without this PR with `5 != 0` 
}
```
link for the test-file inside `foundry-polkadot`:
- [click
  me](https://github.com/paritytech/foundry-polkadot/blob/3ff8bf9fae5505e9a72335e33d4e816dbc6bea41/testdata/default/revive/TransientStorage.t.sol)

#### [#11214]: Kanas/migrating async std to tokio
Migrates bridge relay crates from the deprecated `async-std` to `tokio`. This includes updating sync primitives, task
spawning, and channels across several bridge-related crates to ensure compatibility with modern tokio-based runtimes.

#### [#10882]: statement-store: make encode/hash faster
Optimizes statement encoding and hashing by pre-allocating memory for the encoded buffer.
This reduces allocation overhead and improves performance, particularly when receiving
statements from multiple peers. Benchmarks show ~16% speedup when receiving statements
from 16 peers.


#### [#11053]: tracing-subscriber: Pin version to prevent ANSI colour code issues
Latest version of tracing-subscriber right now doesn't support ASNI colour codes correctly:
https://github.com/tokio-rs/tracing/issues/3378

So, the workaround right now is to pin it to `0.3.19`.


Closes: https://github.com/paritytech/polkadot-sdk/issues/11030

#### [#9947]: Proposer/BlockBuilder: Accept proof recorder & extensions
This pull request fundamentally changes how `Proposer` and `BlockBuilder` are handling the proof recorder and
extensions. Before this pull request the proof recorder was initialized by the `BlockBuilder` and the proposer
statically enabled proof recording or disabled it. With this pull request the proof recorder is passed from the the
caller down to the block builder. This also moves the responsibility for extracting the final storage proof to the
caller and is not part of the block builder logic anymore. The extensions are now also configurable by the caller and
are not longer "guessed" by the block builder.

This pull request also remvoes the `cumulus-client-proposer` crate as it is not really required anymore.

#### [#10201]: telemtry: Downgrade spam log to debug
This PR downgrade the telemetry warning log to debug.
- The log is causing a lot of noise in our test nets: https://grafana.teleport.parity.io/goto/fjTQ_vzDg?orgId=1

#### [#10884]: [benchmarking-cli] Add --keys-limit, --child-keys-limit and --random-seed to storage benchmarks
Adds three new CLI parameters to the `benchmark storage` subcommand:

- `--keys-limit=<N>`: Limits the number of top-level storage keys sampled for
  read/write benchmarks. When omitted all keys are used (previous behaviour).
- `--child-keys-limit=<N>`: When `--include-child-trees` is set, limits how many
  keys are sampled from each child trie (same semantics as `--keys-limit`).
- `--random-seed=<u64>`: Provides a deterministic seed for the random key
  selection, allowing benchmarks to be reproduced under the same conditions.

These options are useful when benchmarking chains with very large state, where
iterating over every key is impractical. A shared `select_entries` helper was
extracted into `keys_selection.rs` to eliminate duplication between the read and
write benchmarking paths.

#### [11458] Fix per-ancestor core assignment and add regression test
`get_our_core` was called with the leaf hash instead of the ancestor
hash when computing core assignments in update_view. This caused all scheduling
parent to get leaf''s core.


### Changelog for `Runtime Dev`

**ℹ️ These changes are relevant to:**  All of those who rely on the runtime. A parachain team that is using a pallet. A
DApp that is using a pallet. These are people who care about the protocol (WASM, not the meta-protocol (client).)


#### [#9086]: Make HRMP advancement rule more restrictive
This PR improves `check_enough_messages_included()` and makes the advancement rule more restrictive for HRMP.

#### [#11320]: Refactor XCM executor, introduce process_holding_transaction macro
The xcm executor's code is now cleaner in how it handles origin manipulation and transactionality.


#### [#10244]: [pallet-revive] add tracing for selfdestruct
Add tracing for selfdestruct

#### [#10831]: Fix fee handling of pay-over-xcm trait(s)
Changed how pay-over-xcm is handling delivery fees. The old behavior was effectively allowing free delivery for any
origin, and it was either burning innexistent tokens (noop at the end of the day), or it was minting "protocol fees"
into the treasury account out of thin air.

In practice, the traits were always used with waived fees configuration so this bug was never exploitable in production,
but it was there nonetheless.

Changed transfer-over-xcm and pay-over-xcm implementations to use the runtime's XCM config, rather than custom Router
and FeeHandler. This reduces the opportunity for misconfiguration since it relies on the message delivery and fee
handling configurations at consolidated at the runtime configuration level.

Waived locations for some system pallets were also correctly configured to explicitly allow what was previously
implicitly allowed by the buggy code.

#### [#10869]: [pallet-assets] add ForeignAssetIdExtractor to assets precompile
fixes https://github.com/paritytech/polkadot-sdk/issues/8659

Adds ForeignAssetIdExtractor which converts a u32 asset id to an XCM Location type.

#### [#11151]: [pallet-revive] Fix evm_sized and update call stipend
**Description**

  Fix evm_sized benchmark helper to use proper EVM init code instead of raw runtime bytecode.

  Previously, `evm_sized(size)`created a Vec of size `STOP` opcodes and passed it directly as the contract code.
  However, in the EVM deployment model the code supplied is init code (constructor), not runtime code. The EVM executes
  the init code and whatever it `RETURN`s becomes the stored runtime code. Passing raw `STOP` bytes meant the init code
  would immediately halt and return nothing, resulting in an empty contract, not a contract of the requested size.

 This PR replaces the implementation with proper EVM init code (PUSH3 size, PUSH1 0, RETURN) that returns size bytes
 from zero-initialized memory, producing a runtime code blob of exactly size bytes (all 0x00 / STOP opcodes). This makes
 the benchmark helper behave correctly and produce contracts whose `PristineCode` actually matches the requested size.

**Integration**
  No integration changes required. This only affects test/benchmark code behind `#[cfg(any(test, feature =
  "runtime-benchmarks"))]`.

**Review Notes**
  The new init code is 7 bytes:
```
  PUSH3 <b1> <b2> <b3>   // push the desired runtime code size (up to 16M)
  PUSH1 0x00              // push memory offset 0
  RETURN                  // return size bytes from offset 0
```

  EVM memory is zero-initialized, so RETURN(0, size) produces size bytes of 0x00 (STOP opcode). This runtime code is
  what gets stored in PristineCode and loaded on every subsequent call.
The size is encoded as 3 bytes (PUSH3), supporting sizes up to ~16M which is well above any practical benchmark need.

**Call stipend**
  Changed `determine_call_stipend()` to `CALL_STIPEND + DepositEvent` weight so the stipend
  covers emitting a LOG event on top of the base 2300 gas operations. Added reentrancy tests
  verifying that the stipend prevents a malicious receiver from calling back into the sender.

**Regenerated benchmark weights**
  Regenerated pallet-revive weights from CI benchmarks.

**Expected test failures**
  12 revert.sol differential tests added to the expectations file. These fail due to CI
  benchmark noise amplifying `ref_time_per_fuel`, reducing the PVM fuel budget per extrinsic.

#### [#10383]: Enable force debug in revive dev node
This change ensures that all types that implement `RuntimeDebug` are fully displayed in log output of the revive dev
node, instead of just showing `<wasm:stripped>`.

Unfortunately, the trait `RuntimeDebugNoBound`, that we also use frequently in pallet-revive, is not affected and will
still output `<wasm:stripped>` (it does not check for the `force-debug` feature flag, instead it only fully outputs
values when either one of the features `std` or `try_runtime` is enabled – this is something we implement as a general
change).

#### [#10472]: V3 Candidate Descriptor Support with Explicit Scheduling Parent
V3 candidate descriptors are validated in the runtime via `check_descriptor_version_and_signals`.
V3 candidates must include UMP signals and have a valid scheduling_parent. The runtime rejects
V3 candidates that violate these rules.


#### [#11115]: Remove MaxSessionKeysLength and MaxSessionKeysProofLength
fixes the issue #11083 where MaxSessionKeysLength and MaxSessionKeysProofLength were unnecessary because there are not
stored, just validated.

#### [#10309]: [pallet-revive] evm remove contract storage slot when writing all zero bytes
fixes https://github.com/paritytech/contract-issues/issues/216

When writing all zero bytes to a storage item, the item shall be deleted and deposit refunded.

#### [#10437]: Remove pallet::getter usage from merkel mountain range pallet
Advances #3326

#### [#11322]: [pallet-assets-precompiles] Idiomatic Rust cleanups
**Summary**
- Remove explicit `return` statements in favor of idiomatic tail expressions
- Use `take()` instead of `get()` + `remove()` for atomic map operations
- Remove redundant type conversions (`.into()`, `H160::from()`)
- Flatten nested `if let` using `let...else` pattern in migration logic
- Fix typos and comments (doc comment, module description, duplicate license header)
- Added pallet-assets-precompiles to westend benchmark

#### [#11044]: [pallet-assets-precompiles] Add EIP-2612 permit support for gasless approvals
fixes https://github.com/paritytech/polkadot-sdk/issues/8660

- Implements EIP-2612 permit functionality for ERC20 asset precompiles, enabling gasless token approvals via signed
  messages
- Adds new `permit` pallet to manage nonces and EIP-712 signature verification
- Extends `IERC20.sol` interface with `permit()`, `nonces()`, and `DOMAIN_SEPARATOR()` functions

#### [#10729]: [FRAME] Bounties return balance and assets on close
Ensures that bounties that got closed with `close_bounty` will return the maximal possible
amount of Native balance and specific relevant Assets.  
This fixes an issue where closed bounties would not refund any balance to the treasury because
assets were blocking the withdrawal through account references.

#### [#10612]: revive eth-rpc Add Polkadot_postDispatchWeight rpc methods
Add a new RPC method to return the post-dispatch weight of a transaction


#### [#10460]: Remove pallet::getter usage from sassafras pallet
This PR removes all pallet::getter occurrences from pallet-sassafras.


#### [#10842]: Remove unused code in staking-async
- remove the `reward-fn` from `pallet-staking-async`. This crate is no longer needed.
- rename `ahm-test` to `integration-tests`

#### [#11144]: Snowbridge: receipt verification with alloy primitives
The new verifier checks both the root and the exact receipt key (transaction index), aligning with how proofs are
generated and preventing proofs that follow a valid hash chain but reference the wrong key.

#### [#10634]: Remove uses of sp-debug-derive/force-debug feature
Removes the `force-debug` feature flag from `sp-debug-derive` dependencies across the codebase.
This feature has been a no-op since #10582 and can be safely removed without any behavioral change.


#### [#10678]: Add relay chain state proof API for parachains
Adds `KeyToIncludeInRelayProof` runtime API allowing parachains to request specific
relay chain storage keys, including child tries, to be proven. Parachains can access
this verified state via `OnSystemEvent::on_relay_state_proof()` hook.


#### [#11037]: Consolidate pallet-assets metadata benchmarks into single get_metadata benchmark
**Summary**

Consolidates the three identical `get_name`, `get_symbol`, and `get_decimals` benchmarks into a single `get_metadata`
benchmark. This addresses the follow-up from #10971 where it was noted that these benchmarks perform the same operation
(`Pallet::get_metadata()`).

**Changes**

### Benchmarks
- **`substrate/frame/assets/src/benchmarking.rs`**
  - Replaced `get_name`, `get_symbol`, `get_decimals` with single `get_metadata` benchmark
  - Updated verification to check all three metadata fields (name, symbol, decimals)

### Weight Functions
- **`substrate/frame/assets/src/weights.rs`**
  - Replaced `get_name()`, `get_symbol()`, `get_decimals()` with single `get_metadata()` in `WeightInfo` trait
  - Updated implementations for `SubstrateWeight<T>` and `()`

### Precompile
- **`substrate/frame/assets/precompiles/src/lib.rs`**
  - Updated `name()`, `symbol()`, and `decimals()` methods to all charge `get_metadata()` weight

### Cumulus Runtimes
Updated weight implementations in:
- `asset-hub-rococo`: `pallet_assets_foreign.rs`, `pallet_assets_local.rs`, `pallet_assets_pool.rs`
- `asset-hub-westend`: `pallet_assets_foreign.rs`, `pallet_assets_local.rs`, `pallet_assets_pool.rs`

**Rationale**

All three original benchmarks were measuring the exact same operation - a single metadata storage read. Consolidating
them:
1. Reduces code duplication
2. Simplifies the `WeightInfo` trait
3. Accurately reflects that `name()`, `symbol()`, and `decimals()` have identical costs

Closes follow-up from https://github.com/paritytech/polkadot-sdk/pull/10971#discussion_r2782977769

#### [#10907]: Update the resolc and retester versions
**Summary**

This PR allows us to use nightly versions of the resolc compiler in the differential tests CI which include fixes not
yet available in the published version of the compiler. It also bumps the commit hash of differential tests used to a
version that allows for gas limits to be specified manually to circumvent the issue observed in
https://github.com/paritytech/contract-issues/issues/259

#### [#10359]: Remove invulnerables form staking-async
The 'staking-async' pallet has inherited the list of invulnerable validators from the 'staking' pallet, but these are no
longer used. We can therefore remove them, together with additional clean-up. This includes removing the
'set_invulnerables(...)' call together with the 'Invulnerables<T: Config>' storage type.

#### [#10524]: Fix Differential Testing CI Flakiness
**Description**

This PR updates the commit hash of the revive differential testing framework that we use to fix the flakiness we
observed in CI. It was fixed in the framework by caching the chainspec of the node's we spawn so that the chainspec is
only generated once and used for all of the nodes.

#### [#10922]: [pallet-revive] small improvements
Small safety and completeness improvements for pallet-revive:

- Add selfdestruct call tracing: Emit terminate trace after successful SELFDESTRUCT
- Add debug assertions for unsafe bytecode operations: in relative_jump, absolute_jump, and read_slice
- Remove transmute in i256 sign detection: Replace unsafe { core::mem::transmute } with explicit conditional logic for
  determining Sign::Zero vs Sign::Plus

#### [#10924]: revive: cap remaining_gas to u64::MAX in Substrate_execution
**Summary**

- Fixes proxy contract calls failing with OutOfGas when using ReviveApi.call
  (Substrate runtime API)
- The same calls succeed through eth_transact (Ethereum RPC)

see https://github.com/paritytech/contract-issues/issues/256

**Problem**

When calculating resource limits for nested calls through
`substrate_execution::new_nested_meter`, the ratio-based scaling fails when
`deposit_left` is very large (e.g., `u128::MAX` default for unlimited deposit).

The calculation flow:
1. `remaining_gas = weight_gas + deposit_gas` → huge number (deposit dominates at ~10^38)
2. Contract requests all gas: `requested_gas = u64::MAX` (~10^19)
3. `ratio = requested_gas / remaining_gas` ≈ 0.0000000000000027
4. `nested_weight_limit = ratio × weight_left` ≈ 0
5. Nested call immediately fails with OutOfGas

**Solution**

Cap `remaining_gas` to `u64::MAX` since Ethereum gas is a u64 value. This ensures
the ratio is 1.0 when a contract requests all gas, giving the nested call the full
remaining weight.

**Test plan**

- [x] Verified fix resolves the issue with proxy contracts (TransparentUpgradeableProxy)
- [ ] Existing tests pass


#### [#10732]: Use the revive-differential-tests reusable action
**Description**

This PR changes how we run differential tests. The `revive-differential-tests` repo now ships with a reusable action
which we use to run the differential tests.

#### [#10444]: Improve `charge_transaction_payment benchmark` ergonomics
Adds a `setup_benchmark_environment()` hook to allow runtimes to configure
required state before running the benchmark (e.g., setting block author for
fee distribution). Also fixes `amount_to_endow` calculation to use actual
computed fee and ensure it meets the existential deposit.


#### [#10698]: [pallet-revive] added trybuild test for precompile compile-time checks
fixes https://github.com/paritytech/polkadot-sdk/issues/8364

This PR adds compile-time tests using try_build to validate invariants enforced on registered precompiles. The tests
ensure collision detection and related compile-time checks are correctly triggered and remain enforced.

#### [#10716]: Migrate `pallet-example-offchain-worker` to use `TransactionExtension` API
Migrates `pallet-example-offchain-worker` from the deprecated `ValidateUnsigned` trait
to the modern `TransactionExtension` API using the `#[pallet::authorize]` attribute.

This change demonstrates how to validate unsigned transactions using the new approach,
which provides better composability and flexibility for transaction validation.

The pallet now uses `#[pallet::authorize]` on unsigned transaction calls to validate:
- Block number is within the expected window
- Price data is valid
- For signed payloads, signature verification using the authority's public key

This serves as a reference example for other pallets migrating away from `ValidateUnsigned`.

#### [#10567]: [Revive] Fix construction of negative zero SignedGas
Closes https://github.com/paritytech-secops/srlabs_findings/issues/603

This fixes an issue where a zero `SignedGas` value can be constructed that uses the `Negative` variant. The rest of the
code relies on the invariant that zero `SignedGas` has always the `Positive` variant and this is also documented in the
code.

#### [#10816]: Update to Rust 1.93
Updates the Rust toolchain to version 1.93. This includes fixes for new compiler warnings,
updated UI test expectations, and fixes for broken rustdoc intra-doc links that are now
detected by the stricter rustdoc in Rust 1.93.

#### [#11263]: XCMP: implement `ConcatenatedOpaqueVersionedXcm` negotiation
Follow-up for: https://github.com/paritytech/polkadot-sdk/pull/9588

As part of https://github.com/paritytech/polkadot-sdk/pull/9588 we added a new `ConcatenatedOpaqueVersionedXcm` XCMP
page format (which uses double encoded XCMs), but we didn't switch to always using it, since we don't know which
parachains support it. This PR introduces a negotiation strategy between parachains in order to switch to using the
`ConcatenatedOpaqueVersionedXcm` format when supported.

The high-level idea is the following:
- let's say we have an HRMP channel between parachains A and B
- parachain A is updated and starts supporting `ConcatenatedOpaqueVersionedXcm`
- when the sending queue is empty, it sends a notification to parachain B that it supports
  `ConcatenatedOpaqueVersionedXcm`. Basically it sends an empty `ConcatenatedOpaqueVersionedXcm` page. This notification
  is sent only once during the entire lifetime of the HRMP channel.
  - if parachain B supports `ConcatenatedOpaqueVersionedXcm`, it starts sending `ConcatenatedOpaqueVersionedXcm` pages
    to parachain A instead of `ConcatenatedVersionedXcm`
  - if parachain B doesn't support `ConcatenatedOpaqueVersionedXcm`, it doesn't do anything for the moment and they
    continue using `ConcatenatedVersionedXcm`. When parachain B is updated, it sends a similar notification to parachain
    A that it supports `ConcatenatedOpaqueVersionedXcm` (basically it also sends an empty
    `ConcatenatedOpaqueVersionedXcm` page).
- when parachain A receives a `ConcatenatedOpaqueVersionedXcm` page from parachain B, it concludes that parachain B also
  supports `ConcatenatedOpaqueVersionedXcm` and starts using `ConcatenatedOpaqueVersionedXcm` instead of
  `ConcatenatedVersionedXcm` when sending messages to parachain B

The information of whether a recipient parachain supports `ConcatenatedOpaqueVersionedXcm` or if a notification related
to the `ConcatenatedOpaqueVersionedXcm` support was sent to it is stored in `OutboundXcmpStatus` by adding a new `flags`
field. For this we also need to do a migration (`cumulus_pallet_xcmp_queue::migration::v6::MigrateV5ToV6`).

#### [#10849]: pallet-revive: Enable call_invalid_opcode test
Fixes https://github.com/paritytech/contract-issues/issues/206

This PR enables the call_invalid_opcode test, which verifies that the INVALID opcode consumes all forwarded gas when
executed in a nested call. The underlying issue was fixed in the following PRs:
https://github.com/paritytech/revive/pull/433
https://github.com/paritytech/polkadot-sdk/pull/9997

#### [#10830]: Rework EC Hostcalls
Rework all elliptic curve hostcalls: BLS12-381, BLS12-377, BW6-761,
Ed-on-BLS12-377, and Ed-on-BLS12-381-Bandersnatch.

Changes:
- Use affine representation for point arguments instead of projective.
  `mul_projective_*` hostcalls renamed to `mul_*` and now accept/return
  affine points. `msm_*` hostcalls now return affine points.
- Eliminate host-side memory allocation. Hostcalls now take an output
  buffer parameter (`&mut [u8]`) instead of returning allocated memory.
- Hostcalls now return `Result<(), Error>` (marshaled as `u32`) to signal
  success or error conditions.

New pass-by strategy (in `sp-runtime-interface`):
- `PassFatPointerAndWrite`: Passes a mutable buffer pointer to the host.
  The host creates a zero-initialized buffer, the host function writes to it,
  and the result is written back to guest memory.

#### [#10843]: Expand multisig pallet tests
Expand the multisig pallet tests to cover more cases. In particular, those concerning out-of-order signatories,
and sender in signatories. Also, reword extrinsic comments.


#### [#10662]: Bulletin as parachain missing features
This PR adds the required support and features for running Bulletin as a parachain. It is a top-level PR that merges
three partial features:

1. Add transaction_index::HostFunctions with NO-OP impl to the Cumulus ParachainSystem validate_block for
   Polkadot-prepare/execute-worker
2. Add custom inherent provider for pallet-transaction-storage to omni node
3. Configurable RetentionPeriod feeded to the inherent provider over runtime API

This PR also refactors `pallet-transaction-storage` and `sp-transaction-storage-proof` (the `new_data_provider` inherent
provider), both of which rely on a hard-coded `DEFAULT_RETENTION_PERIOD`. This PR:
- adds a new configurable argument `retention_period` to the `new_data_provider`
- introduces the `TransactionStorageApi::retention_period` runtime API, which the runtime can specify arbitrary
- provides an example of using `new_data_provider`, with the node client calling the runtime API when constructing
  inherent provider data

#### [#10863]: parachain-system: Ensure left-over message budget fits into the PoV
Ensure we check the buget against the remaining proof size in the block.

#### [#10911]: [pallet-revive] Fix EXTCODESIZE and EXTCODEHASH for mocked addresses
Fixes EXTCODESIZE and EXTCODEHASH opcodes for mocked addresses. Previously, these opcodes did not check the mock
handler, causing them to return values indicating no code exists at mocked addresses. Fixed by adding `mocked_code`
method to `MockHandler` trait to provide dummy bytecode.

#### [#10328]: pallet-revive: eth-rpc improve submit
"eth_sendRawTransaction" should only return a tx hash if the transaction is valid.
With these udpates, we now listen to the tx event stream and only return when the Ready or Future
is emitted.

#### [#10635]: [pallet-revive] remove unstable host function sr25519_verify
fixes part of https://github.com/paritytech/polkadot-sdk/issues/8572

#### [#11334]: Expose ECC host functions
Changes:
- Expose host functions for `BLS12-381`, `Ed-on-BLS12-381-Bandersnatch`, `Pallas`, `Vesta` for parachains
- Add new executor param `EnabledHostFunction` that can be used to enable host function usage.

These were ratified in [RFC 163](https://github.com/polkadot-fellows/RFCs/pull/163). The missing Pasta curves will be
added later https://github.com/paritytech/polkadot-sdk/pull/11035. We will use these on the people chain only.

#### [#11194]: MBM: Add ForceUnstuck handler
For chains doing governance we should be force-unstucking the chain on failed MBMs instead of freezing. This is
equivalent to single-block-migration error handling.
Would have been useful for https://github.com/polkadot-fellows/runtimes/pull/1085

Changes:
- Add a handler to unlock all locked calls and proceed

#### [#11276]: pallet-revive: use u128 Balance in test config
Update the pallet-revive Test runtime configuration to use `u128` instead of `u64` for the `Balance` type
It makes tests closer to production configs where Balance is typically u128

#### [#10735]: Genesis Patch Support for Frame Omni-Bencher
This update adds a --genesis-patch CLI option to frame-omni-bencher,
allowing users to apply custom JSON patches to genesis state during
benchmarking. This enables advanced testing scenarios like stress
testing with many accounts without modifying chain specifications.


#### [#11441]: [pallet-assets] fix: decrement supply when refund burns balance
When a user calls `refund` with `allow_burn = true`, their token balance is destroyed, but the asset's total supply was
never updated. This caused `total_issuance()` to overcount. The fix decrements supply and emits a `Burned` event,
consistent with how every other burn path works.

In production, burning path is rarely triggered. The fungibles trait interface always passes `allow_burn = false`, so
only users manually submitting the refund extrinsic with the burn flag would hit it.

Follow-up issue for migrating the discrepancy (observed on Westend):
https://github.com/paritytech/polkadot-sdk/issues/11443.

Fixes #10412

#### [#10540]: Tighten length estimation during dry running
The length of the RLP-encoded Ethereum transaction will have an effect on the transaction cost (as
pallet-transaction-payment charges a length fee) and therefore the required Ethereum gas.

During dry running we need to estimate the length of the actual RLP-encoded Ethereum transaction that will submitted
later. Some of the parameters that determine the length will usually not be provided at the dry running stage yet:
`gas`, `gas_price` and `max_priority_fee_per_gas`.

If we underestimate the actual lengths of these parameters, then the gas estimate might be too low and transaction
execution will run out of gas. If we over estimate, then the pre-dispatch weight will be unreasonably large and we risk
that a transaction that might still fit into a block, won't be put into the block anymore, which leads to lower block
utilization.

**Current Approach**
The current approach is to just assume that maximal possible length for these fields, which results when they have the
maximum possible value, `U256::MAX`, due to how RLP encoding works. This is a gross over estimation.

**New Approach**
In practice there won't be gas requirements and gas estimates that are more than `u64::MAX` and therefore we assume this
as the maximal value for `gas`.

For `gas_price` and `max_priority_fee_per_gas` we assume that the caller will use the current base fee and will scale it
be some small amount so that the RLP encoding is at most one byte longer than the RLP encoding of the base fee. We
achieve that by determining the RLP encoding of the base fee multiplied by 256.

#### [#10963]: revive-eth-rpc: Use pending block for estimate_gas in dev mode
Use Pending as the default block for eth_estimateGas in dev mode, matching Anvil/EDR behavior. Non-dev mode continues to
use Latest (go-ethereum behavior).

Refs https://github.com/paritytech/contract-issues/issues/261

#### [#11384]: Remove Bandersnatch SW form from host calls
Remove the Short Weierstrass (SW) form host calls for Ed-on-BLS12-381-Bandersnatch curve,
aligning with RFC-0163 which only specifies TE form host calls for this curve.
SW form operations are still available but will be computed entirely within the runtime.
Alternatively, users can map SW to TE representations to leverage the remaining TE host calls.


#### [#10958]: Enforce match_arm_blocks = true for consistent formatting
Flips `match_arm_blocks` from `false` to `true` in rustfmt configuration to ensure
all multi-line match arm bodies are wrapped in braces consistently.

This is a formatting-only change with no functional impact. It enforces a single
valid style for match arms, eliminating ambiguity that caused unnecessary diffs
when LLMs generated code with braces.


#### [#10302]: Fix termination
This PR fixes up termination by changing the behavior to:

- The free balance (without ed) should be send away right away to the beneficiary and not be delayed like the contract
  deletion.
- The ed and storage deposit will be send away only when terminating but to the origin (delayed).
- The scheduling of the terminate needs to be reverted if the scheduling frame reverts.
- `SELFDESTRUCT` should be allowed inside the constructor. The issuing contract will exist as account without code for
  the remainder of the transaction.
- The `terminate` pre-compile should revert if delegate called or its caller was delegate called. This is just my
  opinion but if we are changing semantics we can might as well add some security. We are increasing the attack surface
  by allowing the destruction of any contract (not only created in the current tx).


**Other fixes**
- Storage refunds should no longer use `BestEffort`. This is necessary to fail refunds in case some other locks (due to
  participation in gov for example) prevent sending them away. This is in anticipation of new pre-compiles.
- Moved pre-compile interfaces to sol files and made them available to fixtures
- Added some Solidity written tests to exercise error cases


**Further tests needed**

Those should all be written in Solidity to test both backends at the same time. No more Rust fixtures.

@0xRVE can you take those over as I am ooo.

- Test that checks that scheduled deletions do properly roll back if a frame fails
- Test that value send to a contract after scheduling for deletion is send to the beneficiary (different from Eth where
  this balance is lost)
- Add tests that use `SELFDESTRUCT` to `Terminate.sol`. Need https://github.com/paritytech/devops/issues/4508 but can be
  tested locally with newest `resolc`.

#### [#10397]: Update the commit hash of the revive-differential-tests
**Description**

This is a PR that updates the commit hash of the revive-differential-tests framework and the compilation caches to a
version that includes fixes to certain tests that used hard-coded gas values. The compilation caches required an update
since this was a change to the contract's code.

#### [#11389]: Fix: AssetTrapped event with Fungible(0) due to `SwapFirstAssetTrader::buy_weight` for exact trades
When `PayFees` contained the exact quoted fee, `SwapFirstAssetTrader::buy_weight` produces zero swap change. This
0-amount credit was unconditionally wrapped into an `AssetsInHolding` entry, which propagated through `fees` →
`refund_surplus` → `holding` → `drop_assets`, emitting an `AssetsTrapped` event with `Fungible(0)` that fails to decode.

This PR simply guards that by checking if value is 0 before putting it into the holding, and omitting the step if the
value is 0.

Closes #11388

#### [#9461]: Expose migrating keys
This PR introduces the ability for multi-block migrations to declare which storage prefixes they will modify,
enabling external tooling and monitoring systems to identify potentially affected state during migration execution.

Implement `migrating_prefixes()` in your SteppedMigration to declare modified storage prefixes.
For migration collections, use `nth_migrating_prefixes()` to retrieve prefixes by index.


#### [#10749]: [pallet-revive] fixtures compilation fix for rust 1.92.0
Fix this error after upgrading to rustc 1.92.0:

```
  error: panic_immediate_abort is now a real panic strategy!
  Enable it with `panic = "immediate-abort"` in Cargo.toml,
  or with the compiler flags `-Zunstable-options -Cpanic=immediate-abort`.
  In both cases, you still need to build core, e.g. with `-Zbuild-std`
    --> /Users/robert/.rustup/toolchains/1.92.0-aarch64-apple-darwin/lib/rustlib/src/rust/library/core/src/panicking.rs:36:1
```

#### [#10638]: [pallet-revive] remove unstable host function ecdsa_to_eth_address
fixes part of https://github.com/paritytech/polkadot-sdk/issues/8572

#### [#10597]: Introduce pallet-dap-satellite and redirect system burns to DAP
- Introduce pallet-dap-satellite for system chains (RC, BridgeHub, Coretime, People,
Collectives) to collect funds that would otherwise be burned, staging them for periodic
transfer to the central DAP on AssetHub.
- DAP and DAP satellite  implement `OnUnbalanced<Credit>` to intercept every Credit before
it's dropped/burned and resolve it into the buffer account instead.
- Redirect all system burns to DAP or DAP satellite on Westend system chains, including
tx fees, dust removal, coretime revenue burns and EVM gas rounding burns.

#### [#10902]: [pallet-revive] Enforce weight limit on dry-run RPC calls
**Summary**

- Makes the `weight_limit` field in `TransactionLimits::EthereumGas` non-optional, enforcing bounded execution on all
  Ethereum-style calls
- Updates `dry_run_eth_transact` to use `evm_max_extrinsic_weight()` as the weight limit, preventing unbounded
  computation during dry-run RPC calls
- Simplifies metering code by removing `Option` handling for weight limits in Ethereum execution mode

#### [#10517]: [pallet-revive] remove disabled host functions terminate and set_code_hash
fixes part of https://github.com/paritytech/polkadot-sdk/issues/8570

Removes the following disabled host functions:
- `terminate`
- `set_code_hash`

#### [#10387]: pallet-revive: add DebugSetting for bypassing eip-3607 for contracts and precompiles
Adds a new DebugSetting option which, if enabled, allows transactions coming from contract accounts or precompiles.
This is needed so that test nodes like anvil can send transactions from
contract or precompile accounts, a widely-used feature in tests.

#### [#9925]: Staking-Async + EPMB: Migrate operations to `poll`
Migrate staking-async and its election provider pallet to `on_poll`

#### [#10324]: Cleanup HRMP channels that were force removed from RC state
Cleanup old LastHrmpMqcHeads entries when the corresponding channel was remove from RC state

#### [#10315]: Introduce `MaxParachainBlockWeight` and related functionality
This PR introduces `MaxParachainBlockWeight` to calculate the max weight per parachain block dynamically.
This is a preparation for Block Bundling which requires that the maximum block weight is dynamic.
Block bundling requires a dynamic maximum block weight because it bundles multiple blocks into one PoV.
Each PoV gets 2s of execution time and 10MiB of proof size. These resources need to be split up between
all the blocks of one PoV.

Additionally, this PR adds UMP message size tracking and enforcement across PoV to ensure that message
size limits are respected when multiple blocks are bundled in the same PoV.


#### [#11227]: pallet-revive: add zero-value transfer/send stipend tests
**Summary**

Add tests that verify the `AllowNext` reentrancy path is triggered for zero-value `transfer` and `send` calls.

### How solc 0.8.30 handles the 2300 gas stipend

| Solidity call | value | gas passed by compiler | Stipend source |
|---|---|---|---|
| `target.transfer(amount)` | > 0 | `0` | EVM adds 2300 automatically |
| `target.send(amount)` | > 0 | `0` | EVM adds 2300 automatically |
| `target.transfer(0)` | 0 | `2300` | Compiler injects explicitly |
| `target.send(0)` | 0 | `2300` | Compiler injects explicitly |
| `target.call{value: v}("")` | any | remaining gas | No stipend (forwards all gas) |

The zero-value case is the one detected by our `gas_limit == CALL_STIPEND` heuristic, which triggers `AllowNext`.

**Changes**

- Add `testTransferZero` / `testSendZero` to `Stipends.sol` fixture — these call `transfer(0)` and `send(0)` on EOA,
  DoNothingReceiver, and SimpleReceiver
- Add corresponding Rust tests that exercise the `AllowNext` path
- Add trace logs to the call stipend match for debugging

**Test plan**

- [x] `evm_call_stipends_work_for_transfer_zero` passes, logs show `gas_limit=2300` → `AllowNext`
- [x] `evm_call_stipends_work_for_send_zero` passes, logs show `gas_limit=2300` → `AllowNext`

#### [#10804]: Take the header size into account for the total block size
The `BlockSize` storage item (formerly `AllExtrinsicsLen`) in `frame-system` now includes the header overhead (digest
size and
empty header size) in addition to the extrinsic lengths. This ensures that block size limits
accurately account for the full block size, not just the extrinsics. Additionally, inherent
digests are now limited to 20% of the maximum block size to prevent oversized headers.

**Breaking Change**: `AllExtrinsicsLen` has been renamed to `BlockSize` to better reflect its purpose.
External code using `frame_system::AllExtrinsicsLen` must be updated to use `frame_system::BlockSize`.

The deprecated `BlockLength::max` and `BlockLength::max_with_normal_ratio` functions have been
replaced with the new builder pattern across the entire codebase. Use
`BlockLength::builder().max_length(value).build()` or
`BlockLength::builder().max_length(value).modify_max_length_for_class(DispatchClass::Normal, |m| *m = ratio *
value).build()`
instead.


#### [#10950]: fix(revive): handle transaction hash conflicts during re-org
**Summary**

Fixes a UNIQUE constraint violation when processing blocks after a re-org:
```
UNIQUE constraint failed: transaction_hashes.transaction_hash
```

**Problem**

When a blockchain re-org occurs:
1. Block A contains transaction TX1 → stored in `transaction_hashes`
2. Server restarts (clearing the in-memory `block_number_to_hashes` map)
3. Re-org happens, Block B (different hash) now contains the same TX1
4. INSERT fails because TX1 already exists with old block_hash

#### [#10971]: Implement IERC20Metadata for pallet-assets precompiles
Implements the missing ERC20 metadata functions (`name`, `symbol`, `decimals`) for the
pallet-assets precompile to provide full ERC20 compatibility. These functions are essential
for proper EVM wallet and tooling integration.

The precompile implementation reads metadata from pallet-assets storage and returns properly
formatted values with appropriate gas charging using dedicated weight functions. All functions
include proper error handling for missing metadata and invalid UTF-8 encoding.

Benchmarks have been added to measure the weight of metadata reads, and corresponding weight
functions have been implemented in the WeightInfo trait.

The IERC20.sol interface file has been reorganized to clearly separate and document methods
from the base IERC20 interface and the IERC20Metadata extension, with links to the original
OpenZeppelin contracts for better maintainability.


#### [#11457]: eth-rpc: add support for the earliest block tag
### Summary
1. Resolve the earliest block tag to the first known EVM block across RPC methods (eth_getBlockByNumber, eth_call,
   eth_getLogs, etc.)
2. Add a known_first_evm_block_for_chain() lookup for Polkadot, Kusama, Paseo, and Westend Asset Hubs so earliest works
   without historical sync
3. Fix tracing_block to propagate errors and handle genesis (no parent)

Fixes https://github.com/paritytech/polkadot-sdk/issues/11383

#### [#10427]: Fix assertion
**Description**
According to assertion message and comment("at least"), `T::MaxDebugBufferLen::get() > MIN_DEBUG_BUF_SIZE` should be
changed into `T::MaxDebugBufferLen::get() >= MIN_DEBUG_BUF_SIZE`
```rust
// Debug buffer should at least be large enough to accommodate a simple error message
const MIN_DEBUG_BUF_SIZE: u32 = 256;
assert!(
	T::MaxDebugBufferLen::get() > MIN_DEBUG_BUF_SIZE,
	"Debug buffer should have minimum size of {} (current setting is {})",
	MIN_DEBUG_BUF_SIZE,
	T::MaxDebugBufferLen::get(),
);
```
For this assertion, the assertion message indicates assertion will fail when max_storage_size > storage_size_limit,
which means it requires max_storage_size <= storage_size_limit, but assertion predicate is `max_storage_size <
storage_size_limit`. Based on the code semantics, assertion predicate should be changed into `max_storage_size <=
storage_size_limit`.
```rust
assert!(
	max_storage_size < storage_size_limit,
	"Maximal storage size {} exceeds the storage limit {}",
	max_storage_size,
	storage_size_limit
);
```

#### [#11167]: Tiny fixes for staking weights
Fixes a small weight miscalculation in staking pallet's on-poll hook.


#### [#10680]: revive: fix revive post_upgrade assert
Fix post_upgrade assertion logic in revive v2 migration

#### [#11216]: try state check for pallet babe
This PR introduces the try_state hook to pallet-babe to verify all key storage invariants.

closes part of https://github.com/paritytech/polkadot-sdk/issues/239

#### [#10380]: pallet-revive benchmark opcode fix
Benchmark opcode was using the invalid opcode instead of defining a new one.


#### [#10448]: wasm-builder: Only overwrite wasm files if they changed
When running two different `cargo` commands, they may both compile the same wasm files. When the second `cargo` command
produces the same wasm files, we are now not gonna overwrite it. This has the advantage that we can run the first
command again without it trying to recompile the project. Right now it would lead to the wasm files always getting
recreated, which is wasting a lot of time :)

#### [#10184]: Fix coretime partitioning and improve on-demand latency
Prior to this PR we would statically pre-populate the claim queue by
popping from assignment providers. This behavior was technically not
correct, because we would not advance blocks accordingly, which means if
in upcoming blocks a new assignment comes in, we would not properly
consider it. This is fixed in this PR, by no longer statically
pre-populating the claim queue in advance, but by building it on the fly,
while we check assignments based on the correct block number. We
essentially now fully simulate block chain advancement when peeking into
the future. This also simplifies session handling, as nothing needs to be
pushed back to the assignment provider anymore. Scheduler and core
handling got simplified.

A pleasant side-effect of this fix is, that on-demand orders are now
instant. Meaning, they will be in the claim queue in the very same block
the order gets processed.

Concrete changes:

- Move coretime assigner logic from separate pallet into scheduler
- Migrate on-demand pallet from v1 (affinity-based queues) to v2 (single
  queue) - this is done so we have an efficient API to fetch on-demand orders
  for all on-demand cores of a block at once.
- Scheduler v3 to v4 migration - remove claim queue storage.
- Removing obsolete `assigner_coretime` pallet.
- Updating on-demand benchmarks to reflect new queue structure (removed linear `s` parameter)

Breaking changes:
- `assigner_coretime` pallet functionality moved to `scheduler::assigner_coretime`
- On-demand pallet storage structure changed (handled by migration)
- Scheduler storage version updated to v4


#### [#10918]: Fix delegatecall callTracer addresses
**Summary**
- Fix address tracking in delegatecall operations for callTracer

**Changes**
- Update callTracer to correctly track addresses during delegatecall operations

**Test plan**
- Existing tests should pass
- Verify callTracer correctly reports addresses for delegatecall operations

#### [#10399]: Limit the authority to adjust nomination pool deposits
Up until this point, when EDs of chains using nomination pools were reduced, the subsequent reward account funds in
exces could be claimed by anyone, despite the fact that they had been typically provided at the beginning by the pool
owner. We therefore limit access to these funds only to the pool owner and the (optional) root account when EDs get
reduced. The restriction does not apply to the increase in EDs, as these imply that funds are transferred into the pool
rather than out of it.

#### [#10454]: staking: do not remove an invulnerable in case of bad solution
Invulnerables are not automatically removed from the Invulnerables storage when their solution is rejected.
Removal should occur only through governance, not automatically.
An operational or network issue that leads to an incomplete submission is much more likely than a bad faith action from
an invulnerable.

#### [#9184]: FixedPoint: Support parsing `x.y` format
This makes it easier to declare a fixed point value. The old format is also still supported.

#### [#10510]: [pallet-revive] fix delegate_call_contract in evm-test-suites
evm-test-suite was not correctly executing delegate_call_contract causing pallet-revive to silently reject the
delegatecall. After evm-test-suite was fixed we found that the trace for delegate calls is incorrect. This fixes it.

#### [#10919]: Add revive Substrate runtime-api integration tests for call & instantiate
**Summary**
- Add integration tests for revive runtime API
- Test Fibonacci contract deployment and execution via Substrate APIs

**Changes**
- Add test for Fibonacci contract call via runtime API
- Add test to verify large Fibonacci values run out of gas as expected
- Update dev-node runtime configuration for testing

**Test plan**
- Run new integration tests
- Verify runtime API correctly handles contract deployment
- Verify gas limits are enforced correctly

#### [#10340]: Remove "SolutionImprovementThreshold" logic
The threshold mechanism used by the `election-provider-multi-block` verifier pallet is no longer relevant. There are no
queued solutions to compare during the initial verification. Solutions are subsequently processed in order of decreasing
score, with the first verified solution being selected, while any remaining solutions are not verified.

#### [#10451]: Accept custom capacity for block notifier buffer
Add a setter for a custom block notifier

#### [#10928]: [pallet-revive] Fix gas_cost and weight_cost for nested calls in execution tracer
**Description**

This PR fixes `gasCost` and `weightCost` calculations in execution tracing (structLogs) for opcodes that spawn child
calls (CALL, DELEGATECALL, CREATE, etc.). Previously,  was  and  reflected pre-execution state rather than actual
consumption for any parent call.

This PR:
  - Computes  and `weightCost` in  as the cost of executing the opcode itself, excluding gas/weight consumed by child
    calls
  - Tracks child call consumption via a `PendingStep` stack and subtracts it when the parent opcode completes
  - Adds test coverage for nested call gas tracking in both EVM and PVM modes


**Difference from Ethereum/Geth**
  In Geth's opcode tracing,  for CALL-like opcodes includes the opcode's intrinsic cost plus all gas forwarded to child
  calls. In **our implementation**, `gasCost` reports only the opcode's intrinsic cost, **excluding forwarded gas**. The
  intrinsic cost includes:
  - The *CALL opcode's base cost
  - Post-call costs such as copying return data back to the caller's memory (e.g., CopyToContract)

#### [#10471]: [Revive] Change default value of eth_getStorageAt
Closes https://github.com/paritytech/contract-issues/issues/230

With this change `eth_getStorageAt` of the eth rpc always returns a 32 byte array. If the storage slot has never been
written before, it returns the 32 byte zero value as the default value. Before that it was the empty array.

#### [#10580]: Fix eth-rpc publish
Use the update subxt macro to generate the metadata in OUT_DIR;
not doing so generates the following error when we try to publish the package:


```
error: failed to publish to registry at https://crates.io

Caused by:
  the remote server responded with an error (status 403 Forbidden):
  this crate exists but you don't seem to be an owner.
  If you believe this is a mistake, perhaps you need to accept an
  invitation to be an owner before publishing.

```

see related subxt changes: https://github.com/paritytech/subxt/pull/2142

#### [#10393]: Add configuration to set Ethereum gas scale
This PR adds a new configuration parameter (`GasScale`) to pallet-revive that allows to change the scale of the Ethereum
gas and of the Ethereum gas price.

Before this PR, the Ethereum gas price is simply the next fee multiplier of pallet-transaction-payment multiplied by
`NativeToEthRatio`. Thus, on Polkadot this is 100_000_000 when the multiplier has its default value of 1.

The required gas of a transaction is its total cost divided by the gas price, where the total cost is the sum of the
transaction fee and the storage deposit.

This leads to a situation where the required gas for a transaction on revive is usually orders of magnitude larger than
the required amount of gas on Ethereum. This can lead to issues with tools or systems that interact with revive and hard
code expected gas amounts or upper limits of gas amounts.

Setting `GasScale` has two effects:
- revive's Ethereum gas price is scaled up by the factor `GasScale`
- resulting used/estimated gas amounts get scaled down by the factor `GasScale`.

**Technical Details**
Internally, revive uses exactly the same gas price and gas units as before. Only at the interface these amounts and
prices get scaled by `GasScale`.

**Recommended**
This PR sets `GasScale` for the dev-node to 50_000.

This is motivated by the fact that storing a value in a contract storage slot costs `DepositPerChildTrieItem +
DepositPerByte * 32`, which is `2_000_000_000 + 10_000_000 * 32` (= `2_320_000_000`) plancks. Before this change the gas
price was 1_000_000 wei, so that this
equated to 2_320_000_000 gas units. In EVM this operation requires 22_100 gas only.

Thus, `GasScale` would need to be about 100_000 in order for `SSTORE` to have similar worst case gas requirements.

**Resolved Issues**

This PR addresses https://github.com/paritytech/contract-issues/issues/18 but we also need to find an appropriate
`GasScale` for a mainnet installment of pallet-revive.

#### [#10905]: Bump pallet-staking-reward-fn
sp-arithmetic was bumped in a previous PR but not published yet on crates.io.
Both Polkadot-runtime-common (already bumped and not released in a previous PR) and pallet-staking-reward-fn depend on
it.
Bump pallet-staking-reward-fn so that Parity-publish CI job can correctly resolve sp-arithmetic.
Without this fix, we would end up with a dependency graph with two versions of sp-arithmetic with one of the two missing
a trait impl needed by Polkadot-runtime-common.

#### [#10554]: [pallet-revive] add EVM gas call syscalls
This PR adds two new syscalls for calls accepting EVM gas instead of Weight and Deposit.

This is an important change for the initial release as it will align PVM contracts closer to EVM (the problem can't be
solved in the Solidity compiler).

#### [#11282]: [CI] Download resolc from GitHub release instead of artifact
**Summary**

- Replace the hardcoded artifact-by-ID download of `resolc` in `tests-evm.yml` with a download from the GitHub release
  (`v1.0.0`)
- Artifact IDs expire and break CI; release assets are stable and versioned
- Installs `resolc` to `/usr/local/bin` and verifies the version before running tests

**Test plan**

- [ ] Verify the `tests-evm` workflow passes with the new download method

#### [#11160]: Grandpa `on_new_session()`: simplification + fix
Kill `Stalled::<T>` only if `schedule_change()` has succeeded

#### [#10686]: Weight: Put `must_use` above some of the functions
Some functions return the modified weight and the user must use this or the changes are lost. This way the compiler
informs the user.

#### [#10505]: pallet-aura: Extend `try_state` to also check `CurrentSlot`
This ensures that `CurrentSlot` matches `timestamp / slot_duration`. Which is especially important to ensure that no one
changed the `SlotDuration`.

#### [#11054]: pallet-revive: minor cleanups and fixes
**Summary**

Preparatory cleanup PR extracted from the EIP-7702 branch to simplify review.

- **Counter.sol uint64**: Change `uint256` to `uint64` in Counter/NestedCounter fixtures, to avoid U256 conversion in
  tests.
- **Remove k256 dependency**: Replace `k256::ecdsa::SigningKey` with `sp_core::ecdsa::Pair` in benchmark signing helpers
- **Debug log**: Add debug log for `eth_transact` Substrate tx hash
- **Formatting**: Fix indentation in call.rs closure body, remove stray blank line in lib.rs
- **RLP fix**: Fix `Transaction7702Signed` decoder field order (removed incorrect `gas_price` field at index 4, aligned
  with encoder)

#### [#10385]: Disable polkavm logging in `pallet-revive`
This PR adds configurable control over PolkaVM logging in `pallet-revive` to address performance degradation (details:
https://github.com/paritytech/polkadot-sdk/issues/8760#issuecomment-3499548774)

  - Upgrades PolkaVM to v0.30.0 which provides `set_imperfect_logger_filtering_workaround()`
  - Adds `pvm_logs` flag to `DebugSettings` to control PolkaVM interpreter logging
  - Disables PolkaVM logs by default (when `pvm_logs=false`), enabling them only when explicitly configured
  - Fixes performance issue where excessive PolkaVM logging was impacting block proposal times

  The logging can be re-enabled via debug settings when needed for troubleshooting.

Additionally:
- PolkaVM has been bumped globally across whole codebase.

#### [#11328]: runtime: Allow cross-session relay parents for parachain candidates
Modifies relay chain runtime to allow V3 parachain candidates referencing relay parents from older sessions, as long as
the session age does not exceed `max_relay_parent_session_age`.

Previously, relay parents were tracked in a flat buffer that was cleared on every session
change, meaning candidates could only reference relay parents from the current session.
This was a limitation for cross-session block production scenarios.

Key changes:
- `AllowedRelayParents` is now a `StorageDoubleMap` keyed by `(SessionIndex, Hash)`,
  preserving relay parent info across session boundaries.
- Old sessions are pruned based on `max_relay_parent_session_age` on each new block.
- The scheduling parent tracker (`AllowedSchedulingParentsTracker`) remains session-local
  and is cleared on session change as before.
- `paras_inherent` sanitization and `inclusion::process_candidates` now look up relay
  parents across sessions using the candidate's `session_index` field (V3 descriptors).
- Includes a storage migration from the old `AllowedRelayParents` `StorageValue` to the
  new `StorageDoubleMap` layout.


#### [#11193]: [eth-rpc]: cap block_number_to_hashes map size
When keep_latest_n_blocks (cache-size) is None, every processed block is inserted into the BTreeMap used for detecting
reorgs, but never removed, except during reorgs. Since reorgs deeper than 256 blocks are unlikely, cap the map at 256 to
prevent unbounded growth.

#### [#10558]: pin solc version to 0.8.30 in tests-misc.yml
pin solc version to 0.8.30 in tests-misc.yml

#### [#10663]: [WIP][pallet-revive] replaced binary erc20 fixtures with solidity fixtures
fixes https://github.com/paritytech/polkadot-sdk/issues/8566

#### [#10713]: Fix off-by-one error in child bounty limit validation
Fixes an off-by-one error in `pallet-child-bounties` where the `add_child_bounty`
function allowed creating `MaxActiveChildBountyCount + 1` child bounties instead of
being capped at `MaxActiveChildBountyCount`.

The validation check used `<=` instead of `<`, allowing the count to exceed the limit
by one. This fix changes the comparison to `<` and removes an unnecessary type cast.

This is a bug fix that ensures runtime configuration limits are properly enforced.

Fixes #10652


#### [#9452]: Add comprehensive test data for Ethereum trie root validation
### Summary

This PR adds comprehensive test data for validating Ethereum transaction and receipt trie root calculations in the
`revive` crate. It includes real-world Ethereum blocks covering all supported transaction types.

---

### Details

#### 🧪 Test Data

- **Expanded Test Fixtures**:
  - Added 3 Ethereum blocks with their receipts (2 from mainnet, 1 from Sepolia testnet)
  - Blocks include all supported transaction types (Legacy, EIP-2930, EIP-1559, EIP-4844)
  - Test data validates `transactions_root` and `receipts_root` calculations against real Ethereum data
  - Organized naming: `block_{block_number}_{network}.json` and `receipts_{block_number}_{network}.json`

#### 🛠️ Tooling

- **Test Data Collection Script**:
  - Added `get_test_data.sh` for fetching test data from live Ethereum networks
  - Simple curl-based script that can be extended with additional blocks

Builds on top of: https://github.com/paritytech/polkadot-sdk/pull/9418

Part of: https://github.com/paritytech/contract-issues/issues/139

#### [#10022]: Aura: Support automatic slot migration
This brings support to `pallet-aura` for automatically migrated the `Slot` on a change of `SlotDuration`. This is done
by `on_runtime_upgrade` of `pallet-aura`.

#### [#11035]: Add Pallas and Vesta curve host functions to sp-crypto-ec-utils
Add host function modules for the Pallas and Vesta elliptic curves,
following the established pattern used by existing curves (BLS12-381,
BLS12-377, BW6-761, Ed-on-BLS12-377, Ed-on-BLS12-381-Bandersnatch).

Each curve exposes two host functions:
- `msm_sw`: Multi-scalar multiplication (Short Weierstrass)
- `mul_sw`: Scalar multiplication (Short Weierstrass)

Pallas and Vesta form a curve cycle used in Halo 2 proof systems.
The `ark-pallas-ext` and `ark-vesta-ext` crates from arkworks-extensions
provide hookable curve configurations that delegate expensive operations
to the host.

New feature flags `pallas` and `vesta` are added and included in
`all-curves`.

#### [#10739]: build & deploy eth-rpc docker image for stable release
build and deploy eth-rpc docker image for stable branches


#### [#8175]: Snowbridge V2: Generic inbound message processing
**Description**

This PR adds a new `MessageProcessor` type to the `inbound-queue-v2` pallet's config.

This type allows to make the processing of inbound messages more generic, via the (also new) `MessageProcessor` trait,
which contains the following functions:

- `can_process_message`: a custom (light) preliminary check to ensure that the message can be processed without the need
  of entering the full `process_message` implementation yet.
- `process_message`: actually performs the custom inbound message processing logic.

**Motivation**

At the moment of inbound message processing, it might be the case that, for instance, there is no need to perform any
XCM related logic, as it could happen in solo-chain contexts.
By making use of the functionality included in this PR, projects using Snowbridge can leverage this customization,
implementing any kind of processing they need for inbound queue messages in a more flexible way.

**Note: XcmPayload's name change**

In this PR I also included a small name change for the `XcmPayload` enum. The proposed name it's just a plain `Payload`,
and it still contains the same fields as before.
The reason for this change is to generalize the concept of "raw" bytes we receive in the first variant. At the moment of
processing an inbound message, this bytes could be used not only as XCM but also as other kind of data.
This change doesn't imply further changes on the current Snowbridge smart contract implementations.

#### [#11203]: asset-hub-westend: restrict StakingOperator proxy to explicit utility  batch calls
Replace the RuntimeCall::Utility { .. } wildcard with explicit batch, batch_all, and force_batch calls only.
The wildcard unnecessarily exposed as_derivative, dispatch_as, and with_weight which have no legitimate use for staking
operations, and future utility pallet additions would be automatically exposed.

#### [#10721]: Integrate asset test utilities for asset hub westend
The PR migrates exchange_asset tests from integration tests to unit tests in the AssetHubWestend runtime and introduces
a shared helper to reduce duplication.

#### [#10693]: refund deposit_eth_extrinsic_revert_event on the base_weight
When an eth transaction succeed we refund the pre-charged revert_event.
The refund should be done on the base weight and not the weight_consumed, as the latest could be lower than the cost of
the revert_event

#### [#11184]: Fix `burn` call weight in balances pallet
Fix burn call weight in balances pallet

#### [#10982]: Meta Transactions - Benchmarking update
Update of benchmarking logic to remove possibility of `quadratic complexity` not being weighted when executed.
Introducing witness parameter that would define length of `meta_tx` encoded size.

Update of weight annotation to `saturating add` instead of `add`.

#### [#10697]: [frame-support] remove error reporting in `remote_transfer_xcm` for paid execution
The reason is that it is broken and will result in spamming errors until we fix it properly:
https://github.com/paritytech/polkadot-sdk/issues/10078.

#### [#10366]: [pallet-revive] update evm create benchmark
Add a benchmark for the EVM CREATE instruction.

We are currently reusing the `seal_instantiate` benchmark from PVM instantiation, which is incorrect because
instantiating an EVM contract takes different arguments and  follows a different code path than creating a PVM contract.

This benchmark performs the following steps:

- Generates init bytecode of size i, optionally including a balance with dust.
- Executes the init code that triggers a single benchmark opcode returning a runtime code of the maximum allowed size
  (qrevm::primitives::eip170::MAX_CODE_SIZE`).


#### [#10712]: [pallet-revive] remove code related to stable and unstable_hostfn
Part of https://github.com/paritytech/polkadot-sdk/issues/8572

Removes the proc macro `unstable_hostfn` and attribute `#[stable]` because they are not used anywhere.

#### [#11231]: relay_chain: add max_relay_parent_session_age runtime API and HostConfiguration field
Adds a new `max_relay_parent_session_age` field to `HostConfiguration` and runtime API,
representing the maximum age of the session that a parachain block can build upon,
in terms of relay parent of the candidate.

Includes a storage migration (v12 -> v13) that initialises the new field to zero.


#### [#11150]: Fix `sp-crypto-ec-utils` `no_std` compilation
Fix `encode_into` in `sp-crypto-ec-utils` for `no_std` targets.
In `no_std`, `&mut [u8]` does not implement `scale::Output`, so
passing it directly to `encode_to` fails to compile.
Introduces a thin `SliceOutput` adapter that implements `Output`
over a mutable byte slice.


#### [#10582]: Deprecate `RuntimeDebug` and replace it with `Debug`
I compared multiple builds which each other:


| Runtime | RuntimeDebug .compact.wasm | Debug .compact.wasm | Δ bytes | Δ % |
|---------|---------------------------|---------------------|---------|-----|
| westend-runtime | 10,004,155 | 10,093,902 | +89,747 | +0.90% |
| asset-hub-westend-runtime | 13,453,435 | 13,491,827 | +38,392 | +0.29% |
| bridge-hub-westend-runtime | 6,975,911 | 7,078,591 | +102,680 | +1.47% |
| collectives-westend-runtime | 6,660,307 | 6,725,426 | +65,119 | +0.98% |
| people-westend-runtime | 5,639,941 | 5,661,539 | +21,598 | +0.38% |
| coretime-westend-runtime | 5,667,343 | 5,689,961 | +22,618 | +0.40% |
| glutton-westend-runtime | 2,502,303 | 2,514,727 | +12,424 | +0.50% |


| Runtime | RuntimeDebug .compact.compressed.wasm | Debug .compact.compressed.wasm | Δ bytes | Δ % |
|---------|--------------------------------------|--------------------------------|---------|-----|
| westend-runtime | 1,911,531 | 1,918,414 | +6,883 | +0.36% |
| asset-hub-westend-runtime | 2,402,348 | 2,408,262 | +5,914 | +0.25% |
| bridge-hub-westend-runtime | 1,397,714 | 1,409,183 | +11,469 | +0.82% |
| collectives-westend-runtime | 1,265,180 | 1,268,329 | +3,149 | +0.25% |
| people-westend-runtime | 1,125,880 | 1,126,034 | +154 | +0.01% |
| coretime-westend-runtime | 1,132,311 | 1,135,300 | +2,989 | +0.26% |
| glutton-westend-runtime | 543,780 | 546,127 | +2,347 | +0.43% |


With `--features on-chain-release-build`:

| Runtime | RuntimeDebug .compact.wasm | Debug .compact.wasm | Δ bytes | Δ % |
|---------|---------------------------|---------------------|---------|-----|
| westend-runtime | 10,088,725 | 10,088,725 | 0 | 0.00% |
| asset-hub-westend-runtime | 13,491,318 | 13,491,318 | 0 | 0.00% |
| bridge-hub-westend-runtime | 7,078,244 | 7,078,244 | 0 | 0.00% |
| collectives-westend-runtime | 6,724,948 | 6,724,948 | 0 | 0.00% |
| people-westend-runtime | 5,640,009 | 5,661,591 | +21,582 | +0.38% |
| coretime-westend-runtime | 5,689,735 | 5,689,735 | 0 | 0.00% |
| glutton-westend-runtime | 2,504,593 | 2,517,004 | +12,411 | +0.50% |


| Runtime | RuntimeDebug .compact.compressed.wasm | Debug .compact.compressed.wasm | Δ bytes | Δ % |
|---------|--------------------------------------|--------------------------------|---------|-----|
| westend-runtime | 1,917,250 | 1,917,250 | 0 | 0.00% |
| asset-hub-westend-runtime | 2,408,382 | 2,408,382 | 0 | 0.00% |
| bridge-hub-westend-runtime | 1,409,259 | 1,409,259 | 0 | 0.00% |
| collectives-westend-runtime | 1,267,981 | 1,267,981 | 0 | 0.00% |
| people-westend-runtime | 1,126,034 | 1,130,613 | +4,579 | +0.41% |
| coretime-westend-runtime | 1,135,207 | 1,135,207 | 0 | 0.00% |
| glutton-westend-runtime | 545,344 | 548,753 | +3,409 | +0.63% |


This shows that the size increase is neglectable and not worth the increased hassle when it comes to debuggin inside
wasm.

Closes: https://github.com/paritytech/polkadot-sdk/issues/3005

#### [#9722]: [pallet-revive] opcode tracer
This PR introduces a **Geth-compatible execution tracer**
([StructLogger](https://geth.ethereum.org/docs/developers/evm-tracing/built-in-tracers#struct-opcode-logger)) for
pallet-revive

The tracer can be used to capture both EVM opcode and PVM syscall.
It can be used with  the same RPC endpoint as Geth StructLogger.


Since it can be quite resource intensive, It can only be queried from the node when the **DebugSettings** are enabled
(This is turned on now by default in the dev-node)

Tested in https://github.com/paritytech/evm-test-suite/pull/138


example:

```sh
❯ cast rpc debug_traceTransaction "<TX_HASH>" | jq

# or with options
# See list of options https://geth.ethereum.org/docs/developers/evm-tracing/built-in-tracers#struct-opcode-logger

 ❯ cast rpc debug_traceTransaction "<TX_HASH>", { "tracer": { "enableMemory": true } } | jq
```

The response includes additional fields compared to the original Geth debug RPC endpoints:

For the trace:
- `weight_consumed`: same as gas but expressed in Weight
- `base_call_weight`: the base cost of the transaction

For each step:
- `weight_cost`: same as gas_cost but expressed in Weight

For an EVM execution, the output will look like this

```json
{
  "gas": 4208049,
  "weight_consumed": { "ref_time": 126241470000, "proof_size": 4208 },
  "base_call_weight": { "ref_time": 9000000000, "proof_size": 3000 },
  "failed": false,
  "returnValue": "0x",
  "structLogs": [
    {
      "gas": 4109533,
      "gasCost": 3,
      "weight_cost": { "ref_time": 90000, "proof_size": 0 },
      "depth": 1,
      "pc": 0,
      "op": "PUSH1",
      "stack": []
    },
    {
      "gas": 4109530,
      "gasCost": 3,
      "weight_cost": { "ref_time": 90000, "proof_size": 0 },
      "depth": 1,
      "pc": 2,
      "op": "PUSH1",
      "stack": [
        "0x80"
      ]
    },
    {
      "gas": 4109527,
      "gasCost": 3,
      "weight_cost": { "ref_time": 90000, "proof_size": 0 },
      "depth": 1,
      "pc": 4,
      "op": "MSTORE",
      "stack": [
        "0x80",
        "0x40"
      ]
    }]
}
```

For PVM execution, each step includes additional fields not present in Geth:

- `args`: Array of syscall arguments (register values a0-a5) as hex strings
- `returned`: The syscall return value

These fields are enabled by default. To disable them, use `disableSyscallDetails: true`.

Example output with syscall details:

```json
{
  "gas": 97108,
  "gasCost": 131,
  "weight_cost": { "ref_time": 3930000, "proof_size": 0 },
  "depth": 1,
  "op": "call_data_load",
  "args": ["0x0", "0x4"],
  "returned": "0x2a"
}
```


#### [#10371]: Try State Hook for Pallet Assets
This PR introduces the try_state hook to pallet-assets to verify key storage invariants.

#### [#10593]: Align Common Functions between Bulletin and SDK
The PR aligns common functions between Bulletin and SDK.

#### [#11468]: Add a call to `.unvalidated()` for all eth-rpc interactions
**Description**

This PR updates the `eth-rpc` so that all interactions with subxt are unvalidated. This change was made to allow us to
use any `eth-rpc` version with any version of pallet revive given that there's no actual interface differences in the
runtime functions that we called. Before this change, we would get a lot of metadata mismatch errors for slightly older
versions of revive. Our assumption is that this happened due to us adding more runtime functions into pallet-revive's
runtime API which lead to the hash of the metadata being different, thus to the metadata mismatch.

#### [#10475]: try state check for pallet authority discovery
This PR introduces the try_state hook to pallet-authority-discovery to verify key storage invariants.

closes part of https://github.com/paritytech/polkadot-sdk/issues/239

#### [#10771]: Snowbridge: Describe the token location with the length field included to avoid collisions
For GeneralKey, two XCM junctions that differ only in length can currently produce the same description bytes,
and therefore the same TokenId. To avoid such collisions, this PR includes the length field in the describe function.
We do have several PNAs registered that could be affected by this change. However, these tokens are not currently in
use,
there have been no transfers and no tokens minted so far. As a result, simply re-registering these tokens should be
sufficient,
without requiring a runtime storage migration.

#### [#10682]: add must_use attributes
Add must_use attributes on arithmetic fns


#### [#10866]: Extend remote externalities `Client` and child storage query unit tests
This PR fixes a test failure in the remote externalities crate concerning optionality of child key loading.
It also adds follow-up tests to #10779 to check `Client` construction from valid/invalid URIs.


#### [#10920]: [pallet-revive] Fix storage deposit refunds in nested contract calls
fixes https://github.com/paritytech/contract-issues/issues/213 where storage deposit refunds failed in nested/reentrant
calls.

Problem
Storage refunds were calculated incorrectly when a contract allocated storage, then performed a nested call that cleared
it. Pending storage changes lived only in the parent FrameMeter, so child frames could not see them and refunds were
skipped.

Solution
Apply pending storage deposit changes to a cloned ContractInfo before creating nested frames. This makes the parent’s
storage state visible to child frames during refund calculation.

Implementation
- Added apply_pending_changes_to_contract() to apply pending diffs to ContractInfo
- Added apply_pending_storage_changes() wrapper on FrameMeter
- Applied pending storage changes before nested frame creation in exec.rs (3 locations)

#### [#10794]: [FRAME] Omni bencher run each benchmark at least 10 secs
- Ensure all benchmarks run for at least 10 seconds. Configurable with `--min-duration <s>`
- Turn off runtime logging in bench bot to reduce spam log output

#### [#10166]: Implement general gas tracking
This PR implements [the general gas tracking
spec](https://shade-verse-e97.notion.site/Revive-Resource-Management-2928532a7ab5808381b4e688fcc58838?pvs=74).

This PR ballooned into something much bigger than I expected. Many of the changes are due to the fact that all tests and
a lot of the other logic has some touch points with the resource management logic. Most of the actual changes in logic
are just in the folder `metering` of pallet-revive.

The main changes are that
- Metering now works differently depending on whether the transaction as a whole defines weight and deposit limits
  ("Substrate execution mode") or just an Ethereum gas limit ("Ethereum execution mode"). The Ethereum execution mode is
  used for all `eth_transact` extrinsics.
- There is a third resource (in addition to weight and storage deposits): Ethereum gas. In the Ethereum execution mode
  this is a shared resource (consumable through weight and through storage deposits).

**Metering logic**
Almost all changes in this PR are confined to the folder `metering` of pallet-revive. Before this PR there were two
meters: a weight meter and a gas meter. They have now been combined into a main meter called `ResourceMeter`. Outside
code only interacts with the `ResourceMeter` and not individually with the gas or storage meter. The reason is that in
Ethereum execution mode gas is a shared resource and interacting with one meter influences the limits of the other
meter.

Here are some finer points:
- The previous code of the gas and deposit meters has been moved to the `metering` folder
- Since outside code interacts only with the `ResourceMeter`, most functions now don't use a separate gas meter and
  deposit meter anymore but just a `ResourceMeter`
- Similar to the two two kinds of deposits meters (`Root` and `Nested`), there are two kind of `ResourceMeter`: the
  top-level `TransactionMeter` used at the beginning of a transaction and a `FrameMeter` used once per frame
- The limits of a `TransactionMeter` are specified through the `TransactionLimits` type, which distinguishes between
  Substrate and Ethereum execution mode.
- The limits of a `FrameMeter` is specified through the type `CallResources`, which can either be a) no limits (e.g., in
  the case of contract creation), or b) a weight and deposit, or c) a gas limit.
- The top level name of functions in the meters has been changed to be a bit more explicit about their purpose.
  - This applied particularly to the methods at the end of the lifecycle:
    - `enforce_limit` has been renamed to `finalize` as that describes the semantics better
    - `try_into_deposit` has been renamed to `execute_postponed_deposits`
- For absorbing a frame meter into its parent meter, there are two different absorption functions:
  - `absorb_weight_meter_only`: when a frame reverts. In this case we ignore all storage deposits from the reverting
    frame. We still need to absorb the observed maximum deposit so that we determine the correct maximum deposit during
    dry running.
  - `absorb_all_meters`: when a frame was successful
- The weight meter now has an `effective_weight_limit`, which needs to be recalculated whenever the deposit meter
  changes and is for optimization purposes.
- The limits of the gas meter and deposit meters are now an `Option<...>`. When it is `None`, then this represents
  unlimited meters and this is only used for Ethereum style executions (the meters are not really unlimited, there will
  be a gas limit that effectively limits the resource usages of the weight and deposit meters).
- In the weight meters, the `sync_to_executor` and `sync_from_executor` are a bit simplified and there is no need for
  `engine_fuel_left` anymore.

**Other Changes**
- The old name `gas` for weights has been consistently replaced by `weight`
- `eth_call` and `eth_instantiate_with_code` now take a `weight_limit` (used to ensure that weight does not exceed the
  max extrinsic weight) and an `eth_gas_limit` (the new externally defined limit)
- The numeric calculation in `compute_max_integer_quotient` and `compute_max_integer_pair_quotient` (defined in
  `substrate/frame/revive/src/evm/frees.rs`) are meant to divide a number by the next fee multiplier
- The call tracer does not take a `GasMapper` anymore as it will now be fed directly with the Ethereum gas values
  instead of weights
- Re-entrancy protection now has three modes: no protection, `Strict` protection and `AllowNext`
  - `AllowNext` allows to re-enter the same contract but only for the next frame. This is required to implement
    reentrancy protection for simple transfers with call stipends
  - For `Strict` protection we set `allows_reentry` of the caller to `false` before the creation of the new frame, for
    `AllowNext` we to it after the creation
- We define the max block gas as `u64::MAX` (as discussed with @pgherveou)
- I now calculate the maximal required storage deposits during dry running (called `max_storage_deposit` in the deposit
  meter). For example, if a transaction encounters a storage deposit that is later refunded, then the total storage
  deposit is zero. However, the caller needs to provide enough resources so that temporarily the execution does not run
  out of gas and terminates the call prematurely.
- The function `try_upload_code` now always takes a meter and records the storage deposit charge there
- In this PR I added logic to correctly handle call stipends (this fixes
  https://github.com/paritytech/contract-issues/issues/215)

**Fixes**
This fixes a couple of issues
- fixes https://github.com/paritytech/contract-issues/issues/215
- fixes https://github.com/paritytech/polkadot-sdk/issues/8362
- fixes https://github.com/paritytech-secops/srlabs_findings/issues/589
- fixes https://github.com/paritytech/contract-issues/issues/197
- fixes https://github.com/paritytech/contract-issues/issues/208
- fixes https://github.com/paritytech/contract-issues/issues/212

#### [#11153]: [eth-rpc]: add resumable block sync and improve CLI arguments
**Resumable block sync**
- New block_sync module syncs backward from the latest finalized block to the first EVM block, with restart-safe
  checkpoint tracking via a sync_state SQLite table.
- On restart, fills only the top gap (new blocks) and bottom gap (remaining backfill) without re-syncing completed
  ranges.
- Auto-discovers and persists `first_evm_block` — the lowest block with EVM support on the chain.
- Chain identity verification: validates stored genesis hash on startup to detect database reuse across different
  chains; verifies sync boundary hashes to detect pruned blocks on the connected node.

**CLI rework**
New `--eth-pruning` flag replaces `--database-url`, `--cache-size`, `--index-last-n-blocks`, and
`--earliest-receipt-block`:
- `--eth-pruning archive` (default): persistent on-disk DB with backward historical sync.
- `--eth-pruning <N>`: in-memory DB keeping the latest N blocks.


#### [#10780]: Fix pallet-revive-fixtures
Fixing two issues:

1. Build on rustc >= 1.92 was broken despite https://github.com/paritytech/polkadot-sdk/pull/10749. That PR was broken.
2. The nested cargo didn't properly inherit the parent toolchain (an older error). Leading to the situation where a
   `1.88` was only applied to the parent toolchain

Replacement for https://github.com/paritytech/polkadot-sdk/pull/10778.

#### [#11158]: Add timeout + people-westend to check-runtime CI
@Polkadot-api/check-runtime hangs in case the RPC endpoint is not reachable. A timeout is added to handle this case
gracefully.

Driven-by: add support for metadata-hash extension on `people-westend`  and add it to the list of chains to check.

#### [#10396]: `ExecuteBlock` split up seal verification and actual execution
`ExecuteBlock` exposes the `execute_block` function that is used by `validate_block` to execute a block. In case auf
AuRa the block execution includes the verification of the seal and the removal of the seal. To verify the seal, the
block executor needs to load the current authority set. The problem is that when we have storage proof reclaim enabled
and the host function is used in `on_initialize` before `pallet_aura_ext::on_initialize` (this is where we fetch the
authority set to ensure it appears in the proof) is called, it leads to `validate_block` returning a different size and
thus, breaking the block. To solve this issue `ExecuteBlock` is now split into seal verification and execution of the
verified block. In `validate_block` the seal verification is then run outside of the block execution, not leading to the
issues of reporting different proof sizes.


#### [#10511]: Minor pallet-scheduler documentation/unit test additions
This PR changes the Rust documentation of some extrinsic calls in the scheduler pallet.

It also adds some checks to the `Lookup<T>` storage item in a unit test to named task cancellation via
the anonymous cancellation call, and a unit test for the rescheduling of a failed named/anonymous task
to a full agenda.

#### [#10384]: XCM executor keeps track and resolves all imbalances created by XCM operations
Introduce "ImbalanceAccounting" traits for dynamic dispatch management of imbalances.
These are helper traits to be used for generic Imbalance, helpful for tracking multiple
concrete types of `Imbalance` using dynamic dispatch of these traits.

`xcm-executor` now tracks imbalances in holding.

Change the xcm executor implementation and inner types and adapters so that it keeps
track of imbalances across the stack.

Previously, XCM operations on fungible assets would break the respective fungibles' total
issuance invariants by burning and minting them in different stages of XCM processing pipeline.

This commit fixes that by keeping track of the "withdrawn" or "deposited" fungible assets
in holding and other XCM registers as imbalances. The imbalances are tied to the underlying
pallet managing the asset so that they keep the assets' total issuance correctness throughout
the execution of the XCM program.

Imbalances in XCM registers are resolved by the underlying pallets managing them whenever they
move from XCM registers to other parts of the stack (e.g. deposited to accounts, burned, etc).


### Changelog for `Node Operator`

**ℹ️ These changes are relevant to:**  Those who don't write any code and only run code.


#### [#10373]: Block import improvements
This PR fixes block import during Warp sync, which was failing due to "Unknown parent" errors - a typical case during
Warp sync.

Changes
 - Relaxed verification for Warp synced blocks:
The fix relaxes verification requirements for Warp synced blocks by not performing full verification, with the
assumption that these blocks are part of the finalized chain and have already been verified using the provided warp sync
proof.
- New `BlockOrigin` variants:
For improved clarity, two additional `BlockOrigin` items have been introduced:
  - `WarpSync`
  - `GapSync`
- Gap sync improvements:
Warp synced blocks are now skipped during the gap sync block import phase, which required improvements to gap handling
when committing the block import operation in the database.
- Enhanced testing:
The Warp sync zombienet test has been modified to more thoroughly assert both warp and gap sync phases.

This PR builds on changes by @sistemd in #9678

#### [#11224]: Prometheus: Bind external address to IPv6
Binds the prometheus external interface to `::`, so that the service is also reachable via IPv6.

#### [#10617]: statement-store: use many workers for network statements processing
Adds --statement-network-workers CLI parameter to enable concurrent statement validation from the network.
Previously, statements were validated sequentially by a single worker. This change allows multiple workers
o process statements in parallel, improving throughput when statement store is enabled


#### [#10662]: Bulletin as parachain missing features
- Node developers/operators could enable the transaction storage inherent data provider setup by using
  --enable-tx-storage-idp flag. This is especially useful in the context of bulletin chain.
- Node developers will set up the network `idle_connection_timeout` to 1h when using `--ipfs-server` flag, again, useful
  in the context of bulletin chain.


#### [#10752]:   Gap Sync: Skip Body Requests for Non-Archive Nodes
### Summary
This PR optimizes gap sync bandwidth usage by skipping body requests for non-archive nodes. Bodies are unnecessary
during gap sync when the node doesn't maintain full block history, while archive nodes continue to request bodies to
preserve complete history.
It reduces bandwidth consumption and database size significantly for typical validator/full nodes.

Additionally added some gap sync statistics for observability:
- Introduced `GapSyncStats` to track bandwidth usage: header bytes, body bytes, justification bytes
- Logged on gap sync completion to provide visibility into bandwidth savings

#### [#8541]: collator-protocol-revamp: CollationManager and subsystem impl
This PR adds a new experimental validator-side collator protocol subsystem implementation,
which can be enabled via the `--experimental-collator-protocol` CLI flag.

The new implementation introduces a reputation-based collator selection mechanism. Collators
are assigned scores based on the outcome of their submitted collations: valid included
candidates increase the score, while invalid collations or failed fetches decrease it.
When multiple collation advertisements are received, validators prioritize fetching from
higher-reputation collators first (with timestamp as a tiebreaker for equal scores).

#### [#1739]: Require proof for session key registration
Node operators will now need to provide a proof when registering their `SessionKeys` on chain.
A new rpc `author_rotateKeysWithOwner` is provided to generate the `SessionKeys` plus the `proof`.
Both values then need to be feed into `set_keys` as part of the transaction.
`author_rotateKeysWithOwner` is a replacement for `author_rotateKeys`.

#### [#10978]: Omni-node supports Polkadot-asset-hub
The `polkadot-omni-node` binary now supports Polkadot-asset-hub. Other system chains where already supported, but PAH
uses Ed25519, which makes it a special case.


#### [#10196]: Improve Warp Sync Logging
This update makes warp sync logs more useful. it shows a clear count of synced eras and removes
unnecessary block details during the proof phase, giving a better view of progress.

#### [#11407]: Statement Store: Introduce new CLI args
Introduce new CLI args to control statement store parameters.
Rework the statement store configuration, consolidating all parameters into a single structure.

#### [#10893]: Do not prune blocks with GrandPa justifications
Warp sync requires GRANDPA justifications at authority set change boundaries to construct proofs. When block pruning is
enabled, all block bodies are removed regardless of whether they contain important justifications. The pruned nodes can
then not be used to fetch warp proofs.
We now have the capability to filter which blocks can be safely pruned. For parachain nodes, everything can be pruned,
solochain nodes using grandpa keep blocks with justifications. This ensures warp sync ability within the network.


### Changelog for `Runtime User`

**ℹ️ These changes are relevant to:**  Anyone using the runtime. This can be a token holder or a dev writing a front end
for a chain.


#### [#10828]: [pallet-broker] add extrinsic to forcefully remove the potential renewal
Add an extrinsic allowing to forcefully remove the existing potential renewal from chain without the need to directly
manipulate the storage.

#### [#10767]: Fix auto-renew core tracking on immediate renew
**Summary**
Fix auto-renew tracking when `do_enable_auto_renew` triggers an immediate renewal. The auto-renew record now follows the
new core index returned by `do_renew`, preventing a stale core from being
renewed in the next sale rotation.

Discovered by the Darwinia Network team while attempting a renew.

**Problem**
When enabling auto-renew during the renewal window (`PotentialRenewals` at `sale.region_begin`), `do_enable_auto_renew`
immediately calls `do_renew`. That call can allocate a *different* core
index, but the auto-renew record was stored with the **old** core. On the next rotation, `renew_cores` attempts to renew
that stale core and emits `AutoRenewalFailed`, even though the workload has
already moved to the new core.

**Fix**
Capture the returned core index from `do_renew` inside `do_enable_auto_renew`, and store that core in `AutoRenewals`
(and the enable event).

**Tests**
- Added `enable_auto_renew_immediate_updates_core_and_renews`
- `cargo test -p pallet-broker`


Closes: https://github.com/paritytech/polkadot-sdk/issues/10006

#### [#10697]: [frame-support] remove error reporting in `remote_transfer_xcm` for paid execution
The reason is that it is broken and will result in spamming errors until we fix it properly:
https://github.com/paritytech/polkadot-sdk/issues/10078.

#### [#10856]: [pallet-broker] add extrinsic to force transfer a region
Add an extrinsic to `pallet-broker` which allows a privileged origin (`AdminOrigin` or `Root`) to forcefully transfer a
region, ignoring its current owner.
