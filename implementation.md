# Implementation Plan: Pub-Sub System Enhancements

Based on [RFC-0160](https://github.com/polkadot-fellows/RFCs/pull/160) and [HackMD Plan](https://hackmd.io/jqzh7p8VQz6U8GaBUi5WZA)

## Overview

This document outlines the remaining work needed to complete the pub-sub mechanism implementation based on RFC-0160. The core infrastructure is already in place:

- ✅ **Broadcaster pallet** - Publisher registry and data storage (`polkadot/runtime/parachains/src/broadcaster/`)
- ✅ **Subscriber pallet** - Subscription handling and data processing (`cumulus/pallets/subscriber/`)
- ✅ **XCM `Publish` instruction** - XCM v5 instruction for publishing data
- ✅ **Change detection** - Root-based change detection to avoid redundant processing

### Remaining Work

1. **PoV constraints & caching** - Prune proofs in `provide_inherent` using on-chain trie node cache
2. **TTL & data expiration** - Per-key TTL with automatic cleanup via `on_idle`
3. **Manual deletion APIs** - Parachain self-deletion and root force-deletion
4. **Documentation and testing** - Comprehensive guides and integration tests

**Note:** Prefix-based key reading removed - subscribers must know exact keys to subscribe to.

---

## Architecture Overview

### Publishing Flow

```
Publisher Parachain
    ↓ XCM Publish { data: [(key, value, ttl)] }
Relay Chain (Broadcaster Pallet)
    ↓ Store PublishedEntry { value, ttl, when_inserted } in child trie
    ↓ Store TTL metadata in TtlData map (if ttl != 0)
    ↓ Update child trie root
Relay State Proof
    ↓ Include subscribed keys
Subscriber Parachain
    ↓ Verify proof, extract PublishedEntry with TTL metadata
    ↓ Call SubscriptionHandler::on_data_updated()
```

### Child Trie Key Derivation

**Critical:** Both broadcaster and subscriber must use the same key format:

```rust
// Defined in broadcaster pallet:
fn derive_child_info(para_id: ParaId) -> ChildInfo {
    ChildInfo::new_default(&(b"pubsub", para_id).encode())
}

// Must match in subscriber pallet:
fn derive_storage_key(publisher_para_id: ParaId) -> Vec<u8> {
    (b"pubsub", publisher_para_id).encode()
}
```

This encoding produces: `[112, 117, 98, 115, 117, 98, ...]` + para_id bytes

---

## Component Status

### 1. Broadcaster Pallet (COMPLETE)

**Location:** `polkadot/runtime/parachains/src/broadcaster/`

#### Features Implemented

- ✅ Publisher registration with deposits
- ✅ System parachain support (zero deposit via `force_register_publisher`)
- ✅ Storage limits per publisher (keys and total size)
- ✅ Child trie storage per publisher
- ✅ Cleanup on parachain offboarding
- ✅ `Publish` trait implementation for XCM executor

#### Configuration

```rust
pub trait Config: frame_system::Config {
    type Currency: FunHoldMutate<Self::AccountId>;
    type RuntimeHoldReason: From<HoldReason>;
    type WeightInfo: WeightInfo;
    
    
    #[pallet::constant]
    type MaxValueLength: Get<u32>;          // Max bytes per value (≤2048)
    
    #[pallet::constant]
    type MaxStoredKeys: Get<u32>;           // Max unique keys per publisher
    
    #[pallet::constant]
    type MaxTotalStorageSize: Get<u32>;     // Max total bytes per publisher
    
    #[pallet::constant]
    type MaxPublishers: Get<u32>;           // Max registered publishers
    
    #[pallet::constant]
    type PublisherDeposit: Get<BalanceOf<Self>>; // Registration deposit
}
```

#### Extrinsics

```rust
// Public parachains
register_publisher(para_id: ParaId) -> DispatchResult

// System parachains (Root only)
force_register_publisher(manager: AccountId, deposit: Balance, para_id: ParaId)


// Deregister and reclaim deposit
deregister_publisher(para_id: ParaId) -> DispatchResult

// Force cleanup and deregister (Root only)
force_deregister_publisher(para_id: ParaId) -> DispatchResult

// Delete specific keys (parachain self-service)
delete_keys(keys: Vec<[u8; 32]>) -> DispatchResult

// Force delete any parachain's keys (Root only)
force_delete_keys(para_id: ParaId, keys: Vec<[u8; 32]>) -> DispatchResult
```

#### Key Methods

```rust
// Called by XCM executor when Publish instruction is processed
fn handle_publish(
    origin_para_id: ParaId,
    key: [u8; 32],
    value: BoundedVec<u8, MaxPublishValueLength>,
    ttl: u32,
) -> DispatchResult

```

---

### 2. Subscriber Pallet (COMPLETE - Core Features)

**Location:** `cumulus/pallets/subscriber/src/lib.rs`

#### Features Implemented

- ✅ Subscription management via `SubscriptionHandler` trait
- ✅ Relay proof request generation (`KeyToIncludeInRelayProofApi`)
- ✅ Change detection (root comparison to skip unchanged publishers)
- ✅ Data extraction and handler callbacks
- ✅ `ProcessRelayProofKeys` trait implementation

#### Configuration

```rust
pub trait Config: frame_system::Config {
    type SubscriptionHandler: SubscriptionHandler;
    type WeightInfo: WeightInfo;
    
    #[pallet::constant]
    type MaxPublishers: Get<u32>;  // Max publishers to track
}
```

#### SubscriptionHandler Trait

```rust
/// Entry stored in child trie (includes TTL for subscribers)
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct PublishedEntry<BlockNumber> {
    pub value: BoundedVec<u8, MaxValueLength>,
    pub ttl: u32,              // 0 = infinite, N = expire after N blocks
    pub when_inserted: BlockNumber,
}

/// A subscription key, stored as its Blake2-256 hash (H256)
#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct SubscribedKey(pub H256);

impl SubscribedKey {
    /// Create from pre-computed hash
    pub const fn from_hash(hash: [u8; 32]) -> Self {
        Self(H256::from(hash))
    }
    
    /// Create from runtime data (hashes at runtime)
    pub fn from_data(data: &[u8]) -> Self {
        Self(H256::from(blake2_256(data)))
    }
}

/// Macro to create compile-time hashed subscription keys
/// 
/// # Example
/// ```rust
/// // Hash is computed at compile time
/// const MY_KEY: SubscribedKey = subscribed_key!("my_static_key");
/// 
/// impl SubscriptionHandler for MyHandler {
///     fn subscriptions() -> (Vec<(ParaId, Vec<SubscribedKey>)>, Weight) {
///         (vec![
///             (ParaId::from(1000), vec![
///                 subscribed_key!("pop_ring_root"),
///                 subscribed_key!("another_key"),
///             ]),
///         ], Weight::zero())
///     }
/// }
/// ```
#[macro_export]
macro_rules! subscribed_key {
    ($key:expr) => {
        $crate::SubscribedKey::from_hash(
            sp_crypto_hashing::blake2_256($key.as_bytes())
        )
    };
}

/// TTL state of published data
#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub enum TtlState {
    /// Data never expires (ttl = 0)
    Infinite,
    /// Data is valid for N relay chain blocks from when it was inserted
    ValidFor(u32), // Relay chain blocks remaining until expiration
    /// Data has expired (only sent in edge cases where cleanup hasn't happened yet)
    TimedOut,
}

pub trait SubscriptionHandler {
    /// Return subscriptions: (ParaId, keys as H256 hashes)
    /// Keys should be Blake2-256 hashes computed at compile time via subscribed_key! macro
    /// Weight is the cost of computing subscriptions
    fn subscriptions() -> (Vec<(ParaId, Vec<SubscribedKey>)>, Weight);
    
    /// Called when a subscribed key is updated
    /// Value is the raw published data
    /// TTL indicates the expiration state of the data
    fn on_data_updated(
        publisher: ParaId,
        key: SubscribedKey,
        value: &[u8],
        ttl: TtlState,
    ) -> Weight;
}
```

**Benefits of `SubscribedKey` approach:**
- ✅ **Zero runtime hashing cost** for static keys (computed at compile time)
- ✅ **Type safety** - prevents accidentally passing unhashed keys
- ✅ **Ergonomic API** - `subscribed_key!("my_key")` macro for compile-time hashing
- ✅ **Consistent key format** - all keys stored as `H256` (32 bytes)
- ✅ **Dynamic keys supported** - use `SubscribedKey::from_data()` for runtime-computed keys

#### Change Detection Optimization

The subscriber pallet stores previous child trie roots to avoid re-processing unchanged data:

```rust
#[pallet::storage]
pub type PreviousPublishedDataRoots<T: Config> = StorageValue<
    _,
    BoundedBTreeMap<ParaId, BoundedVec<u8, ConstU32<32>>, T::MaxPublishers>,
    ValueQuery,
>;
```

**Important:** This optimization happens at the runtime level and does NOT reduce PoV size. The proof is already in the block when `set_validation_data` runs. This only reduces:
- Storage writes (if root unchanged)
- Weight from skipped `on_data_updated()` calls

---

### 3. XCM Publish Instruction (COMPLETE)

**Location:** `polkadot/xcm/src/v5/instruction.rs`

#### Instruction Definition

```rust
pub enum Instruction<Call> {
    // ... existing instructions
    
    /// Publish a single key-value pair to relay chain with TTL.
    ///
    /// Origin must be a Parachain junction.
    /// Data is stored in the origin parachain's child trie.
    /// 
    /// To publish multiple items, batch multiple Publish instructions in a single XCM message.
    Publish {
        key: [u8; 32],
        value: BoundedVec<u8, MaxPublishValueLength>,
        ttl: u32,  // 0 = infinite, N = expire after N blocks (capped at MAX_TTL)
    },
}

parameter_types! {
    pub const MaxPublishValueLength: u32 = 2048;
    pub const MaxTTL: u32 = 432_000;  // ~30 days @ 6s blocks
}

#### XCM Executor Integration

The executor validates the origin is a parachain and calls the broadcaster pallet:

```rust
impl XcmExecutor {
    fn execute_instruction(&mut self, instruction: Instruction) -> Result<(), XcmError> {
        match instruction {
            Publish { key, value, ttl } => {
                // Extract ParaId from origin
                let para_id = self.origin.as_ref()
                    .and_then(|loc| {
                        if let Some(Parachain(id)) = loc.interior().first() {
                            Some(ParaId::from(*id))
                        } else {
                            None
                        }
                    })
                    .ok_or(XcmError::BadOrigin)?;
                
                // Delegate to broadcaster pallet
                Config::Broadcaster::publish_data(para_id, key, value, ttl)?;
                Ok(())
            }
        }
    }
}
```

---

## Remaining Implementation Tasks

### Phase 1: Exact Key Reading (IMPLEMENTED)

**Goal:** Support exact key subscriptions for both top and child tries.

**Status:** Implemented

#### 1.1 RelayStorageKey Enum

**File:** `cumulus/primitives/core/src/lib.rs`

```rust
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo)]
pub enum RelayStorageKey {
    /// Exact top-level storage key
    Top(Vec<u8>),
    
    /// Exact child trie storage key
    Child {
        storage_key: Vec<u8>,
        key: Vec<u8>,
    },
}
```

**Note:** Subscribers must know the exact keys they want to subscribe to. Prefix enumeration is not supported - this keeps the implementation simple and predictable.

#### 1.2 SubscriptionHandler Trait

**File:** `cumulus/pallets/subscriber/src/lib.rs`

See section "2. Subscriber Pallet" for the complete `SubscribedKey` type definition with H256-based hashing and the `subscribed_key!` macro.

```rust
pub trait SubscriptionHandler {
    /// Return subscriptions: (ParaId, keys as H256 hashes)
    fn subscriptions() -> (Vec<(ParaId, Vec<SubscribedKey>)>, Weight);
    
    /// Called when subscribed data is updated
    fn on_data_updated(
        publisher: ParaId, 
        key: SubscribedKey, 
        value: &[u8],
        ttl: TtlState,
    ) -> Weight;
}
```

#### 1.3 Proof Collection

**File:** `cumulus/client/parachain-inherent/src/lib.rs`

```rust
async fn collect_relay_storage_proof(
    relay_chain_interface: &impl RelayChainInterface,
    para_id: ParaId,
    relay_parent: PHash,
    relay_proof_request: RelayProofRequest,
) -> Option<StorageProof> {
    let RelayProofRequest { keys } = relay_proof_request;
    let mut child_keys: BTreeMap<Vec<u8>, Vec<Vec<u8>>> = BTreeMap::new();
    
    for key in keys {
        match key {
            RelayStorageKey::Top(k) => {
                if !all_top_keys.contains(&k) {
                    all_top_keys.push(k);
                }
            },
            RelayStorageKey::Child { storage_key, key } => {
                child_keys.entry(storage_key).or_default().push(key);
            },
        }
    }
    
    // Generate proof for collected keys
    // ...
}
```

---

### Phase 2: PoV Constraints & Proof Pruning (MEDIUM PRIORITY)

**Goal:** Prune pub-sub child tries from relay state proof in `provide_inherent`, using remaining budget after messages.

**Status:** Not implemented

**Key Principle:** Follow the same pattern as message filtering with `size_limit`. Messages are filtered first using `into_abridged(&mut size_limit)`, then pub-sub uses the remaining `size_limit` to prune unnecessary data from the relay state proof.

#### 2.1 Integration with Message Filtering

**File:** `cumulus/pallets/parachain-system/src/lib.rs`

The existing message filtering pattern:
```rust
fn messages_collection_size_limit() -> usize {
    let max_block_pov = max_block_weight.proof_size();
    (max_block_pov / 6).saturated_into()  // Each channel gets 1/6 of PoV
}

fn do_create_inherent(data: ParachainInherentData) -> Call<T> {
    // DMQ filtering
    let mut size_limit = messages_collection_size_limit;
    let downward_messages = downward_messages.into_abridged(&mut size_limit);
    
    // HRMP filtering  
    size_limit = size_limit.saturating_add(messages_collection_size_limit);
    let horizontal_messages = horizontal_messages.into_abridged(&mut size_limit);
    
    // size_limit now contains remaining budget for pub-sub
    // ...
}
```

#### 2.2 Add Pub-Sub Proof Pruning to `provide_inherent`

**File:** `cumulus/pallets/parachain-system/src/lib.rs`

```rust
fn do_create_inherent(mut data: ParachainInherentData) -> Call<T> {
    let (vfp, mut downward_messages, mut horizontal_messages) =
        deconstruct_parachain_inherent_data(data);
    
    let messages_collection_size_limit = Self::messages_collection_size_limit();
    
    // DMQ filtering
    let mut size_limit = messages_collection_size_limit;
    let downward_messages = downward_messages.into_abridged(&mut size_limit);
    
    // HRMP filtering
    size_limit = size_limit.saturating_add(messages_collection_size_limit);
    let horizontal_messages = horizontal_messages.into_abridged(&mut size_limit);
    
    // size_limit now contains remaining budget for pub-sub
    // Prune pub-sub child tries from relay_chain_state proof
    let pruned_relay_state = T::PubSubProofPruner::prune_pubsub_proofs(
        data.relay_chain_state,
        vfp.relay_parent_storage_root,
        &mut size_limit,
    );
    // size_limit now contains remaining bytes after pub-sub pruning
    
    log::debug!(
        "PoV budget: messages used, pub-sub budget {} bytes remaining",
        size_limit
    );
    
    let inbound_messages_data =
        InboundMessagesData::new(downward_messages, horizontal_messages);
    
    Call::set_validation_data { 
        data: ParachainInherentData {
            validation_data: vfp,
            relay_chain_state: pruned_relay_state,
            downward_messages: vec![],
            horizontal_messages: BTreeMap::new(),
            relay_parent_descendants: data.relay_parent_descendants,
            collator_peer_id: data.collator_peer_id,
        },
        inbound_messages_data 
    }
}
```

#### 2.3 Pub-Sub Proof Pruner Trait

**File:** `cumulus/pallets/subscriber/src/lib.rs`

```rust
pub trait PubSubProofPruner {
    /// Prune pub-sub child tries from relay state proof
    /// 
    /// - Removes child tries where root hasn't changed
    /// - Includes only new nodes (not in cache) for changed tries
    /// - Respects size_limit (decremented as nodes added)
    fn prune_pubsub_proofs(
        original_proof: StorageProof,
        relay_storage_root: H256,
        size_limit: &mut usize,
    ) -> StorageProof;
}
```

#### 2.4 Custom HashDB for Cache-Aware Pruning

**File:** `cumulus/pallets/subscriber/src/lib.rs`

The pruning uses a custom `HashDB` that checks cache before including nodes:

```rust
use hash_db::{HashDB, Hasher, Prefix};

/// Custom HashDB that checks cache first, only includes new nodes in output
struct CachedHashDB<'a, T: Config, H: Hasher> {
    para_id: ParaId,
    original_proof: &'a StorageProof,
    nodes_to_include: &'a mut Vec<(H::Out, Vec<u8>)>,
    size_limit: &'a mut usize,
    budget_exhausted: bool,
    _phantom: PhantomData<(T, H)>,
}

impl<'a, T: Config, H: Hasher<Out = H256>> HashDB<H, Vec<u8>> for CachedHashDB<'a, T, H> {
    fn get(&self, key: &H256, _prefix: Prefix) -> Option<Vec<u8>> {
        // Check cache first
        if let Some(cached_node) = CachedTrieNodes::<T>::get(self.para_id, *key) {
            // Node in cache - return it but don't add to output
            return Some(cached_node.into_inner());
        }
        
        // Not in cache - must be in original proof
        self.original_proof.read_node(*key)
    }
    
    fn contains(&self, key: &H256, _prefix: Prefix) -> bool {
        CachedTrieNodes::<T>::contains_key(self.para_id, *key) ||
            self.original_proof.contains_node(*key)
    }
    
    // Read-only - panic on write operations
    fn insert(&mut self, _prefix: Prefix, _value: &[u8]) -> H256 {
        panic!("CachedHashDB is read-only")
    }
    fn emplace(&mut self, _key: H256, _prefix: Prefix, _value: Vec<u8>) {
        panic!("CachedHashDB is read-only")
    }
    fn remove(&mut self, _key: &H256, _prefix: Prefix) {
        panic!("CachedHashDB is read-only")
    }
}

impl<'a, T: Config, H: Hasher<Out = H256>> CachedHashDB<'a, T, H> {
    /// Get node and track for inclusion if not cached
    fn get_and_maybe_include(&mut self, key: &H256) -> Option<Vec<u8>> {
        // Check cache first
        if let Some(cached_node) = CachedTrieNodes::<T>::get(self.para_id, *key) {
            // Node in cache - return without adding to output
            return Some(cached_node.into_inner());
        }
        
        // Not in cache - get from original proof
        if let Some(node_data) = self.original_proof.read_node(*key) {
            // Check budget before adding to output
            if !self.budget_exhausted {
                let node_size = node_data.len();
                
                if node_size <= *self.size_limit {
                    // Budget available - add to output
                    self.nodes_to_include.push((*key, node_data.clone()));
                    *self.size_limit = self.size_limit.saturating_sub(node_size);
                } else {
                    // Hit budget limit - stop including new nodes
                    self.budget_exhausted = true;
                }
            }
            
            // Always return node data so trie traversal can continue
            Some(node_data)
        } else {
            None
        }
    }
    
    fn is_budget_exhausted(&self) -> bool {
        self.budget_exhausted
    }
}
```

#### 2.5 Dual-Trie Traversal for Cache Synchronization

During block execution, we traverse the new trie and synchronize the cache:
- **New node found** → Add to cache
- **Cached node not in new trie** → Remove from cache (outdated)  
- **Node in cache matches new trie** → Stop traversal on this branch (subtree unchanged)

```rust
impl<T: Config> Pallet<T> {
    /// Traverse new trie and synchronize cache
    fn traverse_and_sync_cache(
        para_id: ParaId,
        new_proof: &StorageProof,
        new_root: H256,
        subscribed_keys: &[SubscribedKey],
        size_limit: &mut usize,
    ) -> Result<Vec<(H256, Vec<u8>)>, ()> {
        let mut nodes_to_include = Vec::new();
        let mut nodes_to_remove = Vec::new();
        let mut budget_exhausted = false;
        
        for subscribed_key in subscribed_keys {
            if budget_exhausted {
                break;
            }
            
            let result = Self::traverse_key_path(
                para_id,
                new_proof,
                new_root,
                subscribed_key,
                &mut nodes_to_include,
                &mut nodes_to_remove,
                size_limit,
            );
            
            if result.is_err() {
                budget_exhausted = true;
                break;
            }
        }
        
        // Remove outdated nodes from cache
        for node_hash in nodes_to_remove {
            CachedTrieNodes::<T>::remove(para_id, node_hash);
        }
        
        Ok(nodes_to_include)
    }
    
    /// Traverse path to a specific key, comparing with cached nodes
    fn traverse_key_path(
        para_id: ParaId,
        new_proof: &StorageProof,
        current_node_hash: H256,
        key: &[u8],
        nodes_to_include: &mut Vec<(H256, Vec<u8>)>,
        nodes_to_remove: &mut Vec<H256>,
        size_limit: &mut usize,
    ) -> Result<(), ()> {
        let mut current_hash = current_node_hash;
        let mut key_nibbles = Self::key_to_nibbles(key);
        let mut nibble_idx = 0;
        
        loop {
            // Check if we have this node in cache
            let cached_node = CachedTrieNodes::<T>::get(para_id, current_hash);
            
            // Get node from new proof
            let new_node_data = new_proof.read_node(current_hash).ok_or(())?;
            
            match cached_node {
                Some(cached_data) if cached_data.as_slice() == new_node_data.as_slice() => {
                    // Node unchanged - entire subtree is the same
                    // Stop traversal here, no need to go deeper
                    return Ok(());
                }
                Some(_) => {
                    // Node changed - mark old one for removal
                    nodes_to_remove.push(current_hash);
                    
                    // Include new node in proof
                    let node_size = new_node_data.len();
                    if node_size > *size_limit {
                        return Err(()); // Budget exhausted
                    }
                    
                    nodes_to_include.push((current_hash, new_node_data.clone()));
                    *size_limit = size_limit.saturating_sub(node_size);
                }
                None => {
                    // Not in cache - new node, must include
                    let node_size = new_node_data.len();
                    if node_size > *size_limit {
                        return Err(()); // Budget exhausted
                    }
                    
                    nodes_to_include.push((current_hash, new_node_data.clone()));
                    *size_limit = size_limit.saturating_sub(node_size);
                }
            }
            
            // Decode node and get next hash in path
            match Self::decode_and_get_next(&new_node_data, &key_nibbles, nibble_idx)? {
                Some((next_hash, new_nibble_idx)) => {
                    current_hash = next_hash;
                    nibble_idx = new_nibble_idx;
                }
                None => return Ok(()), // Reached leaf or key doesn't exist
            }
        }
    }
}
```

#### 2.6 Block Execution: Verification & Cursor Management

During block execution, verify the collator included all required keys:

```rust
impl<T: Config> Pallet<T> {
    pub fn process_pubsub_data(
        relay_state_proof: &RelayChainStateProof,
        pubsub_size_limit: usize,
    ) -> Weight {
        let (subscriptions, _) = T::SubscriptionHandler::subscriptions();
        let mut proof_size_used = 0usize;
        let resume_from = PubSubProcessingCursor::<T>::get();
        
        for (para_id, subscribed_keys) in subscriptions {
            // Check if root changed
            let current_root = relay_state_proof.read_child_trie_root(para_id);
            let last_root = LastProcessedRoot::<T>::get(para_id);
            
            if current_root == last_root {
                continue;
            }
            
            for subscribed_key in subscribed_keys {
                let mut key_proof_size = 0usize;
                let mut nodes_to_remove = Vec::new();
                let mut nodes_to_cache = Vec::new();
                
                let result = Self::traverse_and_verify_key(
                    para_id,
                    relay_state_proof,
                    current_root,
                    &subscribed_key,
                    &mut key_proof_size,
                    &mut nodes_to_remove,
                    &mut nodes_to_cache,
                );
                
                match result {
                    Ok(Some(value)) => {
                        // Verify budget
                        if proof_size_used + key_proof_size > pubsub_size_limit {
                            // Budget exhausted - set cursor and return
                            PubSubProcessingCursor::<T>::put((para_id, subscribed_key));
                            return /* weight */;
                        }
                        
                        proof_size_used += key_proof_size;
                        
                        // Update cache: remove old, add new
                        for old_hash in nodes_to_remove {
                            CachedTrieNodes::<T>::remove(para_id, old_hash);
                        }
                        for (new_hash, new_data) in nodes_to_cache {
                            if let Ok(bounded) = BoundedVec::try_from(new_data) {
                                CachedTrieNodes::<T>::insert(para_id, new_hash, bounded);
                            }
                        }
                        
                        // Call handler with SubscribedKey and TtlState
                        let ttl_state = Self::compute_ttl_state(&value);
                        T::SubscriptionHandler::on_data_updated(
                            para_id,
                            subscribed_key,
                            &value,
                            ttl_state,
                        );
                    }
                    Ok(None) => continue, // Key doesn't exist
                    Err(_) => {
                        // Missing node - check if budget exhausted
                        if proof_size_used < pubsub_size_limit {
                            // Budget available but node missing - collator cheating!
                            panic!(
                                "Missing node for key {:?}:{:?} with {} bytes available",
                                para_id, subscribed_key,
                                pubsub_size_limit - proof_size_used
                            );
                        } else {
                            // Budget exhausted - set cursor
                            PubSubProcessingCursor::<T>::put((para_id, subscribed_key));
                            return /* weight */;
                        }
                    }
                }
            }
            
            // Update root
            LastProcessedRoot::<T>::insert(para_id, current_root);
        }
        
        // All processed - clear cursor
        PubSubProcessingCursor::<T>::kill();
        /* return weight */
    }
}
```

#### 2.7 Storage for Proof Processing

```rust
/// Cursor tracking which key to resume from next block
#[pallet::storage]
pub type PubSubProcessingCursor<T: Config> = StorageValue<
    _,
    (ParaId, SubscribedKey),
    OptionQuery,
>;

/// Last processed child trie root for each publisher
#[pallet::storage]
pub type LastProcessedRoot<T: Config> = StorageMap<
    _,
    Blake2_128Concat,
    ParaId,
    H256,
    OptionQuery,
>;

/// Cached trie nodes per publisher
#[pallet::storage]
pub type CachedTrieNodes<T: Config> = StorageDoubleMap<
    _,
    Blake2_128Concat,
    ParaId,
    Blake2_128Concat,
    H256,
    BoundedVec<u8, ConstU32<4096>>,
>;
```

#### 2.8 Key Points

1. **`size_limit` pattern** - Same as `into_abridged(&mut size_limit)` for messages
2. **Pruning in `provide_inherent`** - Before block execution, not in collator
3. **Cache-first lookup** - Only include nodes not already cached
4. **Early termination** - Stop at unchanged nodes (entire subtree same)
5. **Dual traversal** - Remove outdated nodes, add new nodes
6. **Cursor on-chain** - Set during block execution when budget exhausted
7. **Collator verification** - Panic if keys missing when budget available

#### 2.9 Testing Requirements

```rust
#[test]
fn pubsub_uses_remaining_message_budget() {
    // Messages get 1/6 + 1/6 = 1/3 of PoV
    // If messages don't use full allocation, pub-sub gets remainder
    let messages_limit = messages_collection_size_limit();  // 1/6 of PoV
    
    // Simulate DMQ using half its allocation
    let mut size_limit = messages_limit;
    let _ = small_dmq_messages.into_abridged(&mut size_limit);
    assert!(size_limit > 0);  // Some budget remaining
    
    // HRMP uses none
    size_limit = size_limit.saturating_add(messages_limit);
    let _ = empty_hrmp.into_abridged(&mut size_limit);
    
    // size_limit now available for pub-sub
    assert!(size_limit > messages_limit);  // More than 1/6 available
    
    // Light block (small, few messages)
    let light_block_size = 500_000usize;  // 500 KB
    let budget = calculate_remaining_pov_budget(allowed_pov_size, light_block_size);
    assert_eq!(budget, 3_750_000);  // 3.75 MB available for pub-sub
    
    // Heavy block (large, many HRMP messages)
    let heavy_block_size = 4_000_000usize;  // 4 MB
    let budget = calculate_remaining_pov_budget(allowed_pov_size, heavy_block_size);
    assert_eq!(budget, 250_000);  // Only 250 KB left for pub-sub
    
    // Block at limit
    let full_block_size = 4_250_000usize;
    let budget = calculate_remaining_pov_budget(allowed_pov_size, full_block_size);
    assert_eq!(budget, 0);  // No space for pub-sub
}

#[test]
fn subscriber_respects_dynamic_budget() {
    // Subscribe to 5000 keys (would be ~13.5 MB unbound)
    MockHandler::set_subscriptions(vec![
        (ParaId::from(1000), vec![SubscribedKey::from_hash([0u8; 32]); 5000]),
    ]);
    
    // Limited budget
    let request = Subscriber::get_relay_proof_requests(1_000_000);  // 1 MB
    let keys_count = count_keys_in_request(&request);
    
    // Should limit to ~370 keys
    assert!(keys_count >= 370 && keys_count <= 380);
}

#[test]
fn partial_publisher_inclusion() {
    // Two publishers with many keys each
    MockHandler::set_subscriptions(vec![
        (ParaId::from(1000), vec![key(); 1000]),  // ~2.7 MB
        (ParaId::from(2000), vec![key(); 1000]),  // ~2.7 MB
    ]);
    
    // Budget fits first publisher + partial second
    let request = Subscriber::get_relay_proof_requests(3_500_000);  // 3.5 MB
    
    let para1_keys = count_keys_for_para(&request, ParaId::from(1000));
    let para2_keys = count_keys_for_para(&request, ParaId::from(2000));
    
    assert_eq!(para1_keys, 1000);  // All keys from para 1000
    assert!(para2_keys > 0 && para2_keys < 1000);  // Partial from 2000
}
```

#### 2.7 Edge Cases

**Case 1: Heavy block (many HRMP messages)** (5 MB max_pov_size, 85% = 4.25 MB allowed)
```
Block PoV after HRMP: 4 MB
Remaining for pub-sub: 250 KB
Result: ~93 keys can be included
```

**Case 2: Light block (few messages)** (5 MB max_pov_size, 85% = 4.25 MB allowed)
```
Block PoV after HRMP: 500 KB
Remaining for pub-sub: 3.75 MB
Result: ~1,389 keys can be included
```

**Case 3: Block at limit (no space for pub-sub)**
```
Block PoV after HRMP: 4.25 MB (full 85%)
Remaining for pub-sub: 0 KB
Result: No pub-sub data this block, retry next block
```

**Note:** Pub-sub gracefully handles zero budget - it simply includes no keys that block.

---

### Phase 3: Documentation and Testing (MEDIUM PRIORITY)

#### 3.1 Integration Test Setup

**Create:** `cumulus/parachains/integration-tests/emulated/chains/parachains/testing/penpal/src/tests/pubsub.rs`

```rust
#[test]
fn publish_and_subscribe_works() {
    // Setup: Publisher and subscriber parachains on relay chain
    TestNet::reset();
    
    // 1. Register publisher parachain
    Relay::execute_with(|| {
        assert_ok!(Broadcaster::register_publisher(
            RuntimeOrigin::signed(ALICE),
            ParaId::from(1000),
        ));
    });
    
    // 2. Publisher publishes data
    Publisher::execute_with(|| {
        let data = vec![
            ([1u8; 32], b"value1".to_vec()),
            ([2u8; 32], b"value2".to_vec()),
        ];
        
        let publish_xcm = Xcm(vec![
            Publish { data: data.try_into().unwrap() },
        ]);
        
        assert_ok!(PolkadotXcm::send(
            RuntimeOrigin::root(),
            Box::new(Parent.into()),
            Box::new(VersionedXcm::V5(publish_xcm)),
        ));
    });
    
    // 3. Wait for relay block to process
    Relay::execute_with(|| {
        // Verify data was published
        let published = Broadcaster::get_all_published_data(ParaId::from(1000));
        assert_eq!(published.len(), 2);
    });
    
    // 4. Subscriber receives data in next block
    Subscriber::execute_with(|| {
        // Configure subscription using SubscribedKey (H256 hash)
        MockSubscriptionHandler::set_subscriptions(vec![
            (ParaId::from(1000), vec![
                SubscribedKey::from_hash([1u8; 32]),
            ]),
        ]);
        
        // Process next block (will include relay proof)
        System::set_block_number(System::block_number() + 1);
        
        // Verify handler was called
        assert_eq!(
            MockSubscriptionHandler::received_data(),
            vec![(ParaId::from(1000), SubscribedKey::from_hash([1u8; 32]), b"value1".to_vec())]
        );
    });
}
```

#### 3.2 User Guide

**Create:** `docs/pub-sub-guide.md`

```markdown
# Pub-Sub User Guide

## For Publishers

### 1. Register as a Publisher

Public parachains must register and pay a deposit:

\`\`\`rust
// Call on relay chain
Broadcaster::register_publisher(origin, para_id)
\`\`\`

System parachains can be registered by governance:

\`\`\`rust
// Root origin
Broadcaster::force_register_publisher(origin, manager, deposit, para_id)
\`\`\`

### 2. Publish Data via XCM

\`\`\`rust
use xcm::v5::{Instruction::Publish, PublishItem};

let data = vec![
    PublishItem {
        key: [0u8; 32],
        value: b"value1".to_vec().try_into().unwrap(),
        ttl: 0,  // 0 = infinite, or specify blocks until expiration
    },
].try_into().unwrap();

let message = Xcm(vec![Publish { data }]);

PolkadotXcm::send(
    RuntimeOrigin::root(),
    Box::new(Parent.into()),  // Send to relay chain
    Box::new(VersionedXcm::V5(message)),
)?;
\`\`\`

### 3. Key Format

Keys must be 32-byte hashes. Use a hash function to derive keys:

\`\`\`rust
use sp_core::blake2_256;

let key = blake2_256(b"my_application_data");
\`\`\`

## For Subscribers

### 1. Implement SubscriptionHandler

\`\`\`rust
use subscriber::{SubscribedKey, subscribed_key, TtlState};

impl SubscriptionHandler for MyPallet {
    fn subscriptions() -> (Vec<(ParaId, Vec<SubscribedKey>)>, Weight) {
        let subs = vec![
            (ParaId::from(1000), vec![
                subscribed_key!("my_key"),  // Compile-time hashed key
            ]),
        ];
        (subs, Weight::zero())
    }
    
    fn on_data_updated(
        publisher: ParaId,
        key: SubscribedKey,
        value: &[u8],
        ttl: TtlState,
    ) -> Weight {
        // Process received data
        MyStorage::insert(publisher, key.0, value.to_vec());
        Weight::from_parts(10_000, 0)
    }
}
\`\`\`

### 2. Configure Subscriber Pallet

\`\`\`rust
impl subscriber::Config for Runtime {
    type SubscriptionHandler = MyPallet;
    type WeightInfo = ();
    type MaxPublishers = ConstU32<100>;
}
\`\`\`

### 3. Implement Runtime API

\`\`\`rust
impl cumulus_primitives_core::KeyToIncludeInRelayProofApi<Block> for Runtime {
    fn keys_to_prove() -> RelayProofRequest {
        Subscriber::get_relay_proof_requests()
    }
}
```
```

---

## Configuration Reference

### Recommended Values

#### Relay Chain (Polkadot/Kusama)

```rust
parameter_types! {
    // Broadcaster Pallet
    pub const MaxPublishItems: u32 = 100;              // Per publish operation
    pub const MaxValueLength: u32 = 2048;              // 2 KB per value
    pub const MaxStoredKeys: u32 = 4000;               // Per publisher
    pub const MaxTotalStorageSize: u32 = 2_097_152;    // 2 MB per publisher
    pub const MaxPublishers: u32 = 1000;               // System-wide limit
    pub const PublisherDeposit: Balance = 100 * UNITS; // 100 DOT/KSM
}
```

#### Parachain (Subscriber)

```rust
parameter_types! {
    pub const MaxPublishers: u32 = 100;  // Max publishers to subscribe to
}
```

#### PoV Budget for Pub-Sub

**No custom constants needed.** Pub-sub uses whatever PoV space remains after block building.

```
allowed_pov_size = validation_data.max_pov_size * 85%  (existing HRMP limit)
block_pov_size = actual PoV after block built (HRMP, inherents, extrinsics)
available_for_pubsub = allowed_pov_size - block_pov_size
```

**Key points:**
- HRMP already uses 85% of `max_pov_size` (see `lookahead.rs:434-442`)
- Pub-sub simply uses the remaining space after block is built
- No minimum/maximum constants - pub-sub gets what's left
- If block fills the PoV, pub-sub gets nothing that block (retry next block)

#### TTL & Expiration Constants

```rust
parameter_types! {
    /// Maximum finite TTL (30 days @ 6s blocks)
    pub const MaxTTL: u32 = 432_000;
    
    /// Maximum entries to scan per on_idle call
    pub const MaxTtlScansPerIdle: u32 = 500;
}
```

**Rationale:**
- **MaxTTL (30 days)**: Balances storage costs vs. reasonable data retention. Prevents unbounded future expirations.
- **MaxTtlScansPerIdle (500)**: ~50 KB read bandwidth per block. Processes 5000 keys across 10 blocks.

### Derivation Rationale

- **MaxPublishItems (100):** Allows batch updates while preventing DoS (100 × 2 KB = ~200 KB per XCM)
- **MaxValueLength (2048):** Ring root (384 bytes) + generous headroom
- **MaxStoredKeys (4000):** Sufficient for POP use case (ring roots)
- **MaxTotalStorageSize (2 MB):** 4000 keys × 512 bytes average (key + value)
- **PublisherDeposit (100 units):** High enough to deter spam, accessible for legitimate chains

---

## Known Limitations

### 1. Fixed Key Size

Keys must be exactly 32 bytes (hash output). This is enforced to:
- Predictable storage calculations
- Prevent key collision attacks
- Simplify proof size estimation

**Workaround:** Use a hash function to derive 32-byte keys from arbitrary data.

### 2. Trie Node Caching for PoV Reduction

The implementation uses on-chain trie node caching to reduce PoV overhead:

- **First block:** Full proof received, all nodes cached
- **Subsequent blocks:** Only new/changed nodes included in proof (cached nodes excluded)
- **Cache synchronization:** Dual-trie traversal detects and removes stale nodes

This achieves significant PoV reduction without requiring Substrate API changes.

### 3. Exact Key Subscriptions Only

Prefix-based key subscriptions are not supported. Subscribers must know the exact storage keys they want to subscribe to. This design choice:
- Simplifies implementation and predictability
- Prevents PoV budget overruns from unbounded key enumeration
- Requires publishers and subscribers to coordinate on key formats

### 4. PoV Size Estimation is Approximate

Actual proof size depends on:
- Trie structure (shared nodes between keys)
- Value sizes (may vary over time)
- Relay chain state density

Estimates may be off by ±20%. Publishers may be skipped if estimates are too conservative.

### 5. System Parachain Privileges

System parachains can publish without registration/deposit but still respect storage limits. This assumes governance ensures system chains are well-behaved.

### 6. PoV Optimization via Trie Node Caching

**Important:** The implementation includes trie node caching (Phase 2) to reduce PoV overhead significantly.

```
Without caching (initial state):
  - Full proof for all subscribed keys
  - Proof size: ~4.7 MB for 5,000 keys (94% of PoV limit!)
  
With caching (after initial sync):
  - On-chain cache stores previously seen trie nodes
  - provide_inherent prunes proof to exclude cached nodes
  - Proof size: ~50-100 KB for typical updates (15-30× reduction)
```

**Implications:**

1. **PoV Budget Depends on Block Size**
   - Full proof for 5,000 keys = 4.7 MB
   - Pub-sub uses whatever space remains after block is built
   - Light blocks (few HRMP messages): 3+ MB available for pub-sub
   - Heavy blocks (many HRMP messages): Little or no space for pub-sub
   - If block fills the 85% PoV limit, pub-sub gets nothing that block

2. **Update Efficiency Varies by Batch Size**
   
   | Keys Updated | Proof Size | Actual Data | Overhead |
   |--------------|-----------|-------------|----------|
   | 1 key | 2.7 KB | 14% | 86% |
   | 10 keys | 18.8 KB | 20% | 80% |
   | 100 keys | 166 KB | 25% | 75% |
   | 500 keys | 612 KB | 31% | 69% |
   
   **Single key updates are extremely inefficient!** The Merkle proof overhead (path from root to leaf) dominates for small updates.

3. **Batching Recommendations**
   - **Avoid:** Publishing 1-10 keys per block (86% overhead)
   - **Good:** Batch 50-100 keys per update (~75% overhead)
   - **Optimal:** Batch 500+ keys per update (~69% overhead)
   - Path sharing in the trie makes bulk updates much more efficient

4. **With Caching**
   - Only new/changed nodes included in proof
   - 10% update (500 keys): 612 KB → ~100 KB (6× reduction)
   - Cache synchronized via dual-trie traversal

**Optimization Tips:**
- Caching is automatic once Phase 2 is implemented
- Batch keys that update together
- Avoid single-key updates (high overhead)

---

## Time-to-Live (TTL) & Data Expiration

### Overview

Published data can optionally expire after a specified number of blocks. This prevents storage bloat on the relay chain and provides downstream consumers with freshness metadata.

**Key features:**
- Per-key TTL configuration
- Automatic cleanup via `on_idle` hook
- Manual deletion (immediate)
- TTL metadata available to subscribers

### TTL Semantics

Each published key has a TTL (time-to-live) specified in blocks:

- **`ttl = 0`**: Infinite TTL (key lives until manually deleted) - **maximum value**
- **`ttl = 1-432000`**: Key expires after specified blocks from publication
- **`ttl > 432000`**: Automatically capped to `MAX_TTL` (432,000 blocks ≈ 30 days)

**Important:** `0` represents the maximum TTL conceptually (infinity).

### Data Structures

```rust
/// Item to publish via XCM
pub struct PublishItem {
    pub key: [u8; 32],
    pub value: BoundedVec<u8, MaxValueLength>,
    pub ttl: u32,  // 0 = infinite, N = expire after N blocks
}

/// Entry stored in child trie (includes TTL for subscribers)
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct PublishedEntry<BlockNumber> {
    pub value: BoundedVec<u8, MaxValueLength>,
    pub ttl: u32,
    pub when_inserted: BlockNumber,
}
```

### Storage Implementation

**Broadcaster pallet storage:**

```rust
/// TTL metadata for efficient on_idle scanning
/// Only keys with finite TTL (ttl != 0) are stored here
#[pallet::storage]
pub type TtlData<T: Config> = StorageDoubleMap<
    _,
    Twox64Concat,
    ParaId,
    Blake2_128Concat,
    [u8; 32],
    (u32, BlockNumberFor<T>),  // (ttl, when_inserted)
    OptionQuery,
>;

/// Cursor for incremental TTL scanning
#[pallet::storage]
pub type TtlScanCursor<T: Config> = StorageValue<_, (ParaId, [u8; 32]), OptionQuery>;
```

**Child trie storage:**

Each key stores `PublishedEntry { value, ttl, when_inserted }` so subscribers can access TTL metadata.

**Why duplicate TTL data?**
- **`TtlData` map:** Efficient scanning during `on_idle` (no child trie reads)
- **Child trie:** Subscribers receive TTL with data in proofs

### Publisher Usage

```rust
// Example 1: Infinite TTL (permanent data)
let items = vec![
    PublishItem {
        key: hash(b"chain_config"),
        value: config_data,
        ttl: 0,  // Never auto-expires
    },
];

// Example 2: Short TTL (temporary cache, 1 hour)
let items = vec![
    PublishItem {
        key: hash(b"price_feed"),
        value: price_data,
        ttl: 600,  // Expires after 600 blocks (~1 hour)
    },
];

// Example 3: Mixed TTLs
let items = vec![
    PublishItem {
        key: hash(b"permanent"),
        value: data1,
        ttl: 0,  // Infinite
    },
    PublishItem {
        key: hash(b"temporary"),
        value: data2,
        ttl: 14400,  // 1 day
    },
];

send_xcm(Publish { data: items });
```

### TTL Update Behavior

Re-publishing a key **resets its TTL**:

```rust
// Block 1000: Publish with ttl=100 → expires at block 1100
// Block 1050: Re-publish same key with ttl=200 → expires at block 1250 (reset)
```

Change from finite to infinite TTL:

```rust
// Block 1000: Publish with ttl=100
// Block 1050: Publish with ttl=0 → now infinite (expiration removed)
```

### Automatic Cleanup (on_idle)

Expired keys are automatically deleted via the `on_idle` hook:

```rust
#[pallet::hooks]
impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
    fn on_idle(n: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
        // Scan up to 500 entries per block
        // Delete expired keys (where n >= when_inserted + ttl)
        // Resume from cursor next block
    }
}
```

**Behavior:**
- Scans `TtlData` for expired keys
- Deletes up to 500 keys per block (weight-limited)
- Uses cursor to resume scanning across blocks
- **Best-effort expiration** (may be delayed by 1-2 blocks if weight exhausted)

### Manual Deletion

#### Parachain Self-Deletion

```rust
/// Delete own published keys immediately (no waiting for expiration)
#[pallet::call_index(3)]
pub fn delete_keys(
    origin: OriginFor<T>,
    keys: Vec<[u8; 32]>,
) -> DispatchResult;
```

**Usage:**
```rust
// Parachain can delete its own keys immediately
pallet_broadcaster::delete_keys(
    parachain_origin,
    vec![hash(b"old_key_1"), hash(b"old_key_2")],
)?;
```

#### Root Force-Deletion

```rust
/// Governance can force-delete any parachain's keys
#[pallet::call_index(4)]
pub fn force_delete_keys(
    origin: OriginFor<T>,
    para_id: ParaId,
    keys: Vec<[u8; 32]>,
) -> DispatchResult;
```

**Key difference from expiration:**
- Manual deletion is **immediate** (executes in same block)
- Auto-expiration is **best-effort** (may take 1-2 blocks via on_idle)

### Subscriber Consumption

Subscribers receive `PublishedEntry` with TTL metadata in proofs:

```rust
impl SubscriptionHandler for MyHandler {
    fn on_data_updated(
        para_id: ParaId,
        data: Vec<([u8; 32], Option<PublishedEntry<BlockNumber>>)>,
    ) -> Weight {
        let current_block = frame_system::Pallet::<T>::block_number();
        
        for (key, entry_opt) in data {
            match entry_opt {
                Some(entry) => {
                    // Check freshness
                    if entry.ttl != 0 {
                        let expires_at = entry.when_inserted + entry.ttl;
                        let remaining = expires_at.saturating_sub(current_block);
                        
                        if remaining == 0 {
                            log::warn!("Received expired data");
                            continue;
                        }
                        
                        log::info!("Data expires in {} blocks", remaining);
                    }
                    
                    // Process value
                    Self::process_value(key, &entry.value);
                }
                None => {
                    // Key deleted (expired or manual)
                    Self::remove_from_cache(key);
                }
            }
        }
        
        Weight::zero()
    }
}
```

**Use cases:**
- **Local caching:** Cache until `expires_at` block
- **Prioritization:** Process freshest data first
- **Staleness detection:** Warn if data near expiration
- **Optimization:** Skip processing expired data

### TTL Configuration Constants

| Constant | Value | Description |
|----------|-------|-------------|
| `MAX_TTL` | 432,000 blocks | ~30 days maximum finite TTL |
| `MAX_TTL_SCANS_PER_IDLE` | 500 entries | Max keys scanned per on_idle call |

**Rationale:**
- **30-day max:** Prevents unbounded future expirations, balances storage costs
- **500 scans/block:** ~50 KB read bandwidth, processes large datasets incrementally

### Storage Overhead

```rust
// Per key with finite TTL:
// - TtlData entry: 44 bytes (ParaId + Key + TTL + BlockNumber)
// - Child trie: +11 bytes (TTL + WhenInserted in PublishedEntry)
// Total overhead: 55 bytes per key with TTL

// For 5000 keys:
// - TtlData: 220 KB
// - Child trie overhead: 55 KB
// Total: 275 KB additional relay chain state
```

**Storage accounting:**
- ✅ Published values: Count against `TotalStorageSize` limit
- ❌ `TtlData` metadata: Does NOT count against limit
- ❌ TTL fields in child trie: Does NOT count against limit

### TTL Events

```rust
#[pallet::event]
pub enum Event<T: Config> {
    /// Key auto-expired via on_idle
    KeyExpired { para_id: ParaId, key: [u8; 32] },
    
    /// Parachain manually deleted own keys
    KeysDeleted { para_id: ParaId, count: u32 },
    
    /// Root force-deleted parachain's keys
    KeysForcedDeleted { para_id: ParaId, count: u32 },
}
```

### TTL Best Practices

1. **Permanent data:** Use `ttl=0` for configuration, system state
2. **Ephemeral data:** Use appropriate finite TTL for caches, feeds
3. **Batch updates:** Group keys that update together to share TTL
4. **Stagger expirations:** Avoid many keys expiring simultaneously
5. **TTL jitter:** Add ±5% random offset to prevent thundering herd
6. **Monitor freshness:** Subscribers should check `expires_at` before use

**Example - Staggered expiration:**
```rust
// Bad: All 100 keys expire at same block
for i in 0..100 {
    publish(key[i], value[i], ttl: 1000);
}

// Good: Stagger with small random offset
let base_ttl = 1000;
for i in 0..100 {
    let jitter = random_range(0..50);  // ±2.5%
    publish(key[i], value[i], ttl: base_ttl + jitter);
}
```

---

## Implementation Order

### Phase 1: Exact Key Reading (DONE)

- [x] `RelayStorageKey` enum with `Top` and `Child` variants
- [x] `SubscribedKey` H256-based type with `subscribed_key!` macro
- [x] `SubscriptionHandler` trait
- [x] Basic proof collection in collator

### Phase 2: PoV Constraints & Caching (4-5 weeks)

- [ ] Integrate `PubSubProofPruner` in `provide_inherent`
- [ ] Implement `CachedHashDB` for cache-aware proof pruning
- [ ] Add `CachedTrieNodes` storage for trie node cache
- [ ] Implement dual-trie traversal for cache synchronization
- [ ] Add `PubSubProcessingCursor` for resumption across blocks
- [ ] Implement budget-constrained key selection
- [ ] Add publisher prioritization (system parachains first)
- [ ] Implement cache eviction (when limit exceeded)
- [ ] Implement `clear_cache_for_publisher()` on subscription change
- [ ] Add logging and metrics for PoV usage
- [ ] Unit tests for budget allocation and caching
- [ ] Integration tests for cache sync

### Phase 3: TTL & Deletion (3-4 weeks)

- [ ] Update `PublishItem` struct with `ttl: u32` field
- [ ] Create `PublishedEntry` struct with `(value, ttl, when_inserted)`
- [ ] Add `TtlData` and `TtlScanCursor` storage
- [ ] Implement `publish()` with TTL handling
- [ ] Implement `on_idle` cleanup with cursor
- [ ] Add `delete_keys()` and `force_delete_keys()` calls
- [ ] Update subscriber pallet to decode `PublishedEntry`
- [ ] Update `SubscriptionHandler` trait signature
- [ ] Unit tests for TTL logic (infinite, finite, capping, reset)
- [ ] Unit tests for manual deletion
- [ ] Integration tests for expiration
- [ ] Benchmarks for `on_idle` and deletion

### Phase 4: Documentation & Testing (2-3 weeks)

- [ ] User guide for publishers (with TTL examples)
- [ ] User guide for subscribers (handling TTL metadata, caching)
- [ ] Integration test examples
- [ ] Zombienet test scenarios
- [ ] Load testing (many publishers/subscribers)
- [ ] TTL stress testing (many simultaneous expirations)
- [ ] Performance benchmarking (cache efficiency)
- [ ] Documentation review

**Total Estimated Effort:** 9-12 weeks

---

## Critical Files Reference

| Component | File Path | Purpose |
|-----------|-----------|---------|
| **Broadcaster Pallet** | `polkadot/runtime/parachains/src/broadcaster/mod.rs` | Publisher registry, data storage |
| **Subscriber Pallet** | `cumulus/pallets/subscriber/src/lib.rs` | Subscription management, data extraction |
| **XCM Publish** | `polkadot/xcm/src/v5/instruction.rs` | XCM instruction definition |
| **XCM Executor** | `polkadot/xcm/xcm-executor/src/lib.rs` | Publish instruction handling |
| **Relay Interface** | `cumulus/client/relay-chain-interface/src/lib.rs` | Relay chain communication trait |
| **Inprocess Impl** | `cumulus/client/relay-chain-inprocess-interface/src/lib.rs` | Full node relay interface |
| **RPC Impl** | `cumulus/client/relay-chain-rpc-interface/src/lib.rs` | RPC-based relay interface |
| **Proof Collection** | `cumulus/client/parachain-inherent/src/lib.rs` | Collator proof generation |
| **Relay Snapshot** | `cumulus/pallets/parachain-system/src/relay_state_snapshot.rs` | Proof verification, data extraction |
| **Primitives** | `cumulus/primitives/core/src/lib.rs` | Types, constants, runtime APIs |

---

## Testing Checklist

### Unit Tests

- [x] Broadcaster: Registration, publishing, deregistration
- [x] Subscriber: Subscription handling, change detection
- [x] XCM: Publish instruction execution
- [ ] Publisher skipping when PoV limit exceeded
- [ ] Budget calculation using validation_data.max_pov_size
- [ ] **TTL: ttl=0 (infinite), ttl=100 (finite), ttl>MAX_TTL (capped)**
- [ ] **TTL: Reset on key update**
- [ ] **TTL: Change finite → infinite**
- [ ] **TTL: Change infinite → finite**
- [ ] **on_idle: Cleanup with full weight budget**
- [ ] **on_idle: Cleanup with partial weight (cursor resume)**
- [ ] **on_idle: Empty queue (no-op)**
- [ ] **Manual deletion: delete_keys() (parachain)**
- [ ] **Manual deletion: force_delete_keys() (root)**
- [ ] **Manual deletion: Removes TtlData metadata**
- [ ] **Cache update: New data published but not subscribed**
  - Publisher publishes keys [A, B, C, D, E]
  - Subscriber only subscribes to [A, B]
  - Verify: Cache contains nodes for paths to [A, B] only
  - Verify: Nodes for [C, D, E] paths are NOT cached
  - Verify: Traversal stops at branches not leading to subscribed keys
- [ ] **Cache update: Subscribed key deleted from published data**
  - Block N: Subscribe to key [A], cache contains path to [A]
  - Block N+1: Publisher deletes key [A]
  - Verify: Cache nodes for path to [A] are removed
  - Verify: Proof verification doesn't fail (key simply absent)
  - Verify: Handler receives None for [A]

### Integration Tests

- [x] Basic publish and subscribe flow
- [ ] Multiple publishers and subscribers
- [ ] PoV limit enforcement
- [ ] Publisher cleanup on offboarding
- [ ] System parachain zero-deposit registration
- [ ] **TTL: Publish → wait TTL blocks → verify deleted**
- [ ] **TTL: Subscriber receives None for expired keys**
- [ ] **TTL: Mixed TTLs (some expire, some remain)**
- [ ] **TTL: on_idle weight exhaustion (verify resume next block)**
- [ ] **Manual deletion: Immediate subscriber notification**
- [ ] **TTL update: Re-publish extends expiration**
- [ ] **Concurrent expirations: Multiple parachains**
- [ ] **Partial subscription with full publication**
  - Publisher publishes 5,000 keys
  - Subscriber only subscribes to 500 keys
  - Verify: Proof contains only paths to 500 subscribed keys (~612 KB)
  - Verify: Cache contains ~1,205 nodes (not full 9,369)
  - Verify: Unsubscribed keys don't waste PoV budget
- [ ] **Key deletion cascade**
  - Subscribe to 100 keys, verify cache size
  - Publisher deletes 50 keys
  - Verify: Cache removes nodes for deleted keys
  - Verify: Shared branch nodes (used by other keys) remain cached
  - Verify: No orphaned nodes remain
- [ ] **Subscription change behavior**
  - Subscribe to keys [1-1000], build cache
  - Change subscription to keys [1001-2000]
  - Verify: Old cache nodes for [1-1000] are removed
  - Verify: New cache nodes for [1001-2000] are added
  - Verify: Total cache size remains bounded

### PoV Budget Tests

- [ ] **Light block → large pub-sub budget**
  - Small block PoV (500 KB)
  - Verify: ~3.75 MB available for pub-sub
- [ ] **Heavy block → small pub-sub budget**
  - Large block PoV (4 MB with many HRMP messages)
  - Verify: ~250 KB available for pub-sub
- [ ] **Full block → no pub-sub**
  - Block uses full 85% PoV limit
  - Verify: Pub-sub gracefully skips, retries next block
- [ ] **Subscriber respects budget constraint**
  - Subscribe to 5000 keys
  - Limit budget to 1 MB
  - Verify: Only ~370 keys included
- [ ] **Partial publisher inclusion**
  - Two publishers with 1000 keys each
  - Budget fits one full + partial second
  - Verify: First publisher fully included, second partially
- [ ] **Publisher prioritization**
  - System parachain (< 2000) vs. regular parachain
  - Verify: System parachain included first when budget limited

### Zombienet Tests

- [ ] Two relay nodes + one publisher + one subscriber
- [ ] Publish-subscribe with multiple blocks
- [ ] Network disruption recovery
- [ ] Large data sets (approaching limits)
- [ ] **PoV budget under load**
  - High HRMP message volume + pub-sub subscriptions
  - Verify: HRMP messages delivered, pub-sub uses remaining space

### Performance Benchmarks

- [ ] Publish operation (varying item counts)
- [ ] Subscription processing (varying key counts)
- [ ] Change detection overhead
- [ ] Proof size vs. estimated size correlation
- [ ] **PoV budget calculation overhead**

---

## Migration Notes

### From Existing Implementations

If you have a fork/branch based on earlier designs:

1. **Update child trie key derivation** to match RFC: `(b"pubsub", para_id).encode()`
2. **Remove any custom XCM instructions** - use standard `Publish` from v5
3. **Update `SubscriptionHandler::subscriptions()`** return type if using old format
4. **Check deposit amounts** - standard is 100 units for public chains, 0 for system chains
5. **Verify storage limits** align with constants in this doc

### Breaking Changes in RFC-0160

- XCM v5 required (older versions don't have `Publish` instruction)
- Keys must be exactly 32 bytes (no variable-length keys)
- Publishers must register before publishing (except system chains)

### Backward Compatibility

- Non-pub-sub parachains are unaffected
- No changes to existing XCM flows
- Opt-in at both publisher and subscriber sides

---

## Frequently Asked Questions

### Q: Can I publish arbitrary data sizes?

**A:** No. Individual values are limited to `MaxValueLength` (typically 2048 bytes). For larger data:
- Publish a content-addressed hash and fetch full data off-chain
- Split data across multiple keys
- Use IPFS or similar for large blobs

### Q: How often should I publish updates?

**A:** Depends on your use case:
- **High-frequency updates** (every block): Keep value sizes small (<512 bytes)
- **Moderate updates** (every ~10 blocks): Can use larger values
- **Infrequent updates** (hourly+): No concerns

**Rule of thumb:** Keep total published data <100 KB per block per publisher.

### Q: What happens if my published data exceeds storage limits?

**A:** The `Publish` XCM instruction will fail with `TotalStorageSizeExceeded`. Options:
- Delete old keys before publishing new ones (publish `None` to delete)
- Request limit increase via governance
- Use multiple publishers (different parachains)

### Q: Can I subscribe to data from multiple publishers?

**A:** Yes, return multiple entries in `SubscriptionHandler::subscriptions()`:

```rust
fn subscriptions() -> (Vec<(ParaId, Vec<SubscribedKey>)>, Weight) {
    (vec![
        (ParaId::from(1000), vec![subscribed_key!("key1")]),
        (ParaId::from(2000), vec![subscribed_key!("key2"), subscribed_key!("key3")]),
    ], Weight::zero())
}
```

### Q: How do I handle publisher downtime?

**A:** The subscriber pallet stores previous roots. If a publisher doesn't publish in a block:
- Previous data remains accessible on relay chain
- Subscriber's `on_data_updated()` won't be called (no change)
- Next update will trigger callback normally

**For critical data:** Implement timeouts in your subscription handler to detect stale data.

### Q: What's the most efficient way to publish updates?

**A:** Batch updates whenever possible! The Merkle proof overhead is significant for small updates:

**Efficiency by batch size:**
- 1 key: 2.7 KB (86% overhead, 14% actual data)
- 10 keys: 1.9 KB per key (80% overhead)
- 100 keys: 1.7 KB per key (75% overhead)
- 500 keys: 1.2 KB per key (69% overhead)

**Best practices:**
1. Accumulate changes and publish every N blocks rather than every block
2. If possible, schedule updates so related keys change together
3. For sparse updates (1-10 keys/block), consider publishing less frequently

**Example - Ring root updates:**
```rust
// ❌ Bad: Publish each ring root immediately when it changes
fn on_ring_finalized(ring_id: u32, root: Hash) {
    publish_immediately(ring_id, root);  // 2.7 KB overhead per update
}

// ✅ Good: Accumulate and batch publish
fn on_ring_finalized(ring_id: u32, root: Hash) {
    PendingUpdates::insert(ring_id, root);
}

fn on_finalize() {
    let updates = PendingUpdates::drain().collect();
    if updates.len() >= 50 {  // Batch threshold
        publish_batch(updates);  // ~1.7 KB overhead per update
    }
}
```

### Q: How does PoV budget sharing work between messages and pub-sub?

**A:** Pub-sub uses whatever space remains after block building. No custom constants needed.

**How it works:**
```
allowed_pov_size = validation_data.max_pov_size * 85%  (existing HRMP limit)
block_pov_size = actual PoV after block built (HRMP messages, inherents, extrinsics)
available_for_pubsub = allowed_pov_size - block_pov_size
```

**Example scenarios (assuming 5 MB max_pov_size, 85% = 4.25 MB allowed):**

| Block PoV | Pub-sub Budget | ~Keys Possible |
|-----------|----------------|----------------|
| 500 KB | 3.75 MB | ~1,389 keys |
| 2 MB | 2.25 MB | ~833 keys |
| 3 MB | 1.25 MB | ~463 keys |
| 4 MB | 250 KB | ~93 keys |
| 4.25 MB (full) | 0 KB | 0 keys (retry next block) |

**Key points:**
- HRMP already applies the 85% limit (see `lookahead.rs:434-442`)
- Pub-sub simply uses the remaining space - no additional caps
- If block is full, pub-sub gracefully skips that block
- Subscriber pallet fits as many keys as possible within remaining budget
- Publishers may be partially included (some keys) if budget limited

**Monitoring:** Check logs for `pov_metrics` to track actual usage.

### Q: How do I handle keys that are deleted or expired?

**A:** Subscribers receive `None` for deleted/expired keys:

```rust
impl SubscriptionHandler for MyHandler {
    fn on_data_updated(
        para_id: ParaId,
        data: Vec<([u8; 32], Option<PublishedEntry<BlockNumber>>)>,
    ) -> Weight {
        for (key, entry_opt) in data {
            match entry_opt {
                Some(entry) => {
                    // Key exists/updated
                    Self::update_cache(key, entry.value);
                }
                None => {
                    // Key deleted (expired or manual deletion)
                    Self::remove_from_cache(key);
                }
            }
        }
        Weight::zero()
    }
}
```

**Publisher deletion:**

```rust
// Manual deletion (immediate)
pallet_broadcaster::delete_keys(
    origin,
    vec![key1, key2],
)?;

// Or let it expire naturally (specify TTL when publishing)
publish(key1, value1, ttl: 600);  // Auto-deletes after 600 blocks
```

**Important:** When keys are deleted:
1. Proof becomes smaller (fewer nodes needed)
2. Subscriber cache automatically cleans up
3. Your `on_data_updated()` handler receives `None` for deleted keys

### Q: How do I choose an appropriate TTL?

**A:** Consider your data's lifespan and update frequency:

| Data Type | Recommended TTL | Rationale |
|-----------|-----------------|-----------|
| Configuration | `0` (infinite) | Changes via governance, explicitly managed |
| Price feeds | 100-600 blocks | 10-60 minutes, frequent updates |
| Daily snapshots | 14,400 blocks | 24 hours, predictable rotation |
| Session data | 28,800 blocks | 48 hours, temporary state |
| Long-term cache | 432,000 blocks | 30 days (max), infrequent access |

**Guidelines:**
- **Update frequency > 1/hour:** Use `ttl = 600` (1 hour)
- **Update frequency < 1/day:** Use `ttl = 14400` (1 day)
- **Manual lifecycle management:** Use `ttl = 0` (infinite)
- **Uncertain:** Start with `ttl = 14400`, adjust based on usage

**Stagger expirations:**
```rust
// Add 5% jitter to prevent all keys expiring simultaneously
let base_ttl = 14400;
let jitter = random_range(0..720);  // ±5%
publish(key, value, ttl: base_ttl + jitter);
```

### Q: How long does published data last?

**A:** Depends on the TTL:

- **`ttl = 0`**: Forever (until manual deletion)
- **`ttl > 0`**: Approximately `ttl` blocks after publication

**Important caveats:**
1. **Best-effort cleanup:** Expiration via `on_idle` may be delayed 1-2 blocks if weight exhausted
2. **Manual deletion:** Immediate (same block)
3. **Subscribers should check freshness:** Don't blindly trust expired data

**Example:**
```rust
// Block 1000: Publish with ttl=100
// Block 1100: Expires logically
// Block 1100-1102: on_idle may still be processing
// Block 1103: Guaranteed deleted

// Subscriber should check:
if entry.ttl != 0 {
    let expires_at = entry.when_inserted + entry.ttl;
    if current_block >= expires_at {
        // Treat as stale/deleted
        return;
    }
}
```

### Q: Can I use this for consensus-critical data?

**A:** **Carefully.** Published data is verified against relay chain state, so it's trustworthy. However:
- Publishers control what they publish (can withhold updates)
- PoV limits may delay updates (publisher skipped)
- Consider fallback mechanisms for critical applications

**Best for:** Oracles, price feeds, configuration updates, non-critical cross-chain data

### Q: What's the latency from publish to subscribe?

**A:** Minimum 2 blocks:
1. **Block N:** Publisher sends `Publish` XCM
2. **Block N+1:** Relay chain processes XCM, updates child trie
3. **Block N+2:** Subscriber includes relay proof, processes data

**Typical:** 12-24 seconds on Polkadot (6s block time × 2-4 blocks)

---

## Support and Resources

- **RFC:** https://github.com/polkadot-fellows/RFCs/pull/160
- **Implementation Branch:** https://github.com/blockdeep/polkadot-sdk/tree/feat/pubsub-rev1225-dev
- **Broadcaster Pallet:** `polkadot/runtime/parachains/src/broadcaster/`
- **Subscriber Pallet:** `cumulus/pallets/subscriber/`
- **Questions:** Substrate Stack Exchange tag `pub-sub`

---

**Document Version:** 5.0 (Simplified: no prefix support, caching integrated into Phase 2)  
**Last Updated:** January 2026
