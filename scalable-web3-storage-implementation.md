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
1. Client calls `request_agreement` with the provider
2. Provider calls `accept_agreement` → `StorageAgreement` created
3. Client uploads data to provider
4. Client requests commit, provider signs → client has provider signature
5. Client calls `checkpoint` with provider signature → provider added to `snapshot.providers` bitfield

**Binding contract:**

Once accepted, agreements are binding for both parties until expiry:
- **No early exit for providers**: Providers cannot voluntarily leave. They committed to store data for the agreed duration.
- **No early cancellation for clients**: Clients cannot cancel and reclaim locked payment. They committed to pay for the agreed duration.
- **Provider's protection**: Before accepting, providers can set `max_duration` and review the terms. They can also block future extensions via `set_extensions_blocked`.
- **Client's protection**: Clients can challenge if provider loses data (slashing). At settlement, clients can burn payment to signal poor service.

**Agreement expiry:**

When `expires_at` is reached:
1. Provider calls `claim_expired_agreement` to receive payment, OR
2. Client calls `end_agreement` with pay/burn decision within settlement window
3. Provider is no longer bound to store data
4. Provider won't be included in future checkpoints

**Snapshot liability**: Providers remain liable for snapshots they signed until those snapshots are superseded by a new checkpoint that doesn't include them, or until the bucket's canonical depth grows past the data they signed for.

### Multi-Provider Coordination

Providers don't sync with each other. Clients are responsible for uploading to each provider they want to store their data.

**Flow**:
1. Client uploads data to Provider A, B, C (separately)
2. Client triggers commit on each provider, collects signatures
3. Client checkpoints on-chain with collected signatures
4. Providers not in the snapshot should sync (client re-uploads)
5. After checkpoint, providers can prune non-canonical roots

**Liability**: A provider is only liable for MMR states they acknowledged (signed). Challenges against the canonical checkpoint only work for providers listed in the snapshot's provider bitfield.

---

## On-Chain: Pallet Interface

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
}

pub struct ProviderSettings<T: Config> {
    /// Minimum agreement duration provider will accept
    pub min_duration: BlockNumberFor<T>,
    /// Maximum agreement duration provider will accept
    pub max_duration: BlockNumberFor<T>,
    /// Price per byte per block
    pub price_per_byte: BalanceOf<T>,
    /// Whether accepting new agreements
    pub accepting_new_agreements: bool,
    /// Whether accepting extensions on existing agreements
    pub accepting_extensions: bool,
}

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
    /// If Some, bucket is append-only from this start_seq
    /// Checkpoints with start_seq < frozen_start_seq are rejected
    pub frozen_start_seq: Option<u64>,
    /// Minimum provider acknowledgments required for checkpoint
    pub min_providers: u32,
    /// Current canonical state
    pub snapshot: Option<BucketSnapshot>,
}

pub struct BucketSnapshot {
    /// Canonical MMR root
    pub mmr_root: H256,
    /// Start sequence number
    pub start_seq: u64,
    /// Number of leaves in the MMR
    pub leaf_count: u64,
    /// Block at which checkpointed
    pub checkpoint_block: BlockNumberFor<T>,
    /// Which providers acknowledged this root (bitfield)
    pub providers: BoundedBitField<T::MaxProvidersPerBucket>,
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
    /// Maximum bytes (quota) — provider accepts uploads up to this
    pub max_bytes: u64,
    /// Payment locked by client
    pub payment_locked: BalanceOf<T>,
    /// Agreement expiration
    pub expires_at: BlockNumberFor<T>,
    /// Whether provider has blocked extensions for this specific agreement
    pub extensions_blocked: bool,
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
}

/// Pending challenges indexed by deadline block
#[pallet::storage]
pub type Challenges<T: Config> = StorageMap<
    _,
    Blake2_128Concat,
    BlockNumberFor<T>,
    BoundedVec<Challenge<T>, T::MaxChallengesPerBlock>,
>;

pub struct ChallengeId {
    pub deadline: BlockNumberFor<T>,
    pub index: u16,
}

pub struct Challenge<T: Config> {
    pub bucket_id: BucketId,
    pub provider: T::AccountId,
    pub challenger: T::AccountId,
    pub mmr_root: H256,
    pub start_seq: u64,
    pub leaf_index: u64,
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

    // ─────────────────────────────────────────────────────────────
    // Agreement events
    // ─────────────────────────────────────────────────────────────
    
    AgreementRequested {
        bucket_id: BucketId,
        provider: T::AccountId,
        requester: T::AccountId,
        max_bytes: u64,
        payment: BalanceOf<T>,
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
        challenge_id: ChallengeId,
        bucket_id: BucketId,
        provider: T::AccountId,
        challenger: T::AccountId,
        respond_by: BlockNumberFor<T>,
    },
    /// Provider responded successfully to a challenge
    ChallengeDefended {
        challenge_id: ChallengeId,
        provider: T::AccountId,
        response_time_blocks: BlockNumberFor<T>,
        challenger_cost: BalanceOf<T>,
        provider_cost: BalanceOf<T>,
    },
    /// Provider failed to respond or provided invalid proof - slashed
    ChallengeSlashed {
        challenge_id: ChallengeId,
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

    pub fn register_provider(
        origin: OriginFor<T>,
        multiaddr: BoundedVec<u8, T::MaxMultiaddrLength>,
        stake: BalanceOf<T>,
    ) -> DispatchResult;

    /// Add stake (stake can only increase; to withdraw, use deregister_provider)
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

    /// Update provider settings (min duration, pricing, whether accepting new agreements)
    #[pallet::weight(...)]
    pub fn update_provider_settings(
        origin: OriginFor<T>,
        settings: ProviderSettings,
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

    /// Create bucket (caller becomes admin)
    pub fn create_bucket(origin: OriginFor<T>, min_providers: u32) -> DispatchResult;

    /// Set minimum providers for checkpoint (admin only)
    pub fn set_min_providers(
        origin: OriginFor<T>,
        bucket_id: BucketId,
        min_providers: u32,
    ) -> DispatchResult;

    /// Freeze bucket — make append-only (admin only, irreversible)
    /// Requires snapshot with min_providers acknowledgments
    pub fn freeze_bucket(origin: OriginFor<T>, bucket_id: BucketId) -> DispatchResult;

    pub fn set_member(
        origin: OriginFor<T>,
        bucket_id: BucketId,
        member: T::AccountId,
        role: Role,
    ) -> DispatchResult;

    /// Remove member from bucket (admin only)
    #[pallet::weight(...)]
    pub fn remove_member(
        origin: OriginFor<T>,
        bucket_id: BucketId,
        member: T::AccountId,
    ) -> DispatchResult;



    // ─────────────────────────────────────────────────────────────
    // Storage agreements (per bucket, per provider)
    // ─────────────────────────────────────────────────────────────

    /// Request a storage agreement (client locks payment, waits for provider to accept)
    #[pallet::weight(...)]
    pub fn request_agreement(
        origin: OriginFor<T>,
        bucket_id: BucketId,
        provider: T::AccountId,
        max_bytes: u64,
        payment: BalanceOf<T>,
        duration: BlockNumberFor<T>,
    ) -> DispatchResult;

    /// Accept a pending agreement request (provider only)
    #[pallet::weight(...)]
    pub fn accept_agreement(
        origin: OriginFor<T>,
        bucket_id: BucketId,
    ) -> DispatchResult;

    /// Reject a pending agreement request (provider only, refunds client)
    #[pallet::weight(...)]
    pub fn reject_agreement(
        origin: OriginFor<T>,
        bucket_id: BucketId,
    ) -> DispatchResult;

    /// Withdraw a pending agreement request (client only, before provider accepts)
    #[pallet::weight(...)]
    pub fn withdraw_agreement_request(
        origin: OriginFor<T>,
        bucket_id: BucketId,
        provider: T::AccountId,
    ) -> DispatchResult;

    /// Top up payment for an existing agreement (anyone can pay).
    /// Adds funds to the agreement, does not change duration.
    #[pallet::weight(...)]
    pub fn top_up_agreement(
        origin: OriginFor<T>,
        bucket_id: BucketId,
        provider: T::AccountId,
        payment: BalanceOf<T>,
    ) -> DispatchResult;

    /// Extend agreement with new terms (immediate, no provider approval needed).
    /// 1. Settles current period: releases payment to provider for elapsed time
    /// 2. Locks new payment
    /// 3. Updates end date to now + duration
    /// 
    /// Fails if:
    /// - Duration below provider's min_duration or above max_duration
    /// - Payment insufficient for duration + bytes
    /// - Provider has globally paused extensions (settings.accepting_extensions == false)
    /// - Provider has blocked extensions for this specific bucket (agreement.extensions_blocked == true)
    #[pallet::weight(...)]
    pub fn extend_agreement(
        origin: OriginFor<T>,
        bucket_id: BucketId,
        provider: T::AccountId,
        payment: BalanceOf<T>,
        duration: BlockNumberFor<T>,
    ) -> DispatchResult;

    /// End agreement: final settlement with pay/burn decision (bucket admin only).
    /// Must be called within T::SettlementTimeout after expiry.
    /// If client doesn't act, provider can call claim_expired_agreement.
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

    // ─────────────────────────────────────────────────────────────
    // Checkpoints
    // ─────────────────────────────────────────────────────────────

    /// Submit checkpoint with provider signatures.
    /// Requires at least min_providers signatures.
    /// For frozen buckets: start_seq must equal frozen_start_seq (only leaf_count can increase).
    pub fn checkpoint(
        origin: OriginFor<T>,
        bucket_id: BucketId,
        mmr_root: H256,
        start_seq: u64,
        leaf_count: u64,
        signatures: BoundedVec<(T::AccountId, Signature), T::MaxProvidersPerBucket>,
    ) -> DispatchResult;

    // ─────────────────────────────────────────────────────────────
    // Challenges
    // ─────────────────────────────────────────────────────────────

    /// Challenge on-chain checkpoint (no signatures needed)
    /// Provider must be in snapshot's provider bitfield
    /// NOTE: Requires bucket to have a snapshot. Fails if snapshot is None.
    pub fn challenge_checkpoint(
        origin: OriginFor<T>,
        bucket_id: BucketId,
        provider: T::AccountId,
        leaf_index: u64,
        chunk_index: u64,
    ) -> DispatchResult;

    /// Challenge off-chain commitment (requires provider signature)
    /// Works regardless of whether bucket has an on-chain snapshot.
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

    /// Provider responds to challenge with proof.
    /// Must provide the challenged chunk with Merkle proofs, or prove the data
    /// was legitimately deleted (newer commitment with higher start_seq).
    #[pallet::weight(...)]
    pub fn respond_to_challenge(
        origin: OriginFor<T>,
        challenge_id: ChallengeId,
        response: ChallengeResponse,
    ) -> DispatchResult;
}

pub enum EndAction {
    /// Pay provider in full
    Pay,
    /// Burn portion, pay rest (0-100%)
    Burn { burn_percent: u8 },
}

pub enum ChallengeResponse {
    /// Provide the chunk with proofs
    Proof {
        chunk_data: BoundedVec<u8, T::MaxChunkSize>,
        mmr_proof: MmrProof,
        chunk_proof: MerkleProof,
    },
    /// Data was deleted - show newer commitment without this seq.
    /// Client signature proves the client agreed to the new MMR (which excludes the challenged data).
    /// Provider signature not needed - they're submitting this response.
    Deleted {
        new_mmr_root: H256,
        new_start_seq: u64,
        client: AccountId,
        client_signature: Signature,
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
  "provider_id": "5GrwvaEF...",
  "multiaddr": "/ip4/...",
  "settings": {
    "min_duration": 100800,
    "max_duration": 2592000,
    "price_per_byte": "1000",
    "accepting_new_agreements": true,
    "accepting_extensions": true
  }
}

Note: Stake and committed_bytes are intentionally omitted — providers could lie.
Clients should query the chain via runtime API for trustworthy stake information.

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

Delete Data
───────────
POST /delete

Request:
{
  "bucket_id": "0x1234...",
  "new_start_seq": 10,
  "client_signature": "0x..."  // signs {bucket_id, new_start_seq}
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

Note: This triggers deletion of data before new_start_seq. Provider returns
new commitment covering remaining data. Client signature authorizes the deletion.

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
    └─ Shows newer client-signed commitment with start_seq > challenged seq
    └─ Challenge rejected
    └─ Client loses deposit (invalid challenge)

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
        
        ChallengeResponse::Deleted { new_start_seq, .. } => {
            // Note: We don't check frozen_start_seq here. Freeze protects canonical
            // checkpoints (enforced at checkpoint time), but off-chain deletions can
            // race with freeze. If client signed a deletion, provider has valid defense
            // regardless of freeze state. Off-chain is "messy but functional."
            
            // Challenged seq must be before new start
            let challenged_seq = challenge.start_seq + challenge.leaf_index;
            ensure!(challenged_seq < *new_start_seq, Error::InvalidDeletionProof);
            
            // Verify client signature on new commitment
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
            // Note: We don't require client signature here (unlike Deleted defense).
            // Superseded is for when canonical evolved independently - possibly by a
            // different client/provider. The provider shouldn't be slashed for state
            // that was superseded by canonical they weren't involved in.
            //
            // Deleted vs Superseded:
            // - Deleted: requires client signature, works without canonical snapshot
            // - Superseded: requires canonical snapshot, works without client signature
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
