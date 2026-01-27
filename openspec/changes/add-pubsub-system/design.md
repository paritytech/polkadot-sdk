# Design: Pub-Sub System

## Context

RFC-0160 specifies a publish-subscribe mechanism for cross-chain data sharing via the relay chain. Publishers (parachains) store key-value data in per-publisher child tries on the relay chain. Subscribers (other parachains) declare subscriptions and receive data via relay state proofs included in their inherent data.

Key stakeholders:
- Publisher parachains (e.g., POP for ring signature roots)
- Subscriber parachains (e.g., chains verifying ring proofs)
- Relay chain governance (publisher registration, limits)

Constraints:
- PoV size limits
- Relay chain storage costs
- XCM v5 compatibility required

## Goals / Non-Goals

**Goals:**
- Enable parachains to publish key-value data to relay chain (subject to size limits)
- Enable parachains to subscribe to and receive published data with proofs
- Minimize PoV overhead via trie node caching
- Support TTL-based automatic expiration
- System parachain privilege (publish without registration)

**Non-Goals:**
- Prefix-based key enumeration (subscribers must know exact keys)
- Real-time streaming (latency is 2+ blocks)

## Decisions

### Decision 1: Fixed 32-byte Keys

Keys are 32-byte values. Publishers are responsible for hashing their logical key names before publishing.

**Rationale:**
- Evenly distributed trie structure
- Predictable storage calculations

### Decision 2: Per-Publisher Child Tries

Each publisher gets a dedicated child trie under key `(b"pubsub", para_id).encode()`.

**Rationale:**
- Prevents unbalanced main trie
- Efficient cleanup on deregistration
- Child trie roots enable change detection

### Decision 3: Maximum Trie Depth

A `MaxTrieDepth` parameter limits how deep trie traversal can go. Tries deeper than this limit are not supported.

**Rationale:**
- Bounds the maximum cache size per key (at most `MaxTrieDepth` nodes per key path)
- Prevents unbounded storage growth from pathologically deep tries
- Provides predictable worst-case resource usage
- With 32-byte keys and radix-16 trie, typical depth is ~8-10 nodes

**Recommended value:** 16 (sufficient for any reasonable trie structure with 32-byte keys)

**Behavior when exceeded:**
- Proof processing fails with `TrieDepthExceeded` error
- The entire publisher is disabled (added to `DisabledPublishers` storage)
- Disabled publishers are skipped in subsequent blocks
- Manual re-enabling required via `enable_publisher()` call
- This is a configuration/pathological case, not expected in normal operation

### Decision 4: On-Chain Trie Node Caching

Subscriber parachain caches trie nodes (structure only) needed to access subscribed keys in `CachedTrieNodes` storage (keyed by ParaId + H256 node hash). Cache size is bounded by `MaxTrieDepth * num_subscribed_keys`.

**V1 Trie Format:**
- Data larger than 32 bytes is stored externally (hash reference in trie node)
- Data 32 bytes or smaller may be inlined in the trie node
- Cache stores only trie structure nodes, NOT the external data
- This keeps cache entries small and predictable

**Storage:**
```rust
#[pallet::storage]
pub type CachedTrieNodes<T: Config> = StorageDoubleMap<
    _,
    Blake2_128Concat, ParaId,
    Blake2_128Concat, H256,
    BoundedVec<u8, ConstU32<512>>,  // Trie nodes are small without external data
>;
```

**Rationale:**
- Trie nodes without external data are bounded in size
- External data is always fetched fresh from the proof (not cached)
- Reduces on-chain storage requirements significantly
- Cache only accelerates trie traversal, not data retrieval

**Block building process (in `provide_inherent`):**

Call `RelayProofPruner::prune_relay_proofs(proof, root, &mut size_limit)` which handles all pruning logic internally:

1. **Detect unchanged child tries:** Compare child trie roots against `PreviousPublishedDataRoots`. Remove unchanged child tries entirely from the proof.

2. **Prune changed child tries:**
   - Use `CachedHashDB` custom HashDB that checks cache before including nodes
   - Remove nodes already cached on-chain
   - Remove nodes leading to keys we haven't subscribed to
   - Include only new/changed nodes for subscribed keys
   - Stop traversal if depth exceeds `MaxTrieDepth`

3. **Enforce size limit:** Decrement `size_limit` as nodes are included. Nodes that don't fit are removed from the proof.

4. **Cache synchronization via dual-trie traversal:**
   - New nodes found → Add to `CachedTrieNodes`
   - Cached nodes not in new trie → Remove from cache (outdated)
   - Node in cache matches new trie → Stop traversal (subtree unchanged)
   - Traversal aborts if depth exceeds `MaxTrieDepth`

5. **Cursor for partial processing:** Sets `RelayProofProcessingCursor` if budget exhausted before all keys processed.

6. **Resume from cursor:** If `RelayProofProcessingCursor` was set in previous block, start from that position.

**On-chain verification (in `set_validation_data`):**

7. **Malicious collator protection:** When a trie node is missing, verify proof is at the budget limit. If not at limit but nodes are missing, panic (collator is cheating).

### Decision 4: Budget Allocation

Pub-sub uses remaining PoV space after message filtering. No minimum budget is guaranteed - pub-sub gets whatever space remains.

**Formula:**
```
allowed_pov_size = validation_data.max_pov_size * 85%  // Existing HRMP limit
size_limit = messages_collection_size_limit            // Initial budget for DMQ
downward_messages.into_abridged(&mut size_limit)       // DMQ consumes from budget
size_limit += messages_collection_size_limit           // Add HRMP budget
horizontal_messages.into_abridged(&mut size_limit)     // HRMP consumes from budget
// size_limit now contains remaining budget for pub-sub
RelayProofPruner::prune_relay_proofs(proof, root, &mut size_limit)
```

**Rationale:**
- Follows same pattern as message filtering with `into_abridged(&mut size_limit)`
- No custom constants needed - pub-sub simply uses remaining space
- If block is full, pub-sub gracefully skips (retry next block)
- Integrates naturally with existing `provide_inherent` flow

### Decision 5: TTL with on_idle Cleanup

Keys can have finite TTL (expire after N blocks) or infinite TTL (0 = never expires). TTL is capped at `MaxTTL` (432,000 blocks ≈ 30 days).

**Storage structures:**

```rust
/// Entry stored in child trie (includes TTL for subscribers)
pub struct PublishedEntry<BlockNumber> {
    pub value: BoundedVec<u8, MaxValueLength>,
    pub ttl: u32,              // 0 = infinite, N = expire after N blocks
    pub when_inserted: BlockNumber,
}

/// TTL metadata for efficient on_idle scanning (only finite TTL keys)
#[pallet::storage]
pub type TtlData<T: Config> = StorageDoubleMap<
    _, Twox64Concat, ParaId,
    Blake2_128Concat, [u8; 32],
    (u32, BlockNumberFor<T>),  // (ttl, when_inserted)
>;

/// Cursor for incremental TTL scanning
#[pallet::storage]
pub type TtlScanCursor<T: Config> = StorageValue<_, (ParaId, [u8; 32])>;
```

**Rationale:**
- Prevents relay chain storage bloat
- Publishers control data lifecycle
- Subscribers receive TTL metadata for freshness decisions via `TtlState` enum
- Duplicate TTL data: `TtlData` for efficient scanning, child trie for subscriber proofs

**Cleanup mechanism:**
- `on_idle` scans `TtlData` storage for keys where `current_block >= when_inserted + ttl`
- Deletes up to `MaxTtlScansPerIdle` (500) keys per block
- Uses `TtlScanCursor` for incremental processing across blocks
- Best-effort expiration (may be delayed 1-2 blocks if weight exhausted)

### Decision 6: Single-Key Publish Instruction

XCM `Publish { key, value, ttl }` publishes one key at a time. Batch via multiple instructions.

**Rationale:**
- Simpler instruction semantics
- Predictable weight calculation
- Aligns with XCM instruction granularity

## Risks / Trade-offs

### Risk: PoV Exhaustion Under High Load

Heavy HRMP message blocks may compete with pub-sub for PoV space.

**Mitigation:**
- Minimum 1 MiB budget guarantees pub-sub progress
- Cursor tracks resumption point for next block
- System parachains prioritized in subscription ordering

### Risk: TTL Cleanup Delays

`on_idle` may not have enough weight to clean all expired keys immediately.

**Mitigation:**
- Best-effort expiration (may delay 1-2 blocks)
- Subscribers should check TTL metadata for freshness
- Manual deletion available for immediate removal

### Trade-off: Exact Keys Only

No prefix-based enumeration. Subscribers must know exact keys.

**Justification:**
- Prevents unbounded PoV from key enumeration
- Publishers and subscribers coordinate on key naming conventions

## Migration Plan

Not applicable - new capability with no existing implementation.

### Rollback

If issues discovered post-deployment:
1. Governance can force-deregister problematic publishers
2. Subscribers can return empty subscriptions to disable
3. Full removal requires runtime upgrade

## Open Questions

1. **System parachain threshold:** The threshold for system parachains (ID < 2000) is hardcoded. Should this be configurable?
2. **Metrics:** What on-chain metrics should be exposed for monitoring pub-sub health?
