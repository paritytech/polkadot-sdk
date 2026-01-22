# Scalable Web3 Storage: Implementation Details

## Overview

This document specifies the on-chain and off-chain interfaces for the storage system described in [Scalable Web3 Storage](./scalable-web3-storage.md).

---

## Bucket Semantics

A **bucket** is the fundamental unit of storage organization. It defines:

1. **Logical container**: What data belongs together
2. **Membership**: Who can read, write, or administer
3. **Canonical state**: The MMR (Merkle Mountain Range) tracking bucket contents
4. **Physical storage**: Which providers store this data (via storage agreements)

### Key Properties

**Per-bucket MMR**: The bucket has ONE canonical MMR state. Multiple providers may store the bucket, and they should all converge to this state. The MMR is not per-provider.

**Roles**:
- **Admin**: Can modify members, manage settings, delete data (if not frozen)
- **Writer**: Can append data
- **Reader**: Can read data (relevant for private/access-controlled buckets where providers only serve to authorized members)

**Redundancy**: A bucket can have storage agreements with multiple providers. The `min_providers` setting controls how many providers must acknowledge a state before it can be checkpointed. This ensures minimum redundancy for critical data.

**Append-only mode**: When `frozen_start_seq` is set, the bucket becomes append-only from that point. The start_seq can never decrease below the frozen value, preventing deletion of historical data. This is irreversible and requires the current snapshot to meet `min_providers` threshold.

### Storage Model

**Upload and Commit are separate operations**:

1. **Upload**: Clients upload content-addressed data (chunks and internal nodes) to providers. This is just storage — no MMR involvement yet. Providers accept all uploads as long as the bucket has quota. Multiple clients can upload different data concurrently without conflicts.

2. **Commit**: A client requests the provider to add data_root(s) to the bucket's MMR. The provider signs a commitment to the new MMR state. This is when data becomes "committed" and the provider becomes liable.

3. **Checkpoint**: A client submits provider signatures to the chain, establishing canonical state. The chain records which providers acknowledged this state. Only providers in the snapshot are challengeable for this state.

**No conflict rejection**: Providers accept all uploads within quota. "Conflicts" (different clients uploading different data) are fine — the checkpoint determines which state becomes canonical.

**Pruning rule**: Non-canonical branches can only be pruned once a canonical branch exists with greater depth. A branch with range `[A, A+N)` can be pruned once canonical has range `[B, B+M)` where `B + M > A + N`. This ensures providers remain liable for any data that could still be challenged.

**Optional snapshots**: On-chain snapshots are optional. Without a snapshot:
- `challenge_offchain` works (challenger provides provider signature)
- `challenge_checkpoint` fails (nothing to challenge)
- `Superseded` defense unavailable (no canonical to compare against)
- Provider is liable for ALL signed commitments
- Conflicting forks cannot be pruned

Users who create conflicts without checkpointing waste their quota—providers must keep all signed data.

**Content-addressed storage**: Everything (chunks and internal nodes) is addressed by hash. Internal nodes are data whose content is child hashes. Upload is bottom-up: children must exist before parent can be stored. If a root hash exists, the entire tree is guaranteed complete.

### Provider Lifecycle in Bucket

**Adding a provider:**
1. Admin calls `request_primary_agreement` with the provider
2. Provider calls `accept_agreement` → `StorageAgreement` created, added to `bucket.primary_providers`
3. Client uploads data to provider
4. Client requests commit, provider signs → client has provider signature
5. Client calls `checkpoint` with provider signature → provider added to `snapshot.primary_signers` bitfield

**Adding a replica provider (optional, permissionless):**
1. Anyone calls `request_agreement` with the provider and sync_balance
2. Provider calls `accept_agreement` → `StorageAgreement` created with `ProviderRole::Replica`
3. Replica syncs data autonomously from primaries or other replicas
4. Replica calls `confirm_replica_sync` on-chain → receives per-sync payment, becomes challengeable

**Binding contract:**

Once accepted, agreements are binding for both parties until expiry:
- **No early exit for providers**: Providers cannot voluntarily leave. They committed to store data for the agreed duration.
- **No early cancellation for clients**: Clients cannot cancel and reclaim locked payment. They committed to pay for the agreed duration.
- **Provider's protection**: Before accepting, providers can set `max_duration` and review the terms. They can also block future extensions via `set_extensions_blocked`.
- **Client's protection**: Clients can challenge if provider loses data (slashing). At settlement, clients can burn payment to signal poor service (burns cost an additional premium, making them a credible but costly signal).

**Agreement expiry:**

When `expires_at` is reached:
1. Provider calls `claim_expired_agreement` to receive payment, OR
2. Client calls `end_agreement` with pay/burn decision within settlement window
3. Provider is no longer bound to store data
4. Provider won't be included in future checkpoints

**Snapshot liability**: Providers remain liable for snapshots they signed until those snapshots are superseded by a new checkpoint that doesn't include them, or until the bucket's canonical depth grows past the data they signed for.

### Multi-Provider Coordination (Primary Providers)

Primary providers don't sync with each other. Clients are responsible for uploading to each primary provider they want to store their data.

**Flow**:
1. Client uploads data to Primary A, B, C (separately)
2. Client triggers commit on each provider, collects signatures
3. Client checkpoints on-chain with collected signatures
4. Primaries not in the snapshot should sync (client re-uploads)
5. After checkpoint, providers can prune non-canonical roots

**Liability**: A provider is only liable for MMR states they acknowledged (signed). Challenges against the canonical checkpoint only work for providers listed in the snapshot's provider bitfield.

**Replica providers** sync autonomously from primaries or other replicas. They confirm sync on-chain and are liable for the roots they've confirmed.

---

## On-Chain: Pallet Interface

### Pallet Config

```rust
#[pallet::config]
pub trait Config: frame_system::Config {
    /// Maximum length of provider multiaddr
    type MaxMultiaddrLength: Get<u32>;
    /// Maximum members per bucket
    type MaxMembers: Get<u32>;
    /// Maximum primary providers per bucket (e.g., 5)
    type MaxPrimaryProviders: Get<u32>;
    /// Minimum stake required to register as a provider.
    /// Governance-controlled to bound total provider count and provide sybil resistance.
    #[pallet::constant]
    type MinProviderStake: Get<BalanceOf<Self>>;
    /// Maximum chunk size for challenge responses (e.g., 256 KiB)
    type MaxChunkSize: Get<u32>;
    /// Timeout for challenge response (e.g., ~48 hours in blocks)
    #[pallet::constant]
    type ChallengeTimeout: Get<BlockNumberFor<Self>>;
    /// Settlement window after agreement expiry for owner to call end_agreement
    #[pallet::constant]
    type SettlementTimeout: Get<BlockNumberFor<Self>>;
}
```

### Storage Items

```rust
/// Provider registry
#[pallet::storage]
pub type Providers<T: Config> = StorageMap<
    _,
    Blake2_128Concat,
    T::AccountId,
    ProviderInfo<T>,
>;

pub struct ProviderInfo<T: Config> {
    /// Multiaddr for connecting to this provider
    pub multiaddr: BoundedVec<u8, T::MaxMultiaddrLength>,
    /// Total stake locked by this provider
    pub stake: BalanceOf<T>,
    /// Total contracted bytes (sum of max_bytes across all agreements)
    /// Used for stake/bytes ratio — represents commitment, not actual storage
    pub committed_bytes: u64,
    /// Provider settings
    pub settings: ProviderSettings<T>,
    /// Provider statistics - clients use these to evaluate quality
    pub stats: ProviderStats<T>,
}

/// On-chain statistics for evaluating provider quality.
/// These are objective, verifiable metrics that help clients make informed decisions.
pub struct ProviderStats<T: Config> {
    /// Block when provider registered (track provider age)
    pub registered_at: BlockNumberFor<T>,
    /// Total agreements ever created with this provider
    pub agreements_total: u32,
    /// Agreements where client chose to extend (signal of satisfaction)
    pub agreements_extended: u32,
    /// Agreements that expired without extension (neutral/negative signal)
    pub agreements_not_extended: u32,
    /// Agreements where client burned payment (strong negative signal)
    pub agreements_burned: u32,
    /// Total bytes ever committed across all agreements (historical volume)
    pub total_bytes_committed: u64,
    /// Number of challenges received
    pub challenges_received: u32,
    /// Number of challenges where provider was slashed (critical failure)
    pub challenges_failed: u32,
}

pub struct ProviderSettings<T: Config> {
    /// Minimum agreement duration provider will accept
    pub min_duration: BlockNumberFor<T>,
    /// Maximum agreement duration provider will accept
    pub max_duration: BlockNumberFor<T>,
    /// Price per byte per block for storage
    pub price_per_byte: BalanceOf<T>,
    /// Whether accepting new primary agreements
    pub accepting_primary: bool,
    /// Price per successful sync confirmation, or None if not accepting replicas.
    /// Replicas are paid this amount each time they confirm sync to a new snapshot.
    /// Covers: sync work, bandwidth costs to fetch from primaries, profit margin.
    pub replica_sync_price: Option<BalanceOf<T>>,
    /// Whether accepting extensions on existing agreements
    pub accepting_extensions: bool,
}

/// Monotonically increasing bucket ID counter. Ensures stable, unique IDs.
#[pallet::storage]
pub type NextBucketId<T: Config> = StorageValue<_, BucketId, ValueQuery>;

/// Bucket ID is a stable, unique identifier (not an index into a collection).
/// Using u64 ensures IDs never get reused even if buckets are deleted.
pub type BucketId = u64;

/// Buckets: containers for data with membership and storage agreements
#[pallet::storage]
pub type Buckets<T: Config> = StorageMap<
    _,
    Blake2_128Concat,
    BucketId,
    Bucket<T>,
>;

pub struct Member<T: Config> {
    pub account: T::AccountId,
    pub role: Role,
}

pub enum Role {
    /// Can modify members, manage settings, delete data (if not frozen)
    Admin,
    /// Can append data
    Writer,
    /// Can read data (for private buckets)
    Reader,
}

pub struct Bucket<T: Config> {
    /// Members who can interact with this bucket
    pub members: BoundedVec<Member<T>, T::MaxMembers>,
    /// If Some, bucket is append-only from this start_seq.
    /// Checkpoints with start_seq < frozen_start_seq are rejected (prevents deletions).
    pub frozen_start_seq: Option<u64>,
    /// Minimum primary provider signatures required for checkpoint.
    pub min_providers: u32,
    /// Primary provider account IDs (limited to T::MaxPrimaryProviders, e.g., 5).
    /// These are admin-controlled providers that:
    /// - Receive data directly from writers
    /// - Count toward min_providers for checkpoints
    /// - Can be early-terminated by admin (with pay/burn)
    /// Stored inline for efficient checkpoint reads (one storage access).
    pub primary_providers: BoundedVec<T::AccountId, T::MaxPrimaryProviders>,
    /// Current canonical state
    pub snapshot: Option<BucketSnapshot<T>>,
    /// Historical MMR roots for replica sync validation.
    /// 
    /// **Why we need this:**
    /// Replicas sync autonomously and may lag behind the current snapshot. When a
    /// replica confirms sync, we need to verify they actually synced to a valid
    /// historical state (not a fabricated root). But storing every historical root
    /// would be unbounded. Prime-based bucketing gives us O(1) storage with
    /// logarithmic time coverage - a replica that's 100 blocks behind can still
    /// prove sync to a valid root, while ancient roots naturally age out.
    /// 
    /// **How it works:**
    /// Uses prime-based bucketing for logarithmic time coverage:
    /// Position 0: updated every 3 blocks (prime = 3)
    /// Position 1: updated every 7 blocks (prime = 7)
    /// Position 2: updated every 11 blocks (prime = 11)
    /// Position 3: updated every 23 blocks (prime = 23)
    /// Position 4: updated every 47 blocks (prime = 47)
    /// Position 5: updated every 113 blocks (prime = 113)
    /// 
    /// Each entry stores (quotient, mmr_root) where quotient = block_number / prime.
    /// On each checkpoint, if current_block / prime != stored quotient, the entry
    /// is updated with (new_quotient, current_snapshot_root). This means each
    /// position remembers the root from the last time its prime boundary was crossed.
    /// 
    /// Primes ensure positions don't align, maximizing coverage. A slow replica
    /// can match against older positions; `position_matched` in events tracks this.
    pub historical_roots: [(u32, H256); 6],
    /// Total snapshots created for this bucket (for statistics)
    pub total_snapshots: u32,
}

pub struct BucketSnapshot<T: Config> {
    /// Canonical MMR root
    pub mmr_root: H256,
    /// Start sequence number
    pub start_seq: u64,
    /// Number of leaves in the MMR
    pub leaf_count: u64,
    /// Block at which checkpointed
    pub checkpoint_block: BlockNumberFor<T>,
    /// Bitfield indicating which primary providers signed this snapshot.
    /// Bit i is set if primary_providers[i] signed.
    /// Using bitfield because primary_providers is stored in Bucket and limited
    /// to T::MaxPrimaryProviders (e.g., 5), so indices are stable within a checkpoint.
    /// When primary_providers changes, the bitfield is adjusted accordingly.
    pub primary_signers: BitVec<u8, bitvec::order::Lsb0>,
}
// Canonical range is [start_seq, start_seq + leaf_count)
// Destructive writes (new MMR that allows pruning old) must set start_seq >= old_start_seq + old_leaf_count

/// Storage agreements: per-provider contracts for a bucket
#[pallet::storage]
pub type StorageAgreements<T: Config> = StorageDoubleMap<
    _,
    Blake2_128Concat,
    BucketId,
    Blake2_128Concat,
    T::AccountId,
    StorageAgreement<T>,
>;

pub struct StorageAgreement<T: Config> {
    /// Who owns this agreement (can top up quota, transfer ownership)
    pub owner: T::AccountId,
    /// Maximum bytes (quota) — provider accepts uploads up to this
    pub max_bytes: u64,
    /// Payment locked for storage (bytes * time)
    pub payment_locked: BalanceOf<T>,
    /// Price per byte locked at creation/last extension.
    /// Used to determine if extension requires owner approval (price increases).
    pub price_per_byte: BalanceOf<T>,
    /// Agreement expiration
    pub expires_at: BlockNumberFor<T>,
    /// Whether provider has blocked extensions for this specific agreement
    pub extensions_blocked: bool,
    /// Provider role for this bucket.
    pub role: ProviderRole<T>,
    /// Block when agreement became active (for statistics)
    pub started_at: BlockNumberFor<T>,
}

#[derive(Clone, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum ProviderRole<T: Config> {
    /// Receives data directly from writers.
    /// - Admin-controlled (stored in bucket.primary_providers)
    /// - Count toward min_providers for checkpoints
    /// - Can be early-terminated by admin
    Primary,
    /// Syncs data from other providers autonomously.
    /// - Permissionless (anyone can add)
    /// - Does NOT count toward min_providers
    /// - Cannot be early-terminated (runs to expiry)
    /// - Receives per-sync payment from sync_balance
    Replica {
        /// Balance for per-sync payments (drawn down on each sync confirmation)
        sync_balance: BalanceOf<T>,
        /// Price per sync locked at creation/last extension
        sync_price: BalanceOf<T>,
        /// Minimum blocks between sync confirmations for this agreement.
        /// Set at agreement creation based on expected bucket activity.
        /// 0 means no time-based limit (only "new root" check applies).
        min_sync_interval: BlockNumberFor<T>,
        /// Last confirmed sync: (mmr_root, block_number).
        /// None if replica hasn't confirmed sync yet.
        last_sync: Option<(H256, BlockNumberFor<T>)>,
    },
}

/// Pending agreement requests (client → provider, awaiting acceptance)
/// Keyed by (provider, bucket) so providers can efficiently query their pending requests
#[pallet::storage]
pub type AgreementRequests<T: Config> = StorageDoubleMap<
    _,
    Blake2_128Concat,
    T::AccountId,  // provider (first key for efficient provider queries)
    Blake2_128Concat,
    BucketId,
    AgreementRequest<T>,
>;

pub struct AgreementRequest<T: Config> {
    /// Who requested the agreement
    pub requester: T::AccountId,
    /// Maximum bytes requested
    pub max_bytes: u64,
    /// Payment locked by requester
    pub payment_locked: BalanceOf<T>,
    /// Requested duration
    pub duration: BlockNumberFor<T>,
    /// Block at which request expires if not accepted/rejected
    pub expires_at: BlockNumberFor<T>,
    /// Replica-specific parameters, None for primary agreements.
    /// Presence distinguishes the agreement type at request time.
    pub replica_params: Option<ReplicaRequestParams<T>>,
}

/// Parameters specific to replica agreement requests.
pub struct ReplicaRequestParams<T: Config> {
    /// Initial sync balance to fund per-sync payments
    pub sync_balance: BalanceOf<T>,
    /// Minimum blocks between sync confirmations.
    /// 0 means no time-based limit (only "new root" check applies).
    pub min_sync_interval: BlockNumberFor<T>,
}

/// Pending challenges indexed by deadline block.
/// Challenges per block are bounded by weight limits (creating challenges consumes weight).
#[pallet::storage]
pub type Challenges<T: Config> = StorageMap<
    _,
    Blake2_128Concat,
    BlockNumberFor<T>,
    Vec<Challenge<T>>,
>;

/// Challenge identifier combining deadline and index.
/// Challenges are stored by deadline block for efficient expiry processing.
pub struct ChallengeId<T: Config> {
    /// Block by which provider must respond
    pub deadline: BlockNumberFor<T>,
    /// Index within the deadline's challenge list
    pub index: u16,
}

pub struct Challenge<T: Config> {
    /// Bucket containing the challenged data
    pub bucket_id: BucketId,
    /// Provider being challenged
    pub provider: T::AccountId,
    /// Account that issued the challenge
    pub challenger: T::AccountId,
    /// MMR root the provider committed to
    pub mmr_root: H256,
    /// Start sequence of the commitment (needed to compute challenged_seq = start_seq + leaf_index)
    pub start_seq: u64,
    /// Leaf index within the MMR (relative to start_seq)
    pub leaf_index: u64,
    /// Chunk index within the leaf's data
    pub chunk_index: u64,
}
```

### Events

```rust
#[pallet::event]
pub enum Event<T: Config> {
    // ─────────────────────────────────────────────────────────────
    // Provider events
    // ─────────────────────────────────────────────────────────────
    
    ProviderRegistered {
        provider: T::AccountId,
        stake: BalanceOf<T>,
    },
    ProviderDeregistered {
        provider: T::AccountId,
        stake_returned: BalanceOf<T>,
    },
    ProviderStakeAdded {
        provider: T::AccountId,
        amount: BalanceOf<T>,
        total_stake: BalanceOf<T>,
    },
    ProviderSettingsUpdated {
        provider: T::AccountId,
    },
    ExtensionsBlocked {
        bucket_id: BucketId,
        provider: T::AccountId,
        blocked: bool,
    },

    // ─────────────────────────────────────────────────────────────
    // Bucket events
    // ─────────────────────────────────────────────────────────────
    
    BucketCreated {
        bucket_id: BucketId,
        admin: T::AccountId,
    },
    BucketFrozen {
        bucket_id: BucketId,
        frozen_start_seq: u64,
    },
    MemberSet {
        bucket_id: BucketId,
        member: T::AccountId,
        role: Role,
    },
    MemberRemoved {
        bucket_id: BucketId,
        member: T::AccountId,
    },
    BucketCheckpointed {
        bucket_id: BucketId,
        mmr_root: H256,
        start_seq: u64,
        leaf_count: u64,
        providers: Vec<T::AccountId>,
    },
    ProviderAddedToBucket {
        bucket_id: BucketId,
        provider: T::AccountId,
    },
    PrimaryProviderRemoved {
        bucket_id: BucketId,
        provider: T::AccountId,
        reason: RemovalReason,
    },
    PrimaryAgreementEndedEarly {
        bucket_id: BucketId,
        provider: T::AccountId,
        payment_to_provider: BalanceOf<T>,
        burned: BalanceOf<T>,
    },
    SlashedProviderRemoved {
        bucket_id: BucketId,
        provider: T::AccountId,
        payment_returned_to_owner: BalanceOf<T>,
    },

    // ─────────────────────────────────────────────────────────────
    // Replica events
    // ─────────────────────────────────────────────────────────────

    /// Emitted when a replica confirms sync to a snapshot.
    /// position_matched indicates sync latency:
    /// - 0 = current snapshot (excellent)
    /// - 1-6 = historical positions [base3, base7, base11, base23, base47, base113]
    /// Higher positions indicate the replica is syncing to older snapshots.
    ReplicaSynced {
        bucket_id: BucketId,
        provider: T::AccountId,
        mmr_root: H256,
        position_matched: u8,
        sync_payment: BalanceOf<T>,
    },
    ReplicaSyncBalanceToppedUp {
        bucket_id: BucketId,
        provider: T::AccountId,
        amount: BalanceOf<T>,
        new_total: BalanceOf<T>,
    },

    // ─────────────────────────────────────────────────────────────
    // Agreement events
    // ─────────────────────────────────────────────────────────────
    
    AgreementRequested {
        bucket_id: BucketId,
        provider: T::AccountId,
        requester: T::AccountId,
        max_bytes: u64,
        payment_locked: BalanceOf<T>,
        duration: BlockNumberFor<T>,
    },
    AgreementAccepted {
        bucket_id: BucketId,
        provider: T::AccountId,
        expires_at: BlockNumberFor<T>,
    },
    AgreementRejected {
        bucket_id: BucketId,
        provider: T::AccountId,
        payment_returned: BalanceOf<T>,
    },
    AgreementRequestWithdrawn {
        bucket_id: BucketId,
        provider: T::AccountId,
        payment_returned: BalanceOf<T>,
    },
    AgreementToppedUp {
        bucket_id: BucketId,
        provider: T::AccountId,
        amount: BalanceOf<T>,
        new_max_bytes: u64,
    },
    AgreementExtended {
        bucket_id: BucketId,
        provider: T::AccountId,
        new_expires_at: BlockNumberFor<T>,
        payment: BalanceOf<T>,
    },
    AgreementOwnershipTransferred {
        bucket_id: BucketId,
        provider: T::AccountId,
        old_owner: T::AccountId,
        new_owner: T::AccountId,
    },
    AgreementEnded {
        bucket_id: BucketId,
        provider: T::AccountId,
        payment_to_provider: BalanceOf<T>,
        burned: BalanceOf<T>,
    },
    AgreementExpiredClaimed {
        bucket_id: BucketId,
        provider: T::AccountId,
        payment_to_provider: BalanceOf<T>,
    },

    // ─────────────────────────────────────────────────────────────
    // Challenge events
    // ─────────────────────────────────────────────────────────────
    
    /// A challenge was issued against a provider
    ChallengeCreated {
        challenge_id: ChallengeId<T>,
        bucket_id: BucketId,
        provider: T::AccountId,
        challenger: T::AccountId,
        respond_by: BlockNumberFor<T>,
    },
    /// Provider responded successfully to a challenge
    ChallengeDefended {
        challenge_id: ChallengeId<T>,
        provider: T::AccountId,
        response_time_blocks: BlockNumberFor<T>,
        challenger_cost: BalanceOf<T>,
        provider_cost: BalanceOf<T>,
    },
    /// Provider failed to respond or provided invalid proof - slashed
    ChallengeSlashed {
        challenge_id: ChallengeId<T>,
        provider: T::AccountId,
        slashed_amount: BalanceOf<T>,
        challenger_reward: BalanceOf<T>,
    },
}
```

### Runtime API

```rust
sp_api::decl_runtime_apis! {
    pub trait StorageApi {
        fn provider_info(provider: AccountId) -> Option<ProviderInfo>;
        fn bucket_info(bucket_id: BucketId) -> Option<Bucket>;
        fn bucket_providers(bucket_id: BucketId) -> Vec<AccountId>;
        fn agreement(bucket_id: BucketId, provider: AccountId) -> Option<StorageAgreement>;
        fn pending_requests(bucket_id: BucketId) -> Vec<(AccountId, AgreementRequest)>;
        fn pending_requests_for_provider(provider: AccountId) -> Vec<(BucketId, AgreementRequest)>;
        fn challenges(from_block: BlockNumber, to_block: BlockNumber) -> Vec<(ChallengeId, Challenge)>;
        fn challenge_period() -> BlockNumber;
    }
}
```

### Extrinsics

```rust
#[pallet::call]
impl<T: Config> Pallet<T> {
    // ─────────────────────────────────────────────────────────────
    // Provider management
    // ─────────────────────────────────────────────────────────────

    /// Register as a storage provider.
    /// 
    /// Creates a new provider entry with the given multiaddr and initial stake.
    /// Stake must be at least `T::MinProviderStake`.
    /// 
    /// Parameters:
    /// - `multiaddr`: Network address where clients can connect to this provider
    /// - `stake`: Initial stake to lock (must meet minimum, provides sybil resistance)
    #[pallet::weight(...)]
    pub fn register_provider(
        origin: OriginFor<T>,
        multiaddr: BoundedVec<u8, T::MaxMultiaddrLength>,
        stake: BalanceOf<T>,
    ) -> DispatchResult;

    /// Add stake to an existing provider registration.
    /// 
    /// Stake can only increase; to withdraw stake, use `deregister_provider`.
    /// Higher stake improves stake/bytes ratio, allowing more agreements.
    /// 
    /// Parameters:
    /// - `amount`: Additional stake to lock
    #[pallet::weight(...)]
    pub fn add_stake(
        origin: OriginFor<T>,
        amount: BalanceOf<T>,
    ) -> DispatchResult;

    /// Deregister provider and withdraw stake.
    /// Fails if committed_bytes > 0 (active agreements exist).
    /// Provider must wait for all agreements to expire before deregistering.
    #[pallet::weight(...)]
    pub fn deregister_provider(origin: OriginFor<T>) -> DispatchResult;

    /// Update provider settings.
    /// 
    /// Allows provider to change pricing, duration limits, and availability.
    /// Changes apply to new agreements only; existing agreements retain their terms.
    /// 
    /// Parameters:
    /// - `settings`: New provider settings (pricing, duration limits, accepting flags)
    #[pallet::weight(...)]
    pub fn update_provider_settings(
        origin: OriginFor<T>,
        settings: ProviderSettings<T>,
    ) -> DispatchResult;

    /// Block or unblock extensions for a specific bucket (provider only).
    /// Allows provider to stop a specific bucket from extending while
    /// continuing to accept extensions from other buckets.
    #[pallet::weight(...)]
    pub fn set_extensions_blocked(
        origin: OriginFor<T>,
        bucket_id: BucketId,
        blocked: bool,
    ) -> DispatchResult;




    // ─────────────────────────────────────────────────────────────
    // Bucket management
    // ─────────────────────────────────────────────────────────────

    /// Create a new bucket.
    /// 
    /// The caller becomes the bucket admin. The bucket starts empty with no
    /// providers or data.
    /// 
    /// Parameters:
    /// - `min_providers`: Minimum primary provider signatures required for checkpoints
    #[pallet::weight(...)]
    pub fn create_bucket(origin: OriginFor<T>, min_providers: u32) -> DispatchResult;

    /// Set minimum providers required for checkpoint (admin only).
    /// 
    /// Controls redundancy: checkpoints require at least this many primary provider
    /// signatures to be accepted. Cannot exceed current primary provider count.
    /// 
    /// Parameters:
    /// - `bucket_id`: The bucket to modify
    /// - `min_providers`: New minimum provider count for checkpoints
    #[pallet::weight(...)]
    pub fn set_min_providers(
        origin: OriginFor<T>,
        bucket_id: BucketId,
        min_providers: u32,
    ) -> DispatchResult;

    /// Freeze bucket — make append-only (admin only, irreversible)
    /// Requires snapshot with min_providers acknowledgments
    pub fn freeze_bucket(origin: OriginFor<T>, bucket_id: BucketId) -> DispatchResult;

    /// Add or update a member's role (admin only).
    /// 
    /// Admins cannot demote other admins - they can only:
    /// - Add new members (any role)
    /// - Update non-admin members' roles
    /// - Demote themselves (remove own admin status)
    /// 
    /// This prevents a single compromised admin from seizing control.
    #[pallet::weight(...)]
    pub fn set_member(
        origin: OriginFor<T>,
        bucket_id: BucketId,
        member: T::AccountId,
        role: Role,
    ) -> DispatchResult;

    /// Remove member from bucket (admin only).
    /// 
    /// Admins cannot remove other admins - they can only:
    /// - Remove non-admin members
    /// - Remove themselves
    /// 
    /// This prevents a single compromised admin from seizing control.
    /// 
    /// Note: This is a very primitive handling of multiple admin accounts, in
    /// practice you should be very careful with adding such accounts and should
    /// lean towards using a single one controlled by a DAO (contract, chain,
    /// ..).
    #[pallet::weight(...)]
    pub fn remove_member(
        origin: OriginFor<T>,
        bucket_id: BucketId,
        member: T::AccountId,
    ) -> DispatchResult;



    // ─────────────────────────────────────────────────────────────
    // Storage agreements (per bucket, per provider)
    // ─────────────────────────────────────────────────────────────

    /// Request a replica storage agreement (anyone can request).
    /// 
    /// Creates a replica provider agreement:
    /// - Does NOT count toward min_providers for checkpoints
    /// - Syncs data autonomously from primaries or other replicas
    /// - Cannot be early-terminated (runs to expiry)
    /// - Unlimited number of replicas per bucket
    /// 
    /// The requester becomes the agreement owner (can top up, transfer ownership).
    /// 
    /// Parameters:
    /// - `bucket_id`: The bucket to add a replica for
    /// - `provider`: The provider to request an agreement with
    /// - `max_bytes`: Maximum storage quota for this agreement
    /// - `duration`: How long the agreement should last
    /// - `max_payment`: Upper bound on storage payment. Actual payment is calculated
    ///   as `provider.price_per_byte * max_bytes * duration`. Fails if this exceeds
    ///   `max_payment` (protects against price changes between query and submission).
    /// - `replica_params`: Replica-specific parameters:
    ///   - `sync_balance`: Transferred from requester to fund per-sync payments at
    ///     the provider's `replica_sync_price`. When exhausted, replica stops
    ///     receiving sync payments but remains bound until expiry. Can top up via
    ///     `top_up_replica_sync_balance`.
    ///   - `min_sync_interval`: Minimum blocks between sync confirmations. Set based
    ///     on expected bucket activity. 0 for no time-based limit.
    #[pallet::weight(...)]
    pub fn request_agreement(
        origin: OriginFor<T>,
        bucket_id: BucketId,
        provider: T::AccountId,
        max_bytes: u64,
        duration: BlockNumberFor<T>,
        max_payment: BalanceOf<T>,
        replica_params: ReplicaRequestParams<T>,
    ) -> DispatchResult;

    /// Accept a pending agreement request (provider only).
    /// 
    /// Creates the storage agreement and adds the provider to the bucket.
    /// For primary agreements: provider is added to `bucket.primary_providers`.
    /// For replica agreements: provider can start syncing immediately.
    /// 
    /// Parameters:
    /// - `bucket_id`: The bucket with the pending request
    #[pallet::weight(...)]
    pub fn accept_agreement(
        origin: OriginFor<T>,
        bucket_id: BucketId,
    ) -> DispatchResult;

    /// Reject a pending agreement request (provider only).
    /// 
    /// Refunds the locked payment to the original requester.
    /// 
    /// Parameters:
    /// - `bucket_id`: The bucket with the pending request to reject
    #[pallet::weight(...)]
    pub fn reject_agreement(
        origin: OriginFor<T>,
        bucket_id: BucketId,
    ) -> DispatchResult;

    /// Withdraw a pending agreement request before provider accepts.
    /// 
    /// Only the original requester can withdraw. Refunds the locked payment.
    /// 
    /// Parameters:
    /// - `bucket_id`: The bucket with the pending request
    /// - `provider`: The provider the request was made to
    #[pallet::weight(...)]
    pub fn withdraw_agreement_request(
        origin: OriginFor<T>,
        bucket_id: BucketId,
        provider: T::AccountId,
    ) -> DispatchResult;

    /// Top up quota for an existing agreement (owner only).
    /// Increases max_bytes, does not change duration.
    /// Actual payment = provider.price_per_byte * additional_bytes * remaining_duration.
    /// Fails if calculated payment > max_payment.
    #[pallet::weight(...)]
    pub fn top_up_agreement(
        origin: OriginFor<T>,
        bucket_id: BucketId,
        provider: T::AccountId,
        additional_bytes: u64,
        max_payment: BalanceOf<T>,
    ) -> DispatchResult;

    /// Extend agreement duration (immediate, no provider approval needed).
    /// 1. Settles current period: releases payment to provider for elapsed time
    /// 2. Calculates and locks new payment for extension at current provider prices
    /// 3. Updates end date to now + additional_duration
    /// 4. Updates agreement.price_per_byte (and sync_price for replicas) to current prices
    /// 
    /// **Price change rules:**
    /// - If provider's current price <= agreement's locked price: anyone can extend
    /// - If provider's current price > agreement's locked price: only owner can extend
    /// This enables permissionless persistence for frozen buckets while protecting
    /// owners from unwanted price increases.
    /// 
    /// Actual payment = provider.price_per_byte * current_max_bytes * additional_duration.
    /// For replicas: also requires topping up sync_balance proportionally.
    /// Fails if calculated payment > max_payment.
    /// 
    /// Also fails if:
    /// - Duration below provider's min_duration or above max_duration
    /// - Provider has globally paused extensions (settings.accepting_extensions == false)
    /// - Provider has blocked extensions for this specific bucket (agreement.extensions_blocked == true)
    #[pallet::weight(...)]
    pub fn extend_agreement(
        origin: OriginFor<T>,
        bucket_id: BucketId,
        provider: T::AccountId,
        additional_duration: BlockNumberFor<T>,
        max_payment: BalanceOf<T>,
    ) -> DispatchResult;

    /// Transfer agreement ownership (current owner only).
    /// 
    /// The new owner can top up quota and transfer ownership further.
    /// Useful for selling agreement slots or transferring to a DAO.
    /// 
    /// Parameters:
    /// - `bucket_id`: The bucket containing the agreement
    /// - `provider`: The provider of the agreement to transfer
    /// - `new_owner`: Account that will become the new agreement owner
    #[pallet::weight(...)]
    pub fn transfer_agreement_ownership(
        origin: OriginFor<T>,
        bucket_id: BucketId,
        provider: T::AccountId,
        new_owner: T::AccountId,
    ) -> DispatchResult;

    /// End agreement with pay/burn decision.
    /// 
    /// **After expiry:** Owner can call within T::SettlementTimeout to settle.
    /// If owner doesn't act, provider can call claim_expired_agreement.
    /// 
    /// **Before expiry (early termination):** Only admin can call, only for primary
    /// providers. The full remaining payment is subject to the action (not pro-rated).
    /// 
    /// **Why early termination for primaries?**
    /// Admin needs ability to remove hostile or misbehaving primary providers.
    /// Without this, a malicious primary could hold the bucket hostage until expiry.
    /// Primary providers are admin-controlled for write coordination; admin must
    /// maintain control over who can accept writes.
    /// 
    /// **Replicas cannot be early-terminated:** There's no use case, and allowing
    /// it would violate the principle of least surprise. A business checking on a
    /// bucket sees "5 providers with agreements until May" and concludes all is
    /// well - they shouldn't find the bucket dead the next day because someone
    /// terminated agreements early. If unhappy with a provider, simply don't extend.
    /// 
    /// Note: For primary agreements, admin is the owner (created via request_primary_agreement).
    /// Admin has no special privileges over replica agreements.
    #[pallet::weight(...)]
    pub fn end_agreement(
        origin: OriginFor<T>,
        bucket_id: BucketId,
        provider: T::AccountId,
        action: EndAction,
    ) -> DispatchResult;

    /// Claim payment for expired agreement (provider only).
    /// Can only be called after agreement expired + T::SettlementTimeout.
    /// Client forfeited their right to burn by not acting in time.
    #[pallet::weight(...)]
    pub fn claim_expired_agreement(
        origin: OriginFor<T>,
        bucket_id: BucketId,
    ) -> DispatchResult;

    /// Request a primary storage agreement (admin only).
    /// 
    /// Creates a primary (admin-added) provider agreement:
    /// - Counts toward min_providers for checkpoints
    /// - Stored in bucket.primary_providers (limited to T::MaxPrimaryProviders)
    /// - Can be early-terminated by admin
    /// 
    /// Fails if bucket has reached T::MaxPrimaryProviders limit.
    /// 
    /// Parameters:
    /// - `bucket_id`: The bucket to add a primary provider for
    /// - `provider`: The provider to request an agreement with
    /// - `max_bytes`: Maximum storage quota for this agreement
    /// - `duration`: How long the agreement should last
    /// - `max_payment`: Upper bound on storage payment. Actual payment is calculated
    ///   as `provider.price_per_byte * max_bytes * duration`. Fails if this exceeds
    ///   `max_payment` (protects against price changes between query and submission).
    #[pallet::weight(...)]
    pub fn request_primary_agreement(
        origin: OriginFor<T>,
        bucket_id: BucketId,
        provider: T::AccountId,
        max_bytes: u64,
        duration: BlockNumberFor<T>,
        max_payment: BalanceOf<T>,
    ) -> DispatchResult;

    /// Remove a slashed provider from a bucket (anyone can call).
    /// 
    /// After a provider is slashed (failed a challenge), they should be removed
    /// from the bucket's provider lists. This is permissionless because:
    /// - Slashing is already a clear on-chain signal of failure
    /// - Keeping slashed providers in lists is misleading
    /// - No payment/burn decision needed (the slash already handled consequences)
    /// 
    /// Removes the agreement entirely. For primary providers, also removes from
    /// bucket.primary_providers and adjusts the snapshot bitfield if they were in it.
    /// 
    /// The agreement's remaining payment is handled as follows:
    /// - If slashed while agreement was active: remaining payment returned to owner
    ///   (provider already punished via stake slash, client shouldn't also lose payment)
    #[pallet::weight(...)]
    pub fn remove_slashed(
        origin: OriginFor<T>,
        bucket_id: BucketId,
        provider: T::AccountId,
    ) -> DispatchResult;

    // ─────────────────────────────────────────────────────────────
    // Checkpoints
    // ─────────────────────────────────────────────────────────────

    /// Submit a new checkpoint with provider signatures (writers/admin only).
    /// 
    /// Creates a new canonical state (new mmr_root, start_seq, leaf_count).
    /// Requires at least min_providers signatures from providers in bucket.primary_providers.
    /// For frozen buckets: start_seq must equal frozen_start_seq (only leaf_count can increase).
    pub fn checkpoint(
        origin: OriginFor<T>,
        bucket_id: BucketId,
        mmr_root: H256,
        start_seq: u64,
        leaf_count: u64,
        signatures: BoundedVec<(T::AccountId, Signature), T::MaxPrimaryProviders>,
    ) -> DispatchResult;

    /// Extend an existing checkpoint's provider bitfield (anyone can call).
    /// 
    /// Adds additional provider signatures to the current snapshot without changing
    /// the mmr_root, start_seq, or leaf_count. This is permissionless because:
    /// - It only adds accountability (more providers are now challengeable)
    /// - It cannot change the canonical state
    /// - Signatures are verified on-chain
    /// 
    /// Providers added this way become liable for the snapshot state.
    pub fn extend_checkpoint(
        origin: OriginFor<T>,
        bucket_id: BucketId,
        signatures: BoundedVec<(T::AccountId, Signature), T::MaxPrimaryProviders>,
    ) -> DispatchResult;

    // ─────────────────────────────────────────────────────────────
    // Challenges
    // ─────────────────────────────────────────────────────────────
    //
    // Three challenge modes exist for different scenarios:
    //
    // **challenge_checkpoint** - Best for cold/stable buckets:
    // - Infrequent writes mean snapshot stays stable
    // - No race conditions between challenge and new checkpoints
    // - Guarantees min_providers are always challengeable via on-chain state
    // - No need for challenger to store signatures locally
    //
    // **challenge_offchain** - Best for hot/active buckets:
    // - Frequent writes cause snapshot races (new checkpoint may not include
    //   the provider you want to challenge)
    // - Writers have fresh signatures from their commits
    // - Writers are the natural challengers (they're active participants)
    // - Signatures are recoverable from block history if needed
    //
    // **challenge_replica** - For replica providers:
    // - Uses the replica's on-chain sync confirmation (last_synced_root)
    // - No signature needed - chain already has their commitment
    // - Replicas are liable for roots they've confirmed synced to
    //
    // For hot buckets, challenge_checkpoint may fail due to race conditions,
    // but this is acceptable: active writers have signatures and can use
    // challenge_offchain. The snapshot primarily protects cold/archival data
    // where nobody has recent signatures or doesn't bother to dig them up.

    /// Challenge on-chain checkpoint (no signatures needed).
    /// Provider must be in current snapshot's provider list.
    /// 
    /// NOTE: May race with new checkpoints in hot buckets. If the provider is
    /// no longer in the snapshot when the transaction executes, this fails.
    /// For hot buckets, prefer challenge_offchain with the signature you have.
    pub fn challenge_checkpoint(
        origin: OriginFor<T>,
        bucket_id: BucketId,
        provider: T::AccountId,
        leaf_index: u64,
        chunk_index: u64,
    ) -> DispatchResult;

    /// Challenge off-chain commitment (requires provider signature).
    /// Works regardless of current snapshot state - the signature proves
    /// the provider committed to this data.
    /// 
    /// Preferred for hot buckets where snapshots change frequently.
    pub fn challenge_offchain(
        origin: OriginFor<T>,
        bucket_id: BucketId,
        provider: T::AccountId,
        mmr_root: H256,
        start_seq: u64,
        leaf_index: u64,
        chunk_index: u64,
        provider_signature: Signature,
    ) -> DispatchResult;

    /// Challenge a replica based on their on-chain sync confirmation.
    /// Uses the replica's last_synced_root stored in their agreement.
    /// No signature needed - the chain already has their commitment.
    pub fn challenge_replica(
        origin: OriginFor<T>,
        bucket_id: BucketId,
        provider: T::AccountId,
        leaf_index: u64,
        chunk_index: u64,
    ) -> DispatchResult;

    /// Cancel an active challenge.
    ///
    /// Allows challenger to cancel if they received the data off-chain.
    /// Full deposit is refunded (only transaction fees are lost).
    /// This prevents unnecessary on-chain data submission when the
    /// issue was resolved off-chain.
    ///
    /// Can only be called by the original challenger.
    pub fn cancel_challenge(
        origin: OriginFor<T>,
        challenge_id: ChallengeId,
    ) -> DispatchResult;

    // ─────────────────────────────────────────────────────────────
    // Replica sync
    // ─────────────────────────────────────────────────────────────

    /// Replica confirms sync to one or more MMR roots.
    /// 
    /// **Why this exists:**
    /// Replicas sync autonomously and need to prove they actually have the data.
    /// By signing which roots they've synced to, replicas become challengeable for
    /// that data. The chain validates against current snapshot and historical_roots
    /// to ensure the replica isn't claiming a fabricated root.
    /// 
    /// **Why historical roots (prime-bucketed)?**
    /// Replicas may lag behind the current snapshot. Rather than requiring exact
    /// sync to current state (which races with new checkpoints), we accept sync
    /// confirmations against recent historical roots. Prime-based bucketing (see
    /// `Bucket.historical_roots`) provides O(1) storage with logarithmic time
    /// coverage, allowing replicas to confirm sync even when slightly behind.
    /// 
    /// **Matching logic:**
    /// The chain checks positions in order: current snapshot first, then historical
    /// positions 0-5. The first position where the replica's claimed root matches
    /// the on-chain root is used. This means replicas are credited for the most
    /// recent state they've synced to, even if they also have older roots.
    /// 
    /// **Rate limiting:**
    /// Two checks prevent excessive sync confirmations:
    /// 1. The matched root must differ from `last_sync.0` (must be new state)
    /// 2. `current_block >= last_sync.1 + min_sync_interval` (per-agreement)
    /// 
    /// The first check ensures payment only for actual sync work. The second
    /// prevents hot buckets (writes every block) from causing excessive on-chain
    /// sync confirmations. `min_sync_interval` is set per-agreement at creation,
    /// based on expected bucket activity. Set to 0 for no time-based limit.
    /// 
    /// Replicas are already paid for storage via `payment_locked` (like primaries),
    /// which covers storage costs (slashing risk is negligible if they do their
    /// job properly). The `sync_price` separately
    /// compensates for sync work: bandwidth costs, incentivizing other providers
    /// to serve data (they may refuse or deprioritize), verification compute, and
    /// tx costs. Sync-specific risks (e.g., uncooperative providers causing sync
    /// failures) should be negligible if the replica syncs regularly.
    /// 
    /// On success (both checks pass):
    /// - Updates replica's `last_sync` to `(matched_root, current_block)`
    /// - Pays sync_price from replica's sync_balance
    /// - Emits ReplicaSynced event with position_matched for performance tracking
    ///   (position 0 = current snapshot, 1-6 = historical positions, higher = more lag)
    /// 
    /// Parameters:
    /// - `bucket_id`: The bucket the replica is syncing
    /// - `roots`: Array of optional MMR roots [current, pos0, pos1, pos2, pos3, pos4, pos5].
    ///   Replica sets Some(root) for positions they have, None for positions they don't.
    /// - `signature`: Provider signature over the roots array
    #[pallet::weight(...)]
    pub fn confirm_replica_sync(
        origin: OriginFor<T>,
        bucket_id: BucketId,
        /// Array of optional MMR roots: [current, pos0, pos1, pos2, pos3, pos4, pos5]
        /// Provider signs this to attest which roots they have.
        roots: [Option<H256>; 7],
        signature: Signature,
    ) -> DispatchResult;

    /// Top up a replica's sync balance (agreement owner or anyone).
    /// 
    /// Adds funds to the replica's sync_balance for future sync payments.
    /// This is permissionless because it only benefits the replica (more funds
    /// to pay for syncs) and the bucket (more redundancy).
    #[pallet::weight(...)]
    pub fn top_up_replica_sync_balance(
        origin: OriginFor<T>,
        bucket_id: BucketId,
        provider: T::AccountId,
        amount: BalanceOf<T>,
    ) -> DispatchResult;

    /// Provider responds to challenge with proof.
    /// 
    /// Must provide the challenged chunk with Merkle proofs, or prove the data
    /// was legitimately deleted (newer commitment with higher start_seq), or
    /// show the challenged state has been superseded by canonical.
    /// 
    /// Parameters:
    /// - `challenge_id`: The challenge to respond to (deadline + index)
    /// - `response`: Proof, Deleted, or Superseded response
    #[pallet::weight(...)]
    pub fn respond_to_challenge(
        origin: OriginFor<T>,
        challenge_id: ChallengeId<T>,
        response: ChallengeResponse<T>,
    ) -> DispatchResult;
}

pub enum EndAction {
    /// Pay provider in full
    Pay,
    /// Burn locked payment entirely. 
    /// Additionally deducts `T::BurnPremium` (e.g., 10%) from caller's free balance.
    /// Fails if caller has insufficient funds for the premium.
    Burn,
}

pub enum RemovalReason {
    /// Provider was slashed for failing a challenge
    Slashed,
    /// Admin terminated agreement early
    AdminTerminated,
    /// Agreement expired naturally
    Expired,
}

pub enum ChallengeResponse<T: Config> {
    /// Provide the chunk with proofs
    Proof {
        chunk_data: BoundedVec<u8, T::MaxChunkSize>,
        mmr_proof: MmrProof,
        chunk_proof: MerkleProof,
    },
    /// Data was deleted - show newer commitment without this seq.
    /// Admin signature proves the admin authorized the deletion (new MMR excludes the challenged data).
    /// Only admins can delete data (by increasing start_seq), so the signature must be from an admin.
    /// Provider signature not needed - they're submitting this response.
    Deleted {
        new_mmr_root: H256,
        new_start_seq: u64,
        admin: T::AccountId,
        admin_signature: Signature,
    },
    /// Challenged state has been superseded by a larger canonical checkpoint.
    /// Valid when: canonical.start_seq <= challenged_seq < canonical.start_seq + canonical.leaf_count
    /// (The leaf exists in canonical - challenger should challenge the snapshot instead)
    /// (For challenged_seq < canonical.start_seq, use Deleted response instead)
    /// (For challenged_seq >= canonical_end, provider is liable - must use Proof)
    Superseded,
}
```

---

## Off-Chain: Provider Node API

### Content-Addressed Storage

Everything is content-addressed by hash. Upload is bottom-up: children must exist before parent.

```
Upload Node (chunk or internal node)
────────────────────────────────────
PUT /node

Request:
{
  "bucket_id": "0x1234...",
  "hash": "0xabc...",
  "data": "<base64 encoded>",
  "children": ["0xchild1...", "0xchild2..."] | null  // null for leaf chunks
}

Note: HTTP API is used for simplicity and firewall-friendliness. Binary protocols
(e.g., libp2p streams) could be added later for efficiency. Base64 encoding adds
~33% overhead but keeps the API JSON-friendly. For high-throughput scenarios,
consider a binary endpoint or chunked transfer encoding.

Response (200 OK):
{ "stored": true }

Response (400 Bad Request):
{ "error": "children_missing", "missing": ["0xchild2..."] }

Response (507 Insufficient Storage):
{ "error": "quota_exceeded", "used": 1000000, "max": 1000000 }
```

### Sync Protocol

Client discovers which nodes are missing before uploading.

```
Check Existence (batched)
─────────────────────────
POST /exists

Request:
{
  "bucket_id": "0x1234...",
  "hashes": ["0xabc...", "0xdef...", "0x123...", ...]
}

Response:
{
  "exists": ["0xabc...", "0x123..."],
  "missing": ["0xdef..."]
}

Note: Client traverses tree top-down, checking level by level.
If a node exists, skip its subtree. Upload missing nodes bottom-up.
```

### Commit

After uploading, client requests provider to add data_root(s) to MMR.

```
Commit
──────
POST /commit

Request:
{
  "bucket_id": "0x1234...",
  "data_roots": ["0xroot1...", "0xroot2..."]  // roots to add to MMR
}

Response (200 OK):
{
  "mmr_root": "0xfed...",
  "start_seq": 0,
  "leaf_indices": [5, 6],  // indices assigned to each data_root
  "provider_signature": "0x..."
}

Response (400 Bad Request):
{ "error": "root_not_found", "missing": ["0xroot2..."] }
```

### Read

```
Read Chunks
───────────
GET /read?data_root=0x...&offset=0&length=2097152

Response:
{
  "chunks": [
    { "hash": "0xabc...", "data": "<base64>", "proof": [...] },
    ...
  ]
}
```

### Other Endpoints

```
Provider Info
─────────────
GET /info

Response:
{
  "status": "healthy",
  "version": "0.1.0"
}

Note: Provider settings (prices, durations, accepting flags) are intentionally
omitted — the chain is the source of truth. Clients should query the chain via
runtime API for authoritative provider information.

Download Node
─────────────
GET /node?hash=0x...

Response (200 OK):
{
  "hash": "0xabc...",
  "data": "<base64 encoded>",
  "children": ["0xchild1...", "0xchild2..."] | null
}

Response (404 Not Found):
{ "error": "not_found" }

Get Commitment
──────────────
GET /commitment?bucket_id=0x...

Response:
{
  "bucket_id": "0x1234...",
  "mmr_root": "0xfed...",
  "start_seq": 0,
  "leaf_count": 42,
  "provider_signature": "0x..."
}

Get MMR Proof
─────────────
GET /mmr_proof?bucket_id=0x...&leaf_index=5

Response:
{
  "leaf": { "data_root": "0x...", "data_size": 2097152, "total_size": 52428800 },
  "proof": { "peaks": [...], "siblings": [...] }
}

Get Chunk Proof
───────────────
GET /chunk_proof?data_root=0x...&chunk_index=3

Response:
{
  "chunk_hash": "0xabc...",
  "proof": { "siblings": [...], "path": [...] }
}

Response (404 Not Found):
{ "error": "data_root_not_found" }

Delete Data (admin only)
────────────────────────
POST /delete

Request:
{
  "bucket_id": "0x1234...",
  "new_start_seq": 10,
  "admin_signature": "0x..."  // signs {bucket_id, new_start_seq}
}

Response (200 OK):
{
  "mmr_root": "0xnew...",
  "start_seq": 10,
  "leaf_count": 5,
  "provider_signature": "0x..."
}

Response (400 Bad Request):
{ "error": "invalid_signature" }

Response (403 Forbidden):
{ "error": "not_admin" }

Note: Only bucket admins can delete data. This triggers deletion of data before
new_start_seq. Provider returns new commitment covering remaining data. Admin
signature authorizes the deletion and serves as proof if challenged later.

List Buckets
────────────
GET /buckets

Response:
{
  "buckets": [
    { "bucket_id": "0x1234...", "mmr_root": "0x...", "start_seq": 0, "leaf_count": 42 },
    { "bucket_id": "0x5678...", "mmr_root": "0x...", "start_seq": 5, "leaf_count": 10 }
  ]
}

Health Check
────────────
GET /health

Response (200 OK):
{ "status": "healthy", "version": "0.1.0" }
```

### Replica Sync API

Replicas sync data autonomously from primaries or other replicas using a
top-down Merkle traversal. This section describes the sync protocol.

**Sync flow overview:**

1. Replica queries the **chain** for current bucket state (MMR root from checkpoint)
2. Replica fetches MMR structure (peaks) from any provider, verifying against chain root
3. Replica performs top-down traversal, checking which nodes it already has
4. Replica fetches missing nodes from providers, verifying hashes along the way
5. Once fully synced, replica confirms on-chain to receive per-sync payment

**Why chain-first?**

The chain checkpoint is the source of truth. Fetching the root from a provider
would require trusting that provider. By getting the root from the chain first,
the replica can verify all fetched data against a trusted commitment.

```
Get MMR Peaks (given trusted root from chain)
─────────────────────────────────────────────
GET /mmr_peaks?bucket_id=0x...

Response:
{
  "bucket_id": "0x1234...",
  "mmr_root": "0xfed...",
  "peaks": ["0xpeak1...", "0xpeak2...", ...]
}

Note: Replica already knows the trusted mmr_root from the chain. It fetches
peaks from a provider and verifies: hash(peaks) == trusted_root. If verification
fails, try another provider. Once verified, use peaks to start top-down traversal.

Get MMR Subtree
───────────────
GET /mmr_subtree?bucket_id=0x...&peak_index=0&depth=2

Request: Fetch nodes in an MMR subtree starting from a peak.
- peak_index: which peak to start from (0 = leftmost)
- depth: how many levels to fetch (0 = just the peak, 1 = peak + children, etc.)

Response:
{
  "nodes": [
    { "position": 0, "hash": "0xabc...", "children": [1, 2] },
    { "position": 1, "hash": "0xdef...", "children": [3, 4] },
    { "position": 2, "hash": "0x123...", "children": [5, 6] },
    ...
  ]
}

Note: Replica can batch requests by depth level. Check which hashes match
locally stored nodes, then fetch children of missing nodes.

Note: To check which nodes exist on a provider, use the existing POST /exists
endpoint from the Sync Protocol section above.

Fetch Nodes (batched, for sync)
───────────────────────────────
POST /fetch_nodes

Request:
{
  "bucket_id": "0x1234...",
  "hashes": ["0xdef...", "0x456...", ...]
}

Response:
{
  "nodes": [
    { "hash": "0xdef...", "data": "<base64>", "children": ["0xchild1...", "0xchild2..."] },
    { "hash": "0x456...", "data": "<base64>", "children": null }  // leaf chunk
  ]
}

Note: Bulk fetch of nodes by hash. More efficient than individual GET /node
requests when syncing many nodes.
```

**Top-down sync algorithm:**

```
1. Query chain for bucket's current snapshot (mmr_root, start_seq, leaf_count)
   Also note historical_roots for fallback positions
2. Fetch mmr_peaks from any provider
3. Verify: hash(peaks) == trusted mmr_root from chain
   If mismatch, try another provider
4. Compare verified peaks with locally stored peaks
5. For each differing peak:
   a. Fetch subtree level by level (breadth-first)
   b. At each level, check which nodes exist locally
   c. Fetch missing nodes from any available provider
   d. Verify fetched nodes: hash(data) == expected_hash
   e. Continue to children of newly fetched nodes
6. Once all nodes fetched and verified:
   a. Build signature over roots array matching on-chain historical_roots
   b. Submit confirm_replica_sync on-chain
   c. Receive per-sync payment from sync_balance
```

**Why top-down?**

- Enables early termination: if a node hash matches, skip entire subtree
- Natural deduplication: unchanged subtrees are detected at first node
- Verifiable: each node's hash is verified before fetching children
- Resumable: sync state is just "which nodes are missing"

**Historical roots for sync confirmation:**

When confirming sync on-chain, replicas provide roots for multiple positions:
- Position 0: current snapshot root
- Positions 1-6: historical roots at prime intervals (3, 7, 11, 23, 47, 113 blocks)

This gives replicas a ~1 minute window to sync without racing against new
checkpoints. If a new checkpoint arrives while syncing, the replica can still
confirm using an older historical root they successfully synced to.

---

## Data Structures

### Signed Commitment

```rust
pub struct SignedCommitment {
    pub payload: CommitmentPayload,
    pub client_signature: Signature,
    pub provider_signature: Signature,
}

pub struct CommitmentPayload {
    /// Protocol version for future compatibility
    pub version: u8,
    /// Reference to on-chain contract, or None for best-effort
    pub bucket_id: Option<BucketId>,
    /// Root of MMR containing all data_roots
    pub mmr_root: H256,
    /// Sequence number of first leaf in this MMR
    pub start_seq: u64,
    /// Number of leaves in this MMR
    pub leaf_count: u64,
}
// Canonical range is [start_seq, start_seq + leaf_count)
// Version field enables protocol evolution without breaking existing signatures
```

### MMR Leaf

```rust
pub struct MmrLeaf {
    /// Merkle root of chunk tree
    pub data_root: H256,
    /// Size of content under this data_root
    pub data_size: u64,
    /// Cumulative unique bytes in MMR at this point
    pub total_size: u64,
}
// Sequence number is implicit: start_seq + leaf_position
```

### Merkle Proofs

```rust
pub struct MerkleProof {
    /// Sibling hashes from leaf to root
    pub siblings: Vec<H256>,
    /// Path bits (0 = left, 1 = right)
    pub path: Vec<bool>,
}

pub struct MmrProof {
    /// Peaks of the MMR
    pub peaks: Vec<H256>,
    /// Proof from leaf to peak
    pub leaf_proof: MerkleProof,
}
```

---

## Challenge Protocol

### Timeline

```
1. Client initiates challenge on-chain
   └─ Provides: signed commitment, leaf_index, chunk_index
   └─ Locks 100% of estimated challenge cost as deposit (margin for price fluctuations)

2. Challenge window opens (1-2 days)
   └─ Provider must respond within window
   └─ Cost split calculated based on response time (in blocks)

3a. Provider responds with valid proof
    └─ Challenge rejected
    └─ Base cost split: 75% client / 25% provider (from stake)
    └─ Dynamic adjustment based on response time:
       • Fast response → provider pays less (e.g., 15%), client refunded more
       • Slow response → provider pays more (e.g., 50%), client refunded less
    └─ Client's deposit: pays their share, remainder refunded
    └─ Client recovers data via the on-chain proof

3b. Provider responds with deletion proof
    └─ Shows newer admin-signed commitment with start_seq > challenged seq
    └─ Challenge rejected
    └─ Challenger loses deposit (invalid challenge)

3c. Provider fails to respond / invalid proof
    └─ Provider's contract stake fully slashed
    └─ Challenger refunded deposit + compensation from slash
    └─ Clear on-chain evidence of provider fault
```

**Why this cost split?**
- Provider always pays *something* when challenged (deterrent for ignoring off-chain requests)
- Attacker pays more than victim in base case (griefing is expensive)
- Fast responses are rewarded, slow responses penalized
- The on-chain path is expensive for both parties, incentivizing off-chain resolution

### Verification

```rust
fn verify_challenge_response(
    challenge: &Challenge,
    response: &ChallengeResponse,
    bucket: &Bucket,
) -> Result<(), Error> {
    match response {
        ChallengeResponse::Proof { chunk_data, mmr_proof, chunk_proof } => {
            // 1. Verify chunk hash
            let chunk_hash = blake2_256(chunk_data);
            
            // 2. Verify chunk is in data_root
            verify_merkle_proof(chunk_hash, challenge.chunk_index, chunk_proof, &mmr_proof.leaf.data_root)?;
            
            // 3. Verify data_root is in MMR
            verify_mmr_proof(&mmr_proof, challenge.leaf_index, &challenge.mmr_root)?;
            
            Ok(())
        }
        
        ChallengeResponse::Deleted { new_start_seq, admin, admin_signature, .. } => {
            // Note: We don't check frozen_start_seq here. Freeze protects canonical
            // checkpoints (enforced at checkpoint time), but off-chain deletions can
            // race with freeze. If admin signed a deletion, provider has valid defense
            // regardless of freeze state. Off-chain is "messy but functional."
            
            // Challenged seq must be before new start
            let challenged_seq = challenge.start_seq + challenge.leaf_index;
            ensure!(challenged_seq < *new_start_seq, Error::InvalidDeletionProof);
            
            // Verify admin signature on new commitment and that signer is bucket admin
            // ...
            
            Ok(())
        }
        
        ChallengeResponse::Superseded => {
            // Provider can defend if challenged state has been superseded by canonical.
            //
            // This defense covers three cases:
            // 1. Same data: challenged leaf exists in canonical with same content
            // 2. Forked data: challenged leaf was on a conflicting branch that lost
            // 3. Deleted data: canonical has moved past via deletion (start_seq increased)
            //
            // In all cases, canonical has "moved past" the challenged state. The provider
            // signed something that is no longer relevant - canonical supersedes it.
            //
            // Note: We don't require admin signature here (unlike Deleted defense).
            // Superseded is for when canonical evolved independently - possibly by a
            // different admin/provider. The provider shouldn't be slashed for state
            // that was superseded by canonical they weren't involved in.
            //
            // Deleted vs Superseded:
            // - Deleted: requires admin signature, works without canonical snapshot
            // - Superseded: requires canonical snapshot, works without admin signature
            // For challenged_seq < snapshot.start_seq, BOTH defenses are valid.
            // Provider can use whichever they have evidence for.
            //
            // Provider IS liable when challenged_seq >= canonical_end: they signed
            // something that extends BEYOND canonical, so they must Proof it.
            
            let snapshot = bucket.snapshot.as_ref().ok_or(Error::NoSnapshot)?;
            let challenged_seq = challenge.start_seq + challenge.leaf_index;
            let canonical_end = snapshot.start_seq + snapshot.leaf_count;
            
            // Superseded is valid if canonical has moved past challenged state:
            // - challenged_seq < snapshot.start_seq: canonical deleted past this
            // - challenged_seq < canonical_end: within canonical range
            // NOT valid if challenged_seq >= canonical_end: provider is liable
            ensure!(challenged_seq < canonical_end, Error::LeafBeyondCanonical);
            
            Ok(())
        }
    }
}
```

---

## Open Questions

1. **Chunk size**: Fixed (256KB) or configurable?

2. **MMR implementation**: Use `pallet-mmr` or custom?

3. **Signature scheme**: Ed25519, Sr25519, or both?
