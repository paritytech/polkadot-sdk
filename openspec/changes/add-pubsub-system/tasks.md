# Tasks: Add Pub-Sub System

## 1. Broadcaster Pallet (Relay Chain)

- [ ] 1.1 Create `polkadot/runtime/parachains/src/broadcaster/mod.rs` with pallet boilerplate
- [ ] 1.2 Implement `Config` trait with storage limits and deposit configuration:
  - `MaxValueLength`, `MaxStoredKeys`, `MaxTotalStorageSize`, `MaxPublishers`, `PublisherDeposit`
- [ ] 1.3 Add `Publishers` storage map for registration tracking
- [ ] 1.4 Implement `register_publisher` extrinsic with deposit handling for public parachains
- [ ] 1.5 Implement `force_register_publisher` for system parachains (Root only, zero deposit)
- [ ] 1.6 Allow system parachains (ID < 2000) to publish without registration
- [ ] 1.7 Implement `deregister_publisher` extrinsic with deposit release
- [ ] 1.8 Implement `force_deregister_publisher` extrinsic for governance
- [ ] 1.9 Add child trie storage using `derive_child_info(para_id)`:
  - Key format: `(b"pubsub", para_id).encode()`
- [ ] 1.10 Define `PublishedEntry<BlockNumber>` struct with (value, ttl, when_inserted)
- [ ] 1.11 Implement `handle_publish` for XCM executor integration
- [ ] 1.12 Add storage limit enforcement (keys, value size, total size)
- [ ] 1.13 Add `TtlData` storage: `StorageDoubleMap<ParaId, [u8; 32], (u32, BlockNumber)>`
- [ ] 1.14 Add `TtlScanCursor` storage: `StorageValue<(ParaId, [u8; 32])>`
- [ ] 1.15 Implement TTL capping at `MaxTTL` (432,000 blocks)
- [ ] 1.16 Implement `on_idle` TTL cleanup:
  - Scan `TtlData` for expired keys
  - Delete up to `MaxTtlScansPerIdle` (500) keys per block
  - Use `TtlScanCursor` for resumption
- [ ] 1.17 Implement `delete_keys` extrinsic for parachain self-deletion
- [ ] 1.18 Implement `force_delete_keys` extrinsic for governance
- [ ] 1.19 Add events: `PublisherRegistered`, `PublisherDeregistered`, `DataPublished`, `KeyExpired`, `KeysDeleted`, `KeysForcedDeleted`
- [ ] 1.20 Implement cleanup on parachain offboarding hook
- [ ] 1.21 Write unit tests for registration/deregistration and system parachain bypass
- [ ] 1.22 Write unit tests for publish operations and limits
- [ ] 1.23 Write unit tests for TTL logic (infinite, finite, capped, reset, finite→infinite, infinite→finite)
- [ ] 1.24 Write unit tests for manual deletion (removes TtlData metadata)
- [ ] 1.25 Add benchmarks for all extrinsics

## 2. XCM Publish Instruction

- [ ] 2.1 Add `Publish { key, value, ttl }` to `polkadot/xcm/src/v5/instruction.rs`
- [ ] 2.2 Add `MaxPublishValueLength` parameter type
- [ ] 2.3 Implement `Publish` instruction handler in XCM executor
- [ ] 2.4 Add origin validation (must be Parachain junction)
- [ ] 2.5 Integrate with broadcaster pallet via `Config::Broadcaster` trait
- [ ] 2.6 Write unit tests for XCM instruction execution
- [ ] 2.7 Add benchmarks for `Publish` instruction weight

## 3. Subscriber Pallet (Parachains)

- [ ] 3.1 Create `cumulus/pallets/subscriber/src/lib.rs` with pallet boilerplate
- [ ] 3.2 Add `MaxTrieDepth` config parameter to limit trie traversal depth
- [ ] 3.3 Add `DisabledPublishers` storage: `StorageMap<ParaId, DisableReason>`
- [ ] 3.4 Implement `enable_publisher(ParaId)` extrinsic for re-enabling disabled publishers
- [ ] 3.5 Add events: `PublisherDisabled`, `PublisherEnabled`
- [ ] 3.6 Define `SubscribedKey` type with H256 wrapper and methods:
  - `from_hash([u8; 32])` for pre-computed hashes
  - `from_data(&[u8])` for runtime hashing
- [ ] 3.7 Implement `subscribed_key!` macro for compile-time hashing via `sp_crypto_hashing::blake2_256`
- [ ] 3.8 Define `TtlState` enum (Infinite, ValidFor(u32), TimedOut)
- [ ] 3.9 Define `PublishedEntry<BlockNumber>` struct with (value, ttl, when_inserted)
- [ ] 3.10 Define `SubscriptionHandler` trait:
  - `subscriptions() -> (Vec<(ParaId, Vec<SubscribedKey>)>, Weight)`
  - `on_data_updated(ParaId, SubscribedKey, &[u8], TtlState) -> Weight`
- [ ] 3.11 Add `PreviousPublishedDataRoots` storage for change detection
- [ ] 3.12 Add `CachedTrieNodes` storage: `StorageDoubleMap<ParaId, H256, BoundedVec<u8, 512>>` (trie nodes only, no external data)
- [ ] 3.13 Add `RelayProofProcessingCursor` storage: `StorageValue<(ParaId, SubscribedKey)>`
- [ ] 3.14 Implement `compute_ttl_state()` to convert PublishedEntry to TtlState
- [ ] 3.15 Implement publisher prioritization (system parachains first)
- [ ] 3.16 Write unit tests for subscription handling
- [ ] 3.17 Write unit tests for trie depth limit enforcement and publisher disabling
- [ ] 3.18 Write unit tests for enable_publisher re-enabling
- [ ] 3.19 Add benchmarks for proof processing

## 4. Proof Collection (Collator)

- [ ] 4.1 Add `RelayStorageKey` enum to `cumulus/primitives/core/src/lib.rs`
- [ ] 4.2 Add `RelayProofRequest` type for proof request specification
- [ ] 4.3 Implement `KeyToIncludeInRelayProofApi` runtime API
- [ ] 4.4 Update `collect_relay_storage_proof` in `cumulus/client/parachain-inherent/src/lib.rs`
- [ ] 4.5 Add child trie key collection for subscriptions
- [ ] 4.6 Write unit tests for proof collection

## 5. Proof Pruning (RelayProofPruner)

- [ ] 5.1 Define `RelayProofPruner` trait in subscriber pallet:
  - `prune_relay_proofs(StorageProof, H256, &mut usize) -> StorageProof`
- [ ] 5.2 Implement `RelayProofPruner` with all pruning logic:
  - Detect unchanged child tries via `PreviousPublishedDataRoots` comparison
  - Remove unchanged child tries entirely from proof
  - Implement `CachedHashDB` custom HashDB that checks cache before including nodes
  - Remove cached nodes from changed child tries
  - Remove nodes leading to unsubscribed keys
  - Enforce `MaxTrieDepth` limit during traversal
  - Enforce `size_limit` budget constraint
  - Set `RelayProofProcessingCursor` when budget exhausted
  - Resume from `RelayProofProcessingCursor` if set from previous block
- [ ] 5.3 Implement dual-trie traversal for cache synchronization:
  - Add new nodes to `CachedTrieNodes`
  - Remove outdated nodes from cache
  - Stop at unchanged nodes (subtree same)
  - Abort if depth exceeds `MaxTrieDepth`
- [ ] 5.4 Implement `clear_cache_for_publisher()` on subscription change
- [ ] 5.5 Integrate pruner in `do_create_inherent` after message filtering:
  - Use remaining `size_limit` after `into_abridged()` calls
  - No minimum budget guarantee (pub-sub gets what's left)
- [ ] 5.6 Implement malicious collator detection in `set_validation_data`:
  - Panic if missing node but budget not exhausted
- [ ] 5.7 Write unit tests for proof pruning (unchanged tries, cached nodes, unsubscribed keys)
- [ ] 5.8 Write unit tests for budget enforcement and cursor handling
- [ ] 5.9 Write unit tests for trie depth limit enforcement
- [ ] 5.10 Write unit tests for light block (large pub-sub budget)
- [ ] 5.11 Write unit tests for heavy block (small pub-sub budget)
- [ ] 5.12 Write unit tests for full block (no pub-sub budget, graceful skip)
- [ ] 5.13 Write unit tests for cache synchronization and subscription change clearing

## 6. Integration and Testing

- [ ] 6.1 Add broadcaster pallet to relay chain runtime
- [ ] 6.2 Add subscriber pallet to test parachain runtime (Penpal)
- [ ] 6.3 Implement example `SubscriptionHandler` in test parachain
- [ ] 6.4 Write integration test: basic publish-subscribe flow
- [ ] 6.5 Write integration test: multiple publishers and subscribers
- [ ] 6.6 Write integration test: PoV limit enforcement and cursor resumption
- [ ] 6.7 Write integration test: TTL expiration and cleanup
- [ ] 6.8 Write integration test: subscription changes and cache clearing
- [ ] 6.9 Write integration test: malicious collator detection
- [ ] 6.10 Create zombienet test: two relay nodes + publisher + subscriber
- [ ] 6.11 Create zombienet test: PoV budget under HRMP load

## 7. Documentation

- [ ] 7.1 Write user guide for publishers (key hashing, XCM usage, TTL examples)
- [ ] 7.2 Write user guide for subscribers (trait implementation, caching, TTL handling)
- [ ] 7.3 Add inline rustdoc for all public types and traits
- [ ] 7.4 Update CHANGELOG with new feature entry
