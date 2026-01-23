# Tasks: Add Pub-Sub System

## 1. Broadcaster Pallet (Relay Chain)

- [ ] 1.1 Create `polkadot/runtime/parachains/src/broadcaster/mod.rs` with pallet boilerplate
- [ ] 1.2 Implement `Config` trait with storage limits and deposit configuration
- [ ] 1.3 Add `Publishers` storage map for registration tracking
- [ ] 1.4 Implement `register_publisher` extrinsic with deposit handling for public parachains
- [ ] 1.5 Allow system parachains (ID < 2000) to publish without registration
- [ ] 1.6 Implement `deregister_publisher` extrinsic with deposit release
- [ ] 1.7 Implement `force_deregister_publisher` extrinsic for governance
- [ ] 1.8 Add child trie storage using `derive_child_info(para_id)`
- [ ] 1.9 Implement `handle_publish` for XCM executor integration
- [ ] 1.10 Add storage limit enforcement (keys, value size, total size)
- [ ] 1.11 Add `TtlData` storage for expiration tracking
- [ ] 1.12 Add `TtlScanCursor` storage for incremental cleanup
- [ ] 1.13 Implement `on_idle` TTL cleanup with cursor-based resumption
- [ ] 1.14 Implement `delete_keys` extrinsic for parachain self-deletion
- [ ] 1.15 Implement `force_delete_keys` extrinsic for governance
- [ ] 1.16 Add events: `PublisherRegistered`, `PublisherDeregistered`, `DataPublished`, `KeyExpired`, `KeysDeleted`, `KeysForcedDeleted`
- [ ] 1.17 Implement cleanup on parachain offboarding hook
- [ ] 1.18 Write unit tests for registration/deregistration and system parachain bypass
- [ ] 1.19 Write unit tests for publish operations and limits
- [ ] 1.20 Write unit tests for TTL logic (infinite, finite, reset)
- [ ] 1.21 Write unit tests for manual deletion
- [ ] 1.22 Add benchmarks for all extrinsics

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
- [ ] 3.2 Define `SubscribedKey` type with H256 wrapper
- [ ] 3.3 Implement `subscribed_key!` macro for compile-time hashing
- [ ] 3.4 Implement `SubscribedKey::from_raw_key()` for runtime hashing
- [ ] 3.5 Define `TtlState` enum (Infinite, ValidFor, TimedOut)
- [ ] 3.6 Define `SubscriptionHandler` trait with `subscriptions()` and `on_data_updated()`
- [ ] 3.7 Add `PreviousPublishedDataRoots` storage for change detection
- [ ] 3.8 Implement change detection (root comparison to skip unchanged publishers)
- [ ] 3.9 Add `CachedTrieNodes` storage for trie node cache (all nodes for subscribed keys)
- [ ] 3.10 Implement cache update logic during proof processing
- [ ] 3.11 Implement `clear_cache_for_publisher()` on subscription change
- [ ] 3.12 Add `PubSubProcessingCursor` storage for budget-constrained resumption
- [ ] 3.13 Add `LastProcessedRoot` storage per publisher
- [ ] 3.14 Implement budget-constrained processing with cursor
- [ ] 3.15 Implement publisher prioritization (system parachains first)
- [ ] 3.16 Implement malicious collator detection (panic if missing node but budget not exhausted)
- [ ] 3.17 Write unit tests for subscription handling
- [ ] 3.18 Write unit tests for change detection
- [ ] 3.19 Write unit tests for caching and subscription change clearing
- [ ] 3.20 Write unit tests for budget constraints, cursor resumption, and malicious collator detection
- [ ] 3.21 Add benchmarks for proof processing

## 4. Proof Collection (Collator)

- [ ] 4.1 Add `RelayStorageKey` enum to `cumulus/primitives/core/src/lib.rs`
- [ ] 4.2 Add `RelayProofRequest` type for proof request specification
- [ ] 4.3 Implement `KeyToIncludeInRelayProofApi` runtime API
- [ ] 4.4 Update `collect_relay_storage_proof` in `cumulus/client/parachain-inherent/src/lib.rs`
- [ ] 4.5 Add child trie key collection for subscriptions
- [ ] 4.6 Write unit tests for proof collection

## 5. Proof Pruning (Parachain System)

- [ ] 5.1 Define `PubSubProofPruner` trait in subscriber pallet
- [ ] 5.2 Implement pruning: remove unchanged child tries entirely
- [ ] 5.3 Implement pruning: remove cached nodes from changed child tries
- [ ] 5.4 Implement pruning: remove nodes leading to unsubscribed keys
- [ ] 5.5 Implement minimum 1 MiB budget guarantee for pub-sub
- [ ] 5.6 Integrate pruner in `do_create_inherent` after message filtering
- [ ] 5.7 Resume pruning from cursor if set from previous block
- [ ] 5.8 Update `set_validation_data` to use pruned relay state
- [ ] 5.9 Write unit tests for proof pruning (unchanged tries, cached nodes, unsubscribed keys)
- [ ] 5.10 Write unit tests for budget enforcement and cursor handling

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
