// Messaging tests (§8).
//
// - Refine-side `set_messages_streams_root` / `set_messages_requires_roots` declarations (§4.3).
// - Accumulate settlement check (§5.1 step 5).
// - Settlement ring push, eviction and list links (§8.1).
// - Ring reservation in the baseline footprint (§6.1) and teardown on clean-up (§6.4).
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
      validationCode: codeHash,
      parentHeadHash: headHash(parent),
      headData: parent + 1,
      upwardMessages: msgs,
      lookupAnchor: 0,
      messagesStreamsRoot: streamsRoot,
      messagesRequiresRoots: requires
    })),
    authTrace: 0
  }

  /// §4.3 / §8: either messaging host call invoked more than once aborts
  /// the validation code with `MessagesCallRepeated`; called at most once
  /// each, the declarations travel verbatim in the digest, the `StreamsRoot`
  /// as an option and the requires roots as the declared set.
  run messagesRepeatedCallRejectedTest = {
    val ct = CoretimeParaId
    val streamsRootTwice =
      refineWithMessaging(initialState, ct, StreamsRootRepeated, RequiresRootsUndeclared)
    val requiresTwice =
      refineWithMessaging(initialState, ct, StreamsRootUndeclared, RequiresRootsRepeated)
    val onceEach = refineWithMessaging(
      initialState, ct,
      StreamsRootDeclared(StreamsRootA),
      RequiresRootsDeclared(List((AssetHubParaId, StreamsRootB))))
    val wellFormed = wellFormedCalls(headHash(0), 1, List())
    all {
      assert(streamsRootTwice == refineCallsErr(initialState, ct, MessagesCallRepeated)),
      assert(requiresTwice == refineCallsErr(initialState, ct, MessagesCallRepeated)),
      // Both called once: the digest carries the declarations verbatim.
      assert(onceEach == WorkOk(Ok({
        paraId: ct,
        validationCode: activeCandidateCode(initialState, ct).hash,
        parentHeadHash: headHash(0),
        headData: 1,
        upwardMessages: List(),
        lookupAnchor: 0,
        messagesStreamsRoot: Some(StreamsRootA),
        messagesRequiresRoots: List((AssetHubParaId, StreamsRootB))
      }))),
      // Failure precedence, pinned for the Rust harness. The first aborting
      // call wins: a `report_error` before the messaging calls means they are
      // never made.
      assert(refineCalls(initialState, ct,
        wellFormed.append(ReportError(9))
          .concat(messagingCalls(StreamsRootRepeated, RequiresRootsRepeated)))
        == refineCallsErr(initialState, ct, Opaque(9))),
      // The messaging calls abort in the call itself, before the wrapper's
      // post-run head-declaration checks.
      assert(refineCalls(initialState, ct,
        messagingCalls(StreamsRootRepeated, RequiresRootsUndeclared))
        == refineCallsErr(initialState, ct, MessagesCallRepeated)),
      // A repeat is settled before the set's own validity.
      assert(refineCalls(initialState, ct,
        wellFormed.concat(
          messagingCalls(StreamsRootRepeated, RequiresRootsDeclared(List((ct, StreamsRootA))))))
        == refineCallsErr(initialState, ct, MessagesCallRepeated)),
      init
    }
  }

  /// §4.3 / §8: a requires set past `MaxRequiresSources`, repeating a
  /// source `ParaId`, or naming the candidate's own para is rejected with
  /// `MessagesInvalidRequires`. Exactly `MaxRequiresSources` distinct foreign
  /// sources pass Refine: whether they SETTLE is Accumulate's concern alone
  /// (§5.1 step 5), so Refine accepts entries no ring will ever match.
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
      assert(oneTooMany == refineCallsErr(initialState, ct, MessagesInvalidRequires)),
      assert(isOkDigest(atCap)),
      assert(dupSource == refineCallsErr(initialState, ct, MessagesInvalidRequires)),
      assert(namesItself == refineCallsErr(initialState, ct, MessagesInvalidRequires)),
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

  /// §5.1 step 5: the settlement check, end to end through the shared
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
        svc.messagesCursor.get(sender) == { oldest: StreamsRootA, newest: StreamsRootA, len: 1 },
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
      validationCode: code,
      parentHeadHash: headHash(parent),
      // A stale candidate declares the sentinel as its new head too, as in
      // `buildPackage`; `+ 1` would overflow it.
      headData: if (parent == StaleParentHead) parent else parent + 1,
      upwardMessages: List(),
      lookupAnchor: 0,
      messagesStreamsRoot: Some(StreamsRootA),
      messagesRequiresRoots: List()
    }
    val rejected = accumulateOnePackage(
      initialState, { result: WorkOk(Ok(candidate(StaleParentHead))), authTrace: 0 },
      Set(), List(), FixedCodeLen, 1, Map())
    val enacted = accumulateOnePackage(
      initialState, { result: WorkOk(Ok(candidate(0))), authTrace: 0 },
      Set(), List(), FixedCodeLen, 1, Map())
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

  /// §8.1 eviction is in insertion order: a root leaves the ring only after
  /// `MaxSettlementRingCapacity` newer distinct pushes. At capacity the ring holds
  /// exactly the last `MaxSettlementRingCapacity` roots: the oldest stops settling,
  /// its `messages_member` entry is deleted and its link becomes the new
  /// `oldest`, every younger root still settles, and the footprint stays at
  /// `1 + MaxSettlementRingCapacity` entries.
  run settlementRingEvictionTest = {
    val para = CoretimeParaId
    val full = pushMany(initialState, para, MaxSettlementRingCapacity)
    val over = settlementRingPush(full, para, rootNo(MaxSettlementRingCapacity))
    all {
      // At capacity: every pushed root still settles.
      assert(full.messagesCursor.get(para) == {
        oldest: rootNo(0),
        newest: rootNo(MaxSettlementRingCapacity - 1),
        len: MaxSettlementRingCapacity
      }),
      assert(full.settlementCheck(List((para, rootNo(0))))),
      // One past capacity: the oldest root is evicted and no longer settles,
      // and both younger neighbours still do.
      assert(over.messagesCursor.get(para) == {
        oldest: rootNo(1), newest: rootNo(MaxSettlementRingCapacity), len: MaxSettlementRingCapacity
      }),
      assert(not(over.settlementCheck(List((para, rootNo(0)))))),
      assert(over.settlementCheck(List((para, rootNo(1))))),
      assert(over.settlementCheck(List((para, rootNo(MaxSettlementRingCapacity))))),
      assert(not(over.messagesMember.keys().contains((para, rootNo(0))))),
      // The previous newest now links to the new root, which links to itself.
      assert(over.messagesMember.get((para, rootNo(MaxSettlementRingCapacity - 1)))
        == rootNo(MaxSettlementRingCapacity)),
      assert(over.messagesMember.get((para, rootNo(MaxSettlementRingCapacity)))
        == rootNo(MaxSettlementRingCapacity)),
      // The ratcheted footprint: one cursor + capacity× member.
      assert(over.messagesMember.keys().size() == MaxSettlementRingCapacity),
      init
    }
      .then(setSvc(over))
      .then(expectThat(settlement_ring_coherent))
  }

  /// Repeated roots are skipped (§8.1).
  ///
  /// - Pushing a root already in the ring leaves the state unchanged, even at capacity.
  /// - The root keeps its original position and is evicted on its original schedule.
  run settlementRingRepeatedRootSkippedTest = {
    val para = CoretimeParaId
    val x: StreamsRoot = { streamsRootBytes: 7777 }
    val once = settlementRingPush(initialState, para, x)
    val twice = settlementRingPush(once, para, x)
    // Fill the ring to capacity behind `x`, which stays the oldest root.
    val s1 = range(0, MaxSettlementRingCapacity - 1).foldl(twice, (acc, i) =>
      settlementRingPush(acc, para, rootNo(i)))
    val s1Repush = settlementRingPush(s1, para, x)
    // Evicts the oldest root, which is still `x`.
    val s2 = settlementRingPush(s1Repush, para, rootNo(MaxSettlementRingCapacity - 1))
    all {
      // Repeated push is a no-op.
      assert(twice == once),
      assert(twice.messagesCursor.get(para) == { oldest: x, newest: x, len: 1 }),
      assert(twice.messagesMember == Map((para, x) -> x)),
      // Repeated push at capacity is a no-op.
      assert(s1.messagesCursor.get(para).len == MaxSettlementRingCapacity),
      assert(s1Repush == s1),
      // `x` leaves on its original schedule, membership included.
      assert(s2.messagesCursor.get(para).oldest == rootNo(0)),
      assert(not(s2.messagesMember.keys().contains((para, x)))),
      assert(not(s2.settlementCheck(List((para, x))))),
      assert(s2.settlementCheck(List((para, rootNo(0))))),
      init
    }
      .then(setSvc(twice))
      .then(expectThat(settlement_ring_coherent))
      .then(setSvc(s2))
      .then(expectThat(settlement_ring_coherent))
  }

  /// §8.1: the ring is a singly linked list in insertion order. Following
  /// the links from `oldest` visits every pushed root once, in push order,
  /// and ends at `newest`, which links to itself. Eviction moves `oldest`
  /// along its link and leaves the rest of the list untouched.
  run settlementRingLinksInPushOrderTest = {
    val para = CoretimeParaId
    val three = pushMany(initialState, para, 3)
    val full = pushMany(initialState, para, MaxSettlementRingCapacity)
    val over = settlementRingPush(full, para, rootNo(MaxSettlementRingCapacity))
    all {
      assert(three.messagesCursor.get(para) == { oldest: rootNo(0), newest: rootNo(2), len: 3 }),
      assert(three.messagesMember == Map(
        (para, rootNo(0)) -> rootNo(1),
        (para, rootNo(1)) -> rootNo(2),
        (para, rootNo(2)) -> rootNo(2))),
      assert(ringRoots(three, para) == List(rootNo(0), rootNo(1), rootNo(2))),
      assert(ringRoots(over, para) == range(1, MaxSettlementRingCapacity + 1)
        .foldl(List(), (acc, i) => acc.append(rootNo(i)))),
      init
    }
      .then(setSvc(three))
      .then(expectThat(settlement_ring_coherent))
      .then(setSvc(over))
      .then(expectThat(settlement_ring_coherent and settlement_ring_only_for_live))
  }

  /// §6.1 / §8.1: the settlement ring at capacity is reserved in the baseline
  /// footprint, so pushes never move `usedStateBalance` and the accounting
  /// invariant keeps holding with a full ring. The figures pin the design
  /// doc's derivation: 7 346 for the ring, 77 171 for the baseline.
  run settlementRingReservedInBaselineTest = {
    val para = CoretimeParaId
    val before = initialState.paraInfo(para).usedStateBalance
    val pushed = pushMany(initialState, para, MaxSettlementRingCapacity + 1)
    all {
      assert(SettlementRingFootprint == 7346),
      assert(BaselineFootprint == 77171),
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

  /// §5.1 step 5 against a ring at CAPACITY: fullness changes nothing about
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
        validationCode: receiverCode,
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
      accumulateOnePackage(fullRing, wanting(root), Set(), List(), FixedCodeLen, 1, Map())
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

  /// §5.1 step 5 at FULL fan-in (`MaxRequiresSources` entries): settlement is
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
        validationCode: receiverCode,
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
      accumulateOnePackage(s, candidate, Set(), List(), FixedCodeLen, 1, Map())
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

  /// §5.1 step 5, the line the check-before-head-write ordering exists for:
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
      initialState, pipeline, Set(), List(), FixedCodeLen, 1, Map())
    val ready = settlementRingPush(initialState, sender, rootNo(0))
    val vsReady = accumulateOnePackage(
      ready, pipeline, Set(), List(), FixedCodeLen, 1, Map())
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

  /// §8.1: the ring is keyed by `(sender, root)`, so a root live in one
  /// para's ring settles nothing for any other source naming the same value.
  run settlementRingPerSenderNamespaceTest = {
    val a = CoretimeParaId
    val receiver = AssetHubParaId
    val receiverCode: ValidationCodeHash = { vchBytes: 2 }
    val r = rootNo(3)
    val onlyA = settlementRingPush(initialState, a, r)
    val wrongSource = accumulateOnePackage(
      onlyA, mkWork(receiver, receiverCode, 0, List(), None, List((MkParaId(3), r))),
      Set(), List(), FixedCodeLen, 1, Map())
    val rightSource = accumulateOnePackage(
      onlyA, mkWork(receiver, receiverCode, 0, List(), None, List((a, r))),
      Set(), List(), FixedCodeLen, 1, Map())
    all {
      // The same root value under a different source misses.
      assert(wrongSource.state == onlyA),
      assert(rightSource.state.paraInfo(receiver).headData == 1),
      init
    }
  }

  /// §8 / §8.1: the ring holds the last `MaxSettlementRingCapacity` ENACTED
  /// roots, not just the latest one. A sender chaining two candidates in one
  /// block leaves both roots settleable: a receiver one root behind is not
  /// cut off until `MaxSettlementRingCapacity` newer pushes evict it.
  run settlementRingKeepsEnactedHistoryTest = {
    val sender = CoretimeParaId
    val senderCode: ValidationCodeHash = { vchBytes: 1 }
    val out = accumulateAll(
      initialState, Set(), List(), FixedCodeLen, 1,
      List(
        mkWork(sender, senderCode, 0, List(), Some(rootNo(1)), List()),
        mkWork(sender, senderCode, 1, List(), Some(rootNo(2)), List())),
      Map())
    all {
      assert(out.state.paraInfo(sender).headData == 2),
      assert(out.state.messagesCursor.get(sender)
        == { oldest: rootNo(1), newest: rootNo(2), len: 2 }),
      assert(out.state.settlementCheck(List((sender, rootNo(1))))),
      assert(out.state.settlementCheck(List((sender, rootNo(2))))),
      init
    }
  }

  /// §6.3 / §8.1: a forced `parachain_set_head` does not touch the ring.
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
      seeded, Set(), List(), FixedCodeLen, 1,
      List(
        mkWork(ct, ctCode, 0, List(ParachainSetHead({ paraId: ah, newHead: 7 })), None, List()),
        mkWork(ct, ctCode, 1, List(), None, List((ah, rootNo(0))))),
      Map())
    all {
      assert(rolled.paraInfo(ah).headData == 42),
      // The ring is exactly as before: cursor and membership.
      assert(rolled.messagesCursor == seeded.messagesCursor),
      assert(rolled.messagesMember == seeded.messagesMember),
      assert(rolled.settlementCheck(List((ah, rootNo(0))))),
      assert(rolled.settlementCheck(List((ah, rootNo(2))))),
      // Both candidates enact: the receiver settles against the surviving ring.
      assert(sameBlock.state.paraInfo(ah).headData == 7),
      assert(sameBlock.state.paraInfo(ct).headData == 2),
      assert(sameBlock.state.settlementCheck(List((ah, rootNo(0))))),
      init
    }
  }

  /// §6.4 / §8.1: `parachain_clean_up` deletes the ring with the rest of the
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
      // ...while the bystander's ring still settles.
      assert(cleaned.settlementCheck(List((bystander, rootNo(0))))),
      init
    }
  }

  /// §6.4 / §8.1: the ring is deleted on the PARTIAL clean-up branch too. A
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
    val provided = markProvided(withCode, code.asPreimage(), FixedCodeLen)
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
      assert(not(out.settlementCheck(List((doomed, rootNo(5)))))),
      init
    }
  }

  /// §8.1: `parachain_clean_up` takes effect when the candidate carrying it
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
      seeded, Set(), List(), FixedCodeLen, 1,
      List(
        mkWork(ct, ctCode, 0, cleanUp, None, List()),
        mkWork(ct, ctCode, 1, List(), None, List((doomed, rootS)))),
      Map())
    val receiverFirst = accumulateAll(
      seeded, Set(), List(), FixedCodeLen, 1,
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

  /// `settlement_ring_coherent` rejects corrupted rings:
  ///
  /// - A member entry with no cursor.
  /// - A `len` of zero or past capacity.
  /// - A link to a root with no member entry.
  /// - A member entry the list never reaches.
  /// - A `newest` root that does not link to itself.
  /// - A live root re-pushed without the §8.1 step 1 skip, which strands
  ///   every root after it.
  ///
  /// A ring built through pushes still passes.
  run settlementRingInvariantCatchesCorruptionTest = {
    val ct = CoretimeParaId
    val r0 = rootNo(0)
    val r1 = rootNo(1)
    val r2 = rootNo(2)
    val orphanMember: ParachainServiceState = { ...initialState,
      messagesMember: Map((ct, r0) -> r0) }
    val emptyLen: ParachainServiceState = { ...initialState,
      messagesCursor: Map(ct -> { oldest: r0, newest: r0, len: 0 }) }
    val lenPastCapacity: ParachainServiceState = { ...initialState,
      messagesCursor: Map(ct -> { oldest: r0, newest: r0, len: MaxSettlementRingCapacity + 1 }),
      messagesMember: Map((ct, r0) -> r0) }
    val brokenLink: ParachainServiceState = { ...initialState,
      messagesCursor: Map(ct -> { oldest: r0, newest: r1, len: 2 }),
      messagesMember: Map((ct, r0) -> r1) }
    val strandedMember: ParachainServiceState = { ...initialState,
      messagesCursor: Map(ct -> { oldest: r0, newest: r0, len: 1 }),
      messagesMember: Map((ct, r0) -> r0, (ct, r1) -> r1) }
    val newestNotSelfLinked: ParachainServiceState = { ...initialState,
      messagesCursor: Map(ct -> { oldest: r0, newest: r1, len: 2 }),
      messagesMember: Map((ct, r0) -> r1, (ct, r1) -> r0) }
    // `r0 -> r1 -> r2` with `r1` pushed again and no skip: `r2` now links to
    // `r1`, `r1` to itself, and the walk from `r0` never reaches `r2`.
    val repushedWithoutSkip: ParachainServiceState = { ...initialState,
      messagesCursor: Map(ct -> { oldest: r0, newest: r1, len: 4 }),
      messagesMember: Map((ct, r0) -> r1, (ct, r1) -> r1, (ct, r2) -> r1) }
    init
      .then(setSvc(orphanMember))
      .then(expectThat(not(settlement_ring_coherent)))
      .then(setSvc(emptyLen))
      .then(expectThat(not(settlement_ring_coherent)))
      .then(setSvc(lenPastCapacity))
      .then(expectThat(not(settlement_ring_coherent)))
      .then(setSvc(brokenLink))
      .then(expectThat(not(settlement_ring_coherent)))
      .then(setSvc(strandedMember))
      .then(expectThat(not(settlement_ring_coherent)))
      .then(setSvc(newestNotSelfLinked))
      .then(expectThat(not(settlement_ring_coherent)))
      .then(setSvc(repushedWithoutSkip))
      .then(expectThat(not(settlement_ring_coherent)))
      .then(setSvc(settlementRingPush(initialState, ct, r0)))
      .then(expectThat(settlement_ring_coherent and settlement_ring_only_for_live))
      .then(setSvc(pushMany(initialState, ct, 5)))
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
