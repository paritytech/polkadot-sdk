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
  import foreign_services.* from "../foreign_services"
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

  /// `accumulateAll` without a due-assign flush before it and without running
  /// out of gas: just the block's packages, in order, from `state`.
  pure def accumulatePackages(
    state: ParachainServiceState, solicited: Set[(ParaId, Hash, int)],
    prevDesignate: List[ValidatorKey], codeLen: int, now: Timeslot,
    packages: List[WorkResult], foreign: ForeignServices
  ): MultiPackageOutcome =
    accumulateAll(
      { state: state, assigns: List() }, solicited, prevDesignate, codeLen, now, packages,
      foreign, Never
    ).current

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
      upwardMessages: withIndexIds(msgs),
      lookupAnchor: 0,
      messagesStreamsRoot: streamsRoot,
      messagesRequiresRoots: requires
    })),
    authTrace: 0
  }

  /// A well-formed candidate for `para` continuing its current head in `s`,
  /// refined against its active code: sends `msgs`, then makes the given
  /// messaging declarations. For driving `initCleanUp` scenarios as one block.
  pure def refinedCandidate(
    s: ParachainServiceState, para: ParaId, msgs: List[UpwardMessage],
    provides: StreamsRootDeclaration, requires: RequiresRootsDeclaration
  ): WorkResult = chainedCandidate(s, para, 0, msgs, provides, requires)

  /// As `refinedCandidate`, but continuing the head `k` candidates past `para`'s
  /// current one, for chaining several of its candidates in one block.
  pure def chainedCandidate(
    s: ParachainServiceState, para: ParaId, k: int, msgs: List[UpwardMessage],
    provides: StreamsRootDeclaration, requires: RequiresRootsDeclaration
  ): WorkResult = {
    val parent = s.paraInfo(para).headData + k
    {
      result: refineCalls(s, para,
        wellFormedCalls(headHash(parent), parent + 1, msgs)
          .concat(messagingCalls(provides, requires))),
      authTrace: 0
    }
  }

  /// Accumulate `pkgs` as one block at `now + 1`, the same `accumulateBlock`
  /// pipeline as the model step.
  action applyBlock(pkgs: List[WorkResult]): bool = {
    val out = accumulateBlock(
      svc, now + 1, jamStagingSet, solicitedSet, FixedCodeLen, pkgs, foreignServices)
    all {
      svc' = out.state,
      now' = now + 1,
      jamStagingSet' = out.staging,
      solicitedSet' = out.solicited,
      prevSvc' = svc,
      lastStepWorkResults' = out.results,
      lastStepAssigns' = out.assigns,
      lastStepAssignBudget' = FullAssignBudget,
      lastStepOutOfGas' = Never,
      lastHeadRoot' = out.headRoot,
      foreignServices' = out.foreign,
      logPrunedBelow' = out.logPruned.foldl(logPrunedBelow, (m, pr) => m.put(pr._1, pr._2)),
      lastStepOkLogs' = out.okLogs
    }
  }

  /// As `applyBlock`, but the invocation runs out of gas at `outOfGas`, which
  /// restores the last checkpoint (§5.1).
  action applyBlockOutOfGas(pkgs: List[WorkResult], outOfGas: OutOfGas): bool = {
    val out = accumulateBlockBudgeted(
      svc, now + 1, jamStagingSet, solicitedSet, FixedCodeLen, pkgs, foreignServices,
      FullAssignBudget, outOfGas)
    all {
      svc' = out.state,
      now' = now + 1,
      jamStagingSet' = out.staging,
      solicitedSet' = out.solicited,
      prevSvc' = svc,
      lastStepWorkResults' = out.results,
      lastStepAssigns' = out.assigns,
      lastStepAssignBudget' = FullAssignBudget,
      lastStepOutOfGas' = outOfGas,
      lastHeadRoot' = out.headRoot,
      foreignServices' = out.foreign,
      logPrunedBelow' = out.logPruned.foldl(logPrunedBelow, (m, pr) => m.put(pr._1, pr._2)),
      lastStepOkLogs' = out.okLogs
    }
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

  /// §8 / §8.1: the ring holds the last `MaxSettlementRingCapacity` distinct ENACTED
  /// roots, not just the latest one. A sender chaining two candidates in one
  /// block leaves both roots settleable: a receiver one root behind is not
  /// cut off until `MaxSettlementRingCapacity` newer pushes evict it.
  run settlementRingKeepsEnactedHistoryTest = {
    val sender = CoretimeParaId
    val senderCode: ValidationCodeHash = { vchBytes: 1 }
    val out = accumulatePackages(
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
    val sameBlock = accumulatePackages(
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
  /// per-para state once everything is expunged. Rings of OTHER paras are
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

  /// §6.4 / §8.1: the ring survives the PARTIAL clean-up branch. A para whose
  /// validation code enters forget-limbo stays live with `isDeregistering` set
  /// and keeps its ring until the call that expunges everything.
  run cleanUpPartialKeepsRingTest = {
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
      // The ring is still in place.
      assert(out.messagesCursor.keys().contains(doomed)),
      assert(out.settlementCheck(List((doomed, rootNo(5))))),
      init
    }
  }

  /// §6.4 / §8.1: the RETRY of a partial clean-up deletes the ring. Once the
  /// code is expungeable (`now > due`), the second `parachain_clean_up` takes
  /// the all-expunged branch: the para and its ring go together, leaving no
  /// ring for a removed para (`settlement_ring_only_for_live`).
  run cleanUpRetryDeletesRingTest = {
    val doomed = MkParaId(3)
    val bystander = CoretimeParaId
    val code: ValidationCodeHash = { vchBytes: 77 }
    val registered =
      parachainSetStateBalance(initialState, doomed, InitialParaBalance).state
    val withCode =
      parachainSetValidationCode(registered, doomed, code, FixedCodeLen, 0).state
    val provided = markProvided(withCode, code.asPreimage(), FixedCodeLen)
    val seeded = pushMany(settlementRingPush(provided, doomed, rootNo(5)), bystander, 1)
    // First call: the forget only unrequests, so the ring stays.
    val partial = parachainCleanUp(seeded, doomed, 0).state
    // Retry strictly past the deadline: the forget expunges, para removed.
    val retried = parachainCleanUp(partial, doomed, ExpungePeriod + 1).state
    all {
      assert(partial.paraInfo(doomed).isDeregistering),
      assert(partial.settlementCheck(List((doomed, rootNo(5))))),
      assert(not(retried.isLive(doomed))),
      assert(not(retried.preimageStatus.keys().contains((code.asPreimage(), FixedCodeLen)))),
      // The ring is gone with the para...
      assert(not(retried.messagesCursor.keys().contains(doomed))),
      assert(retried.messagesMember.keys().filter(k => k._1 == doomed) == Set()),
      assert(not(retried.settlementCheck(List((doomed, rootNo(5)))))),
      // ...while the bystander's ring still settles.
      assert(retried.settlementCheck(List((bystander, rootNo(0))))),
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
    val cleanUpFirst = accumulatePackages(
      seeded, Set(), List(), FixedCodeLen, 1,
      List(
        mkWork(ct, ctCode, 0, cleanUp, None, List()),
        mkWork(ct, ctCode, 1, List(), None, List((doomed, rootS)))),
      Map())
    val receiverFirst = accumulatePackages(
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

  /// §5.1 step 1 / §6.4 / §8.1: a PARTIAL clean-up in the same block. The
  /// code enters forget-limbo, so the para stays live with `isDeregistering`
  /// set and its ring intact. A later candidate of the deregistering para
  /// itself is dropped at step 1 and pushes nothing; a later receiver naming
  /// one of its roots still settles and enacts. The control shows the same
  /// doomed candidate enacts and pushes when no clean-up precedes it.
  run sameBlockPartialCleanUpTest = {
    val ct = CoretimeParaId
    val ctCode: ValidationCodeHash = { vchBytes: 1 }
    val doomed = MkParaId(3)
    val doomedCode: ValidationCodeHash = { vchBytes: 77 }
    val rootOld = rootNo(5)
    val rootNew = rootNo(6)
    val registered =
      parachainSetStateBalance(initialState, doomed, InitialParaBalance).state
    val withCode =
      parachainSetValidationCode(registered, doomed, doomedCode, FixedCodeLen, 0).state
    // Provided code: the clean-up's forget only unrequests, so it is partial.
    val seeded = settlementRingPush(
      markProvided(withCode, doomedCode.asPreimage(), FixedCodeLen), doomed, rootOld)
    val doomedCand = mkWork(doomed, doomedCode, 0, List(), Some(rootNew), List())
    val receiver = mkWork(ct, ctCode, 1, List(), None, List((doomed, rootOld)))
    val partialFirst = accumulatePackages(
      seeded, Set(), List(), FixedCodeLen, 1,
      List(mkWork(ct, ctCode, 0, List(ParachainCleanUp(doomed)), None, List()),
        doomedCand, receiver),
      Map())
    val control = accumulatePackages(
      seeded, Set(), List(), FixedCodeLen, 1, List(doomedCand), Map())
    all {
      // Control: without the clean-up the doomed candidate enacts and pushes.
      assert(control.state.paraInfo(doomed).headData == 1),
      assert(control.state.settlementCheck(List((doomed, rootNew)))),
      // Partial branch taken: live, deregistering, ring kept.
      assert(partialFirst.state.isLive(doomed)),
      assert(partialFirst.state.paraInfo(doomed).isDeregistering),
      assert(partialFirst.state.messagesCursor.get(doomed) ==
        seeded.messagesCursor.get(doomed)),
      // (i) The deregistering para's own candidate is dropped: no head write,
      // no push.
      assert(partialFirst.state.paraInfo(doomed).headData == 0),
      assert(not(partialFirst.state.settlementCheck(List((doomed, rootNew))))),
      // (ii) The receiver settles against the surviving ring and enacts.
      assert(partialFirst.state.paraInfo(ct).headData == 2),
      init
    }
  }

  /// `parent_head_continuity` regression (§5.1 step 4, §6.4, §8.1): Coretime
  /// fully removes deregistering para 4 (`initCleanUp`, `now > due`) and, later
  /// in the same block, para 3 requires `(4, StreamsRootA)`. The ring went with
  /// para 4, so para 3 misses settlement and keeps its head; the replay must
  /// delete the ring too rather than settle para 3 against `prevSvc`'s rings.
  run cleanUpThenReceiverContinuityTest = {
    val gone = MkParaId(4)
    val receiver = MkParaId(3)
    initCleanUp
      .then(applyBlock(List(
        refinedCandidate(svc, CoretimeParaId, List(ParachainCleanUp(gone)),
          StreamsRootUndeclared, RequiresRootsUndeclared),
        refinedCandidate(svc, receiver, List(),
          StreamsRootUndeclared, RequiresRootsDeclared(List((gone, StreamsRootA))))
      )))
      .then(expectThat(all {
        // Reached: 4 and its ring removed, the receiver rejected, Coretime enacted.
        prevSvc.parachains.keys().contains(gone),
        prevSvc.messagesCursor.keys().contains(gone),
        not(svc.parachains.keys().contains(gone)),
        not(svc.messagesCursor.keys().contains(gone)),
        svc.paraInfo(receiver).headData == prevSvc.paraInfo(receiver).headData,
        svc.paraInfo(CoretimeParaId).headData == prevSvc.paraInfo(CoretimeParaId).headData + 1,
        parent_head_continuity
      }))
  }

  /// `parent_head_continuity` regression (§6.2, §6.4, §8.1): one Coretime
  /// candidate fully removes para 4 and re-registers it; para 3 then requires
  /// `(4, StreamsRootA)`. The fresh registration has no ring, so para 3 misses
  /// settlement. Para 4 is present at the end of the step, so the replay must
  /// not read that as a rejected clean-up that kept the old ring.
  run cleanUpReRegisterThenReceiverContinuityTest = {
    val gone = MkParaId(4)
    val receiver = MkParaId(3)
    initCleanUp
      .then(applyBlock(List(
        refinedCandidate(svc, CoretimeParaId,
          List(ParachainCleanUp(gone),
            ParachainSetStateBalance({ paraId: gone, newTotal: InitialParaBalance })),
          StreamsRootUndeclared, RequiresRootsUndeclared),
        refinedCandidate(svc, receiver, List(),
          StreamsRootUndeclared, RequiresRootsDeclared(List((gone, StreamsRootA))))
      )))
      .then(expectThat(all {
        // Reached: 4 re-registered fresh (no longer deregistering, no ring),
        // the receiver rejected.
        prevSvc.paraInfo(gone).isDeregistering,
        svc.parachains.keys().contains(gone),
        not(svc.paraInfo(gone).isDeregistering),
        not(svc.messagesCursor.keys().contains(gone)),
        svc.paraInfo(receiver).headData == prevSvc.paraInfo(receiver).headData,
        svc.paraInfo(CoretimeParaId).headData == prevSvc.paraInfo(CoretimeParaId).headData + 1,
        parent_head_continuity
      }))
  }

  /// `parent_head_continuity` regression (§6.4, §8.1): para 3 first stores a
  /// KV entry, so Coretime's `parachain_clean_up(3)` in the next block is
  /// rejected with `TooMuchStateHeld`. Para 3 stays live, enacts later in that
  /// block and pushes `StreamsRootB`, which Asset Hub then requires. The replay
  /// must keep para 3 (head and ring) across the rejected clean-up.
  run rejectedCleanUpThenPushContinuityTest = {
    val kept = MkParaId(3)
    val ah = AssetHubParaId
    initCleanUp
      .then(applyBlock(List(
        refinedCandidate(svc, kept, List(SetKV({ key: List(1), value: List(1) })),
          StreamsRootUndeclared, RequiresRootsUndeclared))))
      .then(expectThat(svc.keyValueStorage.keys().exists(k => k._1 == kept)))
      .then(applyBlock(List(
        refinedCandidate(svc, CoretimeParaId, List(ParachainCleanUp(kept)),
          StreamsRootUndeclared, RequiresRootsUndeclared),
        refinedCandidate(svc, kept, List(),
          StreamsRootDeclared(StreamsRootB), RequiresRootsUndeclared),
        refinedCandidate(svc, ah, List(),
          StreamsRootUndeclared, RequiresRootsDeclared(List((kept, StreamsRootB))))
      )))
      .then(expectThat(all {
        // Reached: the clean-up's carrier enacted but 3 survived untouched,
        // then 3 pushed B and Asset Hub settled against it.
        svc.paraInfo(CoretimeParaId).headData == prevSvc.paraInfo(CoretimeParaId).headData + 1,
        svc.parachains.keys().contains(kept),
        not(svc.paraInfo(kept).isDeregistering),
        svc.paraInfo(kept).headData == prevSvc.paraInfo(kept).headData + 1,
        svc.settlementCheck(List((kept, StreamsRootB))),
        svc.paraInfo(ah).headData == prevSvc.paraInfo(ah).headData + 1,
        parent_head_continuity
      }))
  }

  /// `parent_head_continuity` regression (§5.1 step 1, §6.4): Coretime
  /// partially cleans up para 3, whose own candidate follows in the same block,
  /// and para 4, deregistering since `initCleanUp`, submits one too. Both are
  /// dropped and keep their heads; the replay freezes 3 at its clean-up, so an
  /// Accumulate that let either candidate enact would fail it.
  run partialCleanUpThenOwnCandidateContinuityTest = {
    val q = MkParaId(3)
    val old = MkParaId(4)
    initCleanUp
      .then(applyBlock(List(
        refinedCandidate(svc, CoretimeParaId, List(ParachainCleanUp(q)),
          StreamsRootUndeclared, RequiresRootsUndeclared),
        refinedCandidate(svc, q, List(), StreamsRootUndeclared, RequiresRootsUndeclared),
        refinedCandidate(svc, old, List(), StreamsRootUndeclared, RequiresRootsUndeclared)
      )))
      .then(expectThat(all {
        // Reached: 3 now deregistering, 4 still is, both heads untouched.
        not(prevSvc.paraInfo(q).isDeregistering),
        svc.paraInfo(q).isDeregistering,
        svc.paraInfo(old).isDeregistering,
        svc.paraInfo(q).headData == prevSvc.paraInfo(q).headData,
        svc.paraInfo(old).headData == prevSvc.paraInfo(old).headData,
        svc.paraInfo(CoretimeParaId).headData == prevSvc.paraInfo(CoretimeParaId).headData + 1,
        parent_head_continuity
      }))
  }

  /// `parent_head_continuity` regression (§6.3, §6.4): one Coretime candidate
  /// partially cleans up para 3, then forces heads on 3 and on para 4,
  /// deregistering since `initCleanUp`. Both forced heads no-op; the replay
  /// freezes 3 at its clean-up, so an Accumulate that applied either would fail
  /// it.
  run partialCleanUpThenSetHeadContinuityTest = {
    val q = MkParaId(3)
    val old = MkParaId(4)
    initCleanUp
      .then(applyBlock(List(
        refinedCandidate(svc, CoretimeParaId,
          List(ParachainCleanUp(q),
            ParachainSetHead({ paraId: q, newHead: 7 }),
            ParachainSetHead({ paraId: old, newHead: 7 })),
          StreamsRootUndeclared, RequiresRootsUndeclared))))
      .then(expectThat(all {
        // Reached: 3 now deregistering, 4 still is, both heads untouched.
        not(prevSvc.paraInfo(q).isDeregistering),
        svc.paraInfo(q).isDeregistering,
        svc.paraInfo(old).isDeregistering,
        svc.paraInfo(q).headData == prevSvc.paraInfo(q).headData,
        svc.paraInfo(old).headData == prevSvc.paraInfo(old).headData,
        parent_head_continuity
      }))
  }

  /// `parent_head_continuity` regression (§6.3, §6.4, §8.1): two clean-ups of
  /// para 3 in one block make its ring unknown to the replay, so a Coretime
  /// candidate requiring `(3, StreamsRootA)` may or may not enact. Here it does
  /// (both clean-ups are partial and keep the ring), and so do the forced head
  /// on Asset Hub it carries, or that a later Coretime candidate built on it
  /// carries. The replay must treat Asset Hub's head as unknown rather than
  /// drop those forced heads with the candidate.
  run unknownSettlementThenSetHeadContinuityTest = {
    val q = MkParaId(3)
    val ah = AssetHubParaId
    val forceAh = ParachainSetHead({ paraId: ah, newHead: 7 })
    val requiresQ = RequiresRootsDeclared(List((q, StreamsRootA)))
    // Reached: 3 deregistering with its ring, Coretime enacted all `n` of its
    // candidates, Asset Hub forced to 7.
    def enacted(n: int): bool = all {
      svc.paraInfo(q).isDeregistering,
      svc.settlementCheck(List((q, StreamsRootA))),
      svc.paraInfo(CoretimeParaId).headData == prevSvc.paraInfo(CoretimeParaId).headData + n,
      svc.paraInfo(ah).headData == 7,
    }
    initCleanUp
      // The requiring candidate forces the head itself.
      .then(applyBlock(List(
        refinedCandidate(svc, CoretimeParaId, List(ParachainCleanUp(q), ParachainCleanUp(q)),
          StreamsRootUndeclared, RequiresRootsUndeclared),
        chainedCandidate(svc, CoretimeParaId, 1, List(forceAh),
          StreamsRootUndeclared, requiresQ))))
      .then(expectThat(enacted(2) and parent_head_continuity))
      .then(initCleanUp)
      // A later candidate of the now unknown Coretime forces it.
      .then(applyBlock(List(
        refinedCandidate(svc, CoretimeParaId, List(ParachainCleanUp(q), ParachainCleanUp(q)),
          StreamsRootUndeclared, RequiresRootsUndeclared),
        chainedCandidate(svc, CoretimeParaId, 1, List(),
          StreamsRootUndeclared, requiresQ),
        chainedCandidate(svc, CoretimeParaId, 2, List(forceAh),
          StreamsRootUndeclared, RequiresRootsUndeclared))))
      .then(expectThat(enacted(3) and parent_head_continuity))
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


  // ===== parent_head_continuity: §5.1 step 2 (validation code) =====

  /// The digest's active code hash of `p`, as stored.
  pure def activeCodeOf(s: ParachainServiceState, p: ParaId): Option[ValidationCodeHash] =
    s.paraInfo(p).validationCode.optionMap(vc => vc.hash)

  /// A candidate for `para` refined with code `code` instead of its active one;
  /// Refine only asks that the code be referenced by some para (`codeAvailable`).
  pure def candidateWithCode(s: ParachainServiceState, para: ParaId, code: ValidationCodeHash): WorkResult = {
    val pi = s.paraInfo(para)
    {
      result: refine(s, { items: List({ paraId: para, validationCode: { hash: code, len: FixedCodeLen } }) },
        wellFormedCalls(headHash(pi.headData), pi.headData + 1, List()), 0)[0],
      authTrace: 0
    }
  }

  /// Coretime sets its own code, then a chained Coretime candidate refined
  /// with the pre-block code follows in the same block: step 2 rejects it, and
  /// the replay must too rather than expect its head.
  run continuityOwnCodeChangeTest = {
    val ct = CoretimeParaId
    val newCode: ValidationCodeHash = { vchBytes: 101 }
    initCleanUp
      .then(applyBlock(List(
        refinedCandidate(svc, ct, List(ParachainSetValidationCode({ paraId: ct, newValidationCode: { hash: newCode, len: FixedCodeLen } })),
          StreamsRootUndeclared, RequiresRootsUndeclared),
        chainedCandidate(svc, ct, 1, List(), StreamsRootUndeclared, RequiresRootsUndeclared))))
      .then(expectThat(all {
        // Reached: both refine Ok, the code moved, only the first enacted.
        isOkDigest(lastStepWorkResults[1].result),
        activeCodeOf(svc, ct) == Some(newCode),
        svc.paraInfo(ct).headData == prevSvc.paraInfo(ct).headData + 1,
        parent_head_continuity
      }))
  }

  /// As above, but Coretime sets para 3's code and para 3's candidate, refined
  /// with its pre-block code, follows.
  run continuityOtherCodeChangeTest = {
    val ct = CoretimeParaId
    val q = MkParaId(3)
    val newCode: ValidationCodeHash = { vchBytes: 102 }
    initCleanUp
      .then(applyBlock(List(
        refinedCandidate(svc, ct, List(ParachainSetValidationCode({ paraId: q, newValidationCode: { hash: newCode, len: FixedCodeLen } })),
          StreamsRootUndeclared, RequiresRootsUndeclared),
        refinedCandidate(svc, q, List(), StreamsRootUndeclared, RequiresRootsUndeclared))))
      .then(expectThat(all {
        isOkDigest(lastStepWorkResults[1].result),
        activeCodeOf(svc, q) == Some(newCode),
        svc.paraInfo(q).headData == prevSvc.paraInfo(q).headData,
        parent_head_continuity
      }))
  }

  /// An Asset Hub candidate refined with para 3's code: Refine accepts it (the
  /// code is referenced), step 2 rejects it, and so must the replay.
  run continuityForeignCodeTest = {
    val ah = AssetHubParaId
    initCleanUp
      .then(applyBlock(List(candidateWithCode(svc, ah, { vchBytes: 3 }))))
      .then(expectThat(all {
        isOkDigest(lastStepWorkResults[0].result),
        activeCodeOf(prevSvc, ah) != Some({ vchBytes: 3 }),
        svc.paraInfo(ah).headData == prevSvc.paraInfo(ah).headData,
        parent_head_continuity
      }))
  }

  /// An undecidable Coretime candidate (it requires a root of para 3, made
  /// unknown by a double clean-up) sets Asset Hub's code to para 3's, then an
  /// Asset Hub candidate with that code follows and enacts. The replay must
  /// forget Asset Hub's code in `markUnknown` rather than reject the candidate.
  run continuityUnknownCodeChangeTest = {
    val ah = AssetHubParaId
    val q = MkParaId(3)
    val c3: ValidationCodeHash = { vchBytes: 3 }
    initCleanUp
      .then(applyBlock(List(
        refinedCandidate(svc, CoretimeParaId, List(ParachainCleanUp(q), ParachainCleanUp(q)),
          StreamsRootUndeclared, RequiresRootsUndeclared),
        chainedCandidate(svc, CoretimeParaId, 1,
          List(ParachainSetValidationCode({ paraId: ah, newValidationCode: { hash: c3, len: FixedCodeLen } })),
          StreamsRootUndeclared, RequiresRootsDeclared(List((q, StreamsRootA)))),
        candidateWithCode(svc, ah, c3))))
      .then(expectThat(all {
        activeCodeOf(svc, ah) == Some(c3),
        svc.paraInfo(ah).headData == prevSvc.paraInfo(ah).headData + 1,
        parent_head_continuity
      }))
  }

  /// Two code changes to para 3 in one block: the replay cannot classify them
  /// from the end state, so it forgets para 3's code and para 3's candidate,
  /// refined with the pre-block code (rejected), goes through `markUnknown`.
  run continuityDoubleCodeChangeTest = {
    val q = MkParaId(3)
    val x: ValidationCodeHash = { vchBytes: 103 }
    val y: ValidationCodeHash = { vchBytes: 104 }
    initCleanUp
      .then(applyBlock(List(
        refinedCandidate(svc, CoretimeParaId, List(
            ParachainSetValidationCode({ paraId: q, newValidationCode: { hash: x, len: FixedCodeLen } }),
            ParachainSetValidationCode({ paraId: q, newValidationCode: { hash: y, len: FixedCodeLen } })),
          StreamsRootUndeclared, RequiresRootsUndeclared),
        refinedCandidate(svc, q, List(), StreamsRootUndeclared, RequiresRootsUndeclared))))
      .then(expectThat(all {
        isOkDigest(lastStepWorkResults[1].result),
        activeCodeOf(svc, q) == Some(y),
        svc.paraInfo(q).headData == prevSvc.paraInfo(q).headData,
        parent_head_continuity
      }))
  }


  /// An undecidable Coretime candidate (it requires a root of para 3, which a
  /// double clean-up made unknown) forces a head on para 4, deregistering
  /// since `initCleanUp`. Whether or not it enacts, the forced head is a no-op,
  /// so the replay keeps para 4's head and still checks it: a model where the
  /// forced head moves a deregistering para must break the invariant here.
  run frozenTargetOfUndecidableCandidateKeepsHeadTest = {
    val q = MkParaId(3)
    val old = MkParaId(4)
    initCleanUp
      .then(applyBlock(List(
        refinedCandidate(svc, CoretimeParaId, List(ParachainCleanUp(q), ParachainCleanUp(q)),
          StreamsRootUndeclared, RequiresRootsUndeclared),
        chainedCandidate(svc, CoretimeParaId, 1, List(ParachainSetHead({ paraId: old, newHead: 7 })),
          StreamsRootUndeclared, RequiresRootsDeclared(List((q, StreamsRootA)))))))
      .then(expectThat(all {
        svc.paraInfo(CoretimeParaId).headData == prevSvc.paraInfo(CoretimeParaId).headData + 2,
        svc.paraInfo(old).isDeregistering,
        parent_head_continuity
      }))
  }

  /// The converse: an undecidable candidate cleans up the frozen para 4, which
  /// completes its removal, and the next one re-registers it and forces a fresh
  /// head. A clean-up target leaves the replay, frozen or not.
  run frozenCleanUpTargetReRegisteredTest = {
    val q = MkParaId(3)
    val old = MkParaId(4)
    initCleanUp
      .then(applyBlock(List(
        refinedCandidate(svc, CoretimeParaId, List(ParachainCleanUp(q), ParachainCleanUp(q)),
          StreamsRootUndeclared, RequiresRootsUndeclared),
        chainedCandidate(svc, CoretimeParaId, 1, List(ParachainCleanUp(old)),
          StreamsRootUndeclared, RequiresRootsDeclared(List((q, StreamsRootA)))),
        chainedCandidate(svc, CoretimeParaId, 2, List(
          ParachainSetStateBalance({ paraId: old, newTotal: BaselineFootprint * 2 }),
          ParachainSetHead({ paraId: old, newHead: 9 })),
          StreamsRootUndeclared, RequiresRootsUndeclared))))
      .then(expectThat(all {
        svc.paraInfo(CoretimeParaId).headData == prevSvc.paraInfo(CoretimeParaId).headData + 3,
        not(svc.paraInfo(old).isDeregistering),
        svc.paraInfo(old).headData == 9,
        parent_head_continuity
      }))
  }

  /// A candidate for `para` continuing the head `k` candidates past its current
  /// one, refined with code `code` and making the given messaging declarations.
  pure def chainedCandidateOnCode(
    s: ParachainServiceState, para: ParaId, k: int, code: ValidationCodeHash,
    msgs: List[UpwardMessage],
    provides: StreamsRootDeclaration, requires: RequiresRootsDeclaration
  ): WorkResult = {
    val parent = s.paraInfo(para).headData + k
    {
      result: refine(s, { items: List({ paraId: para, validationCode: { hash: code, len: FixedCodeLen } }) },
        wellFormedCalls(headHash(parent), parent + 1, msgs)
          .concat(messagingCalls(provides, requires)), 0)[0],
      authTrace: 0
    }
  }

  /// Two code changes to para 3 in separate candidates, with para 3's candidate
  /// on the first new code between them: Coretime sets it to X (para 4's code,
  /// so Refine accepts it), para 3 enacts on X, then a chained Coretime
  /// candidate sets it to Y. The replay must not take Y, the end-of-step code,
  /// as para 3's code at its candidate: two changes leave it unknown, and the
  /// candidate goes through `markUnknown`.
  run continuitySplitDoubleCodeChangeTest = {
    val ct = CoretimeParaId
    val q = MkParaId(3)
    val x: ValidationCodeHash = { vchBytes: 4 }
    val y: ValidationCodeHash = { vchBytes: 105 }
    initCleanUp
      .then(applyBlock(List(
        refinedCandidate(svc, ct,
          List(ParachainSetValidationCode({ paraId: q, newValidationCode: { hash: x, len: FixedCodeLen } })),
          StreamsRootUndeclared, RequiresRootsUndeclared),
        candidateWithCode(svc, q, x),
        chainedCandidate(svc, ct, 1,
          List(ParachainSetValidationCode({ paraId: q, newValidationCode: { hash: y, len: FixedCodeLen } })),
          StreamsRootUndeclared, RequiresRootsUndeclared))))
      .then(expectThat(all {
        // Reached: both Coretime candidates and para 3's enacted, 3 ends on Y.
        isOkDigest(lastStepWorkResults[1].result),
        svc.paraInfo(ct).headData == prevSvc.paraInfo(ct).headData + 2,
        svc.paraInfo(q).headData == prevSvc.paraInfo(q).headData + 1,
        activeCodeOf(svc, q) == Some(y),
        parent_head_continuity
      }))
  }

  /// One code change to para 3, after which it runs, and is then fully removed
  /// and re-registered in the same block. Asset Hub first solicits para 3's
  /// pre-block code, so Coretime's forget of it releases at once; Coretime sets
  /// para 3's code to Asset Hub's; para 3 enacts on it and pushes
  /// `StreamsRootB`; Asset Hub settles against B; a chained Coretime candidate
  /// cleans up para 3 (nothing retained: full removal) and re-registers it with
  /// no code. The end-of-step code is the fresh entry's, not the one para 3 ran
  /// on, so the replay must forget para 3's code rather than reject its
  /// candidate and, with it, Asset Hub's.
  run continuityCodeChangeThenReRegisterTest = {
    val ct = CoretimeParaId
    val ah = AssetHubParaId
    val q = MkParaId(3)
    val c3: ValidationCodeHash = { vchBytes: 3 }
    val x: ValidationCodeHash = { vchBytes: 2 }
    initCleanUp
      .then(applyBlock(List(
        refinedCandidate(svc, ah,
          List(Solicit({ target: Parachain(ah), hash: c3.asPreimage(), len: FixedCodeLen })),
          StreamsRootUndeclared, RequiresRootsUndeclared),
        refinedCandidate(svc, ct, List(
            ParachainSetValidationCode({ paraId: q, newValidationCode: { hash: x, len: FixedCodeLen } }),
            Forget({ target: Parachain(q), preimages: List((c3.asPreimage(), FixedCodeLen)) })),
          StreamsRootUndeclared, RequiresRootsUndeclared),
        chainedCandidateOnCode(svc, q, 0, x, List(),
          StreamsRootDeclared(StreamsRootB), RequiresRootsUndeclared),
        chainedCandidate(svc, ah, 1, List(),
          StreamsRootUndeclared, RequiresRootsDeclared(List((q, StreamsRootB)))),
        chainedCandidate(svc, ct, 1, List(
            ParachainCleanUp(q),
            ParachainSetStateBalance({ paraId: q, newTotal: InitialParaBalance })),
          StreamsRootUndeclared, RequiresRootsUndeclared))))
      .then(expectThat(all {
        // Reached: every candidate enacted; 3 re-registered fresh, with no
        // code and no ring.
        svc.paraInfo(ct).headData == prevSvc.paraInfo(ct).headData + 2,
        svc.paraInfo(ah).headData == prevSvc.paraInfo(ah).headData + 2,
        not(prevSvc.paraInfo(q).isDeregistering),
        svc.parachains.keys().contains(q),
        not(svc.paraInfo(q).isDeregistering),
        activeCodeOf(svc, q) == None,
        not(svc.messagesCursor.keys().contains(q)),
        parent_head_continuity
      }))
  }

  /// A Coretime candidate built on a stale head (Accumulate rejects it, §5.1
  /// step 4) carries `parachain_set_validation_code(3, X)`. A rejected candidate
  /// changes nothing, so para 3's code stays put and para 3's candidate on its
  /// old code enacts in the same block; the replay must agree.
  run continuityRejectedCodeChangeTest = {
    val ct = CoretimeParaId
    val q = MkParaId(3)
    val x: ValidationCodeHash = { vchBytes: 105 }
    initCleanUp
      .then(applyBlock(List(
        chainedCandidate(svc, ct, 5,
          List(ParachainSetValidationCode({ paraId: q, newValidationCode: { hash: x, len: FixedCodeLen } })),
          StreamsRootUndeclared, RequiresRootsUndeclared),
        refinedCandidate(svc, q, List(), StreamsRootUndeclared, RequiresRootsUndeclared))))
      .then(expectThat(all {
        // Reached: both refine Ok, Coretime's is rejected, para 3's enacts.
        isOkDigest(lastStepWorkResults[0].result),
        isOkDigest(lastStepWorkResults[1].result),
        svc.paraInfo(ct).headData == prevSvc.paraInfo(ct).headData,
        activeCodeOf(svc, q) == Some({ vchBytes: 3 }),
        svc.paraInfo(q).headData == prevSvc.paraInfo(q).headData + 1,
        parent_head_continuity
      }))
  }

  /// Para 3 solicits and announces code Y (§5.2) but never applies it. A
  /// candidate refined with Y passes Refine (the code is referenced), but §5.1
  /// step 2 accepts only the active code, so it does not enact.
  run continuityAnnouncedCodeRejectedTest = {
    val q = MkParaId(3)
    val y: ValidationCodeHash = { vchBytes: 107 }
    initCleanUp
      .then(applyBlock(List(refinedCandidate(svc, q,
        List(Solicit({ target: Parachain(q), hash: y.asPreimage(), len: FixedCodeLen })),
        StreamsRootUndeclared, RequiresRootsUndeclared))))
      .then(provide(y.asPreimage(), FixedCodeLen))
      .then(applyBlock(List(refinedCandidate(svc, q,
        List(RequestCodeUpgrade({ hash: y, len: FixedCodeLen, phase: Announcement })),
        StreamsRootUndeclared, RequiresRootsUndeclared))))
      .then(expectThat(svc.paraInfo(q).announcedUpgrade == Some({ hash: y, len: FixedCodeLen })))
      .then(applyBlock(List(candidateWithCode(svc, q, y))))
      .then(expectThat(all {
        isOkDigest(lastStepWorkResults[0].result),
        activeCodeOf(svc, q) == Some({ vchBytes: 3 }),
        svc.paraInfo(q).headData == prevSvc.paraInfo(q).headData,
        svc.paraInfo(q).announcedUpgrade == Some({ hash: y, len: FixedCodeLen }),
        parent_head_continuity
      }))
  }

  /// §6.3 / `validation_code_matches_outcomes` part 2: Coretime's accepted
  /// `parachain_set_validation_code(3, code 4)` must switch para 3's code (it
  /// fits), so a model that charges the code but keeps the old one, letting
  /// para 3's same-block candidate built with the old code enact, is caught.
  run setValidationCodeOutcomeTest = {
    val q = MkParaId(3)
    initCleanUp
      .then(applyBlock(List(
        refinedCandidate(svc, CoretimeParaId,
          List(ParachainSetValidationCode({ paraId: q, newValidationCode: { hash: { vchBytes: 4 }, len: FixedCodeLen } })),
          StreamsRootUndeclared, RequiresRootsUndeclared),
        refinedCandidate(svc, q, List(), StreamsRootUndeclared, RequiresRootsUndeclared))))
      .then(expectThat(all {
        isOkDigest(lastStepWorkResults[0].result),
        isOkDigest(lastStepWorkResults[1].result),
        // Reached: Coretime's candidate enacts and switches para 3 to code 4,
        // so para 3's candidate built with code 3 does not.
        svc.paraInfo(CoretimeParaId).headData == prevSvc.paraInfo(CoretimeParaId).headData + 1,
        activeCodeOf(svc, q) == Some({ vchBytes: 4 }),
        svc.paraInfo(q).headData == prevSvc.paraInfo(q).headData,
        validation_code_matches_outcomes
      }))
  }

  /// §5.2 / `validation_code_matches_outcomes` part 1: an `Apply` of code para 3
  /// solicited (so referenced) but never announced is rejected, so a model in
  /// which it switches the code (and a chained candidate then enacts) is caught;
  /// the reference keeps `validation_code_in_registry` from noticing.
  run unannouncedApplyOutcomeTest = {
    val q = MkParaId(3)
    val c: ValidationCodeHash = { vchBytes: 107 }
    initCleanUp
      .then(applyBlock(List(refinedCandidate(svc, q,
        List(Solicit({ target: Parachain(q), hash: c.asPreimage(), len: FixedCodeLen })),
        StreamsRootUndeclared, RequiresRootsUndeclared))))
      .then(provide(c.asPreimage(), FixedCodeLen))
      .then(applyBlock(List(
        refinedCandidate(svc, q,
          List(RequestCodeUpgrade({ hash: c, len: FixedCodeLen, phase: Apply })),
          StreamsRootUndeclared, RequiresRootsUndeclared),
        chainedCandidate(svc, q, 1, List(), StreamsRootUndeclared, RequiresRootsUndeclared))))
      .then(expectThat(all {
        isOkDigest(lastStepWorkResults[0].result),
        isOkDigest(lastStepWorkResults[1].result),
        svc.hasReferencer(c.asPreimage(), FixedCodeLen, q),
        // Reached: both candidates enact on the unchanged code 3.
        svc.paraInfo(q).headData == prevSvc.paraInfo(q).headData + 2,
        activeCodeOf(svc, q) == Some({ vchBytes: 3 }),
        validation_code_matches_outcomes
      }))
  }

  /// §6.3 / `validation_code_matches_outcomes` part 2, rejected branch: Coretime
  /// sets para 3's total to its used balance, then (next block, so para 3 stays
  /// tracked: a same-block `parachain_set_state_balance` counts as a possible
  /// re-registration) sets its code to code 4. That is rejected with
  /// `InsufficientStateBalance`, para 3 keeps code 3, and its same-block
  /// candidate built on code 3 enacts. Only the log entry excuses the unchanged
  /// code; neither sim reaches this path.
  run setValidationCodeInsufficientBalanceTest = {
    val q = MkParaId(3)
    val c = { hash: { vchBytes: 4 }, len: FixedCodeLen }
    // The `parachain_set_validation_code` is Coretime's first message, so it is
    // sent with `MessageId` 0.
    val isb = (0, InsufficientStateBalance)
    initCleanUp
      .then(applyBlock(List(refinedCandidate(svc, CoretimeParaId,
        List(ParachainSetStateBalance({ paraId: q, newTotal: svc.paraInfo(q).usedStateBalance })),
        StreamsRootUndeclared, RequiresRootsUndeclared))))
      .then(expectThat(svc.paraInfo(q).totalStateBalance == svc.paraInfo(q).usedStateBalance))
      .then(applyBlock(List(
        refinedCandidate(svc, CoretimeParaId,
          List(ParachainSetValidationCode({ paraId: q, newValidationCode: c })),
          StreamsRootUndeclared, RequiresRootsUndeclared),
        refinedCandidate(svc, q, List(), StreamsRootUndeclared, RequiresRootsUndeclared))))
      .then(expectThat(all {
        isOkDigest(lastStepWorkResults[0].result),
        isOkDigest(lastStepWorkResults[1].result),
        svc.paraInfo(CoretimeParaId).headData == prevSvc.paraInfo(CoretimeParaId).headData + 1,
        svc.paraLog(CoretimeParaId).foldl(false, (b, e) => b or (e._1 == now and match e._2 {
          | Accumulate(ls) => ls.foldl(false, (x, l) => x or l == isb)
          | Refine(_) => false
        })),
        continuityReplay.codeSets.get(q) == (0, 0, c),
        activeCodeOf(svc, q) == Some({ vchBytes: 3 }),
        svc.paraInfo(q).headData == prevSvc.paraInfo(q).headData + 1,
        validation_code_matches_outcomes,
        parent_head_continuity
      }))
  }

  /// Coretime's two candidates of one block for the tests below, both sending
  /// their only message with `MessageId` 0: the first solicits a new preimage for
  /// Coretime itself (`InsufficientStateBalance` when Coretime has no headroom),
  /// the second sets para 3's code to code 4.
  pure def twoCoretimeCandidates(s: ParachainServiceState): List[WorkResult] = {
    val ct = CoretimeParaId
    List(
      refinedCandidate(s, ct,
        List(Solicit({ target: Parachain(ct), hash: { hashBytes: 42 }, len: 1024 })),
        StreamsRootUndeclared, RequiresRootsUndeclared),
      chainedCandidate(s, ct, 1,
        List(ParachainSetValidationCode({
          paraId: MkParaId(3), newValidationCode: { hash: { vchBytes: 4 }, len: FixedCodeLen }
        })),
        StreamsRootUndeclared, RequiresRootsUndeclared))
  }

  /// Whether Coretime's log has `(0, InsufficientStateBalance)` at `now`.
  def coretimeLoggedIsbZero: bool =
    svc.paraLog(CoretimeParaId).foldl(false, (b, e) => b or (e._1 == now and match e._2 {
      | Accumulate(ls) => ls.foldl(false, (x, l) => x or l == (0, InsufficientStateBalance))
      | Refine(_) => false
    }))

  /// `validation_code_matches_outcomes` part 2 must tie the excusing log entry to
  /// the candidate that sent the `parachain_set_validation_code`. Coretime's first
  /// candidate logs `InsufficientStateBalance` for its own message 0; its second
  /// sets para 3's code with its message 0 and succeeds. Forcing para 3 back to
  /// code 3, as a model whose call silently did nothing would, must be caught:
  /// the first candidate's entry does not excuse it.
  run setValidationCodeOtherCandidateIsbTest = {
    val ct = CoretimeParaId
    val q = MkParaId(3)
    initCleanUp
      .then(applyBlock(List(refinedCandidate(svc, ct,
        List(ParachainSetStateBalance({ paraId: ct, newTotal: svc.paraInfo(ct).usedStateBalance })),
        StreamsRootUndeclared, RequiresRootsUndeclared))))
      .then(applyBlock(twoCoretimeCandidates(svc)))
      .then(expectThat(all {
        isOkDigest(lastStepWorkResults[0].result),
        isOkDigest(lastStepWorkResults[1].result),
        svc.paraInfo(ct).headData == prevSvc.paraInfo(ct).headData + 2,
        coretimeLoggedIsbZero,
        activeCodeOf(svc, q) == Some({ vchBytes: 4 }),
        validation_code_matches_outcomes
      }))
      .then(setSvc({ ...svc, parachains: svc.parachains.put(q,
        { ...svc.paraInfo(q), validationCode: prevSvc.paraInfo(q).validationCode }) }))
      .then(expectThat(not(validation_code_matches_outcomes)))
  }

  /// The same block when para 3 has no headroom either: both candidates log
  /// `(0, InsufficientStateBalance)`, para 3 keeps code 3, and that is excused.
  /// Then with Coretime's first candidate logging nothing: only the second
  /// candidate's entry exists, and it excuses para 3 keeping code 3.
  run setValidationCodeBothCandidatesIsbTest = {
    val ct = CoretimeParaId
    val q = MkParaId(3)
    initCleanUp
      .then(applyBlock(List(refinedCandidate(svc, ct,
        List(
          ParachainSetStateBalance({ paraId: ct, newTotal: svc.paraInfo(ct).usedStateBalance }),
          ParachainSetStateBalance({ paraId: q, newTotal: svc.paraInfo(q).usedStateBalance })),
        StreamsRootUndeclared, RequiresRootsUndeclared))))
      .then(applyBlock(twoCoretimeCandidates(svc)))
      .then(expectThat(all {
        isOkDigest(lastStepWorkResults[1].result),
        svc.paraInfo(ct).headData == prevSvc.paraInfo(ct).headData + 2,
        svc.paraLog(ct).select(e => e._1 == now).length() == 2,
        activeCodeOf(svc, q) == Some({ vchBytes: 3 }),
        validation_code_matches_outcomes
      }))
      .then(applyBlock(List(
        refinedCandidate(svc, ct, List(), StreamsRootUndeclared, RequiresRootsUndeclared),
        twoCoretimeCandidates(svc)[1])))
      .then(expectThat(all {
        svc.paraInfo(ct).headData == prevSvc.paraInfo(ct).headData + 2,
        svc.paraLog(ct).select(e => e._1 == now).length() == 1,
        coretimeLoggedIsbZero,
        activeCodeOf(svc, q) == Some({ vchBytes: 3 }),
        validation_code_matches_outcomes
      }))
  }

  /// The invocation runs out of gas in Coretime's package, which sets para 3's
  /// code, after Asset Hub's candidate enacted. Only Asset Hub's candidate takes
  /// effect: the replay must skip the rolled-back package, neither advancing
  /// Coretime's head nor expecting para 3's code to be set.
  run continuityOutOfGasRollbackTest = {
    val q = MkParaId(3)
    val c = { hash: { vchBytes: 4 }, len: FixedCodeLen }
    initCleanUp
      .then(applyBlockOutOfGas(List(
        refinedCandidate(svc, AssetHubParaId, List(), StreamsRootUndeclared, RequiresRootsUndeclared),
        refinedCandidate(svc, CoretimeParaId,
          List(ParachainSetValidationCode({ paraId: q, newValidationCode: c })),
          StreamsRootUndeclared, RequiresRootsUndeclared)), InPackage(1)))
      .then(expectThat(all {
        // Reached: both refine Ok, only Asset Hub's candidate enacted.
        isOkDigest(lastStepWorkResults[0].result),
        isOkDigest(lastStepWorkResults[1].result),
        svc.paraInfo(AssetHubParaId).headData == prevSvc.paraInfo(AssetHubParaId).headData + 1,
        svc.paraInfo(CoretimeParaId).headData == prevSvc.paraInfo(CoretimeParaId).headData,
        activeCodeOf(svc, q) == activeCodeOf(prevSvc, q),
        head_state_matches_outcomes,
        parent_head_continuity,
        validation_code_matches_outcomes
      }))
  }
}
