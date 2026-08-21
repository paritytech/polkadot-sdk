// Parachain head commitment (design §5.5).
//
// `accumulate` returns a 32-byte hash committing to the parachain heads that
// changed during the block: a binary Merkle tree over one leaf per changed
// head, ordered by ascending `para_id`, with the root returned.
//
// Hashing is abstract, as everywhere else in this model. `keccak_256` of the
// SCALE encoding is modeled as an injective integer mix, with the variant
// discriminant carried in the low bit so that a leaf hash can never equal a
// node hash, the collision-freedom §5.5 relies on. Encoded sizes (37 octets
// for a leaf, 65 for a node) are not modeled: nothing bounds this tree.
module head_commitment {
  import types.* from "./types"
  import state.* from "./state"

  /// A node of the commitment tree. `Node` holds its two children's hashes;
  /// the design writes it as the tuple variant `Node(Hash, Hash)`.
  type MerkleTree =
  | Leaf({ paraId: ParaId, headHash: Hash })
  | Node({ left: Hash, right: Hash })

  /// Abstract `keccak_256` of a tree element's SCALE encoding. Doubling the
  /// body and adding the discriminant keeps the leaf and node images disjoint,
  /// so a leaf hash can never collide with a node hash (§5.5).
  pure def merkleHash(t: MerkleTree): Hash =
  match t {
    | Leaf(l) => { domain: DomainMerkle, hashBytes: 2 * (paraIdInt(l.paraId) * 257 + l.headHash.hashBytes) }
    | Node(c) => { domain: DomainMerkle, hashBytes: 2 * (c.left.hashBytes * 257 + c.right.hashBytes) + 1 }
  }

  /// Set of para ids as a list ordered by ascending `para_id`, so every
  /// verifier builds the same tree (§5.5). Insertion sort, which makes the
  /// result independent of the set fold's order.
  pure def sortedParaIds(ps: Set[ParaId]): List[ParaId] =
  ps.fold(List(), (acc, p) =>
    acc.select(q => paraIdInt(q) < paraIdInt(p))
      .append(p)
      .concat(acc.select(q => paraIdInt(q) > paraIdInt(p))))

  /// One level of tree reduction: hash adjacent pairs, promoting a trailing
  /// odd element unchanged. §5.5 fixes neither the pairing nor the odd-element
  /// rule, so promotion is this model's reading; see the note in the README.
  pure def pairUp(level: List[Hash]): List[Hash] = {
    val paired = level.foldl(
      { out: List(), pending: None },
      (acc, h) =>
        match acc.pending {
          | None => { out: acc.out, pending: Some(h) }
          | Some(p) =>
            { out: acc.out.append(merkleHash(Node({ left: p, right: h }))),
              pending: None }
        })
    match paired.pending {
      | Some(p) => paired.out.append(p)
      | None => paired.out
    }
  }

  /// Enough levels to collapse any tree this model can build: the parachain
  /// count never approaches 2^16.
  pure val MaxMerkleLevels: int = 16

  /// Root of the binary Merkle tree over `leaves`. No leaves means no hash is
  /// returned at all; a single leaf is its own root (§5.5).
  pure def merkleRoot(leaves: List[Hash]): Option[Hash] =
  if (leaves.length() == 0) None
  else {
    val collapsed = 1.to(MaxMerkleLevels).fold(leaves, (level, _) =>
      if (level.length() <= 1) level else pairUp(level))
    Some(collapsed[0])
  }

  /// Parachains whose `head_data` differs between the block's prior and
  /// posterior state. Diffing the whole block is what gives §5.5's "exactly one
  /// leaf" regardless of how often a head was written, and carries the value
  /// the parachain ended the block with. A newly registered parachain counts as
  /// changed; one removed by `parachain_clean_up` has no ending value and so
  /// contributes nothing.
  pure def changedHeads(
    prev: ParachainServiceState,
    post: ParachainServiceState
  ): Set[ParaId] =
  post.parachains.keys().filter(p =>
    not(prev.parachains.keys().contains(p))
      or prev.paraInfo(p).headData != post.paraInfo(p).headData)

  /// The block's head commitment: the Merkle root over the changed heads, or
  /// `None` when no head changed and the service contributes no entry (§5.5).
  pure def headCommitment(
    prev: ParachainServiceState,
    post: ParachainServiceState
  ): Option[Hash] =
  merkleRoot(
    sortedParaIds(changedHeads(prev, post)).foldl(List(), (acc, p) =>
      acc.append(merkleHash(Leaf({
        paraId: p,
        headHash: headHash(post.paraInfo(p).headData)
      })))))
}
