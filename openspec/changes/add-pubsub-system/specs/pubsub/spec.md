# Pub-Sub System Specification

Cross-chain publish-subscribe mechanism for data sharing via the relay chain.

## ADDED Requirements

### Requirement: Publisher Registration

The broadcaster pallet SHALL allow parachains to register as publishers with a deposit.

#### Scenario: Public parachain registration

- **GIVEN** a parachain with ID 2000 that is not registered
- **WHEN** an account calls `register_publisher(para_id: 2000)` with sufficient balance
- **THEN** the deposit is held from the caller's account
- **AND** the parachain is registered as a publisher
- **AND** a `PublisherRegistered` event is emitted

#### Scenario: System parachain publishes without registration

- **GIVEN** a system parachain with ID 1000 (< 2000)
- **WHEN** the parachain sends a `Publish` instruction
- **THEN** the publish operation succeeds without requiring registration
- **AND** data is stored in the parachain's child trie

#### Scenario: Registration fails when already registered

- **GIVEN** parachain 2000 is already registered
- **WHEN** `register_publisher(para_id: 2000)` is called
- **THEN** the call fails with `AlreadyRegistered` error

#### Scenario: Registration fails when max publishers reached

- **GIVEN** the number of registered publishers equals `MaxPublishers`
- **WHEN** a new registration is attempted
- **THEN** the call fails with `TooManyPublishers` error

### Requirement: Publisher Deregistration

The broadcaster pallet SHALL allow publishers to deregister and reclaim deposits.

#### Scenario: Voluntary deregistration

- **GIVEN** parachain 2000 is registered with a deposit
- **WHEN** the manager calls `deregister_publisher(para_id: 2000)`
- **THEN** all published data is deleted from the child trie
- **AND** the deposit is released to the manager
- **AND** a `PublisherDeregistered` event is emitted

#### Scenario: Force deregistration by governance

- **GIVEN** parachain 2000 is registered
- **WHEN** root calls `force_deregister_publisher(para_id: 2000)`
- **THEN** all published data is deleted
- **AND** the deposit is slashed or returned based on configuration
- **AND** a `PublisherDeregistered` event is emitted

### Requirement: XCM Publish Instruction

The XCM executor SHALL support a `Publish` instruction for storing key-value data on the relay chain. Keys MUST be 32-byte values (publishers hash their keys before publishing).

#### Scenario: Successful publish

- **GIVEN** parachain 2000 is registered as a publisher
- **WHEN** the parachain sends XCM containing `Publish { key: [0u8; 32], value: vec![1, 2, 3], ttl: 0 }`
- **THEN** the key is stored directly as the 32-byte value (no additional hashing)
- **AND** the value is stored in the parachain's child trie
- **AND** a `DataPublished` event is emitted

#### Scenario: Publish with finite TTL

- **GIVEN** parachain 2000 is registered
- **WHEN** the parachain sends `Publish { key, value, ttl: 1000 }`
- **THEN** the entry is stored with expiration at `current_block + 1000`
- **AND** TTL metadata is recorded for cleanup scheduling

#### Scenario: Publish fails for unregistered public parachain

- **GIVEN** public parachain 3000 (>= 2000) is not registered
- **WHEN** the parachain sends a `Publish` instruction
- **THEN** the instruction fails with `NotRegistered` error

#### Scenario: Publish fails when storage limit exceeded

- **GIVEN** parachain 2000 has used its `MaxTotalStorageSize` quota
- **WHEN** the parachain sends a `Publish` instruction
- **THEN** the instruction fails with `TotalStorageSizeExceeded`

### Requirement: Data Storage Limits

The broadcaster pallet SHALL enforce per-publisher storage limits.

#### Scenario: Key count limit

- **GIVEN** a publisher has `MaxStoredKeys` keys stored
- **WHEN** the publisher attempts to store a new key
- **THEN** the operation fails with `TooManyKeys` error

#### Scenario: Value size limit

- **GIVEN** `MaxValueLength` is 2048 bytes
- **WHEN** a publisher attempts to store a value larger than 2048 bytes
- **THEN** the operation fails with `ValueTooLarge` error

#### Scenario: Total storage size limit

- **GIVEN** a publisher's total storage usage equals `MaxTotalStorageSize`
- **WHEN** the publisher attempts to store additional data
- **THEN** the operation fails with `TotalStorageSizeExceeded` error

### Requirement: Subscription Handler

The subscriber pallet SHALL provide a trait for declaring subscriptions and receiving updates.

#### Scenario: Subscription declaration

- **GIVEN** a parachain implements `SubscriptionHandler`
- **WHEN** `subscriptions()` is called
- **THEN** it returns a list of (ParaId, Vec<SubscribedKey>) pairs
- **AND** each `SubscribedKey` is a 32-byte value

#### Scenario: Data update callback

- **GIVEN** a subscription exists for publisher 1000, key K
- **WHEN** publisher 1000 updates key K and a new block is produced
- **THEN** `on_data_updated(1000, K, value, ttl_state)` is called on the subscriber
- **AND** `ttl_state` indicates the TTL status (Infinite, ValidFor(N), or TimedOut)

#### Scenario: Compile-time key hashing

- **GIVEN** a developer uses `subscribed_key!("my_key")`
- **THEN** the Blake2-256 hash is computed at compile time
- **AND** runtime hashing overhead is zero

#### Scenario: Runtime key hashing

- **GIVEN** a developer uses `SubscribedKey::from_raw_key(dynamic_bytes)`
- **THEN** the Blake2-256 hash is computed at runtime
- **AND** the resulting key can be used in subscriptions

### Requirement: Change Detection

The subscriber pallet SHALL skip processing unchanged publishers using root comparison.

#### Scenario: Root unchanged

- **GIVEN** publisher 1000's child trie root is cached as R
- **WHEN** a new block has the same root R for publisher 1000
- **THEN** the child trie is removed entirely from the relay chain proof
- **AND** `on_data_updated` is NOT called for any keys from publisher 1000

#### Scenario: Root changed

- **GIVEN** publisher 1000's cached root is R1
- **WHEN** a new block has root R2 (different from R1) for publisher 1000
- **THEN** subscribed keys are extracted and processed
- **AND** `on_data_updated` is called for each subscribed key with data

### Requirement: Trie Node Caching

The subscriber pallet SHALL cache all trie nodes needed to access subscribed keys.

#### Scenario: Initial proof processing

- **GIVEN** a new subscription to publisher 1000 with 100 keys
- **WHEN** the first proof is processed
- **THEN** all trie nodes in the proof paths to subscribed keys are cached on-chain

#### Scenario: Subsequent proof with cached nodes

- **GIVEN** nodes for publisher 1000 are cached
- **WHEN** a new block's proof is built
- **THEN** cached nodes are pruned from the relay chain proof
- **AND** nodes leading to unsubscribed keys are pruned from the proof
- **AND** only new/changed nodes for subscribed keys are included

#### Scenario: Subscription change clears cache

- **GIVEN** subscription to publisher 1000 changes from keys [A, B] to [C, D]
- **WHEN** subscription change is detected
- **THEN** cached nodes for publisher 1000 are cleared
- **AND** next block includes full proof for new subscribed keys

### Requirement: PoV Budget Constraint

The subscriber pallet SHALL respect PoV budget limits during proof pruning.

#### Scenario: Minimum budget guarantee

- **GIVEN** messages consume most of the PoV budget
- **WHEN** remaining space is less than 1 MiB
- **THEN** pub-sub still receives at least 1 MiB budget

#### Scenario: Budget exhausted mid-processing

- **GIVEN** 1 MiB budget and more nodes than fit
- **WHEN** pruning exhausts the budget
- **THEN** remaining nodes are removed from the proof
- **AND** a cursor is set on-chain indicating incomplete processing

#### Scenario: Cursor resumption

- **GIVEN** cursor was set in the previous block
- **WHEN** the next block begins pruning
- **THEN** pruning resumes from the cursor position

#### Scenario: Malicious collator detection

- **GIVEN** a trie node is missing from the proof
- **WHEN** the proof size is below the budget limit
- **THEN** the block panics (collator is cheating by omitting data)

#### Scenario: Missing node at budget limit

- **GIVEN** a trie node is missing from the proof
- **WHEN** the proof size equals the budget limit
- **THEN** this is valid (budget was exhausted, cursor should be set)

### Requirement: Publisher Prioritization

The subscriber pallet SHALL prioritize system parachains in proof processing.

#### Scenario: System parachain first

- **GIVEN** subscriptions to parachains 1000 (system) and 3000 (public)
- **WHEN** subscriptions are ordered for processing
- **THEN** parachain 1000 is processed before parachain 3000

### Requirement: TTL Expiration

The broadcaster pallet SHALL automatically delete expired keys via `on_idle`.

#### Scenario: Key expires

- **GIVEN** key K was published at block 1000 with TTL 100
- **WHEN** block 1100 is reached and `on_idle` runs
- **THEN** key K is deleted from the child trie
- **AND** `KeyExpired` event is emitted

#### Scenario: Infinite TTL

- **GIVEN** key K was published with TTL 0
- **WHEN** any number of blocks pass
- **THEN** key K is NOT auto-deleted

#### Scenario: Partial cleanup under weight limit

- **GIVEN** 1000 keys are expired
- **WHEN** `on_idle` has weight for only 500 deletions
- **THEN** 500 keys are deleted
- **AND** cursor is set for remaining keys
- **AND** next `on_idle` resumes from cursor

### Requirement: Manual Key Deletion

The broadcaster pallet SHALL allow manual deletion of published keys.

#### Scenario: Parachain self-deletion

- **GIVEN** parachain 2000 has published keys [A, B, C]
- **WHEN** the parachain calls `delete_keys([A, B])`
- **THEN** keys A and B are deleted immediately
- **AND** `KeysDeleted` event is emitted with count 2

#### Scenario: Governance force-deletion

- **GIVEN** parachain 2000 has published keys
- **WHEN** root calls `force_delete_keys(2000, [A, B, C])`
- **THEN** the specified keys are deleted
- **AND** `KeysForcedDeleted` event is emitted

### Requirement: Relay Proof Collection

The parachain collator SHALL collect relay state proofs for subscribed keys.

#### Scenario: Proof collection for subscriptions

- **GIVEN** a parachain subscribes to publisher 1000 keys [A, B, C]
- **WHEN** the collator builds a block
- **THEN** the relay state proof includes paths to keys A, B, C in publisher 1000's child trie

#### Scenario: Child trie key derivation

- **GIVEN** publisher parachain ID is 1000
- **WHEN** the child trie storage key is derived
- **THEN** it equals `(b"pubsub", ParaId(1000)).encode()`
- **AND** both broadcaster and subscriber use identical derivation

### Requirement: Proof Pruning

The parachain system SHALL prune pub-sub proofs in `provide_inherent` based on cache and subscriptions.

#### Scenario: Unchanged child tries removed

- **GIVEN** publisher 1000's child trie root has not changed
- **WHEN** proof pruning runs
- **THEN** the entire child trie for publisher 1000 is removed from the proof

#### Scenario: Cached nodes pruned

- **GIVEN** nodes [N1, N2, N3] are cached for publisher 1000
- **WHEN** proof pruning runs on a changed child trie
- **THEN** nodes N1, N2, N3 are removed from the proof

#### Scenario: Unsubscribed key paths pruned

- **GIVEN** publisher 1000 has keys [A, B, C, D, E] but subscriber only subscribes to [A, B]
- **WHEN** proof pruning runs
- **THEN** nodes leading only to keys [C, D, E] are removed from the proof

#### Scenario: Budget limit enforced

- **GIVEN** 1 MiB budget for pub-sub
- **WHEN** pruning would include 1.5 MiB of new nodes
- **THEN** only 1 MiB of nodes are included
- **AND** remaining nodes are removed from the proof
