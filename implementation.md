# Implementation Plan: Pub-Sub System Enhancements

Based on [RFC-0160](https://github.com/polkadot-fellows/RFCs/pull/160) and [HackMD Plan](https://hackmd.io/jqzh7p8VQz6U8GaBUi5WZA)

## Overview

This document outlines the remaining work needed to complete the pub-sub mechanism implementation based on RFC-0160. The core infrastructure is already in place:

- ✅ **Broadcaster pallet** - Publisher registry and data storage (`polkadot/runtime/parachains/src/broadcaster/`)
- ✅ **Subscriber pallet** - Subscription handling and data processing (`cumulus/pallets/subscriber/`)
- ✅ **XCM `Publish` instruction** - XCM v5 instruction for publishing data
- ✅ **Change detection** - Root-based change detection to avoid redundant processing

### Remaining Work

1. **TTL & data expiration** - Per-key TTL with automatic cleanup via `on_idle`
2. **Manual deletion APIs** - Parachain self-deletion and root force-deletion
3. **Prefix-based key reading** - Support dynamic key ranges (e.g., `pop_ring_root_{NUMBER}`)
4. **PoV constraints** - Enforce ~1 MiB per block limit with size estimation
5. **Runtime API extensions** - Additional methods for proof size estimation
6. **Documentation and testing** - Comprehensive guides and integration tests

### Future Optimizations (Deferred)

- **Trie node caching for diff proofs** - Requires Substrate API changes (see Appendix A)
- TTL-based expiration works with current full proofs; caching would further reduce PoV

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

### Phase 1: Prefix-Based Key Reading (HIGH PRIORITY)

**Goal:** Support dynamic key ranges like `pop_ring_root_{NUMBER}` for both top and child tries.

**Status:** Partially implemented (exact keys work, prefix enumeration missing)

#### 1.1 Extend RelayStorageKey

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
    
    /// Top-level trie prefix query (enumerate all keys with prefix)
    TopPrefix(Vec<u8>),
    
    /// Child trie prefix query (enumerate all keys with prefix)
    ChildPrefix {
        storage_key: Vec<u8>,
        prefix: Vec<u8>,
    },
}
```

#### 1.2 Add Key Enumeration Methods to RelayChainInterface

**File:** `cumulus/client/relay-chain-interface/src/lib.rs`

```rust
#[async_trait::async_trait]
pub trait RelayChainInterface: Send + Sync {
    // ... existing methods ...
    
    /// Enumerate top-level storage keys matching a prefix
    async fn storage_keys(
        &self,
        hash: PHash,
        prefix: Option<&[u8]>,
        start_key: Option<&[u8]>,
    ) -> RelayChainResult<Vec<Vec<u8>>>;
    
    /// Enumerate child storage keys matching a prefix
    async fn child_storage_keys(
        &self,
        hash: PHash,
        child_info: &ChildInfo,
        prefix: Option<&[u8]>,
        start_key: Option<&[u8]>,
    ) -> RelayChainResult<Vec<Vec<u8>>>;
}
```

#### 1.3 Implement in Inprocess Interface

**File:** `cumulus/client/relay-chain-inprocess-interface/src/lib.rs`

```rust
impl RelayChainInterface for RelayChainInProcessInterface {
    async fn storage_keys(
        &self,
        hash: PHash,
        prefix: Option<&[u8]>,
        start_key: Option<&[u8]>,
    ) -> RelayChainResult<Vec<Vec<u8>>> {
        let state = self.backend.state_at(hash)?;
        
        let iter = state.keys(prefix.unwrap_or(&[]))?;
        let keys: Vec<Vec<u8>> = if let Some(start) = start_key {
            iter.skip_while(|k| k.as_ref().map(|k| k.as_slice() < start).unwrap_or(false))
                .take(1000)  // Limit for safety
                .filter_map(|k| k.ok())
                .collect()
        } else {
            iter.take(1000).filter_map(|k| k.ok()).collect()
        };
        
        Ok(keys)
    }
    
    async fn child_storage_keys(
        &self,
        hash: PHash,
        child_info: &ChildInfo,
        prefix: Option<&[u8]>,
        start_key: Option<&[u8]>,
    ) -> RelayChainResult<Vec<Vec<u8>>> {
        let state = self.backend.state_at(hash)?;
        
        let iter = state.child_keys(child_info, prefix.unwrap_or(&[]))?;
        let keys: Vec<Vec<u8>> = if let Some(start) = start_key {
            iter.skip_while(|k| k.as_ref().map(|k| k.as_slice() < start).unwrap_or(false))
                .take(1000)
                .filter_map(|k| k.ok())
                .collect()
        } else {
            iter.take(1000).filter_map(|k| k.ok()).collect()
        };
        
        Ok(keys)
    }
}
```

#### 1.4 Implement in RPC Interface

**File:** `cumulus/client/relay-chain-rpc-interface/src/lib.rs`

```rust
impl RelayChainInterface for RelayChainRpcInterface {
    async fn storage_keys(
        &self,
        hash: PHash,
        prefix: Option<&[u8]>,
        start_key: Option<&[u8]>,
    ) -> RelayChainResult<Vec<Vec<u8>>> {
        self.rpc_client
            .state_get_keys_paged(
                prefix.map(|p| StorageKey(p.to_vec())),
                1000,
                start_key.map(|k| StorageKey(k.to_vec())),
                Some(hash),
            )
            .await
            .map(|keys| keys.into_iter().map(|k| k.0).collect())
            .map_err(|e| RelayChainError::RpcCallError(e.to_string()))
    }
    
    async fn child_storage_keys(
        &self,
        hash: PHash,
        child_info: &ChildInfo,
        prefix: Option<&[u8]>,
        start_key: Option<&[u8]>,
    ) -> RelayChainResult<Vec<Vec<u8>>> {
        self.rpc_client
            .childstate_get_keys_paged(
                PrefixedStorageKey::new(child_info.prefixed_storage_key()),
                prefix.map(|p| StorageKey(p.to_vec())),
                1000,
                start_key.map(|k| StorageKey(k.to_vec())),
                Some(hash),
            )
            .await
            .map(|keys| keys.into_iter().map(|k| k.0).collect())
            .map_err(|e| RelayChainError::RpcCallError(e.to_string()))
    }
}
```

#### 1.5 Handle Prefix in Proof Collection (Enumerate-First Approach)

**File:** `cumulus/client/parachain-inherent/src/lib.rs`

```rust
async fn collect_relay_storage_proof(
    relay_chain_interface: &impl RelayChainInterface,
    para_id: ParaId,
    relay_parent: PHash,
    // ... other params
    relay_proof_request: RelayProofRequest,
) -> Option<StorageProof> {
    // ... existing static keys collection ...
    
    // Process requested keys, expanding prefixes
    let RelayProofRequest { keys } = relay_proof_request;
    let mut child_keys: BTreeMap<Vec<u8>, Vec<Vec<u8>>> = BTreeMap::new();
    
    for key in keys {
        match key {
            RelayStorageKey::Top(k) => {
                if !all_top_keys.contains(&k) {
                    all_top_keys.push(k);
                }
            },
            RelayStorageKey::TopPrefix(prefix) => {
                // Enumerate all keys matching the prefix
                let matching_keys = relay_chain_interface
                    .storage_keys(relay_parent, Some(&prefix), None)
                    .await
                    .ok()?;
                
                for k in matching_keys {
                    if !all_top_keys.contains(&k) {
                        all_top_keys.push(k);
                    }
                }
            },
            RelayStorageKey::Child { storage_key, key } => {
                child_keys.entry(storage_key).or_default().push(key);
            },
            RelayStorageKey::ChildPrefix { storage_key, prefix } => {
                let child_info = ChildInfo::new_default(&storage_key);
                
                // Enumerate all child keys matching the prefix
                let matching_keys = relay_chain_interface
                    .child_storage_keys(relay_parent, &child_info, Some(&prefix), None)
                    .await
                    .ok()?;
                
                child_keys.entry(storage_key).or_default().extend(matching_keys);
            },
        }
    }
    
    // ... continue with proof generation for collected keys ...
}
```

#### 1.6 Update SubscriptionHandler Trait for Prefix Support

**File:** `cumulus/pallets/subscriber/src/lib.rs`

```rust
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
pub enum SubscriptionKey {
    /// Subscribe to a specific key
    Exact(BoundedVec<u8, ConstU32<1024>>),
    /// Subscribe to all keys with this prefix
    Prefix(BoundedVec<u8, ConstU32<256>>),
}

pub trait SubscriptionHandler {
    /// Return subscriptions with optional prefixes
    fn subscriptions() -> (Vec<(ParaId, Vec<SubscriptionKey>)>, Weight);
    
    /// Called when subscribed data is updated
    fn on_data_updated(publisher: ParaId, key: Vec<u8>, value: Vec<u8>) -> Weight;
}
```

**Note:** The current implementation uses `Vec<Vec<u8>>` for keys. To support prefixes, we need to change this to `Vec<SubscriptionKey>` enum.

---

### Phase 2: PoV Constraints (MEDIUM PRIORITY)

**Goal:** Limit pub-sub data per block to ~1 MiB to fit within PoV budget.

**Status:** Not implemented

#### 2.1 Add Configuration Constants

**File:** `cumulus/primitives/core/src/lib.rs`

```rust
/// Maximum pub-sub proof size per block (1 MiB)
pub const MAX_PUBSUB_PROOF_SIZE: u32 = 1024 * 1024;
```

#### 2.2 Implement Proof Size Estimation

**File:** `cumulus/pallets/subscriber/src/lib.rs`

```rust
impl<T: Config> Pallet<T> {
    /// Estimate proof size for a given set of keys
    fn estimate_proof_size(
        para_id: ParaId,
        keys: &[SubscriptionKey],
    ) -> u32 {
        let mut total = 0u32;
        
        for key in keys {
            match key {
                SubscriptionKey::Exact(k) => {
                    // Estimate: key + value + trie overhead
                    // Typical trie overhead: 32 bytes per level × ~4 levels = 128 bytes
                    let value_size = 384u32; // Default estimate for ring root
                    let trie_overhead = 32 * 4; // ~4 levels deep
                    total = total.saturating_add(
                        k.len() as u32 + value_size + trie_overhead
                    );
                },
                SubscriptionKey::Prefix(p) => {
                    // Estimate based on average key count for this prefix
                    // This requires caching discovered key counts
                    let estimated_keys = Self::get_cached_key_count(para_id, p)
                        .unwrap_or(10); // Conservative default
                    let per_key = 384 + 32 * 4; // value + trie path
                    total = total.saturating_add(estimated_keys * per_key);
                },
            }
        }
        
        total
    }
    
    /// Get relay proof requests with PoV limits
    pub fn get_relay_proof_requests() -> cumulus_primitives_core::RelayProofRequest {
        let (subscriptions, _weight) = T::SubscriptionHandler::subscriptions();
        
        let mut total_estimated_size = 0u32;
        let mut limited_subscriptions = Vec::new();
        
        for (para_id, keys) in subscriptions {
            let publisher_size = Self::estimate_proof_size(para_id, &keys);
            
            if total_estimated_size.saturating_add(publisher_size) <= MAX_PUBSUB_PROOF_SIZE {
                limited_subscriptions.push((para_id, keys));
                total_estimated_size = total_estimated_size.saturating_add(publisher_size);
            } else {
                // Log warning: publisher skipped due to PoV limit
                log::warn!(
                    target: "subscriber",
                    "Publisher {:?} skipped due to PoV limit. Estimated size: {} bytes",
                    para_id,
                    publisher_size
                );
            }
        }
        
        // Build request from limited subscriptions
        let storage_keys = limited_subscriptions
            .into_iter()
            .flat_map(|(para_id, keys)| {
                let storage_key = Self::derive_storage_key(para_id);
                keys.into_iter().map(move |key| {
                    match key {
                        SubscriptionKey::Exact(k) => {
                            cumulus_primitives_core::RelayStorageKey::Child {
                                storage_key: storage_key.clone(),
                                key: k.into_inner(),
                            }
                        },
                        SubscriptionKey::Prefix(p) => {
                            cumulus_primitives_core::RelayStorageKey::ChildPrefix {
                                storage_key: storage_key.clone(),
                                prefix: p.into_inner(),
                            }
                        },
                    }
                })
            })
            .collect();
        
        cumulus_primitives_core::RelayProofRequest { keys: storage_keys }
    }
}
```

#### 2.3 Add Runtime API for Size Estimation

**File:** `cumulus/primitives/core/src/lib.rs`

```rust
sp_api::decl_runtime_apis! {
    #[api_version(2)]
    pub trait KeyToIncludeInRelayProofApi {
        /// Returns relay chain storage proof requests
        fn keys_to_prove() -> RelayProofRequest;
        
        /// Estimate proof size for given keys (for PoV budget)
        fn estimate_proof_size(request: RelayProofRequest) -> u32;
    }
}
```

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
        // Configure subscription
        MockSubscriptionHandler::set_subscriptions(vec![
            (ParaId::from(1000), vec![
                SubscriptionKey::Exact(vec![1u8; 32].try_into().unwrap()),
            ]),
        ]);
        
        // Process next block (will include relay proof)
        System::set_block_number(System::block_number() + 1);
        
        // Verify handler was called
        assert_eq!(
            MockSubscriptionHandler::received_data(),
            vec![(ParaId::from(1000), vec![1u8; 32], b"value1".to_vec())]
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
impl SubscriptionHandler for MyPallet {
    fn subscriptions() -> (Vec<(ParaId, Vec<Vec<u8>>)>, Weight) {
        let subs = vec![
            (ParaId::from(1000), vec![
                vec![0u8; 32],  // Subscribe to specific key
            ]),
        ];
        (subs, Weight::zero())
    }
    
    fn on_data_updated(
        publisher: ParaId,
        key: Vec<u8>,
        value: Vec<u8>,
    ) -> Weight {
        // Process received data
        MyStorage::insert(publisher, key, value);
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
\`\`\`
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
    pub const MAX_PUBSUB_PROOF_SIZE: u32 = 1024 * 1024;  // 1 MiB PoV limit
}
```

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
- **MAX_PUBSUB_PROOF_SIZE (1 MiB):** Leaves 4 MiB for other relay proof data in 5 MiB PoV limit

---

## Known Limitations

### 1. Fixed Key Size

Keys must be exactly 32 bytes (hash output). This is enforced to:
- Predictable storage calculations
- Prevent key collision attacks
- Simplify proof size estimation

**Workaround:** Use a hash function to derive 32-byte keys from arbitrary data.

### 2. No Diff Proofs (PoV Optimization Deferred)

Current implementation generates complete Merkle proofs for all subscribed keys every block. The subscriber's change detection (root comparison) avoids redundant processing but does NOT reduce PoV size.

**Future:** Requires Substrate API changes to support differential proofs (see Appendix A).

### 3. Prefix Enumeration Limits

When using prefix subscriptions, key enumeration is limited to 1000 keys per query to prevent RPC timeouts. For prefixes with more keys:
- Implement pagination in your runtime
- Use multiple narrower prefixes
- Consider exact key subscriptions for known keys

### 4. PoV Size Estimation is Approximate

Actual proof size depends on:
- Trie structure (shared nodes between keys)
- Value sizes (may vary over time)
- Relay chain state density

Estimates may be off by ±20%. Publishers may be skipped if estimates are too conservative.

### 5. System Parachain Privileges

System parachains can publish without registration/deposit but still respect storage limits. This assumes governance ensures system chains are well-behaved.

### 6. Current PoV Reality (No Caching Yet)

**Important:** The trie node caching optimization described in Appendix A is DEFERRED. The current implementation works as follows:

```
Current behavior (without caching):
  - Collator generates FULL proof for all subscribed keys
  - Proof size: ~4.7 MB for 5,000 keys (94% of PoV limit!)
  - Subscriber receives full proof in PoV every block
  - Subscriber extracts values and discards proof
  - No caching - next block requires full proof again
```

**Implications:**

1. **PoV Budget is Critical**
   - Full proof for 5,000 keys = 4.7 MB
   - With 1 MB pub-sub limit: requires 5 blocks to fully sync
   - Each block can include ~1,063 keys worth of proof data

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

4. **Future with Caching (Appendix A)**
   - With diff proofs: only send changed nodes
   - 10% update (500 keys): 612 KB → ~100 KB (6× reduction)
   - But requires Substrate API changes first

**Current Workaround:** If you have many keys that update infrequently:
- Don't subscribe to all keys at once
- Rotate subscriptions across blocks
- Batch keys that update together

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

### Phase 1: TTL & Deletion (3-4 weeks)

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

### Phase 2: Prefix Support (2-3 weeks)

- [ ] Add `TopPrefix` and `ChildPrefix` to `RelayStorageKey`
- [ ] Extend `RelayChainInterface` with key enumeration methods
- [ ] Implement in inprocess and RPC interfaces
- [ ] Update proof collection to enumerate-then-prove
- [ ] Add `SubscriptionKey` enum to subscriber
- [ ] Update `SubscriptionHandler` trait for prefix subscriptions
- [ ] Unit tests for prefix enumeration
- [ ] Integration test for end-to-end prefix subscription

### Phase 3: PoV Limits (1-2 weeks)

- [ ] Add `MAX_PUBSUB_PROOF_SIZE` constant
- [ ] Implement `estimate_proof_size()` in subscriber
- [ ] Add size tracking in `get_relay_proof_requests()`
- [ ] Extend runtime API with `estimate_proof_size()`
- [ ] Add logging for skipped publishers
- [ ] Benchmarks for estimation accuracy

### Phase 4: Documentation (1 week)

- [ ] User guide for publishers (with TTL examples)
- [ ] User guide for subscribers (handling TTL metadata)
- [ ] Integration test examples
- [ ] Configuration reference
- [ ] Troubleshooting guide
- [ ] RPC/Runtime API documentation

### Phase 5: Testing & Refinement (2 weeks)

- [ ] Zombienet test scenarios
- [ ] Load testing (many publishers/subscribers)
- [ ] TTL stress testing (many simultaneous expirations)
- [ ] PoV size validation
- [ ] Performance benchmarking
- [ ] Documentation review

**Total Estimated Effort:** 9-12 weeks

---

## Appendix A: Trie Node Caching (Future Optimization)

**Note:** TTL-based expiration (described above) is IMPLEMENTED and works with current full proofs. The caching optimization described in this appendix is DEFERRED and would further reduce PoV overhead for updates.

### Why Deferred

The current `sp_state_machine::prove_read()` API generates complete Merkle proofs without the ability to exclude nodes that the verifier already has. To enable differential proofs:

**Option 1:** Extend Substrate APIs
- Add `prove_read_with_exclusions(keys, cached_node_hashes)` to `sp_state_machine`
- Proof generator skips nodes in the exclusion set
- Requires coordination with Substrate team

**Option 2:** Custom Proof Format
- Design a custom diff proof format outside `sp_state_machine`
- Collator generates base proof + diff
- Parachain runtime verifies custom format
- More complex, but doesn't require Substrate changes

### Estimated Impact

For 4000 keys with typical updates:
- **Current:** ~1.5 MB proof per block (full trie)
- **With caching:** ~50-100 KB proof per block (only changed nodes)
- **Benefit:** 15-30× reduction in PoV usage for pub-sub

### Storage Requirements

Subscriber would need to cache:
- ~347 KB in trie nodes per publisher
- 10 publishers = ~3.5 MB additional state

### Implementation Sketch

When this becomes unblocked:

1. **Add storage for cached nodes** (DoubleMap: ParaId × NodeHash → NodeData)
2. **Merkle diff traversal** to detect changed keys by comparing node hashes
3. **Runtime API** to expose cached nodes to collator
4. **Collator** generates diff proofs excluding cached nodes
5. **Cache eviction** policy based on subscription changes

**Note:** This is a significant optimization but requires upstream Substrate changes first.

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
- [ ] Prefix enumeration (top and child tries)
- [ ] PoV size estimation accuracy
- [ ] Publisher skipping when PoV limit exceeded
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
- [ ] Prefix subscriptions with many keys
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

### Zombienet Tests

- [ ] Two relay nodes + one publisher + one subscriber
- [ ] Publish-subscribe with multiple blocks
- [ ] Network disruption recovery
- [ ] Large data sets (approaching limits)

### Performance Benchmarks

- [ ] Publish operation (varying item counts)
- [ ] Subscription processing (varying key counts)
- [ ] Change detection overhead
- [ ] Proof size vs. estimated size correlation

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
fn subscriptions() -> (Vec<(ParaId, Vec<Vec<u8>>)>, Weight) {
    (vec![
        (ParaId::from(1000), vec![key1]),
        (ParaId::from(2000), vec![key2, key3]),
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

**Document Version:** 3.0 (Added TTL & Deletion)  
**Last Updated:** January 2026
