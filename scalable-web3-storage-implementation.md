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

**No conflict rejection**: Providers accept all uploads within quota. "Conflicts" (different clients uploading different data) are fine — the checkpoint determines which state becomes canonical. After checkpoint, providers can prune non-canonical data to reclaim storage.

**Content-addressed storage**: Everything (chunks and internal nodes) is addressed by hash. Internal nodes are data whose content is child hashes. Upload is bottom-up: children must exist before parent can be stored. If a root hash exists, the entire tree is guaranteed complete.

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
    /// Block at which checkpointed
    pub checkpoint_block: BlockNumberFor<T>,
    /// Which providers acknowledged this root (bitfield)
    pub providers: BoundedBitField<T::MaxProvidersPerBucket>,
}

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
    },
    /// Provider failed to respond or provided invalid proof - slashed
    ChallengeSlashed {
        challenge_id: ChallengeId,
        provider: T::AccountId,
        slashed_amount: BalanceOf<T>,
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

    /// Add stake (stake can only increase; to withdraw, deregister when committed_bytes == 0)
    #[pallet::weight(...)]
    pub fn add_stake(
        origin: OriginFor<T>,
        amount: BalanceOf<T>,
    ) -> DispatchResult;

    /// Update provider settings (min duration, pricing, whether accepting new agreements)
    #[pallet::weight(...)]
    pub fn update_provider_settings(
        origin: OriginFor<T>,
        settings: ProviderSettings,
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
    /// - Duration below provider's minimum: TODO: Why minimum? The provider much rather will have some maximum.
    /// - Payment insufficient for duration + bytes
    /// - Provider has blocked this bucket or paused extensions
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
    /// For frozen buckets: start_seq == frozen_start_seq.
    pub fn checkpoint(
        origin: OriginFor<T>,
        bucket_id: BucketId,
        mmr_root: H256,
        start_seq: u64,
        signatures: BoundedVec<(T::AccountId, Signature), T::MaxProvidersPerBucket>,
    ) -> DispatchResult;

    // ─────────────────────────────────────────────────────────────
    // Challenges
    // ─────────────────────────────────────────────────────────────

    /// Challenge on-chain checkpoint (no signatures needed)
    /// Provider must be in snapshot's provider bitfield
    pub fn challenge_checkpoint(
        origin: OriginFor<T>,
        bucket_id: BucketId,
        provider: T::AccountId,
        leaf_index: u64,
        chunk_index: u64,
    ) -> DispatchResult;

    /// Challenge off-chain commitment (requires provider signature)
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
    /// Challenged state is non-canonical
    NonCanonical {
    // TODO: What is this block number? Why is it needed?
        checkpoint_block: BlockNumberFor<T>,
    },
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
  TODO: base64? Also why a http API?
  "data": "<base64>",
  "children": ["0xchild1...", "0xchild2..."] | null  // null for leaf chunks
}

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
  // TODO: Stake and committed bytes makes little sense to fetch from the provider - they could lie. We should query the chain for that.
  "stake": "1000000000000",
  "committed_bytes": "50000000000",
  "settings": { ... }
}

Get Commitment
──────────────
GET /commitment?bucket_id=0x...

Response:
{
  "bucket_id": "0x1234...",
  "mmr_root": "0xfed...",
  "start_seq": 0,
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
    /// Reference to on-chain contract, or None for best-effort
    pub bucket_id: Option<BucketId>,
    /// Root of MMR containing all data_roots
    pub mmr_root: H256,
    /// Sequence number of first leaf in this MMR
    pub start_seq: u64,
}
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
   └─ Locks challenge deposit

2. Challenge window opens
   └─ Provider must respond within window

3a. Provider responds with valid proof
    └─ Challenge rejected
    └─ Client loses deposit (pays provider): TODO: This is wrong. Provider does not get paid, only fees for the submitted proof are paid with the agreed fraction.

3b. Provider responds with deletion proof
    └─ Shows newer commitment with start_seq > challenged seq
    └─ Challenge rejected
    └─ Client loses deposit

3c. Provider fails to respond / invalid proof
    └─ Provider slashed
    └─ Challenger gets deposit back + slash portion
```

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
            // Not allowed for frozen buckets
            if bucket.frozen_start_seq.is_some() {
                return Err(Error::BucketIsFrozen);
            }
            
            // Challenged seq must be before new start
            let challenged_seq = challenge.start_seq + challenge.leaf_index;
            ensure!(challenged_seq < *new_start_seq, Error::InvalidDeletionProof);
            
            // Verify client signature on new commitment
            // ...
            
            Ok(())
        }
        
        ChallengeResponse::NonCanonical { checkpoint_block } => {
            // Verify checkpoint exists and differs from challenged state
            let snapshot = bucket.snapshot.as_ref().ok_or(Error::NoSnapshot)?;
            // TODO: This check seems unnecessary
            ensure!(snapshot.checkpoint_block == *checkpoint_block, Error::InvalidCheckpoint);
            // TODO: This check does not seem sufficient.
            ensure!(snapshot.mmr_root != challenge.mmr_root, Error::StateIsCanonical);
            
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

4. **Challenge randomness**: On-chain (BABE/VRF) or client-provided?

5. **Encryption**: Protocol-level or application-layer only?
