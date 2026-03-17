# Speculative Messaging: PVF Late Block Validation

**Location:** `cumulus/pallets/parachain-system/src/validate_block/implementation.rs`

**PoV data:** `cumulus/primitives/core/src/parachain_block_data.rs`

The PVF (Parachain Validation Function) is responsible for executing the parachain block and verifying late block proofs when timing mismatches occur. This is the bridge between what the parachain runtime computed and what the relay chain expects.

## The Timing Problem

```
Timeline:
  T0: ParaA publishes provides root_v1
  T1: ParaB builds block consuming messages against root_v1
  T2: ParaA publishes provides root_v2 (new messages added)
  T3: ParaB's candidate reaches relay chain

At T3: IncludedProvidesRoots[ParaA] = root_v2
       ParaB's requires references root_v1
       Mismatch! Without late block proofs, ParaB's candidate is rejected.
```

The LateBlockProof proves that root_v1 and root_v2 are related (root_v1 is a "prefix" of root_v2) and transforms the requires commitment to reference root_v2.

---

## ParachainBlockData V2

**File:** `cumulus/primitives/core/src/parachain_block_data.rs`

```rust
enum ParachainBlockData<Block> {
    V0 { blocks: Vec<Block>, proof: CompactProof },                              // Legacy
    V1 { blocks: Vec<Block>, proof: CompactProof },                              // Versioned
    V2 { blocks: Vec<Block>, proof: CompactProof, late_block_proofs: Vec<LateBlockProof> },  // Spec-msg
}
```

### Encoding Format

```
[VERSIONEDPBD magic bytes][version: u8 = 2][blocks][proof][late_block_proofs]
```

### Bounds

```rust
const MAX_LATE_BLOCK_PROOFS: usize = MAX_REQUIRES_COMMITMENT_NUM as usize;  // 1024
```

Validated at decode-time to prevent memory exhaustion from untrusted PoV data.

### API

```rust
impl ParachainBlockData<Block> {
    fn new_with_late_block_proofs(blocks, proof, late_block_proofs) -> Self;
    fn late_block_proofs(&self) -> &[LateBlockProof];  // Empty for V0/V1
    fn into_inner(self) -> (Vec<Block>, CompactProof, Vec<LateBlockProof>);
}
```

---

## validate_block Integration

**File:** `cumulus/pallets/parachain-system/src/validate_block/implementation.rs`

### Step 1: Decode PoV

```rust
let (blocks, proof, late_block_proofs) = block_data.into_inner();
```

### Step 2: Execute Block

Normal block execution happens — this triggers the speculative messaging pallet's hooks:
- `send_message()` calls build MMRs and update `TopLevelTree`
- `receive_messages_inherent()` processes incoming batches
- `on_finalize` collects `provides_root` and `requires_commitments` into ephemeral storage

### Step 3: Collect Raw Commitments

After block execution, read from ephemeral storage:
- `ProvidesSpecMsgRoot` -> the parachain's provides root (if any)
- `PendingRequiresSpecMsg` -> the raw requires commitments

### Step 4: Process Late Block Proofs (Final Block Only)

Late block proof processing only happens for the **last block** in a multi-block PoV:

```rust
if block_index + 1 == num_blocks {
    // Process late block proofs
}
```

### Step 5: Index Proofs by Source

```rust
let mut proof_map: BTreeMap<ParaId, &LateBlockProof> = BTreeMap::new();
for proof in &late_block_proofs {
    let prev = proof_map.insert(proof.source, proof);
    assert!(prev.is_none(), "duplicate late block proof for source");
}
```

Each source can have at most one late block proof. Duplicates cause a panic (invalid PoV).

### Step 6: Verify and Transform Each Requires

```rust
let mut final_requires = BoundedVec::default();

for raw_req in pending_requires {
    let commitment = if let Some(proof) = proof_map.remove(&raw_req.source) {
        // Late block proof exists — verify and transform
        proof.verify(
            raw_req.expected_root,   // old provides root
            self_para_id,            // our ParaId (receiver)
            raw_req.source,          // expected source
        )?
        // Returns RequiresCommitment { source, expected_root: new_provides_root }
    } else {
        // No late block proof — pass through unchanged
        raw_req
    };

    final_requires.try_push(commitment)?;
}
```

### Step 7: Verify All Proofs Consumed

```rust
assert!(proof_map.is_empty(), "unconsumed late block proofs in PoV");
```

Every late block proof in the PoV must correspond to a requires commitment. Extra proofs bloat the PoV and are rejected.

### Step 8: Return in ValidationResult

```rust
ValidationResult {
    // ... other fields ...
    provides_spec_msg_root,    // Option<H256>
    requires_spec_msg,         // BoundedVec<(Id, H256), MAX_REQUIRES_COMMITMENT_NUM>
}
```

These are then converted to `CandidateCommitments` fields and checked by the relay chain.

---

## LateBlockProof Verification Deep Dive

**File:** `polkadot/primitives/speculative-messaging/src/proofs.rs`

```rust
pub fn verify(
    &self,
    old_provides_root: H256,      // What the receiver built against
    receiver_para_id: ParaId,      // The receiver's ParaId
    expected_source: ParaId,       // Must match self.source
) -> Result<RequiresCommitment, SpeculativeMessagingError>
```

### Verification Steps

```
Step 1: Source validation
  self.source == expected_source
  Otherwise: SourceMismatch

Step 2: Old subtree proof
  DestinationMerkleTree::verify_proof(
      old_provides_root,          // Root of the old top-level tree
      receiver_para_id,           // ParaId of the leaf
      self.old_subtree_root,      // MMR root in old tree for our destination
      &self.old_subtree_proof     // Merkle inclusion proof
  )
  This proves: old_subtree_root was the MMR root for receiver in old provides tree

Step 3: New subtree proof
  DestinationMerkleTree::verify_proof(
      self.new_provides_root,     // Root of the current top-level tree
      receiver_para_id,           // Same ParaId
      self.new_subtree_root,      // MMR root in current tree for our destination
      &self.new_subtree_proof     // Merkle inclusion proof
  )
  This proves: new_subtree_root is the MMR root for receiver in current provides tree

Step 4: Subtree extension
  If old_subtree_root != new_subtree_root:
    REQUIRE subtree_extension is Some
    extension.verify(old_subtree_root, new_subtree_root)
    This proves: old MMR is a prefix of new MMR
    (messages were appended, none removed)
  If old_subtree_root == new_subtree_root:
    REQUIRE subtree_extension is None
    (no new messages for this destination, but tree restructured)

Step 5: Return transformed commitment
  RequiresCommitment {
      source: self.source,
      expected_root: self.new_provides_root  // Updated!
  }
```

---

## MmrExtensionProof Verification

**File:** `polkadot/primitives/speculative-messaging/src/proofs.rs`

```rust
pub struct MmrExtensionProof {
    pub old_peaks: Vec<H256>,
    pub new_peaks: Vec<H256>,
    pub connecting_nodes: Vec<H256>,
    pub merge_directions: Vec<bool>,  // true = current is left child
}
```

### Verification Algorithm

```
1. Validate: merge_directions.len() == connecting_nodes.len()

2. Verify old_root: bag_peaks(old_peaks) == old_root

3. Verify new_root: bag_peaks(new_peaks) == new_root

4. For each old_peak:
   a. Search for it directly in new_peaks
   b. If found: old peak is unchanged, mark new_peak as claimed
   c. If not found: walk connecting nodes
      - current = old_peak
      - While current not in new_peaks:
        - sibling = connecting_nodes[next]
        - if merge_directions[next]:
            current = merge_mmr_nodes(current, sibling)  // current is left
          else:
            current = merge_mmr_nodes(sibling, current)  // current is right
      - Mark matched new_peak as claimed
   d. Track claimed peaks in BTreeSet (no duplicate claims allowed)

5. Verify all connecting_nodes consumed (no extra proof data)
```

### Example: MMR Extension

```
Old MMR (3 leaves):  peaks = [merge(m0,m1), m2]    root = bag(peaks)
New MMR (5 leaves):  peaks = [merge(merge(m0,m1),merge(m2,m3)), m4]

Old peak merge(m0,m1):
  This is the left child of the new peak merge(merge(m0,m1),merge(m2,m3))
  connecting_nodes = [merge(m2,m3)]
  merge_directions = [true]  // old peak is on the left
  merge_mmr_nodes(merge(m0,m1), merge(m2,m3)) = new peak

Old peak m2:
  This was merged into merge(m2,m3), which then merged with merge(m0,m1)
  But wait — m2 alone doesn't appear in connecting_nodes.
  The proof handles this differently: the OLD peaks are the starting points.

  Actually, the extension proof for this case would show:
  - old_peak merge(m0,m1) -> walk to new peak via connecting_nodes
  - old_peak m2 -> walk to new peak via connecting_nodes
```

---

## Proof Size Budget

Worst case estimate: ~1.6 KB per late block proof (1000 destinations, 24h of messages).

PoV reservation: 50 KB for safety margin.

**Components per proof:**
- 2x MerkleProof: O(log D) siblings each, where D = destinations
- 1x MmrExtensionProof: peaks + connecting nodes (proportional to MMR depth)
- Fixed overhead: source ParaId, two H256 roots

---

## Diagram: Full PVF Flow

```
PoV Contents:
  +----------------------------------+
  | ParachainBlockData V2            |
  |   blocks: [Block]                |
  |   proof: CompactProof            |
  |   late_block_proofs: [           |
  |     LateBlockProof {             |
  |       source: ParaA,             |
  |       old_subtree_root,          |
  |       old_subtree_proof,         |
  |       new_provides_root,         |
  |       new_subtree_root,          |
  |       new_subtree_proof,         |
  |       subtree_extension,         |
  |     }                            |
  |   ]                              |
  +----------------------------------+

PVF Execution:
  1. Execute block        -> runtime computes provides/requires
  2. Read ephemeral state -> raw provides root + raw requires list
  3. Index late proofs    -> BTreeMap<ParaId, &LateBlockProof>
  4. For each requires:
     +-- has matching proof? -YES-> verify() -> transformed commitment
     |                               |
     +-- no proof ----------NO--> pass through unchanged
  5. Assert all proofs consumed
  6. Return ValidationResult { provides_root, transformed_requires }

Relay Chain:
  7. Store provides in IncludedProvidesRoots
  8. Check each requires against IncludedProvidesRoots
  9. Accept or reject candidate
```
