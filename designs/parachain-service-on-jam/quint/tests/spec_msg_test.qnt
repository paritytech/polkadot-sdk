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

  /// A well-formed `WorkResult` for `para` chaining `parent -> parent + 1`,
  /// carrying the given upward messages and spec-msg digest fields — for
  /// driving `accumulateAll` / `accumulateOnePackage` blocks directly.
  pure def mkWork(
    para: ParaId, codeHash: ValidationCodeHash, parent: HeadData,
    msgs: List[UpwardMessage],
    provides: Option[StreamsRoot], requires: List[(ParaId, StreamsRoot)]
  ): WorkResult = {
    result: WorkOk(Ok({
      paraId: para,
      validationCode: { hash: codeHash, len: FixedCodeLen },
      parentHeadHash: headHash(parent),
      headData: parent + 1,
      upwardMessages: msgs,
      lookupAnchor: 0,
      specMsgProvides: provides,
      specMsgRequires: requires
    })),
    authTrace: 0
  }

  /// §4.3: either spec-msg host call invoked more than once aborts the PVF
  /// with `SpecMsgCallRepeated`; called at most once each, the declarations
  /// travel verbatim in the digest, `Provides` as an option and `Requires`
  /// as the declared set.
  run specMsgRepeatedCallRejectedTest = {
    val ct = CoretimeParaId
    val ctPkg: WorkPackage = {
      authorizedParas: List(ct),
      items: List({ validationCode: { hash: { vchBytes: 1 }, len: FixedCodeLen }, pov: 0 })
    }
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
      // Failure precedence, pinned for the Rust harness. A PVF abort wins:
      // the declarations never reach the wrapper.
      assert(refineSpec(initialState, ctPkg, false, PvfReportError(9),
        Some(headHash(0)), HeadDeclared(1), List(), 0,
        ProvidesRepeated, RequiresRepeated)[0]
        == WorkOk(Err({ paraId: ct, error: Opaque(9) }))),
      // The spec-msg calls abort in the call itself, before the wrapper's
      // post-run head-declaration checks.
      assert(refineSpec(initialState, ctPkg, false, PvfOk,
        None, HeadUndeclared, List(), 0,
        ProvidesRepeated, RequiresUndeclared)[0]
        == WorkOk(Err({ paraId: ct, error: SpecMsgCallRepeated }))),
      // A repeat is settled before the set's own validity.
      assert(refineSpec(initialState, ctPkg, false, PvfOk,
        Some(headHash(0)), HeadDeclared(1), List(), 0,
        ProvidesRepeated, RequiresDeclared(List((ct, SpecMsgRootA))))[0]
        == WorkOk(Err({ paraId: ct, error: SpecMsgCallRepeated }))),
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
      // Boundary zero: an explicitly EMPTY set is well-formed and
      // indistinguishable from never calling — same digest either way.
      assert(wellFormedRequires(List(), ct)),
      assert(refineWithSpecMsg(initialState, ct, ProvidesUndeclared, RequiresDeclared(List()))
        == refineWithSpecMsg(initialState, ct, ProvidesUndeclared, RequiresUndeclared)),
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

  /// §5.1 step 6 against a ring at CAPACITY: fullness changes nothing about
  /// the gate. A consumer naming the evicted root or a never-pushed root is
  /// silently rejected (the whole state is untouched), while the identical
  /// candidate naming any root still inside the window — oldest or newest —
  /// enacts.
  run specMsgFullRingStillRejectsMissTest = {
    val sender = CoretimeParaId
    val consumer = AssetHubParaId
    val consumerCode: ValidationCodeHash = { vchBytes: 2 }
    // One past capacity: the ring holds exactly rootNo(1..SpecMsgRingCapacity)
    // and rootNo(0) has been evicted.
    val fullRing = pushMany(initialState, sender, SpecMsgRingCapacity + 1)
    pure def wanting(root: StreamsRoot): WorkResult = {
      result: WorkOk(Ok({
        paraId: consumer,
        validationCode: { hash: consumerCode, len: FixedCodeLen },
        parentHeadHash: headHash(0),
        headData: 1,
        upwardMessages: List(),
        lookupAnchor: 0,
        specMsgProvides: None,
        specMsgRequires: List((sender, root))
      })),
      authTrace: 0
    }
    pure def runOn(root: StreamsRoot): PerPackageOutcome =
      accumulateOnePackage(fullRing, wanting(root), Set(), None, FixedCodeLen, 1, Map())
    all {
      // The evicted root and a never-pushed root: rejected, NOTHING changed.
      assert(runOn(rootNo(0)).state == fullRing),
      assert(runOn({ streamsRootBytes: 424242 }).state == fullRing),
      // The oldest still-live root and the newest: the same candidate enacts.
      assert(runOn(rootNo(1)).state.paraInfo(consumer).headData == 1),
      assert(runOn(rootNo(SpecMsgRingCapacity)).state.paraInfo(consumer).headData == 1),
      init
    }
  }

  /// §5.1 step 6 at FULL fan-in (`MaxRequiresSources` entries): settlement is
  /// all-or-nothing. Against empty rings the max-size set is silently
  /// rejected; with every source's ring holding its root the candidate
  /// enacts; with 31 of 32 present the single missing source still rejects
  /// the whole set ("on the first missing entry").
  run specMsgFullFanInSettlementTest = {
    val consumer = AssetHubParaId
    val consumerCode: ValidationCodeHash = { vchBytes: 2 }
    pure def srcPara(i: int): ParaId = MkParaId(100 + i)
    val fullRequires = range(0, MaxRequiresSources).foldl(List(), (acc, i) =>
      acc.append((srcPara(i), rootNo(i))))
    val candidate: WorkResult = {
      result: WorkOk(Ok({
        paraId: consumer,
        validationCode: { hash: consumerCode, len: FixedCodeLen },
        parentHeadHash: headHash(0),
        headData: 1,
        upwardMessages: List(),
        lookupAnchor: 0,
        specMsgProvides: None,
        specMsgRequires: fullRequires
      })),
      authTrace: 0
    }
    // Rings fabricated with the pure push: every source holds its root, or
    // every source but the last.
    val allPresent = range(0, MaxRequiresSources).foldl(initialState, (acc, i) =>
      specMsgPush(acc, srcPara(i), rootNo(i)))
    val oneMissing = range(0, MaxRequiresSources - 1).foldl(initialState, (acc, i) =>
      specMsgPush(acc, srcPara(i), rootNo(i)))
    pure def runOn(s: ParachainServiceState): PerPackageOutcome =
      accumulateOnePackage(s, candidate, Set(), None, FixedCodeLen, 1, Map())
    all {
      // Refine would carry this set: it is well-formed at the cap.
      assert(wellFormedRequires(fullRequires, consumer)),
      // Empty rings: silently rejected, nothing at all changed.
      assert(runOn(initialState).state == initialState),
      // All 32 sources present: enacts.
      assert(runOn(allPresent).state.paraInfo(consumer).headData == 1),
      // 31 of 32: the one missing source rejects the whole set, silently.
      assert(runOn(oneMissing).state == oneMissing),
      assert(runOn(oneMissing).state.paraLog(consumer) == List()),
      init
    }
  }

  /// §5.1 step 6 / §8.3, the line the check-before-head-write ordering
  /// exists for: "a rejected consumer never publishes a root of its own". A
  /// pipeline candidate that both consumes and sends is rejected at
  /// settlement, so neither its head NOR its own `Provides` root lands; the
  /// identical digest, once its source root is live, enacts and publishes in
  /// the same step.
  run specMsgRejectedConsumerPublishesNothingTest = {
    val sender = CoretimeParaId
    val consumer = AssetHubParaId
    val consumerCode: ValidationCodeHash = { vchBytes: 2 }
    val own: StreamsRoot = { streamsRootBytes: 555 }
    val pipeline = mkWork(
      consumer, consumerCode, 0, List(),
      Some(own), List((sender, rootNo(0))))
    val vsEmpty = accumulateOnePackage(
      initialState, pipeline, Set(), None, FixedCodeLen, 1, Map())
    val ready = specMsgPush(initialState, sender, rootNo(0))
    val vsReady = accumulateOnePackage(
      ready, pipeline, Set(), None, FixedCodeLen, 1, Map())
    all {
      // Settlement rejection: no head write and, crucially, no publish.
      assert(vsEmpty.state == initialState),
      assert(not(vsEmpty.state.specMsgSettles(List((consumer, own))))),
      // Control: the same digest settles, enacts, and publishes at once.
      assert(vsReady.state.paraInfo(consumer).headData == 1),
      assert(vsReady.state.specMsgSettles(List((consumer, own)))),
      init
    }
  }

  /// §8.3: the ring is keyed by `(sender, root)`, so a root live in one
  /// para's ring settles nothing for any other source naming the same value.
  run specMsgRingPerSenderNamespaceTest = {
    val a = CoretimeParaId
    val consumer = AssetHubParaId
    val consumerCode: ValidationCodeHash = { vchBytes: 2 }
    val r = rootNo(3)
    val onlyA = specMsgPush(initialState, a, r)
    val wrongSource = accumulateOnePackage(
      onlyA, mkWork(consumer, consumerCode, 0, List(), None, List((MkParaId(3), r))),
      Set(), None, FixedCodeLen, 1, Map())
    val rightSource = accumulateOnePackage(
      onlyA, mkWork(consumer, consumerCode, 0, List(), None, List((a, r))),
      Set(), None, FixedCodeLen, 1, Map())
    all {
      // The same root value under a different source misses.
      assert(wrongSource.state == onlyA),
      assert(rightSource.state.paraInfo(consumer).headData == 1),
      init
    }
  }

  /// §8.3: the ring holds the last `SpecMsgRingCapacity` ENACTED roots, not
  /// just the latest one. A sender chaining two candidates in one block
  /// leaves both roots settleable — a consumer one root behind is not cut
  /// off until `SpecMsgRingCapacity` newer pushes evict it.
  run specMsgRingKeepsEnactedHistoryTest = {
    val sender = CoretimeParaId
    val senderCode: ValidationCodeHash = { vchBytes: 1 }
    val out = accumulateAll(
      initialState, Set(), None, FixedCodeLen, 1,
      List(
        mkWork(sender, senderCode, 0, List(), Some(rootNo(1)), List()),
        mkWork(sender, senderCode, 1, List(), Some(rootNo(2)), List())),
      Map())
    all {
      assert(out.state.paraInfo(sender).headData == 2),
      assert(out.state.specMsgCursor.get(sender) == { head: 2, tail: 0 }),
      assert(out.state.specMsgSettles(List((sender, rootNo(1))))),
      assert(out.state.specMsgSettles(List((sender, rootNo(2))))),
      init
    }
  }

  /// The §8.3 / §2-note same-block race around forced recovery, both ways.
  /// Rollback first: Coretime's `parachain_set_head` replay (step 7) clears
  /// the sender's ring, so a LATER digest's settlement in the same block
  /// misses and only the enacting rollback carrier advances. Consumer first:
  /// it settles against the still-live ring and enacts, then the rollback
  /// replays — the consumer consumed a history that died within the block,
  /// the exposure §8.3 explicitly accepts at MVP instead of draining
  /// Coretime's commands before settlement.
  run specMsgSameBlockForcedRecoveryTest = {
    val ct = CoretimeParaId
    val ctCode: ValidationCodeHash = { vchBytes: 1 }
    val ah = AssetHubParaId
    val rootS = rootNo(9)
    val rollback = List(ParachainSetHead({ paraId: ah, newHead: 7 }))
    val seeded = specMsgPush(initialState, ah, rootS)
    val rollbackFirst = accumulateAll(
      seeded, Set(), None, FixedCodeLen, 1,
      List(
        mkWork(ct, ctCode, 0, rollback, None, List()),
        mkWork(ct, ctCode, 1, List(), None, List((ah, rootS)))),
      Map())
    val consumerFirst = accumulateAll(
      seeded, Set(), None, FixedCodeLen, 1,
      List(
        mkWork(ct, ctCode, 0, List(), None, List((ah, rootS))),
        mkWork(ct, ctCode, 1, rollback, None, List())),
      Map())
    all {
      // Rollback first: the second digest misses the cleared ring.
      assert(rollbackFirst.state.paraInfo(ct).headData == 1),
      assert(rollbackFirst.state.paraInfo(ah).headData == 7),
      assert(not(rollbackFirst.state.specMsgSettles(List((ah, rootS))))),
      // Consumer first: both enact; the consumed history dies moments later.
      assert(consumerFirst.state.paraInfo(ct).headData == 2),
      assert(consumerFirst.state.paraInfo(ah).headData == 7),
      assert(not(consumerFirst.state.specMsgSettles(List((ah, rootS))))),
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

  /// §6.4 / §8.3: the ring teardown happens on the PARTIAL clean-up branch
  /// too. A para whose validation code enters forget-limbo stays live with
  /// `isDeregistering` set — but its ring is already gone, so its final
  /// messages stop being drainable the moment deregistration begins, not
  /// only when the entry is finally dropped.
  run specMsgCleanUpPartialStillClearsRingTest = {
    val doomed = MkParaId(3)
    val code: ValidationCodeHash = { vchBytes: 77 }
    val registered =
      parachainSetStateBalance(initialState, doomed, InitialParaBalance).state
    val withCode =
      parachainSetValidationCode(registered, doomed, code, FixedCodeLen, 0).state
    // Provided code: the clean-up's forget only unrequests (two-step, §6.1),
    // so the para survives in the deregistering state.
    val provided = markProvided(withCode, vchAsHash(code), FixedCodeLen)
    val seeded = specMsgPush(provided, doomed, rootNo(5))
    val out = parachainCleanUp(seeded, doomed, 0).state
    all {
      assert(seeded.specMsgSettles(List((doomed, rootNo(5))))),
      // Partial branch taken: still live, awaiting the expunging forget.
      assert(out.isLive(doomed)),
      assert(out.paraInfo(doomed).isDeregistering),
      // The ring is nonetheless gone already.
      assert(not(out.specMsgCursor.keys().contains(doomed))),
      assert(out.specMsgMember.keys().filter(k => k._1 == doomed) == Set()),
      assert(not(out.specMsgSettles(List((doomed, rootNo(5)))))),
      init
    }
  }

  /// The `spec_msg_ring_coherent` invariant is not vacuous: each corruption
  /// class it exists for — an entry with no cursor, a window past capacity,
  /// a queue root with no membership — is rejected, while a genuinely pushed
  /// ring passes. (Same pattern as the gapped-buckets fixture.)
  run specMsgRingInvariantCatchesCorruptionTest = {
    val ct = CoretimeParaId
    val orphanMember: ParachainServiceState = { ...initialState,
      specMsgMember: Map((ct, rootNo(0)) -> { seq: 0 }) }
    val overWideWindow: ParachainServiceState = { ...initialState,
      specMsgCursor: Map(ct -> { head: SpecMsgRingCapacity + 1, tail: 0 }) }
    val memberlessQueue: ParachainServiceState = { ...initialState,
      specMsgCursor: Map(ct -> { head: 1, tail: 0 }),
      specMsgQueue: Map((ct, 0) -> rootNo(0)) }
    init
      .then(setSvc(orphanMember))
      .then(expectThat(not(spec_msg_ring_coherent)))
      .then(setSvc(overWideWindow))
      .then(expectThat(not(spec_msg_ring_coherent)))
      .then(setSvc(memberlessQueue))
      .then(expectThat(not(spec_msg_ring_coherent)))
      .then(setSvc(specMsgPush(initialState, ct, rootNo(0))))
      .then(expectThat(spec_msg_ring_coherent and spec_msg_ring_only_for_live))
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
