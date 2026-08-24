// Parachain head commitment tree and parent-head continuity (design 5.5).
module head_commitment_test {
  import types.* from "../types"
  import messages.* from "../messages"
  import state.* from "../state"
  import head_commitment.* from "../head_commitment"
  import state_vars.* from "../state_vars"
  import main.* from "../main"
  import support.* from "./support"

  /// Leaf and node hashes occupy disjoint images, so no leaf hash can ever
  /// equal a node hash, what §5.5 gets from covering the SCALE discriminant.
  run headCommitmentLeafNodeDisjointTest = {
    val leaves = Set(MkParaId(1), MkParaId(2), MkParaId(7)).map(p =>
      merkleHash(Leaf({ paraId: p, headHash: headHash(paraIdInt(p) * 3) })))
    val nodes = tuples(leaves, leaves).map(pair =>
      merkleHash(Node({ left: pair._1, right: pair._2 })))
    init.then(expectThat(leaves.intersect(nodes) == Set()))
  }

  /// Tree shape (§5.5): no leaves yields no hash at all, and a single leaf is
  /// its own root rather than being hashed again.
  run headCommitmentRootShapeTest = {
    val leaf = merkleHash(Leaf({
      paraId: AssetHubParaId, headHash: headHash(9)
    }))
    init.then(expectThat(all {
      merkleRoot(List()) == None,
      merkleRoot(List(leaf)) == Some(leaf),
      // Two leaves combine into one node, so the root is neither leaf.
      merkleRoot(List(leaf, leaf)) == Some(merkleHash(Node({
        left: leaf, right: leaf
      }))),
    }))
  }

  /// Leaves are ordered by ascending `para_id` regardless of how the changed
  /// set was assembled, so every verifier builds the same tree (§5.5).
  run headCommitmentOrdersByParaIdTest = {
    init.then(expectThat(
      sortedParaIds(Set(MkParaId(7), MkParaId(1), MkParaId(4)))
        == List(MkParaId(1), MkParaId(4), MkParaId(7))))
  }

  /// An odd leaf count promotes the trailing element to the next level
  /// unchanged rather than duplicating it; §5.5 leaves this open, see `pairUp`
  /// in head_commitment.qnt.
  run headCommitmentOddLevelPromotesTest = {
    // A level holds tree-element hashes, not raw head hashes.
    val a = merkleHash(Leaf({ paraId: MkParaId(1), headHash: headHash(1) }))
    val b = merkleHash(Leaf({ paraId: MkParaId(2), headHash: headHash(2) }))
    val c = merkleHash(Leaf({ paraId: MkParaId(3), headHash: headHash(3) }))
    init.then(expectThat(all {
      pairUp(List(a, b, c)) == List(merkleHash(Node({ left: a, right: b })), c),
      merkleRoot(List(a, b, c)) == Some(merkleHash(Node({
        left: merkleHash(Node({ left: a, right: b })), right: c
      }))),
    }))
  }

  /// End to end (§5.5): a block that advances one para's head commits to
  /// exactly that leaf, and a block that touches no head returns no hash. The
  /// head carried is the value the para ended the block with.
  run headCommitmentTracksChangedHeadsTest = {
    val assetHubCode: ValidationCodeHash = { vchBytes: 2 }
    init
      // A block advancing only Asset Hub's head commits to its single leaf.
      .then(applyHappyPath(AssetHubParaId, assetHubCode, List()))
      .then(expectThat(
        lastHeadRoot == Some(merkleHash(Leaf({
          paraId: AssetHubParaId,
          headHash: headHash(svc.paraInfo(AssetHubParaId).headData)
        })))))
      // A step that moves no head contributes no entry for that block.
      .then(provisionPreimage)
      .then(expectThat(lastHeadRoot == None))
  }

  /// Parent-head check (§5.1 step 3): a candidate declaring a stale parent head
  /// (one the para no longer has) is rejected, leaving the head unchanged; this
  /// is what stops a candidate built on a superseded head from overwriting a
  /// forced `parachain_set_head`. A candidate on the CURRENT head advances it
  /// normally (first step); the stale one does not (second step).
  run staleParentCandidateRejectedTest = {
    val assetHubCode: ValidationCodeHash = { vchBytes: 2 }
    init
      .then(applyHappyPath(AssetHubParaId, assetHubCode, List()))
      .then(expectThat(svc.paraInfo(AssetHubParaId).headData == 1))
      .then(applyStaleParentCandidate(AssetHubParaId, assetHubCode, List()))
      .then(expectThat(svc.paraInfo(AssetHubParaId).headData == 1))
  }

  /// §5.1 step 7: a rejected candidate's upward messages are never replayed, so a
  /// forced `parachain_set_head` from a STALE candidate is ignored. A valid
  /// Coretime candidate first force-sets Asset Hub's head to 42 (set_head applies
  /// on an accepted candidate). Then a stale Coretime candidate (rejected at the
  /// parent-head check) carries `parachain_set_head(AssetHub, 7)`; because it is
  /// rejected, that message is dropped and Asset Hub keeps 42.
  run staleParentSetHeadIgnoredTest = {
    val coretimeCode: ValidationCodeHash = { vchBytes: 1 }
    init
      .then(applyHappyPath(
        CoretimeParaId, coretimeCode,
        List(ParachainSetHead({ paraId: AssetHubParaId, newHead: 42 }))))
      .then(expectThat(all {
        svc.paraInfo(AssetHubParaId).headData == 42,
        svc.paraInfo(CoretimeParaId).headData == 1
      }))
      .then(applyStaleParentCandidate(
        CoretimeParaId, coretimeCode,
        List(ParachainSetHead({ paraId: AssetHubParaId, newHead: 7 }))))
      .then(expectThat(all {
        // The stale candidate's set_head(AssetHub, 7) was dropped: AH keeps 42.
        svc.paraInfo(AssetHubParaId).headData == 42,
        // The stale candidate itself was rejected: Coretime keeps its head.
        svc.paraInfo(CoretimeParaId).headData == 1
      }))
  }

  /// §6.2: `parachain_set_head` is an upsert only for a REGISTERED para; it is a
  /// no-op on a `ParaId` with no `ParaInfo` (Coretime must call
  /// `parachain_set_state_balance` first). A valid Coretime candidate is accepted
  /// and carries `parachain_set_head(MkParaId(9), 42)` for an unregistered para;
  /// the message is replayed but must NOT create a `ParaInfo` entry.
  run setHeadNonExistentParaNoOpTest = {
    val coretimeCode: ValidationCodeHash = { vchBytes: 1 }
    init
      .then(applyHappyPath(
        CoretimeParaId, coretimeCode,
        List(ParachainSetHead({ paraId: MkParaId(9), newHead: 42 }))))
      .then(expectThat(all {
        // The Coretime candidate was accepted (head advanced), so its set_head
        // message WAS replayed...
        svc.paraInfo(CoretimeParaId).headData == 1,
        // ...but set_head on an unregistered para is a no-op: no ParaInfo entry.
        not(svc.parachains.keys().contains(MkParaId(9)))
      }))
  }
}
