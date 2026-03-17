# Speculative Messaging: Primitives Crate

**Location:** `polkadot/primitives/speculative-messaging/`

**Crate:** `polkadot-primitives-speculative-messaging`

This crate defines all shared types, proof structures, and traits used across the sender runtime, receiver runtime, relay chain, PVF, and collator networking layers.

## Dependencies

- `codec` (parity-scale-codec) — SCALE encoding/decoding
- `scale-info` — Type metadata for codec
- `serde` (optional) — JSON serialization
- `sp-core` — H256 hashing, blake2_256
- `sp-runtime` — Runtime primitives
- `polkadot-parachain-primitives` — ParaId type

## Module Structure

```
src/
  lib.rs            — Module declarations, SpeculativeMessagingProvider trait
  error.rs          — SpeculativeMessagingError enum
  commitments.rs    — ProvidesCommitment, RequiresCommitment, CommitmentPair
  state.rs          — SourceState, IncomingMessageState, OutgoingMessageState
  merkle_tree.rs    — DestinationMerkleTree, StoredMerkleTree, MerkleProof
  messages.rs       — OutgoingMessage, MessageBatch
  proofs.rs         — LateBlockProof, MmrExtensionProof, merge_mmr_nodes, bag_peaks
```

## SpeculativeMessagingProvider Trait

```rust
pub trait SpeculativeMessagingProvider {
    fn provides_root() -> Option<H256>;
    fn requires_commitments() -> Vec<RequiresCommitment>;
}

// No-op implementation for runtimes without speculative messaging
impl SpeculativeMessagingProvider for () {
    fn provides_root() -> Option<H256> { None }
    fn requires_commitments() -> Vec<RequiresCommitment> { Vec::new() }
}
```

Called by `pallet-parachain-system` during `on_finalize` to collect commitments for `ValidationResult`.

---

## Error Types (`error.rs`)

```rust
pub enum SpeculativeMessagingError {
    InvalidMerkleProof,         // Top-level Merkle proof verification failed
    InvalidMmrExtensionProof,   // MMR extension proof is invalid
    RootMismatch,               // Computed root doesn't match expected
    SubtreeRootMismatch,        // Subtree root doesn't match
    MissingSubtreeExtension,    // Subtree changed but no extension proof provided
    EmptyTree,                  // Operation on empty tree
    DestinationNotFound,        // Destination ParaId not in Merkle tree
    InvalidMessagePosition,     // Position out of bounds or non-sequential
    EmptyBatch,                 // Message batch contains no messages
    DuplicateDestination,       // Duplicate ParaId found
    UnconsumedProofData,        // Proof contains unconsumed data
    SourceMismatch,             // Source ParaId doesn't match expected
    TooManyDestinations,        // Too many destinations for u32-indexed tree
}
```

---

## Commitments (`commitments.rs`)

### ProvidesCommitment

```rust
pub struct ProvidesCommitment {
    pub root: H256,  // Top-level Merkle root over all per-destination MMR roots
}
```

**Methods:**
- `is_empty() -> bool` — true if root is `H256::zero()`

### RequiresCommitment

```rust
pub struct RequiresCommitment {
    pub source: ParaId,        // Source parachain
    pub expected_root: H256,   // Provides root we built state transition against
}
```

**Methods:**
- `matches_provides(source: ParaId, provides: &ProvidesCommitment) -> bool` — validates both source and root match

### CommitmentPair

```rust
pub struct CommitmentPair {
    pub provides: ProvidesCommitment,
    pub requires: Vec<RequiresCommitment>,
}
```

**Methods:**
- `unmatched_requires(&self, available_provides: &[(ParaId, ProvidesCommitment)]) -> Vec<&RequiresCommitment>` — subset of requires that don't match any available provides

---

## State Types (`state.rs`)

### SourceState (Receiver-Side Per-Source Tracking)

```rust
pub struct SourceState {
    last_processed: u64,      // Number of messages processed (0-indexed count)
    last_seen_root: H256,     // Source's provides root we last built against
}
```

**Methods:**
- `new() -> Self` — default (0, zero hash)
- `last_processed() -> u64`
- `last_seen_root() -> H256`
- `next_expected_position() -> u64` — returns `last_processed`
- `advance(count: u64, new_root: H256) -> Result<()>` — checked addition, updates root

### IncomingMessageState (Receiver Aggregator)

```rust
pub struct IncomingMessageState {
    pub per_source: BTreeMap<ParaId, SourceState>,
}
```

**Methods:**
- `get_source(source: ParaId) -> SourceState` — get or default
- `get_source_mut(source: ParaId) -> &mut SourceState` — mutable get or insert
- `advance_source(source, count, new_root) -> Result<()>`
- `tracked_sources() -> Vec<ParaId>` — sorted list

### OutgoingMessageState (Sender-Side Tracking)

```rust
pub struct OutgoingMessageState {
    pub destination_roots: BTreeMap<ParaId, H256>,  // Per-destination MMR roots
    pub current_root: H256,                          // Top-level Merkle root
}
```

**Methods:**
- `update_destination(dest, new_mmr_root)` — update and auto-recompute root
- `recompute_root()` — rebuild from `destination_roots`
- `provides_commitment() -> ProvidesCommitment`
- `build_tree() -> StoredMerkleTree` — for efficient batch operations
- `sync_from_tree(tree: &StoredMerkleTree)` — persist tree changes back

---

## Messages (`messages.rs`)

### OutgoingMessage

```rust
pub struct OutgoingMessage {
    pub destination: ParaId,
    pub payload: Vec<u8>,
    pub position: u64,        // Position in sender's per-destination MMR
}
```

**Methods:**
- `leaf_hash() -> H256` — `blake2_256(0x03 ++ encode(self))`

### MessageBatch

```rust
pub struct MessageBatch {
    pub source: ParaId,
    pub source_block: H256,
    pub provides_root: H256,
    pub subtree_root: H256,                   // Per-destination MMR root for receiver
    pub subtree_inclusion_proof: MerkleProof,  // Proof subtree_root is in provides_root
    pub messages: Vec<OutgoingMessage>,
}
```

**Methods:**
- `is_empty() -> bool`
- `message_count() -> usize`
- `verify_subtree_inclusion(receiver: ParaId) -> Result<()>` — verifies Merkle proof
- `verify_sequential(expected_start: u64) -> Result<()>` — positions must be sequential
- `positions_range() -> Option<(u64, u64)>` — first and last positions

---

## Proofs (`proofs.rs`)

### MMR Utility Functions

```rust
/// Merge two MMR nodes with 0x02 domain prefix
pub fn merge_mmr_nodes(left: H256, right: H256) -> H256 {
    blake2_256([0x02] ++ left ++ right)
}

/// Bag MMR peaks right-to-left
pub fn bag_peaks(peaks: &[H256]) -> H256 {
    // Fold from right: acc = merge_mmr_nodes(peak, acc)
    // Empty -> H256::zero(), Single -> peak unchanged
}
```

### MmrExtensionProof

Proves that an old MMR is a prefix of a new MMR (the old MMR grew, gaining new leaves/peaks).

```rust
pub struct MmrExtensionProof {
    pub old_peaks: Vec<H256>,
    pub new_peaks: Vec<H256>,
    pub connecting_nodes: Vec<H256>,
    pub merge_directions: Vec<bool>,  // true = current is left child
}
```

**Verification algorithm (`verify(old_root, new_root)`):**

```
1. Validate merge_directions.len() == connecting_nodes.len()
2. Compute old_root from old_peaks via bag_peaks, verify match
3. Compute new_root from new_peaks via bag_peaks, verify match
4. For each old peak:
   a. If it appears directly in new_peaks -> mapped (unchanged)
   b. Otherwise, walk connecting nodes to find which new peak it maps to
   c. Each connecting node merges based on merge_directions
   d. Track claimed new peaks in BTreeSet (no duplicates allowed)
5. Verify all connecting nodes consumed
```

### LateBlockProof

Proves that a receiver's requires commitment can be updated from an old provides root to the current one.

```rust
pub struct LateBlockProof {
    pub source: ParaId,
    pub old_subtree_root: H256,
    pub old_subtree_proof: MerkleProof,     // old_subtree_root in old_provides
    pub new_provides_root: H256,
    pub new_subtree_root: H256,
    pub new_subtree_proof: MerkleProof,     // new_subtree_root in new_provides
    pub subtree_extension: Option<MmrExtensionProof>,
}
```

**Verification algorithm (`verify(old_provides_root, receiver_para_id, expected_source)`):**

```
Step 1: Validate source == expected_source
Step 2: Verify old_subtree_proof against old_provides_root
        (proves old_subtree_root was in old tree for receiver_para_id)
Step 3: Verify new_subtree_proof against new_provides_root
        (proves new_subtree_root is in current tree for receiver_para_id)
Step 4: If old_subtree_root != new_subtree_root:
          - Require subtree_extension is Some
          - Verify extension proves old -> new transition
        If old_subtree_root == new_subtree_root:
          - Require subtree_extension is None
Step 5: Return RequiresCommitment { source, expected_root: new_provides_root }
```

**Diagram — Late Block Proof Verification:**

```
     OLD STATE (receiver built against)         CURRENT STATE (relay chain has)
     ================================          ================================

     old_provides_root                          new_provides_root
           |                                          |
      Merkle Tree                                Merkle Tree
     /     |     \                              /     |     \
   ParaX  recv  ParaZ                         ParaX  recv  ParaZ
           |                                          |
     old_subtree_root                           new_subtree_root
      (old MMR root)                             (new MMR root)
           |                                          |
         MMR                                        MMR (extended)
        / | \                                     / | \ | \
      m0  m1  m2                                m0  m1  m2  m3  m4

     old_subtree_proof                          new_subtree_proof
     verifies recv in old tree                  verifies recv in new tree

                    subtree_extension
                    proves old MMR is prefix of new MMR
```

---

## Type Hierarchy Summary

```
Commitments
  ProvidesCommitment { root: H256 }
  RequiresCommitment { source: ParaId, expected_root: H256 }
  CommitmentPair { provides, requires: Vec }

State
  SourceState { last_processed: u64, last_seen_root: H256 }
  IncomingMessageState { per_source: BTreeMap<ParaId, SourceState> }
  OutgoingMessageState { destination_roots: BTreeMap, current_root: H256 }

Merkle Tree
  MerkleProof { leaf_index: u32, leaf_count: u32, siblings: Vec<H256> }
  DestinationMerkleTree (stateless functions)
  StoredMerkleTree { leaves: Vec<(ParaId, H256)>, levels: Vec<Vec<H256>> }

Messages
  OutgoingMessage { destination: ParaId, payload: Vec<u8>, position: u64 }
  MessageBatch { source, source_block, provides_root, subtree_root, proof, messages }

Proofs
  MmrExtensionProof { old_peaks, new_peaks, connecting_nodes, merge_directions }
  LateBlockProof { source, old/new subtree roots & proofs, subtree_extension }

Trait
  SpeculativeMessagingProvider { provides_root(), requires_commitments() }
```
