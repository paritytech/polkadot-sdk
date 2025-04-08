## Changelog

### Changelog for `Node Dev`

**ℹ️ These changes are relevant to:**  Those who build around the client side code. Alternative client builders, SMOLDOT, those who consume RPCs. These are people who are oblivious to the runtime changes. They only care about the meta-protocol, not the protocol itself.

#### [#6979]: Update prometheus binding failure logging format

Using `{:#?}` for the error details is a bit annoying, this change makes a more consistent formatting style for error messages.

#### [#6405]: `fatxpool`: handling limits and priorities improvements

This PR provides a number of improvements and fixes around handling limits and priorities in the fork-aware transaction pool.

#### [#6284]: backing: improve session buffering for runtime information

This PR implements caching within the backing module for session-stable information, reducing redundant runtime API calls.

Specifically, it introduces a local cache for the:

- validators list;
- node features;
- executor parameters;
- minimum backing votes threshold;
- validator-to-group mapping.

Previously, this data was fetched or computed repeatedly each time `PerRelayParentState` was built. With this update, the cached information is fetched once and reused throughout
the session.

#### [#6768]: `basic-authorship`: debug level is now less spammy

The `debug` level in `sc-basic-authorship`  is now less spammy. Previously it was outputing logs per individual transactions. It made quite hard to follow the logs (and also generates unneeded traffic in grafana).

Now debug level only show some internal details, without spamming output with per-transaction logs. They were moved to `trace` level.

I also added the `EndProposingReason` to the summary INFO message. This allows us to know what was the block limit (which is very useful for debugging).

#### [#7011]: sync: Send already connected peers to new subscribers

Introduce `SyncEvent::InitialPeers` message sent to new subscribers to allow them correctly tracking sync peers. This resolves a race condition described in <https://github.com/paritytech/polkadot-sdk/issues/6573#issuecomment-2563091343>.

Fixes <https://github.com/paritytech/polkadot-sdk/issues/6573>.

#### [#7612]: HashAndNumber: Ord, Eq, PartialOrd, PartialEq implemented

This PR adds implementation of `Ord, Eq, PartialOrd, PartialEq`  traits for `HashAndNumber` struct.

#### [#6534]: Forward logging directives to Polkadot workers

This pull request forward all the logging directives given to the node via `RUST_LOG` or `-l` to the workers, instead of only forwarding `RUST_LOG`.

#### [#6867]: Deprecate ParaBackingState API

Deprecates the `para_backing_state` API. Introduces and new `backing_constraints` API that can be used together with existing `candidates_pending_availability` to retrieve the same information provided by `para_backing_state`.

#### [#7756]: Fix unspecified Hash in NodeBlock

Specified Hash type for BoundedHeader in NodeBlock, fixing possible internal compiler error

#### [#7104]: collation-generation: resolve mismatch between descriptor and commitments core index

This PR resolves a bug where collators failed to generate and submit collations,
resulting in the following error:

```
ERROR tokio-runtime-worker parachain::collation-generation: Failed to construct and distribute collation: V2 core index check failed: The core index in commitments doesn't
match the one in descriptor.
```

This issue affects only legacy and test collators that still use the collation function.
It is not a problem for lookahead or slot-based collators.

This fix ensures the descriptor core index contains the value determined by the core selector UMP signal when the parachain is using RFC103.

#### [#6661]: `txpool api`: `remove_invalid` call improved

Currently the transaction which is reported as invalid by a block builder (or `removed_invalid` by other components) is silently skipped. This PR improves this behavior. The transaction pool `report_invalid` function now accepts optional error associated with every reported transaction, and also the optional block hash which both provide hints how reported invalid transaction shall be handled.  Depending on error, the transaction pool can decide if transaction shall be removed from the view only or entirely from the pool. Invalid event will be dispatched if required.

#### [#6400]: Remove network starter that is no longer needed

# Description

This seems to be an old artifact of the long closed <https://github.com/paritytech/substrate/issues/6827> that I noticed when working on related code earlier.

## Integration

`NetworkStarter` was removed, simply remove its usage:

```diff
-let (network, system_rpc_tx, tx_handler_controller, start_network, sync_service) =
+let (network, system_rpc_tx, tx_handler_controller, sync_service) =
    build_network(BuildNetworkParams {
...
-start_network.start_network();
```

## Review Notes

Changes are trivial, the only reason for this to not be accepted is if it is desired to not start network automatically for whatever reason, in which case the description of network starter needs to change.

# Checklist

- [x] My PR includes a detailed description as outlined in the "Description" and its two subsections above.
- [ ] My PR follows the [labeling requirements](
https://github.com/paritytech/polkadot-sdk/blob/master/docs/contributor/CONTRIBUTING.md#Process
) of this project (at minimum one label for `T` required)
  - External contributors: ask maintainers to put the right label on your PR.

#### [#7042]: networking::TransactionPool should accept Arc

The `sc_network_transactions::config::TransactionPool` trait now returns an `Arc` for transactions.

#### [#6924]: malus-collator: implement malicious collator submitting same collation to all backing groups

This PR modifies the undying collator to include a malus mode, enabling it to submit the same collation to all assigned backing groups.

It also includes a test that spawns a network with the malus collator and verifies that everything functions correctly.

#### [#6897]: Tracing Log for fork-aware transaction pool

Replacement of log crate with tracing crate for better logging.

#### [#7005]: Log peerset set ID -> protocol name mapping

To simplify debugging of peerset related issues like <https://github.com/paritytech/polkadot-sdk/issues/6573#issuecomment-2563091343>.

#### [#5842]: Get rid of libp2p dependency in sc-authority-discovery

Removes `libp2p` types in authority-discovery, and replace them with network backend agnostic types from `sc-network-types`.
The `sc-network` interface is therefore updated accordingly.

#### [#6440]: Remove debug message about pruning active leaves

Removed useless debug message

#### [#7464]: Enable importing sc-tracing macros through polkadot-sdk

This PR makes it possible to use the sc-tracing macros when they are imported through the umbrella crate.

#### [#6163]: Expose more syncing types to enable custom syncing strategy

Exposes additional syncing types to facilitate the development of a custom syncing strategy.

#### [#6832]: Remove collation-generation subsystem from validator nodes

Collation-generation is only needed for Collators, and therefore not needed for validators

#### [#8109]: rpc v2 archive: more verbose error types in API

This PR changes the error types to more precise rather than arbitrary JSON-RPC error

#### [#7127]: Forbid v1 descriptors with UMP signals

Adds a check that parachain candidates do not send out UMP signals with v1 descriptors.

#### [#7195]: Unify Import verifier usage across parachain template and omninode

In polkadot-omni-node block import pipeline it uses default aura verifier without checking equivocation, This Pr replaces the check with full verification with equivocation like in parachain template block import

#### [#7610]: runtime-api: remove redundant version checks

This PR removes unnecessary runtime API version checks for APIs supported by Polkadot (the most recent network to upgrade). Specifically, it applies to the `DisabledValidators`, `MinimumBackingVotes` and `NodeFeatures` APIs.

#### [#6983]: cumulus: bump PARENT_SEARCH_DEPTH to allow for 12-core elastic scaling

Bumps the PARENT_SEARCH_DEPTH constant to a larger value (30).
This is a node-side limit that restricts the number of allowed pending availability candidates when choosing the parent parablock during authoring.
This limit is rather redundant, as the parachain runtime already restricts the unincluded segment length to the configured value in the FixedVelocityConsensusHook.
For 12 cores, a value of 24 should be enough, but bumped it to 30 to have some extra buffer.

#### [#7781]: Punish libp2p notification protocol misbehavior on outbound substreams

This PR punishes behaviors that deviate from the notification spec.
When a peer misbehaves by writing data on an unidirectional read stream, the peer is banned and disconnected immediately.

#### [#6521]: Pure state sync refactoring (part-2)

This is the last part of the pure refactoring of state sync, focusing on encapsulating `StateSyncMetadata` as a separate entity.

#### [#6711]: Expose DHT content providers API from `sc-network`

Expose the Kademlia content providers API for the use by `sc-network` client code:

1. Extend the `NetworkDHTProvider` trait with functions to start/stop providing content and query the DHT for the list of content providers for a given key.
2. Extend the `DhtEvent` enum with events reporting the found providers or query failures.
3. Implement the above for libp2p & litep2p network backends.

#### [#8040]: Make the default 85% usage of the PoV

Make parachain nodes build PoVs up to 85% of the maximum possible. Additionally, also added a cli parameter as a fallback, to make nodes can adjust it easily in case the 85% is too optimistic.

#### [#7649]: frame-benchmarking-cli should not build RocksDB by default

This PR ensures `frame-benchmarking-cli` does not build RocksDB by default and also ensures rocksDB is not built when `default-features=false`.

#### [#7479]: omni-node: add offchain worker

Added support for offchain worker to omni-node-lib for both aura and manual seal nodes.

#### [#6963]: grandpa: Ensure `WarpProof` stays in its limits

There was the chance that a `WarpProof` was bigger than the maximum warp sync proof size. This could have happened when inserting the last justification, which then may pushed the total proof size above the maximum. The solution is simply to ensure that the last justfication also fits into the limits.

Close: <https://github.com/paritytech/polkadot-sdk/issues/6957>

#### [#6553]: Ensure sync event is processed on unknown peer roles

The GossipEngine::poll_next implementation polls both the notification_service and the sync_event_stream.
This PR ensures both events are processed gracefully.

#### [#6452]: elastic scaling RFC 103 end-to-end tests

Adds end-to-end zombienet-sdk tests for elastic scaling using the RFC103 implementation.
Only notable user-facing change is that the default chain configurations of westend and rococo now enable by default the CandidateReceiptV2 node feature.

#### [#7724]: Terminate libp2p the outbound notification substream on io errors

This PR handles a case where we called the poll_next on an outbound substream notification to check if the stream is closed.
It is entirely possible that the poll_next would return an io::error, for example end of file.
This PR ensures that we make the distinction between unexpected incoming data, and error originated from poll_next.
While at it, the bulk of the PR change propagates the PeerID from the network behavior, through the notification handler, to the notification outbound stream for logging purposes.

#### [#6481]: slot-based-collator: Implement dedicated block import

The `SlotBasedBlockImport` job is to collect the storage proofs of all blocks getting imported. These storage proofs alongside the block are being forwarded to the collation task. Right now they are just being thrown away. More logic will follow later. Basically this will be required to include multiple blocks into one `PoV` which will then be done by the collation task.

#### [#6647]: `fatxpool`: proper handling of priorities when mempool is full

Higher-priority transactions can now replace lower-priority transactions even when the internal _tx_mem_pool_ is full.

#### [#7505]: `fatxpool`: transaction statuses metrics added

This PR introduces a new mechanism to capture and report Prometheus metrics related to timings of transaction lifecycle events, which are currently not available. By exposing these timings, we aim to augment transaction-pool reliability dashboards and extend existing Grafana boards.

A new `unknown_from_block_import_txs` metric is also introduced. It provides the number of transactions in imported block which are not known to the node's  transaction pool. It allows to monitor alignment of transaction pools across the nodes in the network.

#### [#6215]: Remove `ProspectiveParachainsMode` from backing subsystem

Removes `ProspectiveParachainsMode` usage from the backing subsystem and assumes `async_backing_params` runtime api is always available. Since the runtime api v7 is released on
all networks it should always be true.

#### [#7708]: Support adding extra request-response protocols to the node

Allow adding extra request-response protocols during polkadot service initialization. This is required to add a request-response protocol described in [RFC-0008](https://polkadot-fellows.github.io/RFCs/approved/0008-parachain-bootnodes-dht.html) to the relay chain side of the parachain node.

#### [#7554]: sc-informant: Print full hash when debug logging is enabled

When debugging stuff, it is useful to see the full hashes and not only the "short form". This makes it easier to read logs and follow blocks.

#### [#7885]: Rename archive call method result to value

Previously, the method result was encoded to a json containing a "result" field. However, the spec specifies a "value" field. This aims to rectify that.

#### [#6913]: Enable approval-voting-parallel by default on polkadot

Enable approval-voting-parallel by default on polkadot

#### [#6628]: Remove ReportCollator message

Remove unused message ReportCollator and test related to this message on the collator protocol validator side.

#### [#7014]: Remove `yamux_window_size` from network config

# Description

resolve #6468

#### [#6455]: Add litep2p network protocol benches

Adds networking protocol benchmarks with litep2p backend

#### [#7368]: Add chain properties to chain-spec-builder

- Adds support for chain properties to chain-spec-builder.

#### [#7254]: deprecate AsyncBackingParams

Removes all usage of the static async backing params, replacing them with dynamically computed equivalent values (based on the claim queue and scheduling lookahead).

Adds a new runtime API for querying the scheduling lookahead value. If not present, falls back to 3 (the default value that is backwards compatible with values we have on production networks for allowed_ancestry_len)

Also removes most code that handles async backing not yet being enabled, which includes support for collation protocol version 1 on collators, as it only worked for leaves not supporting async backing (which are none).

#### [#4880]: Collation fetching fairness in collator protocol

Implements collation fetching fairness in the validator side of the collator protocol. With core time in place if two (or more) parachains share a single core no fairness was guaranteed between them in terms of collation fetching. The current implementation was accepting up to `max_candidate_depth + 1` seconded collations per relay parent and once this limit is reached no new collations are accepted. A misbehaving collator can abuse this fact and prevent other collators/parachains from advertising collations by advertising `max_candidate_depth + 1` collations of its own.
To address this issue two changes are made:

1. For each parachain id the validator accepts advertisements until the number of entries in
   the claim queue equals the number of seconded candidates.
2. When new collation should be fetched the validator inspects what was seconded so far,
   what's in the claim queue and picks the first slot which hasn't got a collation seconded
   and there is no candidate pending seconding for it. If there is an advertisement in the
   waiting queue for it it is fetched. Otherwise the next free slot is picked.
These two changes guarantee that:
1. Validator doesn't accept more collations than it can actually back.
2. Each parachain has got a fair share of core time based on its allocations in the claim
   queue.

#### [#6262]: Size limits implemented for fork aware transaction pool

Size limits are now obeyed in fork aware transaction pool

#### [#7338]: [net/libp2p] Use raw `Identify` observed addresses to discover external addresses

Instead of using libp2p-provided external address candidates, susceptible to address translation issues, use litep2p-backend approach based on confirming addresses observed by multiple peers as external.

Fixes <https://github.com/paritytech/polkadot-sdk/issues/7207>.

#### [#7585]: Add export PoV on slot base collator

Add functionality to export the Proof of Validity (PoV) when the slot-based collator is used.

#### [#7021]: Improve remote externalities logging

Automatically detect if current env is tty. If not disable the spinner logging.

#### [#6636]: Optimize initialization of networking protocol benchmarks

These changes should enhance the quality of benchmark results by excluding worker initialization time from the measurements and reducing the overall duration of the benchmarks.

#### [#6561]: slot-based-collator: Move spawning of the futures

Move spawning of the slot-based collator into the `run` function. Also the tasks are being spawned as blocking task and not just as normal tasks.

#### [#7449]: remove handling of validation protocol versions 1 and 2

This PR removes handling for validation protocol versions 1 and 2, as they are no longer in use, leaving only version 3. Specifically, it eliminates handling for V1 and V2 of `BitfieldDistributionMessage`, `ApprovalDistributionMessage` and `StatementDistributionMessage`. However, the logic for handling different versions remains to allow for future additions.

#### [#8062]: frame-system: Don't underflow the sufficients

Fixes a potential underflow for `dec_sufficients`.

#### [#7286]: Remove node-side feature flag checks for Elastic Scaling MVP

This PR removes node-side conditional checks for FeatureIndex::ElasticScalingMVP, by default elastic scaling is always enabled. This simplifies the backing and provisioner logic.

#### [#6865]: Rename PanicInfo to PanicHookInfo

Starting with Rust 1.82 `PanicInfo` is deprecated and will throw warnings when used. The new type is available since Rust 1.81 and should be available on our CI.

#### [#5855]: Remove feature `test-helpers` from sc-service

Removes feature `test-helpers` from sc-service.

#### [#8024]: Change the hash of `PendingOrders` storage item

Change the hash to `Twox64Concat`, which is more secure than `Identity`.

#### [#7777]: `fatxpool`: report_invalid: do not ban Future/Stale txs from re-entering the view

Avoid banning future/stale transactions reported as invalid by the authorship module.

#### [#7639]: `fatxpool`: improved handling of finality stalls

This pull request introduces measures to handle finality stalls by :

- notifying outdated transactions with a `FinalityTimeout` event.
- removing outdated views from the `view_store`

An item is considered _outdated_ when the difference between its associated block and the current block exceeds a pre-defined threshold.

#### [#6889]: Remove polkadot-omni-node-lib unused dependency

Removed an unused dependency for `polkadot-omni-node-lib`.

#### [#6450]: Add omni-node checks for runtime parachain compatibility

OmniNode parses runtime metadata and checks against the existence of `cumulus-pallet-parachain-system` and `frame-system`, by filtering pallets by names: `ParachainSystem` and `System`. It also checks the `frame-system` pallet storage `Number` type, and then uses it to configure AURA if `u32` or `u64`.

#### [#6248]: Upgrade libp2p to 0.54.1

Upgrade libp2p from 0.52.4 to 0.54.1

#### [#7102]: `fatxpool`: rotator cache size now depends on pool's limits

This PR modifies the hard-coded size of extrinsics cache within `PoolRotator` to be inline with pool limits. It only applies to fork-aware transaction pool. For the legacy (single-state) transaction pool the logic remains untouched.

#### [#7073]: Implement NetworkRequest for litep2p

# Description

Implements NetworkRequest::request for litep2p that we need for networking benchmarks

## Review Notes

Duplicates implementation for NetworkService
<https://github.com/paritytech/polkadot-sdk/blob/5bf9dd2aa9bf944434203128783925bdc2ad8c01/substrate/client/network/src/service.rs#L1186-L1205>

#### [#6528]: TransactionPool API uses async_trait

This PR refactors `TransactionPool` API to use `async_trait`, replacing the`Pin<Box<...>>` pattern. This should improve readability and maintainability.

The change is not altering any functionality.

#### [#8104]: Stabilize RPC `archive` methods to V1

This PR renames the V2 `archive_unstable_*` RPC calls to be `archive_v1_*`, signalling that they have been stabilized.

#### [#6703]: network/libp2p-backend: Suppress warning adding already reserved node as reserved

Fixes <https://github.com/paritytech/polkadot-sdk/issues/6598>.

#### [#7981]: Bump ParachainHost runtime API to 13

Bump `backing_constraints` and `scheduling_lookahead` API version to 13.
The `validation_code_bomb_limit` API remains at version 12.
Bump all ParachainHost runtime to version 13 in all test runtimes.

#### [#6417]: fix prospective-parachains best backable chain reversion bug

Fixes a bug in the prospective-parachains subsystem that prevented proper best backable chain reorg.

#### [#6249]: Pure state sync refactoring (part-1)

The pure refactoring of state sync is preparing for <https://github.com/paritytech/polkadot-sdk/issues/4>. This is the first part, focusing on isolating the function `process_state_key_values()` as the central point for storing received state data in memory. This function will later be adapted to forward the state data directly to the DB layer to resolve the OOM issue and support persistent state sync.

#### [#7494]: Enhance libp2p logging targets for granular control

This PR modifies the libp2p networking-specific log targets for granular control (e.g., just enabling trace for req-resp).

Previously, all logs were outputted to `sub-libp2p` target, flooding the log messages on busy validators.

- Discovery: `sub-libp2p::discovery`;
- Notification/behaviour: `sub-libp2p::notification::behaviour`;
- Notification/handler: `sub-libp2p::notification::handler`;
- Notification/service: `sub-libp2p::notification::service`;
- Notification/upgrade: `sub-libp2p::notification::upgrade`;
- Request response: `sub-libp2p::request-response`.

#### [#7545]: `fatxpool`: event streams moved to view domain

This pull request refactors the transaction pool `graph` module by renaming components for better clarity and decouples `graph` module from `view` module related specifics.
This PR does not introduce changes in the logic.

#### [#7866]: Make litep2p the default backend in Kusama

A new trait `IdentifyNetworkBackend` is introduced for the polkadot-service. The purpose of the trait is to specify the default network backend for individual chains. For Kusama based chains, the default is now litep2p. For other chains, the default remains unchanged to libp2p.
The network backend field of the network configuration is made optional to accomodate for this change.

#### [#5703]: Properly handle block gap created by fast sync

Implements support for handling block gaps generated during fast sync. This includes managing the creation, Note that this feature is not fully activated until the `body` attribute is removed from the `LightState` block request in chain sync, which will occur after the issue #5406 is resolved.

#### [#7075]: Snowbridge - Ethereum Electra Upgrade Support

Adds support for the Ethereum Electra hard-fork in the Ethereum light client. Maintains backwards compatibility with the current Deneb hard-fork.
Relayers should update to the latest binary to support sending Electra consensus updates.

#### [#7402]: Snowbridge V2

The implementation of Snowbridge V2, which is additive and does not affect the V1 protocol.
The original specification in <https://github.com/paritytech/polkadot-sdk/blob/master/bridges/snowbridge/docs/v2.md> FYI.

### Changelog for `Runtime Dev`

**ℹ️ These changes are relevant to:**  All of those who rely on the runtime. A parachain team that is using a pallet. A DApp that is using a pallet. These are people who care about the protocol (WASM, not the meta-protocol (client).)

#### [#7164]: [pallet-revive] Remove revive events

Remove all pallet::events except for the `ContractEmitted` event that is emitted by contracts

#### [#7676]: [pallet-revive] tracing should wrap around call stack execution

Fix tracing should wrap around the entire call stack execution

#### [#7634]: derive `DecodeWithMemTracking` for `RuntimeCall`

This PR derives `DecodeWithMemTracking` for `RuntimeCall`.
All the types used in the `RuntimeCall` should implement `DecodeWithMemTracking` as well.

#### [#7412]: Pallet view functions: improve metadata, API docs and testing

- refactor view functions metadata according to #6833 in preparation for V16, and move them to pallet-level metadata
- add `view_functions_experimental` macro to `pallet_macros` with API docs
- improve UI testing for view functions

#### [#7928]: Fix pallet-revive-fixtures build.rs

Fix pallet-revive-uapi resolution when building pallet-revive-fixtures
contracts

#### [#7069]: Fix defensive! macro to be used in umbrella crates

PR for #7054

Replaced frame_support with $crate from @gui1117 's suggestion to fix the dependency issue

#### [#7492]: Make `pallet-bridge-rewards` generic over `RewardKind`

The PR enhances the pallet-bridge-rewards by making it generic over the `Reward` type (previously hardcoded as `RewardsAccountParams`). This modification allows the pallet to support multiple reward types (e.g., P/K bridge, Snowbridge), increasing its flexibility and applicability across various bridge scenarios.

Other pallets can register rewards using bp_relayers::RewardLedger, which is implemented by the rewards pallet. The runtime can then be configured with different mechanisms for paying/claiming rewards via bp_relayers::PaymentProcedure (e.g., see the pub struct BridgeRewardPayer; implementation for BridgeHubWestend).

#### [#7263]: Fix `frame-benchmarking-cli` not buildable without rocksdb

## Description

The `frame-benchmarking-cli`  crate has not been buildable without the `rocksdb` feature since version 1.17.0.

**Error:**

```rust
self.database()?.unwrap_or(Database::RocksDb),
                             ^^^^^^^ variant or associated item not found in `Database`
```

This issue is also related to the `rocksdb` feature bleeding (#3793), where the `rocksdb` feature was always activated even when compiling this crate with `--no-default-features`.

**Fix:**

- Resolved the error by choosing `paritydb` as the default database when compiled without the `rocksdb` feature.
- Fixed the issue where the `sc-cli` crate's `rocksdb` feature was always active, even compiling `frame-benchmarking-cli` with `--no-default-features`.

## Review Notes

Fix the crate to be built without rocksdb, not intended to solve #3793.

#### [#6741]: pallet-revive: Adjust error handling of sub calls

We were trapping the host context in case a sub call was exhausting the storage deposit limit set for this sub call. This prevents the caller from handling this error. In this PR we added a new error code that is returned when either gas or storage deposit limit is exhausted by the sub call.

We also remove the longer used `NotCallable` error. No longer used because this is no longer an error: It will just be a balance transfer.

We also make `set_code_hash` infallible to be consistent with other host functions which just trap on any error condition.

#### [#6880]: [pallet-revive] implement the call data copy API

This PR implements the call data copy API by adjusting the input method.

Closes #6770

#### [#6446]: Make pallet-recovery supports `BlockNumberProvider`

pallet-recovery now allows configuring the block provider to be utilized within this pallet. This block is employed for the delay in the recovery process.

A new associated type has been introduced in the `Config` trait: `BlockNumberProvider`. This can be assigned to `System` to maintain the previous behavior, or it can be set to another block number provider, such as `RelayChain`.

If the block provider is configured with a value different from `System`, a migration will be necessary for the `Recoverable` and `ActiveRecoveries` storage items.

#### [#7423]: Fix issue with InitiateTransfer and UnpaidExecution

Fix issue where setting the `remote_fees` field of `InitiateTransfer` to `None` could lead to unintended bypassing of fees in certain conditions. `UnpaidExecution` is now appended **after** origin alteration. If planning to use `UnpaidExecution`, you need to set `preserve_origin = true`.
The `AllowExplicitUnpaidExecutionFrom` barrier now allows instructions for receiving funds before origin altering instructions before the actual `UnpaidExecution`. It takes a new generic, `Aliasers`, needed for executing `AliasOrigin` to see if the effective origin is allowed to use `UnpaidExecution`. This should be set to the same value as `Aliasers` in the XCM configuration.

#### [#6836]: [pallet-revive-eth-rpc] persist eth transaction hash

Add an option to persist EVM transaction hash to a SQL db.
This make it possible to run a full archive ETH RPC node (assuming the substrate node is also a full archive node)

Some queries such as eth_getTransactionByHash, eth_getBlockTransactionCountByHash, and other need to work with a transaction hash index, which is not available in Substrate and need to be stored by the eth-rpc proxy.

The refactoring break down the Client  into a `BlockInfoProvider` and `ReceiptProvider`

- BlockInfoProvider does not need any persistence data, as we can fetch all block info from the source substrate chain
- ReceiptProvider comes in two flavor,
  - An in memory cache implementation - This is the one we had so far.
  - A DB implementation - This one persist rows with the block_hash, the transaction_index and the transaction_hash, so that we can later fetch the block and extrinsic for that receipt and reconstruct the ReceiptInfo object.

#### [#7889]: Remove execute_with_origin implementation in the XCM executor

The XCM executor will not support the `ExecuteWithOrigin` instruction from the start.
It might be added later when more time can be spent on it.

#### [#6867]: Deprecate ParaBackingState API

Deprecates the `para_backing_state` API. Introduces and new `backing_constraints` API that can be used together with existing `candidates_pending_availability` to retrieve the same information provided by `para_backing_state`.

#### [#7562]: pallet-revive: Add env var to allow skipping of validation for testing

When trying to reproduce bugs we sometimes need to deploy code that wouldn't pass validation. This PR adds a new environment variable `REVIVE_SKIP_VALIDATION` that when set will skip all validation except the contract blob size limit.

Please note that this only applies to when the pallet is compiled for `std` and hence will never be part of on-chain.

#### [#6367]: Refactor pallet society

Derives `MaxEncodedLen` implementation for stored types and removes `without_storage_info` attribute.
Migrates benchmarks from v1 to v2 API.

#### [#7790]: pallet-scheduler: Put back postponed tasks into the agenda

Right now `pallet-scheduler` is not putting back postponed tasks into the agenda when the early weight check is failing. This pull request ensures that these tasks are put back into the agenda and are not just "lost".

#### [#7580]: implement web3_clientVersion

Implements the `web3_clientVersion`  method. This is a common requirement for external Ethereum libraries when querying a client.

Reference issue with more details: <https://github.com/paritytech/contract-issues/issues/26>.

#### [#7959]: Update expire date on treasury payout

Resets the `payout.expire_at` field with the `PayoutPeriod` every time that there is a valid Payout attempt.
Prior to this change, when a spend is approved, it receives an expiry date so that if it’s never claimed, it automatically expires. This makes sense under normal circumstances. However, if someone attempts to claim a valid payout and there isn’t sufficient liquidity to fulfill it, the expiry date currently remains unchanged. This effectively penalizes the claimant in the same way as if they had never requested the payout in the first place.
With this change users are not penalized for liquidity shortages and have a fair window to claim once the funds are available.

#### [#7282]: AHM Multi-block staking election pallet

NOTE: This is reverted in #7939.

## Multi Block Election Pallet

This PR adds the first iteration of the multi-block staking pallet.

From this point onwards, the staking and its election provider pallets are being customized to work in AssetHub. While usage in solo-chains is still possible, it is not longer the main focus of this pallet. For a safer usage, please fork and user an older version of this pallet.

#### [#6926]: [pallet-revive] implement the gas limit API

This PR implements the gas limit API, returning the maximum ref_time per block. Solidity contracts only know a single weight dimension and can use this method to get the block ref_time limit.

#### [#7614]: [pallet-revive] tracing improvements

Various pallet-revive improvements

- add check for precompiles addresses,
So we can easily identified which one are being called and not supported yet

- fixes debug_call for revert call
If a call revert we still want to get the traces for that call, that matches geth behaviors, diff tests will be added to the test suite for this

- fixes traces for staticcall
The call type was not always being reported properly.

#### [#7260]: [eth-indexer] subscribe to finalize blocks instead of best blocks

For eth-indexer, it's probably safer to use `subscribe_finalized` and index these blocks into the DB rather than `subscribe_best`

#### [#7607]: Add Runtime Api version to metadata

The runtime API implemented version is not explicitly shown in metadata, so here we add it to improve developer experience.

This closes #7352 .

#### [#7703]: Add voting hooks to Conviction_Voting

This change introduces voting hooks to the conviction-voting pallet, enabling developers to customize behavior during various stages of the voting process. These hooks provide a mechanism to execute specific logic before a vote is recorded, before a vote is removed, or when a vote fails to be recorded, while maintaining compatibility with the existing conviction-voting pallet.

The key hooks include:

- `on_vote`: Called before a vote is recorded. This hook allows developers to validate or perform actions based on the vote. If it returns an error, the voting operation is reverted. However, any storage modifications made by this hook will persist even if the vote fails later.
- `on_remove_vote`: Called before a vote is removed. This hook cannot fail and is useful for cleanup or additional logic when a vote is removed.
- `lock_balance_on_unsuccessful_vote`: Called when a vote fails to be recorded, such as due to insufficient balance. It allows locking a specific balance amount as part of the failure handling.

Advantages of using voting hooks:

- Flexibility: Developers can implement custom logic to extend or modify the behavior of the conviction-voting pallet.
- Control: Hooks provide fine-grained control over different stages of the voting process.
- Error Handling: The `on_vote` hook enables early validation, preventing (to some extent) invalid votes from being recorded.

How to use:

- Implement the `VotingHooks` trait in your runtime or custom module.
- Define the desired behavior for each hook method, such as validation logic in `on_vote` or cleanup actions in `on_remove_vote`.
- Integrate the implementation with the conviction-voting pallet to enable the hooks.

#### [#6856]: Enable report_fork_voting()

This PR enables calling `report_fork_voting`.
In order to do this we needed to also check that the ancestry proof is optimal.

#### [#7805]: New `staking::manual_slash` extrinsic

A new `manual_slash` extrinsic that allows slashing a validator's stake manually by governance.

#### [#6928]: [Backport] Version bumps and `prdocs` reordering form 2412

This PR includes backport of the regular version bumps and `prdocs` reordering from the `stable2412` branch back ro master

#### [#2072]: Return iterator in pallet_referenda::TracksInfo::tracks

Change the return type of the trait method `pallet_referenda::TracksInfo::tracks` to return an iterator of `Cow<'static, Tracks<_, _, _>>` instead of a static slice in order to support more flexible implementations that can define referenda tracks dynamically.

#### [#7579]: [AHM] Make pallet types public

Preparation for AHM and making stuff public.

#### [#7126]: xcm: Fixes for `UnpaidLocalExporter`

This PR deprecates `UnpaidLocalExporter` in favor of the new `LocalExporter`. First, the name is misleading, as it can be used in both paid and unpaid scenarios. Second, it contains a hard-coded channel 0, whereas `LocalExporter` uses the same algorithm as `xcm-exporter`.

#### [#6435]: frame-benchmarking: Use correct components for pallet instances

When benchmarking multiple instances of the same pallet, each instance was executed with the components of all instances. While actually each instance should only be executed with the components generated for the particular instance. The problem here was that in the runtime only the pallet-name was used to determine if a certain pallet should be benchmarked. When using instances, the pallet name is the same for both of these instances. The solution is to also take the instance name into account.

The fix requires to change the `Benchmark` runtime api to also take the `instance`. The node side is written in a backwards compatible way to also support runtimes which do not yet support the `instance` parameter.

#### [#7319]: [pallet-revive] pack exceeding syscall arguments into registers

This PR changes how we call runtime API methods with more than 6 arguments: They are no longer spilled to the stack but packed into registers instead. Pointers are 32 bit wide so we can pack two of them into a single 64 bit register. Since we mostly pass pointers, this technique effectively increases the number of arguments we can pass using the available registers.

To make this work for `instantiate` too we now pass the code hash and the call data in the same buffer, akin to how the `create` family opcodes work in the EVM. The code hash is fixed in size, implying the start of the constructor call data.

#### [#6920]: [pallet-revive] change some getter APIs to return value in register

Call data, return data and code sizes can never exceed `u32::MAX`; they are also not generic. Hence we know that they are guaranteed to always fit into a 64bit register and `revive` can just zero extend them into a 256bit integer value. Which is slightly more efficient than passing them on the stack.

#### [#6917]: Remove unused dependencies from pallet_revive

Removing apparently unused dependencies from `pallet_revive` and related crates.

#### [#7506]: [pallet-revive] Add eth_get_logs

Add support for eth_get_logs rpc method

#### [#6336]: pallet-xcm: add support to authorize aliases

Added `AuthorizedAliasers` type exposed by `pallet-xcm`, that acts as a filter for explicitly authorized
aliases using `pallet-xcm::add_authorized_alias()` and `pallet-xcm::remove_authorized_alias()`.
Runtime developers can simply plug this `pallet-xcm::AuthorizedAliasers` type in their runtime's `XcmConfig`,
specifically in `<XcmConfig as xcm_executor::Config>::Aliasers`.

#### [#6820]: Add XCM benchmarks to collectives-westend

Collectives-westend was using `FixedWeightBounds`, meaning the same weight per instruction. Added proper benchmarks.

#### [#7594]: Improve XCM Debugging by Capturing Logs in Unit Tests

This PR introduces a lightweight log-capturing mechanism for XCM unit tests, making it easier to troubleshoot failures when needed.

#### [#7324]: Replace derivative dependency with derive-where

The `derivative` crate, previously used to derive basic traits for structs with
generics or enums, is no longer actively maintained. It has been replaced with
the `derive-where` crate, which offers a more straightforward syntax while
providing the same features as `derivative`.

#### [#7620]: Derive `DecodeWithMemTracking` for bridge and xcm pallets

Related to <https://github.com/paritytech/polkadot-sdk/issues/7360>

Just deriving `DecodeWithMemTracking` for the types used by the bridge, snowbridge and xcm pallets

#### [#7206]: Add an extra_constant to pallet-collator-selection

- Allows to query collator-selection's pot account via extra constant.

#### [#7669]: Introduce ark-ec-vrfs

Superseeds `bandersnatch_vrfs` with [ark-vrf](https://crates.io/crates/ark-vrf)

- Same crypto as JAM
- With a spec: github.com/davxy/bandersnatch-vrf-spec
- Published on crates.io

<https://github.com/paritytech/polkadot-sdk/pull/7670> follow up

NOTE: this crypto is under experimental feat

#### [#7571]: frame-benchmarking: Improve macro hygiene

Improve macro hygiene of benchmarking macros.

#### [#7598]: implement `DecodeWithMemTracking` for frame pallets

Related to <https://github.com/paritytech/polkadot-sdk/issues/7360>

This PR implements `DecodeWithMemTracking` for the types in the frame pallets

The PR is verbose, but it's very simple. `DecodeWithMemTracking` is simply derived for most of the types. There are only 3 exceptions which are isolated into 2 separate commits.

#### [#7091]: [pallet-revive] Add new host function `to_account_id`

A new host function `to_account_id` is added. It allows retrieving the account id for a `H160` address.

#### [#7656]: Authorize upgrade tests for testnet runtimes + `execute_as_governance` refactor

This PR contains improved test cases that rely on the governance location as preparation for AHM to capture the state as it is.
It introduces `execute_as_governance_call`, which can be configured with various governance location setups instead of the hard-coded `Location::parent()`.

#### [#7662]: pallet_revive: Change address derivation to use hashing

## Motivation

Internal auditors recommended to not truncate Polkadot Addresses when deriving Ethereum addresses from it. Reasoning is that they are raw public keys where truncating could lead to collisions when weaknesses in those curves are discovered in the future. Additionally,  some pallets generate account addresses in a way where only the suffix we were truncating contains any entropy. The changes in this PR act as a safe guard against those two points.

## Changes made

We change the `to_address` function to first hash the AccountId32 and then use trailing 20 bytes as `AccountId20`. If the `AccountId32` ends with 12x 0xEE we keep our current behaviour of just truncating those trailing bytes.

## Security Discussion

This will allow us to still recover the original `AccountId20` because those are constructed by just adding those 12 bytes. Please note that generating an ed25519 key pair where the trailing 12 bytes are 0xEE is theoretically possible as 96bits is not a huge search space. However, this cannot be used as an attack vector. It will merely allow this address to interact with `pallet_revive` without registering as the fallback account is the same as the actual address. The ultimate vanity address. In practice, this is not relevant since the 0xEE addresses are not valid public keys for sr25519 which is used almost everywhere.

tl:dr: We keep truncating in case of an Ethereum address derived account id. This is safe as those are already derived via keccak. In every other case where we have to assume that the account id might be a public key. Therefore we first hash and then take the trailing bytes.

## Do we need a Migration for Westend

No. We changed the name of the mapping. This means the runtime will not try to read the old data. Ethereum keys are unaffected by this change. We just advise people to re-register their AccountId32 in case they need to use it as it is a very small circle of users (just 3 addresses registered). This will not cause disturbance on Westend.

#### [#6459]: Fix version conversion in XcmPaymentApi::query_weight_to_asset_fee

The `query_weight_to_asset_fee` function of the `XcmPaymentApi` was trying to convert versions in the wrong way.
This resulted in all calls made with lower versions failing.
The version conversion is now done correctly and these same calls will now succeed.

#### [#6695]: [pallet-revive] bugfix decoding 64bit args in the decoder

The argument index of the next argument is dictated by the size of the current one.

#### [#7879]: [pallet-revive] Support blocktag in eth_getLogs RPC

Support "latest" blocktag in ethGetLogs from_block and to_block parameters

#### [#7507]: Fix compilation warnings

This should fix some compilation warnings discovered under rustc 1.83

#### [#4529]: Removed `pallet::getter` usage from pallet-grandpa

This PR removed the `pallet::getter`s from `pallet-grandpa`.
The syntax `StorageItem::<T, I>::get()` should be used instead

#### [#6857]: [pallet-revive] implement the call data size API

This PR adds an API method to query the contract call data input size.

Part of #6770

#### [#7441]: Update Scheduler to have a configurable block number provider

This PR makes `pallet_scheduler` configurable by introducing `BlockNumberProvider` in `pallet_scheduler::Config`. Instead of relying solely on `frame_system::Pallet::<T>::block_number()`, the scheduler can now use any block number source, including external providers like the relay chain.

Parachains can continue using `frame_system::Pallet::<Runtime>` without issue. To retain the previous behavior, set `BlockNumberProvider` to `frame_system::Pallet::<Runtime>`.

#### [#7040]: [pallet-node-authorization] Migrate to using frame umbrella crate

This PR migrates the pallet-node-authorization to use the frame umbrella crate. This is part of the ongoing effort to migrate all pallets to use the frame umbrella crate. The effort is tracked [here](https://github.com/paritytech/polkadot-sdk/issues/6504).

#### [#7430]: [pallet-revive] fix tracing gas used

- Charge the nested gas meter for loading the code of the child contract, so that we can properly associate the gas cost to the child call frame.
- Move the enter_child_span and exit_child_span around the do_transaction closure to  properly capture all failures
- Add missing trace capture for call transfer

#### [#7163]: [pallet-revive] Remove debug buffer

Remove the `debug_buffer` feature

#### [#6743]: umbrella: Remove `pallet-revive-fixtures`

No need to have them in the umbrella crate also by having them in the umbrella crate they are bleeding into the normal build.

#### [#7313]: [XCM] add generic location to account converter that also works with external ecosystems

Adds a new `ExternalConsensusLocationsConverterFor` struct to handle external global consensus locations and their child locations.
This struct extends the functionality of existing converters (`GlobalConsensusParachainConvertsFor` and `EthereumLocationsConverterFor`) while maintaining backward compatibility.

#### [#7663]: Validator disabling in session enhancements

This PR introduces changes to the pallet-session interface. Disabled validators can still be disabled with just the index but it will default to highest possible severity.
pallet-session also additionally exposes DisabledValidators with their severities.
The staking primitive OffenceSeverity received min, max and default implementations for ease of use.

#### [#7916]: Removed pallet::getter from XCM pallets

This pr removes all pallet::getter occurrences from XCM pallets, replacing them with explicit implementations.

#### [#7127]: Forbid v1 descriptors with UMP signals

Adds a check that parachain candidates do not send out UMP signals with v1 descriptors.

#### [#7660]: [pallet-revive] Remove js examples

Remove JS examples, they now belongs to the evm-test-suite repo

#### [#6981]: [pallet-revive] fix file case

fix <https://github.com/paritytech/polkadot-sdk/issues/6970>

#### [#7581]: Move validator disabling logic to pallet-session

This decouples disabling logic from staking, and moves it to session. This ensures validators can be disabled directly when staking transitions to the system parachain and offences are reported on RC, eliminating cross-network hops.

#### [#7030]: [core-fellowship] Add permissionless import_member

Changes:

- Add call `import_member` to the core-fellowship pallet.
- Move common logic between `import` and `import_member` into `do_import`.

This is a minor change as to not impact UI and downstream integration.

## `import_member`

Can be used to induct an arbitrary collective member and is callable by any signed origin. Pays no fees upon success.
This is useful in the case that members did not induct themselves and are idling on their rank.

#### [#7729]: [pallet-revive] allow delegate calls to non-contract accounts

This PR changes the behavior of delegate calls when the callee is not a contract account: Instead of returning a `CodeNotFound` error, this is allowed and the caller observes a successful call with empty output.

The change makes for example the following contract behave the same as on EVM:

```Solidity
contract DelegateCall {
    function delegateToLibrary() external returns (bool) {
        address testAddress = 0x0000000000000000000000000000000000000000;
        (bool success, ) = testAddress.delegatecall(
            abi.encodeWithSignature("test()")
        );
        return success;
    }
}
```

Closes <https://github.com/paritytech/revive/issues/235>

#### [#7414]: [pallet-revive] do not trap the caller on instantiations with duplicate contracts

This PR changes the behavior of `instantiate` when the resulting contract address already exists (because the caller tried to instantiate the same contract with the same salt multiple times): Instead of trapping the caller, return an error code.

Solidity allows `catch`ing this, which doesn't work if we are trapping the caller. For example, the change makes the following snippet work:

```Solidity
try new Foo{salt: hex"00"}() returns (Foo) {
    // Instantiation was successful (contract address was free and constructor did not revert)
} catch {
    // This branch is expected to be taken if the instantiation failed because of a duplicate salt
}
```

#### [#7700]: [AHM] Poke deposits: Multisig pallet

This PR adds a new extrinsic `poke_deposit` to `pallet-multisig`. This extrinsic will be used to re-adjust the deposits made in the pallet to create a multisig operation.

#### [#7844]: [pallet-revive] Update fixture build script

Update the fixture build script so that it can be built from crates.io registry

#### [#6184]: Remove pallet::getter from pallet-staking

This PR removes all pallet::getter occurrences from pallet-staking and replaces them with explicit implementations.
It also adds tests to verify that retrieval of affected entities works as expected so via storage::getter.

#### [#6461]: [pallet-revive] add support for all eth tx types

Add support for 1559, 4844, and 2930 transaction types

#### [#7801]: add poke_deposit extrinsic to pallet-proxy

This PR adds a new extrinsic `poke_deposit` to `pallet-proxy`. This extrinsic will be used to re-adjust the deposits made in the pallet to create a proxy or to create an announcement.

#### [#6509]: Migrate pallet-democracy benchmark to v2

"Part of issue #6202."

#### [#6937]: [pallet-revive] bump polkavm to 0.18

Update to the latest polkavm version, containing a linker fix I need for revive.

#### [#8000]: Optimize origin checks

Optimize origin checks, avoid cloning and conversion when not needed.

#### [#7820]: Make pallet-transaction-payment-benchmark work with ed 0

Make it possible to use the transaction-payment work with existential deposit 0

#### [#7081]: [pallet-mmr] Migrate to using frame umbrella crate

This PR migrates the pallet-mmr to use the frame umbrella crate. This is part of the ongoing effort to migrate all pallets to use the frame umbrella crate. The effort is tracked [here](https://github.com/paritytech/polkadot-sdk/issues/6504).

#### [#7479]: omni-node: add offchain worker

Added support for offchain worker to omni-node-lib for both aura and manual seal nodes.

#### [#7952]: Add expensive scenario for asset exchange

This PR introduces an implementation for `worst_case_asset_exchange()` in the `AssetHubWestend` benchmarking setup.

#### [#7843]: Fix XCM Barrier Rejection Handling to Return Incomplete with Weight

This PR addresses an issue with the handling of message execution when blocked by the barrier. Instead of returning an `Outcome::Error`, we modify the behaviour to return `Outcome::Incomplete`, which includes the weight consumed up to the point of rejection and the error that caused the blockage.

This change ensures more accurate weight tracking during message execution, even when interrupted. It improves resource management and aligns the XCM executor’s behaviour with better error handling practices.

#### [#7281]: [pallet-revive] fix eth fee estimation

Fix EVM fee cost estimation.
The current estimation was shown in Native and not EVM decimal currency.

#### [#7481]: add genesis presets for glutton westend

Extracted from #7473.

Part of: <https://github.com/paritytech/polkadot-sdk/issues/5704>.

I did not use the presets in the parachain-bin, as we rely on passing custom para-ids to the chains specs to launch many glutton chains on one relay chain. This is currently not compatible with the genesis presets, which hard code the para ID, see <https://github.com/paritytech/polkadot-sdk/issues/7618> and <https://github.com/paritytech/polkadot-sdk/issues/7384>.

#### [#6220]: Fix metrics not shutting down if there are open connections

Fix prometheus metrics not shutting down if there are open connections

#### [#7848]: [pallet-revive] Add support for eip1898 block notation

[pallet-revive] Add support for eip1898 block notation
<https://eips.ethereum.org/EIPS/eip-1898>

#### [#5990]: On-demand credits

The PR implements functionality on the relay chain for purchasing on-demand Coretime using credits. This means on-demand Coretime should no longer be purchased with the relay chain balance but rather with credits acquired on the Coretime chain. The extrinsic to use for purchasing Coretime is `place_order_with_credits`. It is worth noting that the PR also introduces a minimum credit purchase requirement to prevent potential attacks.

#### [#6896]: pallet-revive: Fix docs.rs

- Fixed failing docs.rs build for `pallet-revive-uapi` by fixing a writing attribute in the manifest (we were using `default-target` instead of `targets`)
- Removed the macros defining host functions because the cfg attributes introduced in #6866 won't work on them
- Added an docs.rs specific attribute so that the `unstable-hostfn` feature tag will show up on the functions that are guarded behind it.

#### [#8057]: Moved bridge primitives to `parity-bridge-common` and `polkadot-fellows/runtimes` repos

Removed packages `bp-polkadot`, `bp-kusama`, `bp-bridge-hub-polkadot` and `bp-bridge-hub-kusama`

#### [#7794]: [glutton-westend] remove `CheckNonce` from `TXExtension` and add sudo key to genesis config

I discovered in <https://github.com/paritytech/polkadot-sdk/pull/7459>, that the overhead benchmark is not working for glutton-westend, as the client can't send `system.remark` extrinsics. This was due to 2 issues:

1. Alice was not set as sudo. Hence, the `CheckOnlySudoAccount` deemed the extrinsic as invalid.
2. The `CheckNonce` TxExtension also marked the extrinsic as invalid, as the account doesn't exist (because glutton has no balances pallet).

This PR fixes the 1.) for now. I wanted to simply remove the `CheckNonce` in the TxExtension to fix 2., but it turns out that this is not possible, as the tx-pool needs the nonce tag to identify the transaction. <https://github.com/paritytech/polkadot-sdk/pull/6884> will fix sending extrinsics on glutton.

#### [#7764]: Add Serialize & Deserialize to umbrella crate derive module

This PR adds serde::Serialize and serde::Deserialize to the frame umbrella crate
`derive` and indirectly `prelude` modules. They can now be accessed through those.
Note: serde will still need to be added as a dependency in consuming crates. That or you'll need to specify th `#[serde(crate = "PATH_TO_SERDE::serde")]` attribute at the
location where Serialize/Deserialize are used.

#### [#7939]: Revert pallet-staking changes which should be released as a separate pallet

Revert multi-block election, slashing and staking client pallets.

Reverted PRs: #7582, #7424, #7282

#### [#6565]: pallet_revive: Switch to 64bit RISC-V

This PR updates pallet_revive to the newest PolkaVM version and adapts the test fixtures and syscall interface to work under 64bit.

Please note that after this PR no 32bit contracts can be deployed (they will be rejected at deploy time). Pre-deployed 32bit contracts are now considered defunct since we changes how parameters are passed for functions with more than 6 arguments.

## Fixtures

The fixtures are now built for the 64bit target. I also removed the temporary directory mechanism that triggered a full rebuild every time. It also makes it easier to find the compiled fixtures since they are now always in `target/pallet-revive-fixtures`.

## Syscall interface

### Passing pointer

Registers and pointers are now 64bit wide. This allows us to pass u64 arguments in a single register. Before we needed two registers to pass them. This means that just as before we need one register per pointer we pass. We keep pointers as `u32` argument by truncating the register. This is done since the memory space of PolkaVM is 32bit.

### Functions with more than 6 arguments

We only have 6 registers to pass arguments. This is why we pass a pointer to a struct when we need more than 6. Before this PR we expected a packed struct and interpreted it as SCALE encoded tuple. However, this was buggy because the `MaxEncodedLen` returned something that was larger than the packed size of the structure. This wasn't a problem before. But now the memory space changed in a way that things were placed at the edges of the memory space and those extra bytes lead to an out of bound access.

This is why this PR drops SCALE and expects the arguments to be passed as a pointer to a `C` aligned struct. This avoids unaligned accesses. However, revive needs to adapt its codegen to properly align the structure fields.

## TODO

- [ ] Add multi block migration that wipes all existing contracts as we made breaking changes to the syscall interface

#### [#6673]: chain-spec-guide-runtime: path to wasm blob fixed

In `chain-spec-guide-runtime` crate's tests, there was assumption that release version of wasm blob exists. This PR uses `chain_spec_guide_runtime::runtime::WASM_BINARY_PATH` const to use correct path to runtime blob.

#### [#5656]: Use Relay Blocknumber in Pallet Broker

Changing `sale_start`, `interlude_length` and `leading_length` in `pallet_broker` to use relay chain block numbers instead of parachain block numbers.
Relay chain block numbers are almost deterministic and more future proof.

#### [#7376]: Documentation update for weight

Document the usage of `#[pallet::call(weight = <T as Config>::WeightInfo)]` within FRAME macros.
This update enhances the documentation for `#[pallet::call]` and `#[pallet::weight]`, providing examples and clarifying weight specifications for dispatchable functions, ensuring consistency with existing guidelines.

#### [#5723]: Adds `BlockNumberProvider` in multisig, proxy and nft pallets

This PR adds the ability for these pallets to specify their source of the block number.
This is useful when these pallets are migrated from the relay chain to a parachain and vice versa.

This change is backwards compatible:

1. If the `BlockNumberProvider` continues to use the system pallet's block number
2. When a pallet deployed on the relay chain is moved to a parachain, but still uses the relay chain's block number

However, we would need migrations if the deployed pallets are upgraded on an existing parachain,
and the `BlockNumberProvider` uses the relay chain block number.

#### [#6452]: elastic scaling RFC 103 end-to-end tests

Adds end-to-end zombienet-sdk tests for elastic scaling using the RFC103 implementation.
Only notable user-facing change is that the default chain configurations of westend and rococo
now enable by default the CandidateReceiptV2 node feature.

#### [#7377]: Add missing events to nomination pool extrinsics

Introduces events to extrinsics from `pallet_nomination_pools` that previously had none:

- `set_metadata`
- `nominate`
- `chill`
- `set_configs`
- `set_claim_permission`

#### [#7784]: [pallet-revive] block.timestamps should return seconds

In solidity `block.timestamp` should be expressed in seconds
see <https://docs.soliditylang.org/en/latest/units-and-global-variables.html#block-and-transaction-properties>

#### [#7619]: Add chain-spec-builder as a subcommand to the polkadot-omni-node

This PR add chain-spec-builder as a subcommand to the polkadot-omni-node

#### [#6349]: runtimes: presets are provided as config patches

This PR introduces usage of build_struct_json_patch macro in all
runtimes (also guides) within the code base.  It also fixes macro to support
field init shorthand, and Struct Update syntax which were missing in original
implementation.

#### [#6689]: [pallet-revive] Update gas encoding

Update the current approach to attach the `ref_time`, `pov` and `deposit` parameters to an Ethereum transaction.
Previously, these three parameters were passed along with the signed payload, and the fees resulting from gas × gas_price were checked to ensure they matched the actual fees paid by the user for the extrinsic
This approach unfortunately can be attacked. A malicious actor could force such a transaction to fail by injecting low values for some of these extra parameters as they are not part of the signed payload.
The new approach encodes these 3 extra parameters in the lower digits of the transaction gas, using the log2 of the actual values to  encode each components on 2 digits

#### [#6302]: migrate pallet-nomination-pool-benchmarking to benchmarking syntax v2

migrate pallet-nomination-pool-benchmarking to benchmarking syntax v2

#### [#7625]: Update to Rust stable 1.84.1

Ref <https://github.com/paritytech/ci_cd/issues/1107>

We mainly need that so that we can finally compile the `pallet_revive` fixtures on stable. I did my best to keep the commits focused on one thing to make review easier.

All the changes are needed because rustc introduced more warnings or is more strict about existing ones. Most of the stuff could just be fixed and the commits should be pretty self explanatory. However, there are a few this that are notable:

## `non_local_definitions`

A lot of runtimes to write `impl` blocks inside functions. This makes sense to reduce the amount of conditional compilation. I guess I could have moved them into a module instead. But I think allowing it here makes sense to avoid the code churn.

## `unexpected_cfgs`

The FRAME macros emit code that references various features like `std`, `runtime-benchmarks` or `try-runtime`. If a create that uses those macros does not have those features we get this warning. Those were mostly when defining a `mock` runtime. I opted for silencing the warning in this case rather than adding not needed features.

For the benchmarking ui tests I opted for adding the `runtime-benchmark` feature to the `Cargo.toml`.

## Failing UI test

I am bumping the `trybuild` version and regenerating the ui tests. The old version seems to be incompatible. This requires us to pass `deny_warnings` in `CARGO_ENCODED_RUSTFLAGS` as `RUSTFLAGS` is ignored in the new version.

## Removing toolchain file from the pallet revive fixtures

This is no longer needed since the latest stable will compile them fine using the `RUSTC_BOOTSTRAP=1`.

#### [#7721]: revive: Rework the instruction benchmark

Fixes <https://github.com/paritytech/polkadot-sdk/issues/6157>

This fixes the last remaining benchmark that was not correct since it was too low level to be written in Rust. Instead, we opted.

This PR changes the benchmark that determines the scaling from `ref_time` to PolkaVM `Gas` by benchmarking the absolute worst case of an instruction: One that causes two cache misses by touching two cache lines.

The Contract itself is designed to be as simple as possible. It does random unaligned reads in a loop until the `r` (repetition) number is reached. The randomness is fully generated by the host and written to the guests memory before the benchmark is run. This allows the benchmark to determine the influence of one loop iteration via linear regression.

#### [#3926]: Introduce pallet-asset-rewards

Introduce pallet-asset-rewards, which allows accounts to be rewarded for freezing fungible tokens. The motivation for creating this pallet is to allow incentivising LPs.
See the pallet docs for more info about the pallet.

#### [#6624]: Use `cmd_lib` instead of `std::process::Command` when using `#[docify::export]`

Simplified the display of commands and ensured they are tested for chain spec builder's `polkadot-sdk` reference docs.

#### [#6986]: [pallet-mixnet] Migrate to using frame umbrella crate

This PR migrates the pallet-mixnet to use the frame umbrella crate. This is part of the ongoing effort to migrate all pallets to use the frame umbrella crate. The effort is tracked [here](https://github.com/paritytech/polkadot-sdk/issues/6504).

#### [#7587]: [AHM] Poke deposits: Indices pallet

Add a new extrinsic `poke_deposit` to `pallet-indices`. This extrinsic will be used to re-adjust the deposits made in the pallet after Asset Hub Migration.

#### [#7424]: Bounded Slashing: Paginated Offence Processing & Slash Application

NOTE: This is reverted in #7939.
This PR refactors the slashing mechanism in `pallet-staking` to be bounded by introducing paged offence processing and paged slash application.

      ### Key Changes
      - Offences are queued instead of being processed immediately.
      - Slashes are computed in pages, stored as a `StorageDoubleMap` with `(Validator, SlashFraction, PageIndex)` to uniquely identify them.
      - Slashes are applied incrementally across multiple blocks instead of a single unbounded operation.
      - New storage items: `OffenceQueue`, `ProcessingOffence`, `OffenceQueueEras`.
      - Updated API for cancelling and applying slashes.
      - Preliminary benchmarks added; further optimizations planned.

      This enables staking slashing to scale efficiently and removes a major blocker for staking migration to a parachain (AH).

#### [#6623]: Update Society Pallet to Support Block Number Provider

This PR makes the block number provider in the Society pallet configurable so that runtimes can choose between using the system block number or an alternative source like the relay chain’s block number.
If you want to keep the existing behavior, simply set the provider to System. For scenarios that require a different notion of block number—such as using a relay chain number you can select another provider,
ensuring flexibility in how the Society pallet references the current block.

#### [#7167]: [pallet-revive] Add tracing support (2/2)

- Add debug endpoint to eth-rpc for capturing a block or a single transaction traces
- Use in-memory DB for non-archive node

See:

- PR #7166

#### [#7760]: Dynamic uncompressed code size limit

Deprecates node constant `VALIDATION_CODE_BOMB_LIMIT` and introduces `validation_code_bomb_limit` runtime API that computes the maximum uncompressed code size as the maximum code size multiplied by a compression ratio of 10.

#### [#7818]: [pallet-revive] eth-rpc-tester quick fixes

Small tweaks to the eth-rpc-tester bin

#### [#7198]: [pallet-revive] implement the block author API

This PR implements the block author API method. Runtimes ought to implement it such that it corresponds to the `coinbase` EVM opcode.

#### [#7008]: feat(wasm-builder): add support for new `wasm32v1-none` target

Resolves [#5777](https://github.com/paritytech/polkadot-sdk/issues/5777)

Previously `wasm-builder` used hacks such as `-Zbuild-std` (required `rust-src` component) and `RUSTC_BOOTSTRAP=1` to build WASM runtime without WASM features: `sign-ext`, `multivalue` and `reference-types`, but since Rust 1.84 (will be stable on 9 January, 2025) the situation has improved as there is new [`wasm32v1-none`](https://doc.rust-lang.org/beta/rustc/platform-support/wasm32v1-none.html) target that disables all "post-MVP" WASM features except `mutable-globals`.

Wasm builder requires the following prerequisites for building the WASM binary:

- Rust >= 1.68 and Rust < 1.84:
  - `wasm32-unknown-unknown` target
  - `rust-src` component
- Rust >= 1.84:
  - `wasm32v1-none` target
  - no more `-Zbuild-std` and `RUSTC_BOOTSTRAP=1` hacks and `rust-src` component requirements!

#### [#6059]: [mq pallet] Custom next queue selectors

Changes:

- Expose a `force_set_head` function from the `MessageQueue` pallet via a new trait: `ForceSetHead`. This can be used to force the MQ pallet to process this queue next.
- The change only exposes an internal function through a trait, no audit is required.

## Context

For the Asset Hub Migration (AHM) we need a mechanism to prioritize the inbound upward messages and the inbound downward messages on the AH. To achieve this, a minimal (and no breaking) change is done to the MQ pallet in the form of adding the `force_set_head` function.

An example use of how to achieve prioritization is then demonstrated in `integration_test.rs::AhmPrioritizer`. Normally, all queues are scheduled round-robin like this:

`| Relay | Para(1) | Para(2) | ... | Relay | ...`

The prioritizer listens to changes to its queue and triggers if either:

- The queue processed in the last block (to keep the general round-robin scheduling)
- The queue did not process since `n` blocks (to prevent starvation if there are too many other queues)

In either situation, it schedules the queue for a streak of three consecutive blocks, such that it would become:

`| Relay | Relay | Relay | Para(1) | Para(2) | ... | Relay | Relay | Relay | ...`

It basically transforms the round-robin into an elongated round robin. Although different strategies can be injected into the pallet at runtime, this one seems to strike a good balance between general service level and prioritization.

#### [#7438]: Fix DryRunApi client-facing XCM versions

Fixes <https://github.com/paritytech/polkadot-sdk/issues/7413>

This PR updates the DryRunApi. The signature of the dry_run_call is changed, and the XCM version of the return values of dry_run_xcm now follows the version of the input XCM program.

It also fixes xcmp-queue's Router's `clear_messages`: the channel details `first_index` and `last_index` are reset when clearing.

#### [#6411]: Support more types in TypeWithDefault

This PR supports more integer types to be used with `TypeWithDefault` and makes `TypeWithDefault<u8/u16/u32, ..>: BaseArithmetic` satisfied

#### [#7568]: pallet-revive: Fix the contract size related benchmarks

Partly addresses <https://github.com/paritytech/polkadot-sdk/issues/6157>

The benchmarks measuring the impact of contract sizes on calling or instantiating a contract were bogus because they needed to be written in assembly in order to tightly control the basic block size.

This fixes the benchmarks for:

- call_with_code_per_byte
- upload_code
- instantiate_with_code

And adds a new benchmark that accounts for the fact that the interpreter will always compile whole basic blocks:

- basic_block_compilation

After this PR only the weight we assign to instructions need to be addressed.

#### [#7234]: Add EventEmitter to XCM Executor

This PR introduces the `EventEmitter` trait to the XCM executor, allowing configurable event emission.

#### [#7177]: Make frame crate not experimental

Frame crate may still be unstable, but it is no longer feature gated by the feature `experimental`.

#### [#6428]: FRAME: Meta Transaction

Introduces the meta-tx pallet that implements Meta Transactions.

The meta transaction follows a layout similar to that of a regular transaction and can leverage the same extensions that implement the `TransactionExtension` trait. Once signed and
shared by the signer, it can be relayed by a relayer. The relayer then submits a regular transaction with the `meta-tx::dispatch` call, passing the signed meta transaction as an
argument.

To see an example, refer to the mock setup and the `sign_and_execute_meta_tx` test case within the pallet.

#### [#7589]: [pallet-revive] rpc add --earliest-receipt-block

Add a cli option to skip searching receipts for blocks older than the specified limit

#### [#7476]: add genesis presets for coretime parachains

Extracted from #7473.

Part of: <https://github.com/paritytech/polkadot-sdk/issues/5704>.

#### [#6835]: [pallet-revive] implement the call data load API

This PR implements the call data load API akin to [how it works on ethereum](https://www.evm.codes/?fork=cancun#35).

#### [#7086]: [pallet-revive] Fix `caller_is_root` return value

The return type of the host function `caller_is_root` was denoted as `u32` in `pallet_revive_uapi`. This PR fixes the return type to `bool`. As a drive-by, the PR re-exports `pallet_revive::exec::Origin` to extend what can be tested externally.

#### [#6453]: [pallet-revive] breakdown integration tests

Break down the single integration tests into multiple tests, use keccak-256 for tx.hash

#### [#7918]: Add an extra_constant to pallet-treasury

- Allows to query pallet-treasury's pot account via extra constant.

#### [#6954]: [pallet-revive] implement the gas price API

This PR implements the EVM gas price syscall API method. Currently this is a compile time constant in revive, but in the EVM it is an opcode. Thus we should provide an opcode for this in the pallet.

#### [#7327]: Correctly register the weight n `set_validation_data` in `cumulus-pallet-parachain-system`

The actual weight of the call was register as a refund, but the pre-dispatch weight is 0, and we can't refund from 0. Now the actual weight is registered manually instead of ignored.

#### [#7493]: [pallet-revive] fix eth-rpc indexing

- Fix a deadlock on the RWLock cache
- Remove eth-indexer, we won't need it anymore, the indexing will be started from within eth-rpc directly

#### [#7407]: Fixes #219

Add a new extrinsic `dispatch_as_fallible`.

It's almost the same as `dispatch_as` but check the result of the call.

Closes #219.

And add more unit tests to cover `dispatch_as` and `dispatch_as_fallible`.

---

Polkadot address: 156HGo9setPcU2qhFMVWLkcmtCEGySLwNqa3DaEiYSWtte4Y

#### [#7671]: Fix: [Referenda Tracks] Resolve representation issues that are breaking PJS apps

The PR #2072 introduces a change in the representation of the `name` field, from a `&str` to a `[u8; N]` array. This is because tracks can be retrieves from storage, and thus, a static string representation doesn't meet with the storage traits requirements.

This PR encapsulates this array into a `StringLike` structure that allows representing the value as a `str` for SCALE and metadata purposes. This is to avoid breaking changes.

This PR also reverts the representation of the `Tracks` constant as a tuple of `(TrackId, TrackInfo)` to accomplish the same purpose of avoid breaking changes to runtime users and clients.

#### [#6311]: Migrate pallet-fast-unstake and pallet-babe benchmark to v2

Migrate pallet-fast-unstake and pallet-babe benchmark to v2

#### [#7254]: deprecate AsyncBackingParams

Removes all usage of the static async backing params, replacing them with dynamically computed equivalent values (based on the claim queue and scheduling lookahead).

Adds a new runtime API for querying the scheduling lookahead value. If not present, falls back to 3 (the default value that is backwards compatible with values we have on production networks for allowed_ancestry_len)

Also removes most code that handles async backing not yet being enabled, which includes support for collation protocol version 1 on collators, as it only worked for leaves not supporting async backing (which are none).

#### [#7048]: [pallet-salary] Migrate to using frame umbrella crate

This PR migrates the `pallet-salary` to use the FRAME umbrella crate.   This is part of the ongoing effort to migrate all pallets to use the FRAME umbrella crate.   The effort is tracked [here](https://github.com/paritytech/polkadot-sdk/issues/6504).

#### [#6621]: Update Conviction Voting Pallet to Support Block Number Provider

This PR makes the block number provider used in the society pallet configurable. Before this PR, society pallet always used the system block number, with this PR some runtime can opt to use the relay chain block number instead.

#### [#7802]: [AHM] child bounties and recovery: make more stuff public

Make some items in the child-bounties and recovery pallet public to reduce code-duplication for the Asset Hub migration.

#### [#6293]: Migrate pallet-nis benchmark to v2

- Refactor to use the `frame` crate.
- Use procedural macro version `construct_runtime` in mock.
- Expose `PalletId` to `frame::pallet_prelude`.
- Part of #6202.

#### [#6526]: sp-runtime: Be a little bit more functional :D

Some internal refactorings in the `Digest` code.

#### [#7325]: [pallet-revive] eth-rpc minor fixes

- Add option to specify database_url from an environment variable
- Add  a test-deployment.rs rust script that can be used to test deployment and call of a contract before releasing eth-rpc
- Make evm_block non fallible so that it can return an Ok response for older blocks when the runtime API is not available
- Update subxt version to integrate changes from <https://github.com/paritytech/subxt/pull/1904>

#### [#6544]: Add and test events to conviction voting pallet

Add event for the unlocking of an expired conviction vote's funds, and test recently added
voting events.

#### [#7203]: pallet_revive: Bump PolkaVM

Update to PolkaVM `0.19`. This version renumbers the opcodes in order to be in-line with the grey paper. Hopefully, for the last time. This means that it breaks existing contracts.

#### [#6715]: Update Nomination Pool Pallet to Support Block Number Provider

This PR makes the block number provider in the Society pallet configurable so that runtimes can choose between using the system block number or an alternative source like the relay chain’s block number.
If you want to keep the existing behavior, simply set the provider to System. For scenarios that require a different notion of block number—such as using a relay chain number you can select another provider,
ensuring flexibility in how the nomination pools pallet references the current block.

#### [#6960]: Add sudo pallet to coretime-westend

Add the sudo pallet to coretime-westend, allowing use in development/testing. Previously the coretime-rococo runtime was used in situations like this, but since Rococo is now gone this can be used instead.

#### [#6321]: Utility call fallback

This PR adds the `if_else` call to `pallet-utility`
enabling an error fallback when the main call is unsuccessful.

#### [#6439]: pallet-membership: Do not verify the `MembershipChanged` in bechmarks

There is no need to verify in the `pallet-membership` benchmark that the `MemembershipChanged` implementation works as the pallet thinks it should work. If you for example set it to `()`, `get_prime()` will always return `None`.

TLDR: Remove the checks of `MembershipChanged` in the benchmarks to support any kind of implementation.

#### [#7723]: [pallet-bounties] Allow bounties to never expire

Refactored the `update_due` calculation to use `saturating_add`, allowing bounties to remain active indefinitely without requiring `extend_bounty_expiry` and preventing automatic curator slashing for inactivity. Previously, setting `BountyUpdatePeriod` to a large value, such as `BlockNumber::max_value()`, could cause an overflow.

#### [#7734]: Simplify event assertion with predicate-based check

Simplify event assertions by introducing `contains_event`, reducing duplicated code.

#### [#6533]: Migrate executor into PolkaVM 0.18.0

Bump `polkavm` to 0.18.0, and update `sc-polkavm-executor` to be compatible with the API changes. In addition, bump also `polkavm-derive` and `polkavm-linker` in order to make sure that the all parts of the Polkadot SDK use the exact same ABI for `.polkavm` binaries.

Purely relying on RV32E/RV64E ABI is not possible, as PolkaVM uses a RISCV-V alike ISA, which is derived from RV32E/RV64E but it is still its own microarchitecture, i.e. not fully binary compatible.

#### [#6796]: pallet-revive: Remove unused dependencies

The dependency on `pallet_balances` doesn't seem to be necessary. At least everything compiles for me without it. Removed this dependency and a few others that seem to be left overs.

#### [#6759]: pallet-revive: Statically verify imports on code deployment

Previously, we failed at runtime if an unknown or unstable host function was called. This requires us to keep track of when a host function was added and when a code was deployed. We used the `api_version` to track at which API version each code was deployed. This made sure that when a new host function was added that old code won't have access to it. This is necessary as otherwise the behavior of a contract that made calls to this previously non existent host function would change from "trap" to "do something".

In this PR we remove the API version. Instead, we statically verify on upload that no non-existent host function is ever used in the code. This will allow us to add new host function later without needing to keep track when they were added.

This simplifies the code and also gives an immediate feedback if unknown host functions are used.

#### [#7109]: Add "run to block" tools

Introduce `frame_system::Pallet::run_to_block`, `frame_system::Pallet::run_to_block_with`, and `frame_system::RunToBlockHooks` to establish a generic `run_to_block` mechanism for mock tests, minimizing redundant implementations across various pallets.

Closes #299.

#### [#6608]: [pallet-revive] eth-prc fix geth diff

* Add a bunch of differential tests to ensure that responses from eth-rpc matches the one from `geth`
- EVM RPC server will not fail gas_estimation if no gas is specified, I updated pallet-revive to add an extra `skip_transfer` boolean check to replicate this behavior in our pallet
- `eth_transact` and `bare_eth_transact` api have been updated to use `GenericTransaction` directly as this is what is used by `eth_estimateGas` and `eth_call`

#### [#5724]: Validator Re-Enabling (master PR)

Implementation of the Stage 3 for the New Disabling Strategy: <https://github.com/paritytech/polkadot-sdk/issues/4359>

This PR changes when an active validator node gets disabled for comitting offences.
When Byzantine Threshold Validators (1/3) are already disabled instead of no longer disabling the highest offenders will be disabled potentially re-enabling low offenders.

#### [#7685]: Introduce filters to restrict accounts from staking

Introduce filters to restrict accounts from staking.
This is useful for restricting certain accounts from staking, for example, accounts staking via pools, and vice versa.

#### [#6792]: Add fallback_max_weight to snowbridge Transact

We removed the `require_weight_at_most` field and later changed it to `fallback_max_weight`.
This was to have a fallback when sending a message to v4 chains, which happens in the small time window when chains are upgrading.
We originally put no fallback for a message in snowbridge's inbound queue but we should have one.
This PR adds it.

#### [#6140]: Accurate weight reclaim with frame_system::WeightReclaim and cumulus `StorageWeightReclaim` transaction extensions

Since the introduction of transaction extension, the transaction extension weight is no longer part of base extrinsic weight. As a consequence some weight of transaction extensions are missed when calculating post dispatch weight and reclaiming unused block weight.

For solo chains, in order to reclaim the weight accurately `frame_system::WeightReclaim` transaction extension must be used at the end of the transaction extension pipeline.

For para chains `StorageWeightReclaim` in `cumulus-primitives-storage-weight-reclaim` is deprecated.
A new transaction extension `StorageWeightReclaim` in `cumulus-pallet-weight-reclaim` is introduced.
`StorageWeightReclaim` is meant to be used as a wrapping of the whole transaction extension pipeline, and will take into account all proof size accurately.

The new wrapping transaction extension is used like this:

```rust
/// The TransactionExtension to the basic transaction logic.
pub type TxExtension = cumulus_pallet_weight_reclaim::StorageWeightReclaim<
       Runtime,
       (
               frame_system::CheckNonZeroSender<Runtime>,
               frame_system::CheckSpecVersion<Runtime>,
               frame_system::CheckTxVersion<Runtime>,
               frame_system::CheckGenesis<Runtime>,
               frame_system::CheckEra<Runtime>,
               frame_system::CheckNonce<Runtime>,
               pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
               BridgeRejectObsoleteHeadersAndMessages,
               (bridge_to_rococo_config::OnBridgeHubWestendRefundBridgeHubRococoMessages,),
               frame_metadata_hash_extension::CheckMetadataHash<Runtime>,
               frame_system::CheckWeight<Runtime>,
       ),
>;
```

NOTE: prior to transaction extension, `StorageWeightReclaim` also missed the some proof size used by other transaction extension prior to itself. This is also fixed by the wrapping `StorageWeightReclaim`.

#### [#8024]: Change the hash of `PendingOrders` storage item

Change the hash to `Twox64Concat`, which is more secure than `Identity`.

#### [#7378]: fix pre-dispatch PoV underweight for ParasInherent

This should fix the error log related to PoV pre-dispatch weight being lower than post-dispatch for `ParasInherent`:

```
ERROR tokio-runtime-worker runtime::frame-support: Post dispatch weight is greater than pre dispatch weight. Pre dispatch weight may underestimating the actual weight. Greater post dispatch weight components are ignored.
                                        Pre dispatch weight: Weight { ref_time: 47793353978, proof_size: 1019 },
                                        Post dispatch weight: Weight { ref_time: 5030321719, proof_size: 135395 }
```

#### [#7893]: Use non-native token to benchmark on asset hub

Asset Hub was using the native token for benchmarking xcm instructions. This is not the best since it's cheaper than using something in `pallet-assets` for example.
Had to remove some restrictive checks from `pallet-xcm-benchmarks`.

#### [#7120]: Remove pallet::getter from bridges/modules

This PR removes all pallet::getter occurrences from pallet-bridge-grandpa, pallet-bridge-messages and pallet-bridge-relayers and replaces them with explicit implementations.

#### [#4530]: Implement `pallet-assets-holder` and consider ED part of frozen amount in `pallet-assets`

This change creates the `pallet-assets-holder` pallet, as well as changes `pallet-assets` to support querying held balances via a new trait: `BalanceOnHold`.

## Changes in Balance Model

The change also adjusts the balance model implementation for fungible sets. This aligns the calculation of the _spendable_ balance (that can be reduced either via withdrawals, like
paying for fees, or transfer to other accounts) to behave like it works with native tokens.

As a consequence, when this change is introduced, adding freezes (a.k.a. locks) or balances on hold (a.k.a. reserves) to an asset account will constraint the amount of balance for such account that can be withdrawn or transferred, and will affect the ability for these accounts to be destroyed.

### Example

Before the changes in the balance model, an asset account balance could look like something like this:

```
|____________balance____________|
       |__frozen__|
|__ed__|
|___untouchable___|__spendable__|
```

In the previous model, you could spend funds up to `ed + frozen` where `ed` is the minimum balance for an asset class, and `frozen` is the frozen amount (if any `freezes` are in place).

Now, the model looks like this:

```
|__total__________________________________|
|__on_hold__|_____________free____________|
|__________frozen___________|
|__on_hold__|__ed__|
            |__untouchable__|__spendable__|
```

There's now a balance `on_hold` and a `free` balance. The balance `on_hold` is managed by a `Holder` (typically `pallet-assets-holder`) and `free` is the balance that remains in `pallet-assets`. The `frozen` amount can be subsumed into the balance `on_hold`, and now you can spend funds up to `max(frozen, ed)`, so if for an account, `frozen` is less or equal than `on_hold + ed`, you'd be able to spend your `free` balance up to `ed`. If for the account, `frozen` is more than `on_hold + ed`, the remaining amount after subtracting `frozen` to `on_hold + ed` is the amount you cannot spend from your `free` balance.

See [sdk docs](https://paritytech.github.io/polkadot-sdk/master/frame_support/traits/tokens/fungible/index.html#visualising-balance-components-together-)
to understand how to calculate the spendable balance of an asset account on the client side.

## Implementation of `InspectHold` and `MutateHold`

The `pallet-assets-holder` implements `hold` traits for `pallet-assets`, by extending this pallet and implementing the `BalanceOnHold` trait so the held balance can be queried by
`pallet-assets` to calculate the reducible (a.k.a. spendable) balance.

These changes imply adding a configuration type in `pallet-assets` for `Holder`

## Default implementation of `Holder`

Use `()` as the default value, when no holding capabilities are wanted in the runtime implementation.

## Enable `pallet-assets-holder`

Define an instance of `pallet-assets-holder` (we'll call it `AssetsHolder`) and use `AssetsHolder` as the type for `Holder`, when intend to use holding capabilities are
wanted in the runtime implementation.

#### [#5363]: [pallet-xcm] waive transport fees based on XcmConfig

pallet-xcm::send() no longer implicitly waives transport fees for the local root location, but instead relies on xcm_executor::Config::FeeManager to determine whether certain locations have free transport.

🚨 Warning: 🚨 If your chain relies on free transport for local root, please make sure to add Location::here() to the waived-fee locations in your configured xcm_executor::Config::FeeManager.

#### [#7769]: Ensure Logs Are Captured for Assertions and Printed During Tests

This PR enhances test_log_capture, ensuring logs are captured for assertions and printed to the console during test execution.

#### [#7093]: initial docify readme with some content #6333

Docifying the README.MD under templates/parachain by adding a Docify.
Also Adding the Cargo.toml under the same folder, essentially making it a crate as Docify acts for Readmes only under the same crate.

#### [#7813]: Improve metadata for `SkipCheckIfFeeless`

If the inner transaction extension used inside `SkipCheckIfFeeless` are multiples then the metadata is not correct, it is now fixed.

E.g. if the transaction extension is `SkipCheckIfFeeless::<Runtime, (Payment1, Payment2)>` then the metadata was wrong.

#### [#6866]: Refactor `pallet-revive-uapi` pallet

Puts unstable host functions in `uapi` under `unstable-api` feature while moving those functions after stable functions.

#### [#6460]: [pallet-revive] set logs_bloom

Set the logs_bloom in the transaction receipt

#### [#7846]: Incrementable: return None instead of saturating

Fix implementation to follow the trait specification more closely. Closes #7845

#### [#6111]: [pallet-revive] Update delegate_call to accept address and weight

Enhance the `delegate_call` function to accept an `address` target parameter instead of a `code_hash`.
This allows direct identification of the target contract using the provided address.
Additionally, introduce parameters for specifying a customizable `ref_time` limit and `proof_size` limit, thereby improving flexibility and control during contract interactions.

#### [#6450]: Add omni-node checks for runtime parachain compatibility

OmniNode parses runtime metadata and checks against the existence of `cumulus-pallet-parachain-system` and `frame-system`, by filtering pallets by names: `ParachainSystem` and `System`. It also checks the `frame-system` pallet storage `Number` type, and then uses it to configure AURA if `u32` or `u64`.

#### [#6562]: Hide nonce implementation details in metadata

Use custom implementation of TypeInfo for TypeWithDefault to show inner value's type info.
This should bring back nonce to u64 in metadata.

#### [#4722]: Implement pallet view functions

Read-only view functions can now be defined on pallets. These functions provide an interface for querying state, from both outside and inside the runtime. Common queries can be defined on pallets, without users having to access the storage directly.

#### [#6290]: Migrate pallet-transaction-storage and pallet-indices to benchmark v2

Part of:
# 6202

#### [#6425]: Introduce `ConstUint` to make dependent types in `DefaultConfig` more adaptable

Introduce `ConstUint` that is a unified alternative to `ConstU8`, `ConstU16`, and similar types, particularly useful for configuring `DefaultConfig` in pallets.
It enables configuring the underlying integer for a specific type without the need to update all dependent types, offering enhanced flexibility in type management.

#### [#6267]: Allow configurable number of genesis accounts with specified balances for benchmarking

This pull request adds an additional field `dev_accounts` to the `GenesisConfig` of the balances pallet, feature gated by `runtime-benchmarks`.

Bringing about an abitrary number of derived dev accounts when building the genesis state. Runtime developers should supply a derivation path that includes an index placeholder
(i.e. "//Sender/{}") to generate multiple accounts from the same root in a consistent manner.

#### [#7786]: pallet revive: rpc build script should not panic

Fix a build error in the pallet revive RPC build scrip that can occur when using `cargo remote` or `cargo vendor`.

#### [#8063]: Bridges: Add initial primitives for AssetHub bridging

Add initial primitives for AssetHubRococo and AssetHubWestend bridging

#### [#6310]: Migrate pallet-child-bounties benchmark to v2

Part of:

- #6202.

#### [#7924]: sp-api: Support `mut` in `impl_runtime_apis!`

This brings support for declaring variables in parameters as `mut` inside of `impl_runtime_apis!`.

#### [#5899]: Remove usage of AccountKeyring

Compared with AccountKeyring, Sr25519Keyring and Ed25519Keyring are more intuitive.
When both Sr25519Keyring and Ed25519Keyring are required, using AccountKeyring bring confusion.
There are two AccountKeyring definitions, it becomes more complex if export two AccountKeyring from frame.

#### [#7590]: [pallet-revive] move exec tests

Moving exec tests into a new file

#### [#6466]: [pallet-revive] add piggy-bank sol example

This PR update the pallet to use the EVM 18 decimal balance in contracts call and host functions instead of the native balance.

It also updates the js example to add the piggy-bank solidity contract that expose the problem

#### [#7835]: XCM: Some weight fixes for `InitiateTransfer`

- Added some base weight for `InitiateTransfer` no matter what.
- Short circuit on `AllCounted(0)` to not have to go through all fungibles.

#### [#7810]: [pallet-revive] precompiles 2->9

Add missing pre-compiles 02 -> 09

#### [#7359]: Improve `set_validation_data` error message

Adds a more elaborate error message to the error that appears when `set_validation_data` is missing in a parachain block.

#### [#7477]: add genesis presets for the people chains

Extracted from #7473.

Part of: <https://github.com/paritytech/polkadot-sdk/issues/5704>.

#### [#7046]: adding warning when using default substrateWeight in production

PR for #3581
Added a cfg to show a deprecated warning message when using std

#### [#7856]: Fix XCM decoding inconsistencies

This PR adjusts the XCM decoding logic in order to deduplicate the logic used for decoding `v3::Xcm`, `v4::Xcm` and `v5::Xcm` and also to use `decode_with_depth_limit()` in some more places.
Also `VersionedXcm::validate_xcm_nesting()` is renamed to `VersionedXcm::check_is_decodable()`.

#### [#7563]: Bump frame-metadata v16 to 19.0.0

Update to latest version of `frame-metadata` and `merkleized-metadata` in order to support pallet view function metadata.

#### [#7809]: [XCM] Add generic location to account converter that also works with external ecosystems for bridge hubs

Adds a new `ExternalConsensusLocationsConverterFor` struct to handle external global consensus locations and their child locations for Bridge Hubs.
This struct extends the functionality of existing converters (`GlobalConsensusParachainConvertsFor` and `EthereumLocationsConverterFor`) while maintaining backward compatibility.

#### [#6665]: Fix runtime api impl detection by construct runtime

Construct runtime uses autoref-based specialization to fetch the metadata about the implemented runtime apis. This is done to not fail to compile when there are no runtime apis implemented. However, there was an issue with detecting runtime apis when they were implemented in a different file. The problem is solved by moving the trait implemented by `impl_runtime_apis!` to the metadata ir crate.

Closes: <https://github.com/paritytech/polkadot-sdk/issues/6659>

#### [#7627]: Derive `DecodeWithMemTracking` for cumulus pallets and for `polkadot-sdk` runtimes

Related to <https://github.com/paritytech/polkadot-sdk/issues/7360>

Derive `DecodeWithMemTracking` for the structures in the cumulus pallets and for the structures in the `polkadot-sdk` runtimes.

The PR contains no functional changes and no manual implementation. Just deriving `DecodeWithMemTracking`.

#### [#7418]: Refactor

This PR contains a small refactor in the logic of #[benchmarks] so if a where clause is included the expanded code set the bound T:Config inside the where clause

#### [#6419]: Use the custom target riscv32emac-unknown-none-polkavm

Closes: <https://github.com/paritytech/polkadot-sdk/issues/6335>

#### [#6681]: update scale-info to 2.11.6

Updates scale-info to 2.11.1 from 2.11.5.
Updated version of scale-info annotates generated code with `allow(deprecated)`

#### [#7482]: [pallet-revive] rpc - gas used fixes

# 7463 follow up with RPC fixes

#### [#7763]: [staking] Currency Migration and Overstake fix

- Fixes Currency to fungible migration with force-unstake of partial unbonding accounts.
- Removes the extrinsic `withdraw_overstake` which is not useful post fungibe migration of pallet-staking.

#### [#7176]: [pallet-revive] Bump asset-hub westend spec version

Bump asset-hub westend spec version

#### [#7787]: Add asset-hub-next as a trusted teleporter

Asset Hub Next has been deployed on Westend as parachain 1100, but it's not yet a trusted teleporter.
This minimal PR adds it in stable2412 so that it can be deployed right away without waiting for the rest of the release to be finalised and deployed.

#### [#7981]: Bump ParachainHost runtime API to 13

Bump `backing_constraints` and `scheduling_lookahead` API version to 13.
The `validation_code_bomb_limit` API remains at version 12.
Bump all ParachainHost runtime to version 13 in all test runtimes.

#### [#7652]: [pallet-revive] ecrecover

Add ECrecover 0x1 precompile and remove the unstable equivalent host function.

#### [#7194]: [FRAME] `pallet_asset_tx_payment`: replace `AssetId` bound from `Copy` to `Clone`

`OnChargeAssetTransaction`'s associated type `AssetId` is bounded by `Copy` which makes it impossible to use `staging_xcm::v4::Location` as `AssetId`. This PR bounds `AssetId` to `Clone` instead, which is more lenient.

#### [#6368]: Migrate inclusion benchmark to v2

Migrate inclusion benchmark to v2.

#### [#6503]: xcm: minor fix for compatibility with V4

Following the removal of `Rococo`, `Westend` and `Wococo` from `NetworkId`, fixed `xcm::v5::NetworkId` encoding/decoding to be compatible with `xcm::v4::NetworkId`

#### [#7983]: [XCM] allow signed account to be aliased between system chains

New alias filter available `AliasAccountId32FromSiblingSystemChain`:
that allows account `X` on a system chain to alias itself on another chain where the filter is installed.
Enables UX improvements like configuring other chains to allow signed account on AH to operate over XCM on another chain using the same signed account on the remote chain (rather than use a sovereign account).

#### [#7251]: [pallet-revive] eth-rpc error logging

Log error instead of failing with an error when block processing fails

#### [#7318]: revive: Fix compilation of `uapi` crate when `unstable-hostfn` is not set

This regression was introduced with some of the recent PRs. Regression fixed and test added.

#### [#6583]: Bump Westend AH

Bump Asset-Hub westend spec version

#### [#6301]: migrate pallet-nft-fractionalization to benchmarking v2 syntax

Migrates pallet-nft-fractionalization to benchmarking v2 syntax.

Part of:
- #6202

#### [#6604]: dmp: Check that the para exist before delivering a message

Ensure that a para exists before trying to deliver a message to it.
Besides that `ensure_successful_delivery` function is added to `SendXcm`. This function should be used by benchmarks to ensure that the delivery of a Xcm will work in the benchmark.

#### [#6502]: sp-trie: correctly avoid panicking when decoding bad compact proofs

"Fixed the check introduced in [PR #6486](https://github.com/paritytech/polkadot-sdk/pull/6486). Now `sp-trie` correctly avoids panicking when decoding bad compact proofs."

#### [#7003]: Added logging for xcm filters/helpers/matchers/types

This PR adds error logs to assist in debugging xcm.
Specifically, for filters, helpers, matchers.
Additionally, it replaces the usages of `log` with `tracing`.

#### [#6486]: sp-trie: minor fix to avoid panic on badly-constructed proof

"Added a check when decoding encoded proof nodes in `sp-trie` to avoid panicking when receiving a badly constructed proof, instead erroring out."

#### [#7641]: XCM: Process PayFees only once

The `PayFees` instruction should only ever be used once. If it's used more than once, it's just a noop.

#### [#6890]: Alter semantic meaning of 0 in metering limits of EVM contract calls

A limit of 0, for gas meters and storage meters, no longer has the meaning of unlimited metering.

#### [#6393]: [pallet-revive] adjust fee dry-run calculation

- Fix bare_eth_transact so that it estimate more precisely the transaction fee
- Add some context to the build.rs to make it easier to troubleshoot errors
- Add TransactionBuilder for the RPC tests.
- Tweaked some error message, We will need to wait for the next subxt release to properly downcast some errors and
adopt MM error code (<https://eips.ethereum.org/EIPS/eip-1474#error-codes>)

#### [#6034]: Adds multi-block election types and refactors current single logic to support it

This PR adds election types and structs required to run a multi-block election. In addition, it modifies EPM, staking pallet and all dependent pallets and logic to use the multi-block types.

#### [#7570]: [pallet-revive] fix subxt version

Cargo.lock change to subxt were rollback. Fixing it and updating it in Cargo.toml so it does not happen again

#### [#7582]: Implementation of `ah-client` and `rc-client` staking pallets

NOTE: This is reverted in #7939.
This PR introduces the initial structure for `pallet-ah-client` and `pallet-rc-client`. These pallets will reside on the relay chain and AssetHub, respectively, and will manage the interaction between `pallet-session` on the relay chain and `pallet-staking` on AssetHub.
Both pallets are experimental and not intended for production use.

#### [#6908]: [pallet-revive] implement the ref_time_left API

This PR implements the ref_time_left API method. Solidity knows only a single "gas" dimension; Solidity contracts will use this to query the gas left.

#### [#7463]: [pallet-revive] tx fee fixes

Apply some fixes to properly estimate ethereum tx fees:

- Set the `extension_weight` on the dispatch_info to properly calculate the fee with pallet_transaction_payment
- Expose the gas_price through Runtime API, just in case we decide to tweak the value in future updates, it should be read from the chain rather than be a shared constant exposed by the crate
- add a `evm_gas_to_fee` utility function to properly convert gas to substrate fee
- Fix some minor gas encoding for edge cases

#### [#7986]: Assume elastic scaling MVP feature is always enabled in the runtime

Remove the relay chain runtime logic that handled the potential of the ElasticScalingMVP node feature being disabled.
All networks have enabled it and removing this code simplifies the codebase.

#### [#7379]: Add support for feature pallet_balances/insecure_zero_ed in benchmarks and testing

Currently benchmarks and tests on pallet_balances would fail when the feature insecure_zero_ed is enabled. This PR allows to run such benchmark and tests keeping into account the fact that accounts would not be deleted when their balance goes below a threshold.

#### [#6844]: pallet-revive: disable host functions unused in solidity PolkaVM compiler

Disables host functions in contracts that are not enabled in solidity PolkaVM compiler to reduce surface of possible attack vectors.

#### [#7200]: XCM: Deny barrier checks for nested XCMs with specific instructions to be executed on the local chain

This PR improves the validation of nested XCM instructions by introducing a new barrier, `DenyRecursively`, which provides more precise control over instruction denial. Previously, `DenyThenTry<Deny, Allow>` was used, which primarily applied denial rules at the top level. This has now been replaced with `DenyThenTry<DenyRecursively<Deny>, Allow>`, ensuring that both top-level and nested local instructions are properly checked. This change enhances the security and predictability of XCM execution by enforcing consistent denial policies across all levels of message execution. If you need to deny instructions recursively make sure to change your barrier in the XCM configuration.

#### [#7124]: Remove pallet::getter from pallet-nft-fractionalization

This PR removes all pallet::getter occurrences from pallet-nft-fractionalization and replaces them with explicit implementations.

#### [#6728]: [pallet-revive] eth-rpc add missing tests

Add tests for #6608

fix <https://github.com/paritytech/contract-issues/issues/12>

#### [#7655]: derive `DecodeWithMemTracking` for `Block`

This PR adds `DecodeWithMemTracking` as a trait bound for `Header`, `Block` and `TransactionExtension` and derives it for all the types that implement these traits in `polkadot-sdk`.
All the external types that implement these traits will need to implement `DecodeWithMemTracking` as well.

#### [#7170]: Fix reversed error message in DispatchInfo

Fix error message in `DispatchInfo` where post-dispatch and pre-dispatch weight was reversed.

#### [#7043]: Remove usage of `sp-std` from Substrate

# Description

This PR removes usage of deprecated `sp-std` from Substrate. (following PR of #5010)

## Integration

This PR doesn't remove re-exported `sp_std` from any crates yet, so downstream projects using re-exported `sp_std` will not be affected.

## Review Notes

The existing code using `sp-std` is refactored to use `alloc` and `core` directly. The key-value maps are instantiated from an array of tuples directly instead of using `sp_std::map!` macro.

This PR replaces `sp_std::Writer`, a helper type for using `Vec<u8>` with `core::fmt::Write` trait, with `alloc::string::String`.

#### [#7230]: revive: Include immutable storage deposit into the contracts `storage_base_deposit`

This PR is centered around a main fix regarding the base deposit and a bunch of drive by or related fixtures that make sense to resolve in one go. It could be broken down more but I am constantly rebasing this PR and would appreciate getting those fixes in as-one.

## Record the deposit for immutable data into the `storage_base_deposit`

The `storage_base_deposit` are all the deposit a contract has to pay for existing. It included the deposit for its own metadata and a deposit proportional (< 1.0x) to the size of its code. However, the immutable code size was not recorded there. This would lead to the situation where on terminate this portion wouldn't be refunded staying locked into the contract. It would also make the calculation of the deposit changes on `set_code_hash` more complicated when it updates the immutable data (to be done in #6985). Reason is because it didn't know how much was payed before since the storage prices could have changed in the mean time.

In order for this solution to work I needed to delay the deposit calculation for a new contract for after the contract is done executing  is constructor as only then we know the immutable data size. Before, we just charged this eagerly in `charge_instantiate` before we execute the constructor. Now, we merely send the ED as free balance before the constructor in order to create the account. After the constructor is done we calculate the contract base deposit and charge it. This will make `set_code_hash` much easier to implement.

As a side effect it is now legal to call `set_immutable_data` multiple times per constructor (even though I see no reason to do so). It simply overrides the immutable data with the new value. The deposit accounting will be done after the constructor returns (as mentioned above) instead of when setting the immutable data.

## Don't pre-charge for reading immutable data

I noticed that we were pre-charging weight for the max allowable immutable data when reading those values and then refunding after read. This is not necessary as we know its length without reading the storage as we store it out of band in contract metadata. This makes reading it free. Less pre-charging less problems.

## Remove delegate locking

Fixes #7092

This is also in the spirit of making #6985 easier to implement. The locking complicates `set_code_hash` as we might need to block settings the code hash when locks exist. Check #7092 for further rationale.

## Enforce "no terminate in constructor" eagerly

We used to enforce this rule after the contract execution returned. Now we error out early in the host call. This makes it easier to be sure to argue that a contract info still exists (wasn't terminated) when a constructor successfully returns. All around this his just much simpler than dealing this check.

## Moved refcount functions to `CodeInfo`

They never really made sense to exist on `Stack`. But now with the locking gone this makes even less sense. The refcount is stored inside `CodeInfo` to lets just move them there.

## Set `CodeHashLockupDepositPercent` for test runtime

The test runtime was setting `CodeHashLockupDepositPercent` to zero. This was trivializing many code paths and excluded them from testing. I set it to `30%` which is our default value and fixed up all the tests that broke. This should give us confidence that the lockup doeposit collections properly works.

## Reworked the `MockExecutable` to have both a `deploy` and a `call` entry point

This type used for testing could only have either entry points but not both. In order to fix the `immutable_data_set_overrides` I needed to a new function `add_both` to `MockExecutable` that allows to have both entry points. Make sure to make use of it in the future :)

#### [#6338]: Update Referenda to Support Block Number Provider

This PR makes the referenda pallet uses the relay chain as a block provider for a parachain on a regular schedule.
To migrate existing referenda implementations, simply add `type BlockNumberProvider = System` to have the same behavior as before.

#### [#7637]: Expose extension weights from frame-system

This PR exposes the Extension weights from the `frame-system`

#### [#6964]: [pallet-revive] implement the base fee API

This PR implements the base fee syscall API method. Currently this is implemented as a compile time constant in the revive compiler, returning 0. However, since this is an opocde, if we ever need to implement it for compatibility reasons with [EIP-1559](https://github.com/ethereum/EIPs/blob/master/EIPS/eip-1559.md), it would break already deployed contracts. Thus we provide a syscall method instead.

#### [#6522]: Removes constraint in BlockNumberProvider from treasury
<https://github.com/paritytech/polkadot-sdk/pull/3970> updated the treasury pallet to support
relay chain block number provider. However, it added a constraint to the `BlockNumberProvider`
trait to have the same block number type as `frame_system`:

```rust
type BlockNumberProvider: BlockNumberProvider<BlockNumber = BlockNumberFor<Self>>;
```

This PR removes that constraint and allows the treasury pallet to use any block number type.

### Changelog for `Node Operator`

**ℹ️ These changes are relevant to:**  Those who don't write any code and only run code.

#### [#6923]: omni-node: Tolerate failing metadata check

# 6450 introduced metadata checks. Supported are metadata v14 and higher.

However, of course old chain-specs have a genesis code blob that might be on older version. This needs to be tolerated. We should just skip the checks in that case.

Fixes #6921

#### [#7353]: Shorter availability data retention period for testnets

Allows specifying a shorter availability data retention period for testnets.

#### [#6605]: Notify telemetry only every second about the tx pool status

Before this was done for every imported transaction. When a lot of transactions got imported, the import notification channel was filled. The underlying problem was that the `status` call is read locking the `validated_pool` which will be write locked by the internal submitting logic. Thus, the submitting and status reading was interferring which each other.

#### [#7781]: Punish libp2p notification protocol misbehavior on outbound substreams

This PR punishes behaviors that deviate from the notification spec.
When a peer misbehaves by writing data on an unidirectional read stream, the peer is banned and disconnected immediately.

#### [#7479]: omni-node: add offchain worker

Added support for offchain worker to omni-node-lib for both aura and manual seal nodes.

#### [#7724]: Terminate libp2p the outbound notification substream on io errors

This PR handles a case where we called the poll_next on an outbound substream notification to check if the stream is closed.
It is entirely possible that the poll_next would return an io::error, for example end of file.
This PR ensures that we make the distinction between unexpected incoming data, and error originated from poll_next.
While at it, the bulk of the PR change propagates the PeerID from the network behavior, through the notification handler, to the notification outbound stream for logging purposes.

#### [#7885]: Rename archive call method result to value

Previously, the method result was encoded to a json containing a "result" field. However,
the spec specifies a "value" field. This aims to rectify that.

#### [#7585]: Add export PoV on slot base collator

Add functionality to export the Proof of Validity (PoV) when the slot-based collator is used.

#### [#5724]: Validator Re-Enabling (master PR)

Implementation of the Stage 3 for the New Disabling Strategy: <https://github.com/paritytech/polkadot-sdk/issues/4359>

This PR changes when an active validator node gets disabled within parachain consensus (reduced responsibilities and reduced rewards) for comitting offences. This should not affect active validators on a day-to-day basis and will only be relevant when the network is under attack or there is a wide spread malfunction causing slashes. In that case lowest offenders might get eventually re-enabled (back to normal responsibilities and normal rewards).

#### [#7994]: Expose rpc_rate_limit* cli options to parachains

Expose rpc_rate_limit* cli options to parachains

#### [#6248]: Upgrade libp2p to 0.54.1

Upgrade libp2p from 0.52.4 to 0.54.1

#### [#7569]: slot-based-collator: Allow multiple blocks per slot

Adds multiple blocks per slot support to the slot-based collator. This PR deprecates the `--experimental-use-slot-based` flag in favor of `--authoring slot-based`. The deprecated flag will be removed in the next release.
Parachain runtimes using the `FixedVelocityConsensusHook` now no longer support building blocks with slots shorter than 6 seconds. We advise elastic-scaling chains to use the mechanisms introduced in this PR and produce multiple blocks in a single slot.

#### [#7020]: Remove warning log from frame-omni-bencher CLI

# Description

This PR removes the outdated warning message from the `frame-omni-bencher` CLI that states the tool is "not yet battle tested". Fixes #7019

## Integration

No integration steps are required.

## Review Notes

The functionality of the tool remains unchanged. Removes the warning message from the CLI output.

#### [#6546]: Increase default trie cache size to 1GiB

The default trie cache size before was set to `64MiB`, which is quite low to achieve real speed ups. `1GiB` should be a reasonable number as the requirements for validators/collators/full nodes are much higher when it comes to minimum memory requirements. Also the cache will not use `1GiB` from the start and fills over time. The setting can be changed by setting `--trie-cache-size BYTE_SIZE`.The CLI option `--state-cache-size` is also removed, which was not having any effect anymore.

#### [#7494]: Enhance libp2p logging targets for granular control

This PR modifies the libp2p networking-specific log targets for granular control (e.g., just enabling trace for req-resp).

Previously, all logs were outputted to `sub-libp2p` target, flooding the log messages on busy validators.

- Discovery: `sub-libp2p::discovery`;
- Notification/behaviour: `sub-libp2p::notification::behaviour`;
- Notification/handler: `sub-libp2p::notification::handler`;
- Notification/service: `sub-libp2p::notification::service`;
- Notification/upgrade: `sub-libp2p::notification::upgrade`;
- Request response: `sub-libp2p::request-response`.

#### [#7866]: Make litep2p the default backend in Kusama

This PR makes the litep2p backend the default network backend in Kusama, but also for system chains.
We performed a gradual rollout in Kusama by asking validators to manually switch to litep2p.
The rollout went smoothly, with 250 validators running litep2p without issues. This PR represents the next step in testing the backend at scale.

#### [#7266]: Add `offchain_localStorageClear` RPC method

Adds RPC method `offchain_localStorageClear` to clear the offchain local storage.

### Changelog for `Runtime User`

**ℹ️ These changes are relevant to:**  Anyone using the runtime. This can be a token holder or a dev writing a front end for a chain.

#### [#6540]: Only allow apply slash to be executed if the slash amount is atleast ED

This change prevents `pools::apply_slash` from being executed when the pending slash amount of the member is lower than the ED. With this change, such small slashes will still be applied but only when member funds are withdrawn.

#### [#6856]: Enable report_fork_voting()

This PR enables calling `report_fork_voting`.
In order to do this we needed to also check that the ancestry proof is optimal.

#### [#2072]: Return iterator in pallet_referenda::TracksInfo::tracks

There is a change in `pallet-referenda`. Now, the tracks are retrieved as a list of `Track`s. Also, the names of the tracks might have some trailing null values (`\0`). This means display representation of the tracks' names must be sanitized.

#### [#7134]: xcm: convert properly assets in xcmpayment apis

Port #6459 changes to relays as well, which were probably forgotten in that PR.
Thanks!

#### [#6336]: pallet-xcm: add support to authorize aliases

Added new `add_authorized_alias()` and `remove_authorized_alias()` calls to `pallet-xcm`.
These can be used by a "caller" to explicitly authorize another location to alias into the "caller" origin.
Usually useful to allow one's local account to be aliased into from a remote location also under one's control (one's account on another chain).
WARNING: make sure that you as the caller `origin` trust the `aliaser` location to act in your name on this chain. Once authorized using this call, the `aliaser` can freely impersonate `origin` in XCM programs executed on the local chain.

#### [#7838]: `CheckOnlySudoAccount`: Provide some tags

Let `CheckOnlySudoAccount` provide some tags to make the tx pool happy.

#### [#7030]: [core-fellowship] Add permissionless import_member

Changes:

- Add call `import_member` to the core-fellowship pallet.
- Move common logic between `import` and `import_member` into `do_import`.

This is a minor change as to not impact UI and downstream integration.

## `import_member`

Can be used to induct an arbitrary collective member and is callable by any signed origin. Pays no fees upon success.
This is useful in the case that members did not induct themselves and are idling on their rank.

#### [#7169]: xcm: fix DenyThenTry when work with multiple Deny tuples

This PR changes the behavior of DenyThenTry to fix #7148
If any of the tuple elements returns `Err(())`, the execution stops.
Else, `Ok(_)` is returned if all elements accept the message.

#### [#7080]: [pallet-broker] add extrinsic to remove an assignment

A new `remove_assignment` extrinsic is introduced to the broker pallet to allow an assignment to be removed by the root origin.

#### [#7026]: [pallet-broker] add extrinsic to remove a lease

A new `remove_lease` extrinsic is introduced to the broker pallet to allow a lease to be removed by the root origin.

#### [#5990]: On-demand credits

The PR implements functionality on the relay chain for purchasing on-demand Coretime using credits. This means on-demand Coretime should no longer be purchased with the relay chain balance but rather with credits acquired on the Coretime chain. The extrinsic to use for purchasing Coretime is `place_order_with_credits`. It is worth noting that the PR also introduces a minimum credit purchase requirement to prevent potential attacks.

#### [#7812]: `apply_authorized_upgrade`: Remote authorization if the version check fails

This pr ensures that we remove the authorization for a runtime upgrade if the version check failed.
If that check is failing, it means that the runtime upgrade is invalid and the check will never succeed.

Besides that the pr is doing some clean ups.

#### [#7377]: Add missing events to nomination pool extrinsics

Introduces events to extrinsics from `pallet_nomination_pools` that previously had none:

- `set_metadata`
- `nominate`
- `chill`
- `set_configs`
- `set_claim_permission`

#### [#4273]: [pallet-broker] add extrinsic to reserve a system core without having to wait two sale boundaries

When calling the reserve extrinsic after sales have started, the assignment will be reserved, but two sale period boundaries must pass before the core is actually assigned. A new
`force_reserve` extrinsic is introduced to allow a core to be immediately assigned.

#### [#6506]: Zero refund check for FungibleAdapter

`FungibleAdapter` will now check if the _refund amount_ is zero before calling deposit & emitting an event.

Fixes <https://github.com/paritytech/polkadot-sdk/issues/6469>.

#### [#7320]: Add view functions to Proxy pallet for runtime-specific type configuration

Adds two view functions to `pallet-proxy`:
`check_permissions`: Checks if a given RuntimeCall is allowed for a specific ProxyType using the InstanceFilter trait.
`is_superset`: Checks if one ProxyType is a superset of another ProxyType by comparing them using the PartialOrd trait.

#### [#6989]: paras-registrar: Improve error reporting

This pr improves the error reporting by paras registrar when an owner wants to access a locked parachain.

Closes: <https://github.com/paritytech/polkadot-sdk/issues/6745>

#### [#6995]: added new proxy ParaRegistration to Westend

This adds a new Proxy type to Westend Runtime called ParaRegistration. This is related to: <https://github.com/polkadot-fellows/runtimes/pull/520>.

This new proxy allows:

1. Reserve paraID
2. Register Parachain
3. Leverage Utilites pallet
4. Remove proxy.

#### [#7846]: Incrementable: return None instead of saturating

Fix implementation to follow the trait specification more closely. Closes #7845

#### [#4722]: Implement pallet view functions

Querying the runtime state is now easier with the introduction of pallet view functions. Clients can call commonly defined view functions rather than accessing the storage directly. These are similar to the Runtime APIs, but are defined within the runtime itself.

#### [#5501]: Currency to Fungible migration for pallet-staking

Lazy migration of staking balance from `Currency::locks` to `Fungible::holds`. New extrinsic `staking::migrate_currency` removes the old lock along with other housekeeping. Additionally, any ledger mutation creates hold if it does not exist.

The pallet-staking configuration item `Currency` is updated to use `fungible::hold::Mutate` type while still requiring `LockableCurrency` type to be passed as `OldCurrency` for migration purposes.

#### [#6503]: xcm: minor fix for compatibility with V4

Following the removal of `Rococo`, `Westend` and `Wococo` from `NetworkId`, fixed `xcm::v5::NetworkId` encoding/decoding to be compatible with `xcm::v4::NetworkId`

#### [#7983]: [XCM] allow signed account to be aliased between system chains

Aliasing configuration change for system chains:

- Asset Hub: does not allow same account aliasing: there is no real world demand for it, the direction is usually reversed, users already have accounts on AH and want to use them cross-chain on other chains. Without real world demand, it's better to keep AH permissions as tight as possible.
- Bridge Hub: does not allow same account aliasing: there is no real world demand for it, only low-level power users (like relayers) directly interact with Bridge Hub. They don't need aliasing to operate cross-chain they can operate locally.
- Collectives: allows account A on a sibling system chain to alias into the local account A.
- Coretime: allows account A on a sibling system chain to alias into the local account A.
- People: allows account A on a sibling system chain to alias into the local account A.
Practical example showcased with new configuration:
`Alice` on AssetHub can set identity for `Alice` on People over XCM.

#### [#6486]: sp-trie: minor fix to avoid panic on badly-constructed proof

"Added a check when decoding encoded proof nodes in `sp-trie` to avoid panicking when receiving a badly constructed proof, instead erroring out."

#### [#6890]: Alter semantic meaning of 0 in metering limits of EVM contract calls

A limit of 0, for gas meters and storage meters, no longer has the meaning of unlimited metering.
