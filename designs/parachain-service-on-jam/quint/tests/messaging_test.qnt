// Messaging (design §8): the Refine-side `set_messages_streams_root` /
// `set_messages_requires_roots` declarations (§8.1), the Accumulate
// settlement check (§5.1 step 6), the per-parachain settlement ring with its
// wrapping `u32` positions and position-based eviction (§8.2), its
// reservation in the baseline footprint (§6.1), and teardown (§8.3).
module messaging_test {
  import types.* from "../types"
  import messages.* from "../messages"
  import state.* from "../state"
  import state_balance.* from "../state_balance"
  import refine.* from "../refine"
  import accumulate.* from "../accumulate"
  import management.* from "../management"
  import invariants.* from "../invariants"
  import state_vars.* from "../state_vars"
  import main.* from "../main"
  import support.* from "./support"

  // Roots local to the pure ring tests; the action tests reuse main.qnt's
  // `StreamsRootA` / `StreamsRootB` menu roots.
  pure def rootNo(i: int): StreamsRoot = { streamsRootBytes: 1000 + i }

  /// `n` pushes of distinct roots `rootNo(0) .. rootNo(n-1)` for `para`.
  pure def pushMany(
    s: ParachainServiceState, para: ParaId, n: int
  ): ParachainServiceState =
    range(0, n).foldl(s, (acc, i) => settlementRingPush(acc, para, rootNo(i)))

  /// A well-formed `WorkResult` for `para` chaining `parent -> parent + 1`,
  /// carrying the given upward messages and messaging digest fields, for
  /// driving `accumulateAll` / `accumulateOnePackage` blocks directly.
  pure def mkWork(
    para: ParaId, codeHash: ValidationCodeHash, parent: HeadData,
    msgs: List[UpwardMessage],
    streamsRoot: Option[StreamsRoot], requires: List[(ParaId, StreamsRoot)]
  ): WorkResult = {
    result: WorkOk(Ok({
      paraId: para,
      validationCode: { hash: codeHash, len: FixedCodeLen },
      parentHeadHash: headHash(parent),
      headData: parent + 1,
      upwardMessages: msgs,
      lookupAnchor: 0,
      messagesStreamsRoot: streamsRoot,
      messagesRequiresRoots: requires
    })),
    authTrace: 0
  }

  /// §4.3 / §8.1: either messaging host call invoked more than once aborts
  /// the PVF with `MessagesCallRepeated`; called at most once each, the
  /// declarations travel verbatim in the digest, the `StreamsRoot` as an
  /// option and the requires roots as the declared set.
  run messagesRepeatedCallRejectedTest = {
    val ct = CoretimeParaId
    val ctPkg: WorkPackage = {
      authorizedParas: List(ct),
      items: List({ validationCode: { hash: { vchBytes: 1 }, len: FixedCodeLen }, pov: 0 })
    }
    val streamsRootTwice =
      refineWithMessaging(initialState, ct, StreamsRootRepeated, RequiresRootsUndeclared)
    val requiresTwice =
      refineWithMessaging(initialState, ct, StreamsRootUndeclared, RequiresRootsRepeated)
    val onceEach = refineWithMessaging(
      initialState, ct,
      StreamsRootDeclared(StreamsRootA),
      RequiresRootsDeclared(List((AssetHubParaId, StreamsRootB))))
    all {
      assert(streamsRootTwice == WorkOk(Err({ paraId: ct, error: MessagesCallRepeated }))),
      assert(requiresTwice == WorkOk(Err({ paraId: ct, error: MessagesCallRepeated }))),
      // Both called once: the digest carries the declarations verbatim.
      assert(onceEach == WorkOk(Ok({
        paraId: ct,
        validationCode: { hash: { vchBytes: 1 }, len: FixedCodeLen },
        parentHeadHash: headHash(0),
        headData: 1,
        upwardMessages: List(),
        lookupAnchor: 0,
        messagesStreamsRoot: Some(StreamsRootA),
        messagesRequiresRoots: List((AssetHubParaId, StreamsRootB))
      }))),
      // Failure precedence, pinned for the Rust harness. A PVF abort wins:
      // the declarations never reach the wrapper.
      assert(refineWithRoots(initialState, ctPkg, false, PvfReportError(9),
        Some(headHash(0)), HeadDeclared(1), List(), 0,
        StreamsRootRepeated, RequiresRootsRepeated)[0]
        == WorkOk(Err({ paraId: ct, error: Opaque(9) }))),
      // The messaging calls abort in the call itself, before the wrapper's
      // post-run head-declaration checks.
      assert(refineWithRoots(initialState, ctPkg, false, PvfOk,
        None, HeadUndeclared, List(), 0,
        StreamsRootRepeated, RequiresRootsUndeclared)[0]
        == WorkOk(Err({ paraId: ct, error: MessagesCallRepeated }))),
      // A repeat is settled before the set's own validity.
      assert(refineWithRoots(initialState, ctPkg, false, PvfOk,
        Some(headHash(0)), HeadDeclared(1), List(), 0,
        StreamsRootRepeated, RequiresRootsDeclared(List((ct, StreamsRootA))))[0]
        == WorkOk(Err({ paraId: ct, error: MessagesCallRepeated }))),
      init
    }
  }

  /// §4.3 / §8.1: a requires set past `MaxRequiresSources`, repeating a
  /// source `ParaId`, or naming the candidate's own para is rejected with
  /// `MessagesInvalidRequires`. Exactly `MaxRequiresSources` distinct foreign
  /// sources pass Refine: whether they SETTLE is Accumulate's concern alone
  /// (§5.1 step 6), so Refine accepts entries no ring will ever match.
  run messagesInvalidRequiresRejectedTest = {
    val ct = CoretimeParaId
    pure def sources(n: int): List[(ParaId, StreamsRoot)] =
      range(0, n).foldl(List(), (acc, i) => acc.append((MkParaId(100 + i), rootNo(i))))
    val oneTooMany = refineWithMessaging(
      initialState, ct, StreamsRootUndeclared,
      RequiresRootsDeclared(sources(MaxRequiresSources + 1)))
    val atCap = refineWithMessaging(
      initialState, ct, StreamsRootUndeclared,
      RequiresRootsDeclared(sources(MaxRequiresSources)))
    val dupSource = refineWithMessaging(
      initialState, ct, StreamsRootUndeclared,
      RequiresRootsDeclared(List(
        (MkParaId(100), rootNo(0)), (MkParaId(100), rootNo(1)))))
    val namesItself = refineWithMessaging(
      initialState, ct, StreamsRootUndeclared,
      RequiresRootsDeclared(List((ct, rootNo(0)))))
    all {
      assert(oneTooMany == WorkOk(Err({ paraId: ct, error: MessagesInvalidRequires }))),
      assert(isOkDigest(atCap)),
      assert(dupSource == WorkOk(Err({ paraId: ct, error: MessagesInvalidRequires }))),
      assert(namesItself == WorkOk(Err({ paraId: ct, error: MessagesInvalidRequires }))),
      // The same rules as pure predicates, for the Rust harness to mirror.
      assert(wellFormedRequiresRoots(sources(MaxRequiresSources), ct)),
      assert(not(wellFormedRequiresRoots(sources(MaxRequiresSources + 1), ct))),
      // Boundary zero: an explicitly EMPTY set is well-formed and
      // indistinguishable from never calling: same digest either way.
      assert(wellFormedRequiresRoots(List(), ct)),
      assert(refineWithMessaging(initialState, ct, StreamsRootUndeclared, RequiresRootsDeclared(List()))
        == refineWithMessaging(initialState, ct, StreamsRootUndeclared, RequiresRootsUndeclared)),
      init
    }
  }

  /// §5.1 step 6: the settlement check, end to end through the shared
  /// `accumulateBlock` pipeline. A receiver naming a root its source never
  /// enacted is rejected SILENTLY (head unchanged, nothing logged, like every
  /// other Accumulate rejection); once the sender enacts a candidate whose
  /// digest declares that root, the identical receiver candidate settles and
  /// enacts. The ring invariants hold at every stop.
  run settlementGateTest = {
    val sender = CoretimeParaId
    val senderCode: ValidationCodeHash = { vchBytes: 1 }
    val receiver = AssetHubParaId
    val receiverCode: ValidationCodeHash = { vchBytes: 2 }
    val wants = RequiresRootsDeclared(List((sender, StreamsRootA)))
    init
      // Receiver first: the root is not in the sender's ring yet.
      .then(applyMessagingPath(receiver, receiverCode, StreamsRootUndeclared, wants))
      .then(expectThat(all {
        // Rejected silently: no head write, no log entry, no ring change.
        svc.paraInfo(receiver).headData == 0,
        svc.paraLog(receiver) == List(),
        svc.messagesMember.keys() == Set(),
        settlement_ring_coherent
      }))
      // The sender enacts a candidate declaring the root.
      .then(applyMessagingPath(sender, senderCode, StreamsRootDeclared(StreamsRootA), RequiresRootsUndeclared))
      .then(expectThat(all {
        svc.paraInfo(sender).headData == 1,
        svc.settlementCheck(List((sender, StreamsRootA))),
        svc.messagesCursor.get(sender) == { head: 1, tail: 0 },
        settlement_ring_coherent,
        settlement_ring_only_for_live
      }))
      // The identical receiver candidate now settles and enacts.
      .then(applyMessagingPath(receiver, receiverCode, StreamsRootUndeclared, wants))
      .then(expectThat(all {
        svc.paraInfo(receiver).headData == 1,
        svc.paraLog(receiver) == List(),
        settlement_ring_coherent
      }))
  }

  /// §5.1 step 6 / §8: a root enters the ring only for a candidate that
  /// ENACTED. A candidate rejected before the head write (here: stale parent
  /// head) publishes nothing, even though its digest carries a
  /// `messages_streams_root`; the control shows the same digest on the
  /// current head pushes.
  run streamsRootOnlyOnEnactmentTest = {
    val para = CoretimeParaId
    val code: ValidationCodeHash = { vchBytes: 1 }
    pure def candidate(parent: HeadData): ParachainWorkDigestOk = {
      paraId: para,
      validationCode: { hash: code, len: FixedCodeLen },
      parentHeadHash: headHash(parent),
      headData: parent + 1,
      upwardMessages: List(),
      lookupAnchor: 0,
      messagesStreamsRoot: Some(StreamsRootA),
      messagesRequiresRoots: List()
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
      assert(not(rejected.state.settlementCheck(List((para, StreamsRootA))))),
      // The enacted control wrote the head and pushed the root.
      assert(enacted.state.paraInfo(para).headData == 1),
      assert(enacted.state.settlementCheck(List((para, StreamsRootA)))),
      init
    }
  }

  /// §8.2 eviction is strictly position-based: a root leaves the ring only
  /// after `MaxSettlementRingCapacity` newer pushes. At capacity the ring
  /// holds exactly the last `MaxSettlementRingCapacity` roots: the oldest
  /// stops settling, its `messages_queue` entry at the old `tail` is deleted,
  /// every younger root still settles, and the footprint stays at
  /// `1 + 2 × MaxSettlementRingCapacity` entries across the three maps.
  run settlementRingEvictionTest = {
    val para = CoretimeParaId
    val full = pushMany(initialState, para, MaxSettlementRingCapacity)
    val over = settlementRingPush(full, para, rootNo(MaxSettlementRingCapacity))
    all {
      // At capacity: every pushed root still settles.
      assert(full.messagesCursor.get(para) == { head: MaxSettlementRingCapacity, tail: 0 }),
      assert(ringLive(full.messagesCursor.get(para)) == MaxSettlementRingCapacity),
      assert(full.settlementCheck(List((para, rootNo(0))))),
      // One past capacity: position 0 is evicted, its root no longer
      // settles, and both younger neighbours still do.
      assert(over.messagesCursor.get(para) == { head: MaxSettlementRingCapacity + 1, tail: 1 }),
      assert(not(over.settlementCheck(List((para, rootNo(0)))))),
      assert(over.settlementCheck(List((para, rootNo(1))))),
      assert(over.settlementCheck(List((para, rootNo(MaxSettlementRingCapacity))))),
      // The `(para_id, tail)` queue entry is deleted on eviction (§8.2).
      assert(not(over.messagesQueue.keys().contains((para, 0)))),
      assert(not(over.messagesMember.keys().contains((para, rootNo(0))))),
      // The ratcheted footprint: one cursor + capacity× (member + queue).
      assert(over.messagesMember.keys().size() == MaxSettlementRingCapacity),
      assert(over.messagesQueue.keys().size() == MaxSettlementRingCapacity),
      init
    }
  }

  /// §8.2 re-push rule: a repeated root only moves its membership (`seq`) to
  /// the new position, leaving the old queue entry behind. Evicting that old
  /// position must NOT drop the membership (its `seq` differs from `tail`);
  /// the root finally leaves once its newest position is evicted.
  run settlementRingRepushKeepsMembershipTest = {
    val para = CoretimeParaId
    val x: StreamsRoot = { streamsRootBytes: 7777 }
    // `x` at positions 0 AND 1, then distinct roots up to one past capacity:
    // position 0 (holding `x`) is evicted.
    val twice = settlementRingPush(settlementRingPush(initialState, para, x), para, x)
    val s1 = range(0, MaxSettlementRingCapacity - 1).foldl(twice, (acc, i) =>
      settlementRingPush(acc, para, rootNo(i)))
    // One more push evicts position 1, `x`'s newest position.
    val s2 = settlementRingPush(s1, para, rootNo(MaxSettlementRingCapacity - 1))
    all {
      // Position 0 evicted, but `x` was re-pushed at 1: membership survives.
      assert(s1.messagesCursor.get(para).tail == 1),
      assert(not(s1.messagesQueue.keys().contains((para, 0)))),
      assert(s1.messagesMember.get((para, x)) == { seq: 1 }),
      assert(s1.settlementCheck(List((para, x)))),
      // Position 1 evicted too: now the root is really gone.
      assert(s2.messagesCursor.get(para).tail == 2),
      assert(not(s2.settlementCheck(List((para, x))))),
      init
    }
  }

  /// §8.2: `head` and `tail` are `u32` counters advanced with wrapping
  /// arithmetic, and the live window `[tail, head)` is a wrapping range. A
  /// ring whose cursor sits just below `u32::MAX` keeps working across the
  /// wrap: positions run `U32Modulus - 2, U32Modulus - 1, 0, 1, ...`, the
  /// window length is `head.wrapping_sub(tail)`, eviction still takes the
  /// oldest position, and the ring invariant holds throughout.
  run settlementRingWrapsAroundTest = {
    val para = CoretimeParaId
    val start = U32Modulus - 2
    // An empty ring whose next write lands two positions before the wrap.
    val nearWrap: ParachainServiceState = { ...initialState,
      messagesCursor: Map(para -> { head: start, tail: start }) }
    val two = settlementRingPush(settlementRingPush(nearWrap, para, rootNo(0)), para, rootNo(1))
    val three = settlementRingPush(two, para, rootNo(2))
    // Fill to capacity, then one past it, across the wrap.
    val full = range(3, MaxSettlementRingCapacity).foldl(three, (acc, i) =>
      settlementRingPush(acc, para, rootNo(i)))
    val over = settlementRingPush(full, para, rootNo(MaxSettlementRingCapacity))
    all {
      // The second push wraps `head` to 0; `tail` stays put.
      assert(two.messagesCursor.get(para) == { head: 0, tail: start }),
      assert(two.messagesQueue.get((para, start)) == rootNo(0)),
      assert(two.messagesQueue.get((para, U32Modulus - 1)) == rootNo(1)),
      assert(ringLive(two.messagesCursor.get(para)) == 2),
      // Positions continue at 0 after the wrap.
      assert(three.messagesCursor.get(para) == { head: 1, tail: start }),
      assert(three.messagesQueue.get((para, 0)) == rootNo(2)),
      assert(ringLive(three.messagesCursor.get(para)) == 3),
      // At capacity the window spans the wrap.
      assert(full.messagesCursor.get(para)
        == { head: MaxSettlementRingCapacity - 2, tail: start }),
      assert(ringLive(full.messagesCursor.get(para)) == MaxSettlementRingCapacity),
      assert(full.settlementCheck(List((para, rootNo(0))))),
      // One past: the oldest position (just below the wrap) is evicted.
      assert(over.messagesCursor.get(para)
        == { head: MaxSettlementRingCapacity - 1, tail: U32Modulus - 1 }),
      assert(not(over.messagesQueue.keys().contains((para, start)))),
      assert(not(over.settlementCheck(List((para, rootNo(0)))))),
      assert(over.settlementCheck(List((para, rootNo(1))))),
      assert(over.settlementCheck(List((para, rootNo(MaxSettlementRingCapacity))))),
      assert(over.messagesQueue.keys().size() == MaxSettlementRingCapacity),
      init
    }
      // The ring invariant is satisfied on both sides of the wrap.
      .then(setSvc(two))
      .then(expectThat(settlement_ring_coherent))
      .then(setSvc(full))
      .then(expectThat(settlement_ring_coherent))
      .then(setSvc(over))
      .then(expectThat(settlement_ring_coherent and settlement_ring_only_for_live))
  }

  /// §6.1 / §8.2: the settlement ring at capacity is reserved in the baseline
  /// footprint, so pushes never move `usedStateBalance` and the accounting
  /// invariant keeps holding with a full ring. The figures pin the design
  /// doc's derivation: 10 937 for the ring, 80 768 for the baseline.
  run settlementRingReservedInBaselineTest = {
    val para = CoretimeParaId
    val before = initialState.paraInfo(para).usedStateBalance
    val pushed = pushMany(initialState, para, MaxSettlementRingCapacity + 1)
    all {
      assert(SettlementRingFootprint == 10937),
      assert(BaselineFootprint == 80768),
      assert(pushed.paraInfo(para).usedStateBalance == before),
      init
    }
      .then(setSvc(pushed))
      .then(expectThat(all {
        used_balance_consistency,
        used_above_baseline,
        settlement_ring_coherent
      }))
  }

  /// §5.1 step 6 against a ring at CAPACITY: fullness changes nothing about
  /// the check. A receiver naming the evicted root or a never-pushed root is
  /// silently rejected (the whole state is untouched), while the identical
  /// candidate naming any root still inside the window, oldest or newest,
  /// enacts.
  run fullRingStillRejectsMissTest = {
    val sender = CoretimeParaId
    val receiver = AssetHubParaId
    val receiverCode: ValidationCodeHash = { vchBytes: 2 }
    // One past capacity: the ring holds exactly rootNo(1..MaxSettlementRingCapacity)
    // and rootNo(0) has been evicted.
    val fullRing = pushMany(initialState, sender, MaxSettlementRingCapacity + 1)
    pure def wanting(root: StreamsRoot): WorkResult = {
      result: WorkOk(Ok({
        paraId: receiver,
        validationCode: { hash: receiverCode, len: FixedCodeLen },
        parentHeadHash: headHash(0),
        headData: 1,
        upwardMessages: List(),
        lookupAnchor: 0,
        messagesStreamsRoot: None,
        messagesRequiresRoots: List((sender, root))
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
      assert(runOn(rootNo(1)).state.paraInfo(receiver).headData == 1),
      assert(runOn(rootNo(MaxSettlementRingCapacity)).state.paraInfo(receiver).headData == 1),
      init
    }
  }

  /// §5.1 step 6 at FULL fan-in (`MaxRequiresSources` entries): settlement is
  /// all-or-nothing. Against empty rings the max-size set is silently
  /// rejected; with every source's ring holding its root the candidate
  /// enacts; with 31 of 32 present the single missing source still rejects
  /// the whole set.
  run fullFanInSettlementTest = {
    val receiver = AssetHubParaId
    val receiverCode: ValidationCodeHash = { vchBytes: 2 }
    pure def srcPara(i: int): ParaId = MkParaId(100 + i)
    val fullRequires = range(0, MaxRequiresSources).foldl(List(), (acc, i) =>
      acc.append((srcPara(i), rootNo(i))))
    val candidate: WorkResult = {
      result: WorkOk(Ok({
        paraId: receiver,
        validationCode: { hash: receiverCode, len: FixedCodeLen },
        parentHeadHash: headHash(0),
        headData: 1,
        upwardMessages: List(),
        lookupAnchor: 0,
        messagesStreamsRoot: None,
        messagesRequiresRoots: fullRequires
      })),
      authTrace: 0
    }
    // Rings fabricated with the pure push: every source holds its root, or
    // every source but the last.
    val allPresent = range(0, MaxRequiresSources).foldl(initialState, (acc, i) =>
      settlementRingPush(acc, srcPara(i), rootNo(i)))
    val oneMissing = range(0, MaxRequiresSources - 1).foldl(initialState, (acc, i) =>
      settlementRingPush(acc, srcPara(i), rootNo(i)))
    pure def runOn(s: ParachainServiceState): PerPackageOutcome =
      accumulateOnePackage(s, candidate, Set(), None, FixedCodeLen, 1, Map())
    all {
      // Refine would carry this set: it is well-formed at the cap.
      assert(wellFormedRequiresRoots(fullRequires, receiver)),
      // Empty rings: silently rejected, nothing at all changed.
      assert(runOn(initialState).state == initialState),
      // All 32 sources present: enacts.
      assert(runOn(allPresent).state.paraInfo(receiver).headData == 1),
      // 31 of 32: the one missing source rejects the whole set, silently.
      assert(runOn(oneMissing).state == oneMissing),
      assert(runOn(oneMissing).state.paraLog(receiver) == List()),
      init
    }
  }

  /// §5.1 step 6, the line the check-before-head-write ordering exists for:
  /// "a rejected candidate publishes no root of its own". A candidate that
  /// both consumes and declares a root is rejected at settlement, so neither
  /// its head NOR its own `messages_streams_root` lands; the identical
  /// digest, once its source root is live, enacts and publishes in the same
  /// step.
  run rejectedReceiverPublishesNothingTest = {
    val sender = CoretimeParaId
    val receiver = AssetHubParaId
    val receiverCode: ValidationCodeHash = { vchBytes: 2 }
    val own: StreamsRoot = { streamsRootBytes: 555 }
    val pipeline = mkWork(
      receiver, receiverCode, 0, List(),
      Some(own), List((sender, rootNo(0))))
    val vsEmpty = accumulateOnePackage(
      initialState, pipeline, Set(), None, FixedCodeLen, 1, Map())
    val ready = settlementRingPush(initialState, sender, rootNo(0))
    val vsReady = accumulateOnePackage(
      ready, pipeline, Set(), None, FixedCodeLen, 1, Map())
    all {
      // Settlement rejection: no head write and, crucially, no publish.
      assert(vsEmpty.state == initialState),
      assert(not(vsEmpty.state.settlementCheck(List((receiver, own))))),
      // Control: the same digest settles, enacts, and publishes at once.
      assert(vsReady.state.paraInfo(receiver).headData == 1),
      assert(vsReady.state.settlementCheck(List((receiver, own)))),
      init
    }
  }

  /// §8.2: the ring is keyed by `(sender, root)`, so a root live in one
  /// para's ring settles nothing for any other source naming the same value.
  run settlementRingPerSenderNamespaceTest = {
    val a = CoretimeParaId
    val receiver = AssetHubParaId
    val receiverCode: ValidationCodeHash = { vchBytes: 2 }
    val r = rootNo(3)
    val onlyA = settlementRingPush(initialState, a, r)
    val wrongSource = accumulateOnePackage(
      onlyA, mkWork(receiver, receiverCode, 0, List(), None, List((MkParaId(3), r))),
      Set(), None, FixedCodeLen, 1, Map())
    val rightSource = accumulateOnePackage(
      onlyA, mkWork(receiver, receiverCode, 0, List(), None, List((a, r))),
      Set(), None, FixedCodeLen, 1, Map())
    all {
      // The same root value under a different source misses.
      assert(wrongSource.state == onlyA),
      assert(rightSource.state.paraInfo(receiver).headData == 1),
      init
    }
  }

  /// §8 / §8.2: the ring holds the last `MaxSettlementRingCapacity` ENACTED
  /// roots, not just the latest one. A sender chaining two candidates in one
  /// block leaves both roots settleable: a receiver one root behind is not
  /// cut off until `MaxSettlementRingCapacity` newer pushes evict it.
  run settlementRingKeepsEnactedHistoryTest = {
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
      assert(out.state.messagesCursor.get(sender) == { head: 2, tail: 0 }),
      assert(out.state.settlementCheck(List((sender, rootNo(1))))),
      assert(out.state.settlementCheck(List((sender, rootNo(2))))),
      init
    }
  }

  /// §6.3 / §8.3: a forced `parachain_set_head` does not touch the ring.
  /// Roots declared before the head reset stay settleable until evicted, so
  /// a receiver naming one still enacts, even in the same block right after
  /// the reset replays. Deleting the ring here would let a Coretime call made
  /// for one parachain invalidate the in-flight candidates of every
  /// parachain consuming from it.
  run forcedSetHeadKeepsRingTest = {
    val ct = CoretimeParaId
    val ctCode: ValidationCodeHash = { vchBytes: 1 }
    val ah = AssetHubParaId
    val seeded = pushMany(initialState, ah, 3)
    val rolled = parachainSetHead(seeded, ah, 42).state
    // Same block: Coretime's first candidate resets AH's head, its second
    // consumes from AH's pre-reset ring.
    val sameBlock = accumulateAll(
      seeded, Set(), None, FixedCodeLen, 1,
      List(
        mkWork(ct, ctCode, 0, List(ParachainSetHead({ paraId: ah, newHead: 7 })), None, List()),
        mkWork(ct, ctCode, 1, List(), None, List((ah, rootNo(0))))),
      Map())
    all {
      assert(rolled.paraInfo(ah).headData == 42),
      // The ring is exactly as before: cursor, membership, and queue.
      assert(rolled.messagesCursor == seeded.messagesCursor),
      assert(rolled.messagesMember == seeded.messagesMember),
      assert(rolled.messagesQueue == seeded.messagesQueue),
      assert(rolled.settlementCheck(List((ah, rootNo(0))))),
      assert(rolled.settlementCheck(List((ah, rootNo(2))))),
      // Both candidates enact: the receiver settles against the surviving ring.
      assert(sameBlock.state.paraInfo(ah).headData == 7),
      assert(sameBlock.state.paraInfo(ct).headData == 2),
      assert(sameBlock.state.settlementCheck(List((ah, rootNo(0))))),
      init
    }
  }

  /// §6.4 / §8.3: `parachain_clean_up` deletes the ring with the rest of the
  /// per-para state on the first accepted call. Rings of OTHER paras are
  /// untouched.
  run cleanUpDeletesRingTest = {
    val doomed = MkParaId(3)
    val bystander = CoretimeParaId
    // A registered para with no validation code holds exactly its baseline,
    // so clean-up completes in one call (nothing awaits expunge).
    val registered =
      parachainSetStateBalance(initialState, doomed, InitialParaBalance).state
    val seeded = pushMany(pushMany(registered, doomed, 2), bystander, 1)
    val cleaned = parachainCleanUp(seeded, doomed, 0).state
    all {
      assert(seeded.settlementCheck(List((doomed, rootNo(0))))),
      assert(not(cleaned.isLive(doomed))),
      // The doomed para's ring is gone with it...
      assert(not(cleaned.messagesCursor.keys().contains(doomed))),
      assert(cleaned.messagesMember.keys().filter(k => k._1 == doomed) == Set()),
      assert(cleaned.messagesQueue.keys().filter(k => k._1 == doomed) == Set()),
      // ...while the bystander's ring still settles.
      assert(cleaned.settlementCheck(List((bystander, rootNo(0))))),
      init
    }
  }

  /// §6.4 / §8.3: the ring is deleted on the PARTIAL clean-up branch too. A
  /// para whose validation code enters forget-limbo stays live with
  /// `isDeregistering` set, but its ring is already gone: the ring is plain
  /// storage, not a preimage, so it needs no two-step release.
  run cleanUpPartialStillDeletesRingTest = {
    val doomed = MkParaId(3)
    val code: ValidationCodeHash = { vchBytes: 77 }
    val registered =
      parachainSetStateBalance(initialState, doomed, InitialParaBalance).state
    val withCode =
      parachainSetValidationCode(registered, doomed, code, FixedCodeLen, 0).state
    // Provided code: the clean-up's forget only unrequests (two-step, §6.1),
    // so the para survives in the deregistering state.
    val provided = markProvided(withCode, vchAsHash(code), FixedCodeLen)
    val seeded = settlementRingPush(provided, doomed, rootNo(5))
    val out = parachainCleanUp(seeded, doomed, 0).state
    all {
      assert(seeded.settlementCheck(List((doomed, rootNo(5))))),
      // Partial branch taken: still live, awaiting the expunging forget.
      assert(out.isLive(doomed)),
      assert(out.paraInfo(doomed).isDeregistering),
      // The ring is nonetheless gone already.
      assert(not(out.messagesCursor.keys().contains(doomed))),
      assert(out.messagesMember.keys().filter(k => k._1 == doomed) == Set()),
      assert(out.messagesQueue.keys().filter(k => k._1 == doomed) == Set()),
      assert(not(out.settlementCheck(List((doomed, rootNo(5)))))),
      init
    }
  }

  /// §8.3: `parachain_clean_up` takes effect when the candidate carrying it
  /// replays (§5.1 step 7), and within a block the work packages are taken in
  /// order. Clean-up first: a LATER digest consuming from the deleted ring
  /// misses settlement and only the carrier enacts. Receiver first: it
  /// settles against the still-live ring and enacts, then the clean-up
  /// replays; the receiver's enactment is valid and nothing is rolled back.
  run sameBlockCleanUpTest = {
    val ct = CoretimeParaId
    val ctCode: ValidationCodeHash = { vchBytes: 1 }
    val doomed = MkParaId(3)
    val rootS = rootNo(9)
    // A registered para with no validation code holds exactly its baseline,
    // so the clean-up completes in one call.
    val seeded = settlementRingPush(
      parachainSetStateBalance(initialState, doomed, InitialParaBalance).state,
      doomed, rootS)
    val cleanUp = List(ParachainCleanUp(doomed))
    val cleanUpFirst = accumulateAll(
      seeded, Set(), None, FixedCodeLen, 1,
      List(
        mkWork(ct, ctCode, 0, cleanUp, None, List()),
        mkWork(ct, ctCode, 1, List(), None, List((doomed, rootS)))),
      Map())
    val receiverFirst = accumulateAll(
      seeded, Set(), None, FixedCodeLen, 1,
      List(
        mkWork(ct, ctCode, 0, List(), None, List((doomed, rootS))),
        mkWork(ct, ctCode, 1, cleanUp, None, List())),
      Map())
    all {
      assert(seeded.settlementCheck(List((doomed, rootS)))),
      // Clean-up first: the para and its ring are gone, the receiver misses.
      assert(cleanUpFirst.state.paraInfo(ct).headData == 1),
      assert(not(cleanUpFirst.state.isLive(doomed))),
      assert(not(cleanUpFirst.state.settlementCheck(List((doomed, rootS))))),
      // Receiver first: both enact; the ring dies afterwards and the
      // receiver's head stays.
      assert(receiverFirst.state.paraInfo(ct).headData == 2),
      assert(not(receiverFirst.state.isLive(doomed))),
      assert(not(receiverFirst.state.settlementCheck(List((doomed, rootS))))),
      init
    }
  }

  /// The `settlement_ring_coherent` invariant is not vacuous: each corruption
  /// class it exists for, an entry with no cursor, a window past capacity
  /// (plain or wrapped), a queue root with no membership, is rejected, while
  /// a genuinely pushed ring passes. (Same pattern as the gapped-buckets
  /// fixture.)
  run settlementRingInvariantCatchesCorruptionTest = {
    val ct = CoretimeParaId
    val orphanMember: ParachainServiceState = { ...initialState,
      messagesMember: Map((ct, rootNo(0)) -> { seq: 0 }) }
    val overWideWindow: ParachainServiceState = { ...initialState,
      messagesCursor: Map(ct -> { head: MaxSettlementRingCapacity + 1, tail: 0 }) }
    // `head` behind `tail` by one is a wrapping window of `U32Modulus - 1`
    // positions, far past capacity.
    val wrappedOverWide: ParachainServiceState = { ...initialState,
      messagesCursor: Map(ct -> { head: 0, tail: 1 }) }
    val memberlessQueue: ParachainServiceState = { ...initialState,
      messagesCursor: Map(ct -> { head: 1, tail: 0 }),
      messagesQueue: Map((ct, 0) -> rootNo(0)) }
    init
      .then(setSvc(orphanMember))
      .then(expectThat(not(settlement_ring_coherent)))
      .then(setSvc(overWideWindow))
      .then(expectThat(not(settlement_ring_coherent)))
      .then(setSvc(wrappedOverWide))
      .then(expectThat(not(settlement_ring_coherent)))
      .then(setSvc(memberlessQueue))
      .then(expectThat(not(settlement_ring_coherent)))
      .then(setSvc(settlementRingPush(initialState, ct, rootNo(0))))
      .then(expectThat(settlement_ring_coherent and settlement_ring_only_for_live))
  }

  /// §5.1: within one block, Accumulate takes the work packages in order, and
  /// §8 settles each against the rings as they stand at that point.
  /// Sender-then-receiver settles in the SAME block, because the sender's
  /// step-6 push lands before the receiver's settlement check;
  /// receiver-then-sender leaves the receiver silently rejected while the
  /// sender still enacts, and the receiver resubmits against the now-live
  /// root (§8).
  run sameBlockOrderingTest = {
    val sender = CoretimeParaId
    val senderCode: ValidationCodeHash = { vchBytes: 1 }
    val receiver = AssetHubParaId
    val receiverCode: ValidationCodeHash = { vchBytes: 2 }
    val send = (sender, senderCode, StreamsRootDeclared(StreamsRootB), RequiresRootsUndeclared)
    val consume = (receiver, receiverCode, StreamsRootUndeclared,
      RequiresRootsDeclared(List((sender, StreamsRootB))))
    init
      .then(applyMessagingPaths(List(send, consume)))
      .then(expectThat(all {
        // Sender first: both enact in one block.
        svc.paraInfo(sender).headData == 1,
        svc.paraInfo(receiver).headData == 1,
        settlement_ring_coherent
      }))
      .then(init)
      .then(applyMessagingPaths(List(consume, send)))
      .then(expectThat(all {
        // Receiver first: it misses the ring and only the sender enacts.
        svc.paraInfo(sender).headData == 1,
        svc.paraInfo(receiver).headData == 0,
        svc.settlementCheck(List((sender, StreamsRootB))),
        settlement_ring_coherent
      }))
  }
}
