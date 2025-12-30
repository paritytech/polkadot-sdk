# Scalable Web3 Storage: Implementation Details

## Overview

This document specifies the on-chain and off-chain interfaces for the storage system described in [Scalable Web3 Storage](./scalable-web3-storage.md).

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
    /// Multiaddr for connecting to this provider (e.g., "/ip4/1.2.3.4/tcp/4001/p2p/Qm...")
    pub multiaddr: BoundedVec<u8, T::MaxMultiaddrLength>,
    /// Total stake locked by this provider
    pub stake: BalanceOf<T>,
    /// Bytes currently committed under storage agreements
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

/// Clients blocked by a provider (cannot extend agreements)
#[pallet::storage]
pub type BlockedClients<T: Config> = StorageDoubleMap<
    _,
    Blake2_128Concat,
    T::AccountId,  // provider
    Blake2_128Concat,
    T::AccountId,  // blocked client
    (),
>;

/// Buckets: named containers that multiple clients can write to
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
    /// Can modify members, delete data, and append
    Admin,
    /// Can only append data
    Writer,
    /// Can only read (for access-controlled content)
    Reader,
}

// Note: To make a bucket "append-only forever", remove all admins.
// No one can delete, only append. Agreement can still be extended by anyone.

pub struct Bucket<T: Config> {
    /// Members who can interact with this bucket
    pub members: BoundedVec<Member<T>, T::MaxMembers>,
}

/// Storage agreements: per-provider contracts for a bucket
#[pallet::storage]
pub type StorageAgreements<T: Config> = StorageDoubleMap<
    _,
    Blake2_128Concat,
    BucketId,
    Blake2_128Concat,
    T::AccountId,                    // provider
    StorageAgreement<T>,
>;

pub struct StorageAgreement<T: Config> {
    /// Maximum bytes covered by this agreement
    pub max_bytes: u64,
    /// Payment locked by client
    pub payment_locked: BalanceOf<T>,
    /// Agreement expiration block
    pub expires_at: BlockNumberFor<T>,
    /// Latest MMR snapshot (updated via checkpoint)
    pub mmr_snapshot: Option<MmrSnapshot>,
}

pub struct MmrSnapshot {
    pub mmr_root: H256,
    pub start_seq: u64,
}

// Note: Provider stake is global (ProviderInfo.stake). The ratio of
// stake to committed_bytes determines the provider's "skin in the game".
// Clients choose providers based on this ratio.

/// Discovery: bucket → providers is derived from StorageAgreements.
/// Clients query by bucket to find which providers store it.
/// 
/// Note: We intentionally don't index by mmr_root. The bucket_id is the
/// stable reference. Specific versions are tracked off-chain via signed
/// commitments.

/// Pending challenges indexed by deadline block.
/// On each block, check Challenges[current_block] for timeouts.
#[pallet::storage]
pub type Challenges<T: Config> = StorageMap<
    _,
    Blake2_128Concat,
    BlockNumberFor<T>,  // deadline block
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

// Note: No provider → challenges index. Providers track locally which block
// they last checked and scan Challenges[last_checked..current] for their account.
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
        // ─────────────────────────────────────────────────────────────
        // Provider queries
        // ─────────────────────────────────────────────────────────────

        /// Get provider info (multiaddr, stake, committed bytes)
        fn provider_info(provider: AccountId) -> Option<ProviderInfo>;

        // ─────────────────────────────────────────────────────────────
        // Bucket queries
        // ─────────────────────────────────────────────────────────────

        /// Get bucket info (members, roles)
        fn bucket_info(bucket_id: BucketId) -> Option<Bucket>;

        /// Get providers for a bucket (derived from active agreements)
        fn bucket_providers(bucket_id: BucketId) -> Vec<AccountId>;

        // ─────────────────────────────────────────────────────────────
        // Agreement queries
        // ─────────────────────────────────────────────────────────────

        /// Get storage agreement details (including MMR snapshot)
        fn agreement(bucket_id: BucketId, provider: AccountId) -> Option<StorageAgreement>;

        /// Get pending agreement requests for a bucket
        fn pending_requests(bucket_id: BucketId) -> Vec<(AccountId, AgreementRequest)>;

        /// Get pending agreement requests awaiting this provider
        fn pending_requests_for_provider(provider: AccountId) -> Vec<(BucketId, AgreementRequest)>;

        // ─────────────────────────────────────────────────────────────
        // Challenge queries
        // ─────────────────────────────────────────────────────────────

        /// Get challenges for a block range (providers scan for their account)
        fn challenges(from_block: BlockNumber, to_block: BlockNumber) -> Vec<(ChallengeId, Challenge)>;

        /// Get challenge period (blocks to respond)
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

    /// Register as a storage provider
    #[pallet::weight(...)]
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

    /// Block a client from extending agreements (provider only)
    #[pallet::weight(...)]
    pub fn block_client(
        origin: OriginFor<T>,
        client: T::AccountId,
    ) -> DispatchResult;

    /// Unblock a previously blocked client (provider only)
    #[pallet::weight(...)]
    pub fn unblock_client(
        origin: OriginFor<T>,
        client: T::AccountId,
    ) -> DispatchResult;

    // ─────────────────────────────────────────────────────────────
    // Bucket management
    // ─────────────────────────────────────────────────────────────

    /// Create a new bucket (caller becomes admin)
    #[pallet::weight(...)]
    pub fn create_bucket(
        origin: OriginFor<T>,
    ) -> DispatchResult;

    /// Add or update member in bucket (admin only)
    #[pallet::weight(...)]
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
    /// - Duration below provider's minimum
    /// - Payment insufficient for duration + bytes
    /// - Provider has blocked this client or paused extensions
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

    /// Checkpoint MMR root for a provider (updates on-chain snapshot)
    #[pallet::weight(...)]
    pub fn checkpoint(
        origin: OriginFor<T>,
        bucket_id: BucketId,
        provider: T::AccountId,
        mmr_root: H256,
        start_seq: u64,
        provider_signature: Signature,
    ) -> DispatchResult;

    // ─────────────────────────────────────────────────────────────
    // Challenges
    //
    // Two ways to challenge:
    // 1. challenge_checkpoint: Challenge the on-chain MMR snapshot. No signatures
    //    needed - the chain already has the committed mmr_root.
    // 2. challenge_offchain: Challenge an off-chain commitment. Requires both
    //    client and provider signatures to prove the commitment exists.
    //
    // In both cases, challenger specifies which leaf (by index) and which chunk
    // within that leaf's data_root to challenge. Provider must respond with the
    // chunk data and Merkle proofs, or get slashed.
    // ─────────────────────────────────────────────────────────────

    /// Challenge the on-chain checkpoint.
    /// Uses the mmr_root stored in the StorageAgreement.
    #[pallet::weight(...)]
    pub fn challenge_checkpoint(
        origin: OriginFor<T>,
        bucket_id: BucketId,
        provider: T::AccountId,
        leaf_index: u64,
        chunk_index: u64,
    ) -> DispatchResult;

    /// Challenge an off-chain commitment.
    /// Provider signature proves they committed to this MMR root.
    /// Client signature not needed - if provider signed, they're on the hook.
    #[pallet::weight(...)]
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
    PartialBurn { burn_percent: u8 },
    /// Burn everything
    FullBurn,
}

pub enum ChallengeResponse {
    /// Provide the chunk with Merkle proof
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
        client_signature: Signature,
    },
}
```

---

## Off-Chain: Provider Node API

### REST/WebSocket Endpoints

```
Provider Metadata
─────────────────
GET /info

Response:
{
  "provider_id": "5GrwvaEF...",
  "endpoint": "wss://storage.example.com",
  "stake": "1000000000000",
  "terms": {
    "free_storage": {
      "max_bytes": 1073741824,
      "max_duration_hours": 24,
      "per_peer": true
    },
    "paid_storage": {
      "price_per_gb_month_planck": 10000000000
    },
    "contract_required": false,
    "min_stake_ratio": 0.001
  }
}
```

```
Write Chunks
────────────
POST /write

Request:
{
  "bucket_id": "0x1234..." | null,   // Option<BucketId>
  "chunks": [
    { "hash": "0xabc...", "data": "<base64>" },
    { "hash": "0xdef...", "data": "<base64>" }
  ]
}

Response:
{
  "data_root": "0x789...",
  "mmr_root": "0xfed...",
  "start_seq": 0,
  "leaf_index": 5,
  "data_size": 2097152,
  "total_size": 52428800,
  "provider_signature": "0x..."
}
```

```
Read Chunks
───────────
GET /read?data_root=0x...&offset=0&length=2097152

Response:
{
  "chunks": [
    { "hash": "0xabc...", "data": "<base64>", "proof": [...] },
    { "hash": "0xdef...", "data": "<base64>", "proof": [...] }
  ],
  "data_root": "0x789..."
}
```

```
Get Commitment
──────────────
GET /commitment?bucket_id=0x...

Response:
{
  "bucket_id": "0x1234...",
  "mmr_root": "0xfed...",
  "start_seq": 0,
  "client_signature": "0x...",
  "provider_signature": "0x..."
}
```

```
Get MMR Proof
─────────────
GET /mmr_proof?bucket_id=0x...&leaf_index=5

Response:
{
  "leaf": {
    "data_root": "0x789...",
    "data_size": 2097152,
    "total_size": 52428800
  },
  "proof": [...]
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

2. Challenge window opens (e.g., 100 blocks)
   └─ Provider must respond within window

3a. Provider responds with valid proof
    └─ Challenge rejected
    └─ Client loses deposit (pays provider)

3b. Provider responds with deletion proof
    └─ Shows newer commitment with start_seq > challenged seq
    └─ Challenge rejected
    └─ Client loses deposit

3c. Provider fails to respond / invalid proof
    └─ Provider slashed
    └─ Client receives: deposit back + portion of slash
```

### Verification

```rust
fn verify_challenge_response(
    challenge: &Challenge,
    response: &ChallengeResponse,
) -> Result<(), Error> {
    match response {
        ChallengeResponse::Proof { chunk_data, mmr_proof, chunk_proof } => {
            // 1. Verify chunk hash
            let chunk_hash = blake2_256(chunk_data);
            
            // 2. Verify chunk is in data_root
            verify_merkle_proof(
                chunk_hash,
                challenge.chunk_index,
                &chunk_proof,
                &mmr_proof.leaf.data_root,
            )?;
            
            // 3. Verify data_root is in MMR
            let leaf_hash = hash_mmr_leaf(&mmr_proof.leaf);
            verify_mmr_proof(
                leaf_hash,
                challenge.leaf_index,
                &mmr_proof,
                &challenge.mmr_root,
            )?;
            
            Ok(())
        }
        
        ChallengeResponse::Deleted { new_mmr_root, new_start_seq, .. } => {
            // Verify the challenged seq is before new MMR's range
            let challenged_seq = challenge.start_seq + challenge.leaf_index;
            ensure!(challenged_seq < *new_start_seq, Error::InvalidDeletionProof);
            
            // Verify signatures on new commitment
            verify_signatures(new_mmr_root, new_start_seq, ...)?;
            
            Ok(())
        }
    }
}
```

---

## Client Library Interface

```rust
pub trait StorageClient {
    /// Write data to provider, optionally under a contract
    async fn write(
        &self,
        provider: &ProviderEndpoint,
        bucket_id: Option<BucketId>,
        data: &[u8],
    ) -> Result<WriteReceipt, Error>;

    /// Read data by root and byte range
    async fn read(
        &self,
        provider: &ProviderEndpoint,
        data_root: H256,
        offset: u64,
        length: u64,
    ) -> Result<Vec<u8>, Error>;

    /// Get current commitment for a bucket
    async fn get_commitment(
        &self,
        provider: &ProviderEndpoint,
        bucket_id: BucketId,
    ) -> Result<SignedCommitment, Error>;

    /// Initiate a challenge on-chain
    async fn challenge(
        &self,
        bucket_id: BucketId,
        provider: AccountId,
        commitment: SignedCommitment,
        leaf_index: u64,
        chunk_index: u64,
    ) -> Result<ChallengeId, Error>;
}

pub struct WriteReceipt {
    pub data_root: H256,
    pub commitment: SignedCommitment,
    pub leaf_index: u64,
}
```

---

## Open Implementation Questions

1. **Chunk size:** Fixed (e.g., 256KB) or configurable per contract?

2. **MMR implementation:** Use existing `pallet-mmr` or custom implementation for off-chain use?

3. **Signature scheme:** Ed25519, Sr25519, or support both?

4. **Challenge randomness:** How to select random chunk for challenge? On-chain randomness (BABE/VRF) or client-provided with commitment?

5. **Batch operations:** Support for writing multiple data_roots in single request?

6. **Streaming:** WebSocket streaming for large reads/writes, or chunked HTTP?

7. **Encryption:** Any protocol-level support, or purely application layer?
