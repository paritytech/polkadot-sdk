// Speculative messaging at the Enacted tier (design §8.3): the Refine-side
// `set_provides_root` / `set_requires_root` declarations, the Accumulate
// settlement gate (§5.1 step 6), the per-para settlement ring with its
// position-based eviction, and the §6 teardown paths.
module spec_msg_test {
  import types.* from "../types"
  import messages.* from "../messages"
  import state.* from "../state"
  import refine.* from "../refine"
  import accumulate.* from "../accumulate"
  import management.* from "../management"
  import invariants.* from "../invariants"
  import state_vars.* from "../state_vars"
  import main.* from "../main"
  import support.* from "./support"

  // Roots local to the pure ring tests; the action tests reuse main.qnt's
  // `SpecMsgRootA` / `SpecMsgRootB` menu roots.
  pure def rootNo(i: int): StreamsRoot = { streamsRootBytes: 1000 + i }

  /// `n` pushes of distinct roots `rootNo(0) .. rootNo(n-1)` for `para`.
  pure def pushMany(
    s: ParachainServiceState, para: ParaId, n: int
  ): ParachainServiceState =
    range(0, n).foldl(s, (acc, i) => specMsgPush(acc, para, rootNo(i)))

  /// §4.3: either spec-msg host call invoked more than once aborts the PVF
  /// with `SpecMsgCallRepeated`; called at most once each, the declarations
  /// travel verbatim in the digest, `Provides` as an option and `Requires`
  /// as the declared set.
  run specMsgRepeatedCallRejectedTest = {
    val ct = CoretimeParaId
    val providesTwice =
      refineWithSpecMsg(initialState, ct, ProvidesRepeated, RequiresUndeclared)
    val requiresTwice =
      refineWithSpecMsg(initialState, ct, ProvidesUndeclared, RequiresRepeated)
    val onceEach = refineWithSpecMsg(
      initialState, ct,
      ProvidesDeclared(SpecMsgRootA),
      RequiresDeclared(List((AssetHubParaId, SpecMsgRootB))))
    all {
      assert(providesTwice == WorkOk(Err({ paraId: ct, error: SpecMsgCallRepeated }))),
      assert(requiresTwice == WorkOk(Err({ paraId: ct, error: SpecMsgCallRepeated }))),
      // Both called once: the digest carries the declarations verbatim.
      assert(onceEach == WorkOk(Ok({
        paraId: ct,
        validationCode: { hash: { vchBytes: 1 }, len: FixedCodeLen },
        parentHeadHash: headHash(0),
        headData: 1,
        upwardMessages: List(),
        lookupAnchor: 0,
        specMsgProvides: Some(SpecMsgRootA),
        specMsgRequires: List((AssetHubParaId, SpecMsgRootB))
      }))),
      init
    }
  }

  /// §4.3: a requires set past `MaxRequiresSources`, repeating a source
  /// `ParaId`, or naming the candidate's own para is rejected with
  /// `SpecMsgInvalidRequires`. Exactly `MaxRequiresSources` distinct foreign
  /// sources pass Refine: whether they SETTLE is Accumulate's concern alone
  /// (§5.1 step 6), so Refine accepts entries no ring will ever match.
  run specMsgInvalidRequiresRejectedTest = {
    val ct = CoretimeParaId
    pure def sources(n: int): List[(ParaId, StreamsRoot)] =
      range(0, n).foldl(List(), (acc, i) => acc.append((MkParaId(100 + i), rootNo(i))))
    val oneTooMany = refineWithSpecMsg(
      initialState, ct, ProvidesUndeclared,
      RequiresDeclared(sources(MaxRequiresSources + 1)))
    val atCap = refineWithSpecMsg(
      initialState, ct, ProvidesUndeclared,
      RequiresDeclared(sources(MaxRequiresSources)))
    val dupSource = refineWithSpecMsg(
      initialState, ct, ProvidesUndeclared,
      RequiresDeclared(List(
        (MkParaId(100), rootNo(0)), (MkParaId(100), rootNo(1)))))
    val namesItself = refineWithSpecMsg(
      initialState, ct, ProvidesUndeclared,
      RequiresDeclared(List((ct, rootNo(0)))))
    all {
      assert(oneTooMany == WorkOk(Err({ paraId: ct, error: SpecMsgInvalidRequires }))),
      assert(isOkDigest(atCap)),
      assert(dupSource == WorkOk(Err({ paraId: ct, error: SpecMsgInvalidRequires }))),
      assert(namesItself == WorkOk(Err({ paraId: ct, error: SpecMsgInvalidRequires }))),
      // The same rules as pure predicates, for the Rust harness to mirror.
      assert(wellFormedRequires(sources(MaxRequiresSources), ct)),
      assert(not(wellFormedRequires(sources(MaxRequiresSources + 1), ct))),
      init
    }
  }

  /// §5.1 step 6: the settlement gate, end to end through the shared
  /// `accumulateBlock` pipeline. A consumer naming a root its source never
  /// enacted is rejected SILENTLY (head unchanged, nothing logged, like every
  /// other Accumulate rejection); once the sender enacts a candidate whose
  /// digest provides that root, the identical consumer candidate settles and
  /// enacts. The ring invariants hold at every stop.
  run specMsgSettlementGateTest = {
    val sender = CoretimeParaId
    val senderCode: ValidationCodeHash = { vchBytes: 1 }
    val consumer = AssetHubParaId
    val consumerCode: ValidationCodeHash = { vchBytes: 2 }
    val wants = RequiresDeclared(List((sender, SpecMsgRootA)))
    init
      // Consumer first: the root is not in the sender's ring yet.
      .then(applySpecMsgPath(consumer, consumerCode, ProvidesUndeclared, wants))
      .then(expectThat(all {
        // Rejected silently: no head write, no log entry, no ring change.
        svc.paraInfo(consumer).headData == 0,
        svc.paraLog(consumer) == List(),
        svc.specMsgMember.keys() == Set(),
        spec_msg_ring_coherent
      }))
      // The sender enacts a candidate providing the root.
      .then(applySpecMsgPath(sender, senderCode, ProvidesDeclared(SpecMsgRootA), RequiresUndeclared))
      .then(expectThat(all {
        svc.paraInfo(sender).headData == 1,
        svc.specMsgSettles(List((sender, SpecMsgRootA))),
        svc.specMsgCursor.get(sender) == { head: 1, tail: 0 },
        spec_msg_ring_coherent,
        spec_msg_ring_only_for_live
      }))
      // The identical consumer candidate now settles and enacts.
      .then(applySpecMsgPath(consumer, consumerCode, ProvidesUndeclared, wants))
      .then(expectThat(all {
        svc.paraInfo(consumer).headData == 1,
        svc.paraLog(consumer) == List(),
        spec_msg_ring_coherent
      }))
  }

  /// §5.1 step 6 / §8.3: a root enters the ring only for a candidate that
  /// ENACTED. A candidate rejected before the head write (here: stale parent
  /// head) publishes nothing, even though its digest carries a `Provides`
  /// root; the control shows the same digest on the current head pushes.
  run specMsgProvidesOnlyOnEnactmentTest = {
    val para = CoretimeParaId
    val code: ValidationCodeHash = { vchBytes: 1 }
    pure def candidate(parent: HeadData): ParachainWorkDigestOk = {
      paraId: para,
      validationCode: { hash: code, len: FixedCodeLen },
      parentHeadHash: headHash(parent),
      headData: parent + 1,
      upwardMessages: List(),
      lookupAnchor: 0,
      specMsgProvides: Some(SpecMsgRootA),
      specMsgRequires: List()
    }
    val rejected = accumulateOnePackage(
      initialState, { result: WorkOk(Ok(candidate(StaleParentHead))), authTrace: 0 },
      Set(), None, FixedCodeLen, 1, Map())
    val enacted = accumulateOnePackage(
      initialState, { result: WorkOk(Ok(candidate(0))), authTrace: 0 },
      Set(), None, FixedCodeLen, 1, Map())
    all {
      // The rejected candidate changed nothing: no head write, no push.
      assert(rejected.state == initialState),
      assert(not(rejected.state.specMsgSettles(List((para, SpecMsgRootA))))),
      // The enacted control wrote the head and pushed the root.
      assert(enacted.state.paraInfo(para).headData == 1),
      assert(enacted.state.specMsgSettles(List((para, SpecMsgRootA)))),
      init
    }
  }

  /// §8.3 eviction is strictly position-based: a root leaves the ring only
  /// after `SpecMsgRingCapacity` newer pushes. At capacity the ring holds
  /// exactly the last `SpecMsgRingCapacity` roots — the oldest stops
  /// settling, every younger one still settles, and the ratcheting window is
  /// `1 + 2 × SpecMsgRingCapacity` entries across the three maps.
  run specMsgRingEvictionTest = {
    val para = CoretimeParaId
    val full = pushMany(initialState, para, SpecMsgRingCapacity)
    val over = specMsgPush(full, para, rootNo(SpecMsgRingCapacity))
    all {
      // At capacity: every pushed root still settles.
      assert(full.specMsgCursor.get(para) == { head: SpecMsgRingCapacity, tail: 0 }),
      assert(full.specMsgSettles(List((para, rootNo(0))))),
      // One past capacity: position 0 is evicted, its root no longer
      // settles, and both younger neighbours still do.
      assert(over.specMsgCursor.get(para) == { head: SpecMsgRingCapacity + 1, tail: 1 }),
      assert(not(over.specMsgSettles(List((para, rootNo(0)))))),
      assert(over.specMsgSettles(List((para, rootNo(1))))),
      assert(over.specMsgSettles(List((para, rootNo(SpecMsgRingCapacity))))),
      assert(not(over.specMsgQueue.keys().contains((para, 0)))),
      // The ratcheted footprint: one cursor + capacity× (member + queue).
      assert(over.specMsgMember.keys().size() == SpecMsgRingCapacity),
      assert(over.specMsgQueue.keys().size() == SpecMsgRingCapacity),
      init
    }
  }

  /// §8.3 re-push rule: a repeated root only moves its membership to the new
  /// position, leaving the old queue entry behind. Evicting that old
  /// position must NOT drop the membership (its `seq` points elsewhere); the
  /// root finally leaves once its newest position is evicted.
  run specMsgRepushKeepsMembershipTest = {
    val para = CoretimeParaId
    val x: StreamsRoot = { streamsRootBytes: 7777 }
    // `x` at positions 0 AND 1, then distinct roots up to one past capacity:
    // position 0 (holding `x`) is evicted.
    val twice = specMsgPush(specMsgPush(initialState, para, x), para, x)
    val s1 = range(0, SpecMsgRingCapacity - 1).foldl(twice, (acc, i) =>
      specMsgPush(acc, para, rootNo(i)))
    // One more push evicts position 1, `x`'s newest position.
    val s2 = specMsgPush(s1, para, rootNo(SpecMsgRingCapacity - 1))
    all {
      // Position 0 evicted, but `x` was re-pushed at 1: membership survives.
      assert(s1.specMsgCursor.get(para).tail == 1),
      assert(not(s1.specMsgQueue.keys().contains((para, 0)))),
      assert(s1.specMsgMember.get((para, x)) == { seq: 1 }),
      assert(s1.specMsgSettles(List((para, x)))),
      // Position 1 evicted too: now the root is really gone.
      assert(s2.specMsgCursor.get(para).tail == 2),
      assert(not(s2.specMsgSettles(List((para, x))))),
      init
    }
  }

  /// §6 / §8.3: a `parachain_set_head` overwriting a LIVE head clears the
  /// parachain's settlement ring, so no consumer can settle against the
  /// rolled-back history — a consumer naming a pre-rollback root is silently
  /// rejected from then on (it buffers and expires only in the full port
  /// design; at MVP it is plain rejection).
  run specMsgForcedSetHeadClearsRingTest = {
    val para = CoretimeParaId
    val seeded = pushMany(initialState, para, 3)
    val rolled = parachainSetHead(seeded, para, 42).state
    all {
      assert(seeded.specMsgSettles(List((para, rootNo(0))))),
      assert(rolled.paraInfo(para).headData == 42),
      // The whole ring is gone: cursor, membership, and queue.
      assert(not(rolled.specMsgCursor.keys().contains(para))),
      assert(rolled.specMsgMember.keys() == Set()),
      assert(rolled.specMsgQueue.keys() == Set()),
      assert(not(rolled.specMsgSettles(List((para, rootNo(0)))))),
      init
    }
  }

  /// §6.4 / §8.3: `parachain_clean_up` tears the ring down with the rest of
  /// the per-para state, permanently ending the drainability of the para's
  /// final messages (a dormant chain's ring would otherwise persist). Rings
  /// of OTHER paras are untouched.
  run specMsgCleanUpTearsDownRingTest = {
    val doomed = MkParaId(3)
    val bystander = CoretimeParaId
    // A registered para with no validation code holds exactly its baseline,
    // so clean-up completes in one call (nothing awaits expunge).
    val registered =
      parachainSetStateBalance(initialState, doomed, InitialParaBalance).state
    val seeded = pushMany(pushMany(registered, doomed, 2), bystander, 1)
    val cleaned = parachainCleanUp(seeded, doomed, 0).state
    all {
      assert(seeded.specMsgSettles(List((doomed, rootNo(0))))),
      assert(not(cleaned.isLive(doomed))),
      // The doomed para's ring is gone with it...
      assert(not(cleaned.specMsgCursor.keys().contains(doomed))),
      assert(cleaned.specMsgMember.keys().filter(k => k._1 == doomed) == Set()),
      assert(cleaned.specMsgQueue.keys().filter(k => k._1 == doomed) == Set()),
      // ...while the bystander's ring still settles.
      assert(cleaned.specMsgSettles(List((bystander, rootNo(0))))),
      init
    }
  }

  /// §8.3 at MVP has no topological sort: within one block, Accumulate takes
  /// the digests in the order JAM provides. Sender-then-consumer settles in
  /// the SAME block, because the sender's step-6 push lands before the
  /// consumer's settlement check; consumer-then-sender leaves the consumer
  /// silently rejected while the sender still enacts. (The §8.3 port
  /// document's reorder and buffering would rescue the second case.)
  run specMsgSameBlockOrderingTest = {
    val sender = CoretimeParaId
    val senderCode: ValidationCodeHash = { vchBytes: 1 }
    val consumer = AssetHubParaId
    val consumerCode: ValidationCodeHash = { vchBytes: 2 }
    val provide = (sender, senderCode, ProvidesDeclared(SpecMsgRootB), RequiresUndeclared)
    val consume = (consumer, consumerCode, ProvidesUndeclared,
      RequiresDeclared(List((sender, SpecMsgRootB))))
    init
      .then(applySpecMsgPaths(List(provide, consume)))
      .then(expectThat(all {
        // Sender first: both enact in one block.
        svc.paraInfo(sender).headData == 1,
        svc.paraInfo(consumer).headData == 1,
        spec_msg_ring_coherent
      }))
      .then(init)
      .then(applySpecMsgPaths(List(consume, provide)))
      .then(expectThat(all {
        // Consumer first: it misses the ring and only the sender enacts.
        svc.paraInfo(sender).headData == 1,
        svc.paraInfo(consumer).headData == 0,
        svc.specMsgSettles(List((sender, SpecMsgRootB))),
        spec_msg_ring_coherent
      }))
  }
}
