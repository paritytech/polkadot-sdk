# Speculative Messaging: Merkle Tree & StoredMerkleTree

**Location:** `polkadot/primitives/speculative-messaging/src/merkle_tree.rs`

## Purpose

The top-level commitment structure maps each destination `ParaId` to its per-destination MMR root via a binary Merkle tree. Two implementations exist:

1. **DestinationMerkleTree** — stateless functions for computing roots, generating proofs, and verifying proofs. Used for one-shot operations and verification.
2. **StoredMerkleTree** — stateful structure that caches all intermediate node hashes. Enables O(log D) incremental updates for the hot path (existing destination's MMR root changes).

## Tree Structure

```
                     root (levels[2])
                    /               \
          hash_pair                  hash_pair          (levels[1])
          /       \                  /       \
  hash_leaf(A)  hash_leaf(B)  hash_leaf(C)  hash_leaf(C)  (levels[0])
      |              |              |              |
  (ParaA,rootA)  (ParaB,rootB)  (ParaC,rootC)  (odd promotion)
```

- Leaves are **sorted by ParaId** (ascending)
- Odd nodes are **promoted** by hashing with themselves: `hash_pair(node, node)`
- Tree is complete (not sparse) — all levels stored

## Hash Functions

### hash_leaf(para_id, mmr_root) -> H256

```
Input:  [0x00] ++ encode(para_id, mmr_root)
Output: blake2_256(input)
```

Domain prefix `0x00` prevents second-preimage attacks between leaves and internal nodes.

### hash_pair(left, right) -> H256

```
Input:  [0x01][left: 32 bytes][right: 32 bytes]  (65 bytes, stack-allocated)
Output: blake2_256(input)
```

Domain prefix `0x01` distinguishes internal nodes from leaves (`0x00`), MMR merges (`0x02`), and message hashes (`0x03`).

---

## MerkleProof

```rust
pub struct MerkleProof {
    pub leaf_index: u32,     // Position of leaf in sorted array
    pub leaf_count: u32,     // Total number of leaves
    pub siblings: Vec<H256>, // Sibling hashes from leaf to root
}
```

Proof size: `O(log D)` siblings where D = number of destinations.

---

## DestinationMerkleTree (Stateless)

### compute_root(destinations: &[(ParaId, H256)]) -> H256

1. Sort by ParaId, deduplicate
2. Hash each leaf: `hash_leaf(para_id, root)`
3. Build tree bottom-up: pair adjacent nodes with `hash_pair()`
4. Odd nodes hash with themselves
5. Return final root (or `H256::zero()` for empty input)

**Complexity:** O(D log D) due to sorting

### generate_proof(destinations, target) -> Result<(H256, MerkleProof)>

1. Validate no duplicates, find target position via sort
2. Build tree level-by-level, collecting siblings at each level
3. Handle odd promotion (node hashing with itself produces no sibling)
4. Return `(root, MerkleProof { leaf_index, leaf_count, siblings })`

**Errors:** `DestinationNotFound`, `DuplicateDestination`, `TooManyDestinations`

### verify_proof(root, para_id, mmr_root, proof) -> Result<()>

```
1. Validate: leaf_count > 0, leaf_index < leaf_count
2. current = hash_leaf(para_id, mmr_root)
3. For each sibling in proof.siblings:
   - If current index is even: current = hash_pair(current, sibling)
   - If current index is odd:  current = hash_pair(sibling, current)
   - Move index up: idx /= 2, remaining /= 2 (with odd handling)
4. Verify no unconsumed siblings remain
5. Verify current == expected root
```

**Errors:** `InvalidMerkleProof`, `UnconsumedProofData`, `RootMismatch`

---

## StoredMerkleTree (Stateful)

```rust
pub struct StoredMerkleTree {
    leaves: Vec<(ParaId, H256)>,   // Sorted by ParaId
    levels: Vec<Vec<H256>>,        // All tree levels; levels[0]=leaf hashes, levels[last]=[root]
}
```

### Construction: from_destinations(destinations) -> Self

1. Sort by ParaId, deduplicate
2. Call `build_levels(leaves)` to construct all levels
3. Return `StoredMerkleTree { leaves, levels }`

### build_levels(leaves) -> Vec<Vec<H256>> (internal)

```
level_0 = leaves.map(|(id, root)| hash_leaf(id, root))
while current_level.len() > 1:
    next_level = []
    for i in (0..current_level.len()).step_by(2):
        left = current_level[i]
        right = current_level.get(i+1).unwrap_or(left)  // odd promotion
        next_level.push(hash_pair(left, right))
    levels.push(next_level)
continue until single root
```

**Complexity:** O(D) — sum of all level sizes = 2D - 1

### root() -> H256

```rust
self.levels.last()
    .and_then(|top| top.first().copied())
    .unwrap_or(H256::zero())
```

**Complexity:** O(1)

### update(dest, new_mmr_root) -> Result<()> **(HOT PATH)**

```
1. Binary search for dest in self.leaves              O(log D)
2. Update leaf value: self.leaves[idx].1 = new_root   O(1)
3. Recompute leaf hash at levels[0][idx]               O(1)
4. Call rehash_path(idx)                                O(log D)
```

**Errors:** `DestinationNotFound`

### rehash_path(leaf_idx) (internal)

```
idx = leaf_idx
for level in 0..levels.len()-1:
    pair_start = idx & !1                    // round down to even
    left  = levels[level][pair_start]
    right = levels[level].get(pair_start+1).unwrap_or(left)  // odd
    parent_idx = pair_start / 2
    levels[level+1][parent_idx] = hash_pair(left, right)
    idx = parent_idx
```

Only touches one node per level -> O(log D) total.

### upsert(dest, mmr_root)

- If destination exists: use `update()` -> O(log D)
- If new destination: push, re-sort, full `build_levels()` rebuild -> O(D)

### remove(dest) -> Result<()>

- Remove from leaves, full rebuild -> O(D)

### generate_proof(target) -> Result<(H256, MerkleProof)>

```
1. Binary search for target in leaves                  O(log D)
2. Walk levels collecting siblings (same logic as
   stateless verify_proof but reading stored nodes)     O(log D)
3. Return (root, MerkleProof)
```

### validate() -> Result<()>

Called after decoding from untrusted data:

1. Verify leaves are sorted by ParaId with no duplicates
2. Recompute levels from leaves, compare with stored levels
3. Errors: `DuplicateDestination`, `RootMismatch`

---

## Performance Summary

| Operation | StoredMerkleTree | DestinationMerkleTree |
|-----------|-------------------|----------------------|
| Build | O(D log D) | O(D log D) |
| Root lookup | O(1) | N/A (recompute) |
| Update existing | **O(log D)** | O(D log D) rebuild |
| Insert new | O(D) rebuild | O(D log D) rebuild |
| Remove | O(D) rebuild | N/A |
| Generate proof | O(log D) | O(D) rebuild |
| Verify proof | O(log D) | O(log D) |

The critical advantage: when a pallet processes multiple `send_message` calls in one block, each call to the same destination only triggers an O(log D) rehash instead of a full rebuild.

---

## Integration with OutgoingMessageState

`OutgoingMessageState` (in `state.rs`) bridges between persistent storage and the tree:

```rust
// Build a StoredMerkleTree from the BTreeMap for batch operations
pub fn build_tree(&self) -> StoredMerkleTree {
    let entries: Vec<(ParaId, H256)> =
        self.destination_roots.iter().map(|(&k, &v)| (k, v)).collect();
    StoredMerkleTree::from_destinations(&entries)
}

// Sync mutated tree back to persistent state
pub fn sync_from_tree(&mut self, tree: &StoredMerkleTree) {
    self.destination_roots.clear();
    for &(id, root) in tree.destinations() {
        self.destination_roots.insert(id, root);
    }
    self.current_root = tree.root();
}
```

**Pattern:** Build once at start of block -> apply multiple O(log D) updates -> sync once at end.

---

## Diagram: Update Path Rehashing

```
Before update(ParaC, new_root):

        ROOT_old                    Level 2
       /        \
    H_AB        H_CD_old           Level 1
   /    \      /      \
 H_A    H_B  H_C_old  H_D         Level 0
  |      |      |       |
 (A,rA) (B,rB) (C,rC) (D,rD)     Leaves

After update(ParaC, new_root):
                                   Rehash path: H_C -> H_CD -> ROOT
        ROOT_new        *          Level 2  (rehashed)
       /        \
    H_AB        H_CD_new  *       Level 1  (rehashed)
   /    \      /      \
 H_A    H_B  H_C_new  H_D         Level 0  (rehashed)
  |      |      |       |
 (A,rA) (B,rB) (C,new) (D,rD)     Leaves   (updated)

Only * nodes are recomputed = O(log D)
```
