// Shared deterministic test helpers and fixtures.
//
// `main.qnt` exposes `stepRefineAccumulate` as a heavily nondeterministic
// top-level step. The tests need DETERMINISTIC scenarios instead: input to
// output relationships the Rust harness can replay. These helpers construct
// well-formed work packages against the current state and thread them
// through `applyAlwaysAccumulate` + `refine` + `accumulateOnePackage`
// without any nondet. No `run` tests live here.
module support {
  import types.* from "../types"
  import messages.* from "../messages"
  import state.* from "../state"
  import foreign_services.* from "../foreign_services"
  import refine.* from "../refine"
  import accumulate.* from "../accumulate"
  import head_commitment.* from "../head_commitment"
  import state_vars.* from "../state_vars"
  import main.* from "../main"

  pure def markProvided(
    s: ParachainServiceState, h: Hash, len: int
  ): ParachainServiceState =
    { ...s, preimageStatus: s.preimageStatus.put((h, len), Provided) }

  pure def isUnrequested(st: RequestStatus): bool =
    match st { | Unrequested(_) => true | _ => false }

  /// Apply a sequence of happy-path work packages in one block via the shared
  /// `accumulateBlock` pipeline, the SAME function the model step
  /// (`stepRefineAccumulate`) uses, so these tests exercise the real logic
  /// rather than a parallel copy. Each package is refined (at guarantee time,
  /// against the pre-block state) and paired with lookup-anchor 0 (prune
  /// nothing). Each element is `(para, codeHash, msgs)`; `codeHash` is what the
  /// collator says it validated against (active *or* pending, both accepted by
  /// `accumulateOkPrefix` step 3).
  action applyHappyPaths(
    pkgs: List[(ParaId, ValidationCodeHash, List[UpwardMessage])]
  ): bool = {
    // A block advances the clock to its own timeslot.
    val blockNow = now + 1
    // Refine each package at guarantee time against the pre-block state (it must
    // not see this block's always-accumulate effects).
    val packages: List[WorkResult] = pkgs.foldl(List(), (acc, p) => {
      val pi = svc.paraInfo(p._1)
      val pkg: WorkPackage = {
        authorizedParas: List(p._1),
        items: List({ validationCode: { hash: p._2, len: FixedCodeLen }, pov: 0 })
      }
      val wr: WorkResult = {
        result: refine(
          svc, pkg, false, PvfOk,
          Some(headHash(pi.headData)), HeadDeclared(pi.headData + 1), p._3, 0
        )[0],
        authTrace: 0
      }
      acc.append(wr)
    })
    // Run the exact per-block Accumulate pipeline the model step runs.
    val blockOut = accumulateBlock(
      svc, blockNow, jamStagingSet, solicitedSet, FixedCodeLen, packages,
      foreignServices)
    all {
      svc' = blockOut.state,
      now' = blockNow,
      jamStagingSet' = blockOut.staging,
      solicitedSet' = blockOut.solicited,
      prevSvc' = svc,
      lastStepWorkResults' = blockOut.results,
      lastStepAssigns' = blockOut.assigns,
      lastHeadRoot' = blockOut.headRoot,
      foreignServices' = blockOut.foreign,
      logPrunedBelow' =
        blockOut.logPruned.foldl(logPrunedBelow, (m, pair) => m.put(pair._1, pair._2))
    }
  }

  /// Singular convenience wrapper: apply one happy-path work package.
  action applyHappyPath(
    paraId: ParaId, codeHash: ValidationCodeHash, msgs: List[UpwardMessage]
  ): bool = applyHappyPaths(List((paraId, codeHash, msgs)))

  /// `applyHappyPaths` with per-package messaging declarations (§8.1): each
  /// element also carries what the PVF's `set_messages_streams_root` /
  /// `set_messages_requires_roots` calls amounted to. Packages carry no upward
  /// messages, since the messaging fields travel in the digest, not the
  /// message list. Same shared `accumulateBlock` pipeline as the model step,
  /// so a package rejected at settlement leaves its para's head unchanged.
  action applyMessagingPaths(
    pkgs: List[(ParaId, ValidationCodeHash, StreamsRootDeclaration, RequiresRootsDeclaration)]
  ): bool = {
    val blockNow = now + 1
    val packages: List[WorkResult] = pkgs.foldl(List(), (acc, p) => {
      val pi = svc.paraInfo(p._1)
      val pkg: WorkPackage = {
        authorizedParas: List(p._1),
        items: List({ validationCode: { hash: p._2, len: FixedCodeLen }, pov: 0 })
      }
      val wr: WorkResult = {
        result: refineWithRoots(
          svc, pkg, false, PvfOk,
          Some(headHash(pi.headData)), HeadDeclared(pi.headData + 1), List(), 0,
          p._3, p._4
        )[0],
        authTrace: 0
      }
      acc.append(wr)
    })
    val blockOut = accumulateBlock(
      svc, blockNow, jamStagingSet, solicitedSet, FixedCodeLen, packages,
      foreignServices)
    all {
      svc' = blockOut.state,
      now' = blockNow,
      jamStagingSet' = blockOut.staging,
      solicitedSet' = blockOut.solicited,
      prevSvc' = svc,
      lastStepWorkResults' = blockOut.results,
      lastStepAssigns' = blockOut.assigns,
      lastHeadRoot' = blockOut.headRoot,
      foreignServices' = blockOut.foreign,
      logPrunedBelow' =
        blockOut.logPruned.foldl(logPrunedBelow, (m, pair) => m.put(pair._1, pair._2))
    }
  }

  /// Singular convenience wrapper: apply one messaging work package.
  action applyMessagingPath(
    paraId: ParaId, codeHash: ValidationCodeHash,
    provides: StreamsRootDeclaration, requires: RequiresRootsDeclaration
  ): bool = applyMessagingPaths(List((paraId, codeHash, provides, requires)))

  /// Refine one otherwise well-formed candidate for `caller`, carrying the
  /// given messaging declarations (§8.1) and no upward messages.
  pure def refineWithMessaging(
    s: ParachainServiceState, caller: ParaId,
    provides: StreamsRootDeclaration, requires: RequiresRootsDeclaration
  ): WorkExecResult = {
    val pi = s.paraInfo(caller)
    val pkg: WorkPackage = {
      authorizedParas: List(caller),
      items: List({
        validationCode: { hash: activeCodeHashOr(pi, NoCodeHash), len: FixedCodeLen },
        pov: 0
      })
    }
    refineWithRoots(s, pkg, false, PvfOk,
      Some(headHash(pi.headData)), HeadDeclared(pi.headData + 1), List(), 0,
      provides, requires)[0]
  }

  /// Generic guard-only assertion wrapper: holds all primes at their
  /// current values and requires `pred` to be true.
  action expectThat(pred: bool): bool = all {
    pred,
    svc' = svc,
    now' = now,
    jamStagingSet' = jamStagingSet,
    solicitedSet' = solicitedSet,
    prevSvc' = prevSvc,
    lastStepWorkResults' = lastStepWorkResults,
    lastStepAssigns' = lastStepAssigns,
    lastHeadRoot' = lastHeadRoot,
    foreignServices' = foreignServices,
    logPrunedBelow' = logPrunedBelow
  }

  /// Force `svc` to a specific state so an invariant can be evaluated on it.
  action setSvc(s: ParachainServiceState): bool = all {
    svc' = s,
    now' = now,
    jamStagingSet' = jamStagingSet,
    solicitedSet' = solicitedSet,
    prevSvc' = prevSvc,
    lastStepWorkResults' = lastStepWorkResults,
    lastStepAssigns' = lastStepAssigns,
    lastHeadRoot' = lastHeadRoot,
    foreignServices' = foreignServices,
    logPrunedBelow' = logPrunedBelow
  }

  /// Action wrapper for `markProvided`: flips a solicited preimage's `query`
  /// status to `Provided`.
  action provide(h: Hash, len: int): bool = all {
    svc' = markProvided(svc, h, len),
    now' = now,
    jamStagingSet' = jamStagingSet,
    solicitedSet' = solicitedSet,
    prevSvc' = prevSvc,
    lastStepWorkResults' = lastStepWorkResults,
    lastStepAssigns' = lastStepAssigns,
    lastHeadRoot' = lastHeadRoot,
    foreignServices' = foreignServices,
    logPrunedBelow' = logPrunedBelow
  }

  /// `incoming_transfers` at capacity, maximally fragmented: one transfer per
  /// bucket in ids `0 .. MaxIncomingTransfers`, which is what one transfer per
  /// accumulate invocation produces.
  pure val fullIncomingTransfersFixture:
    BucketId -> List[IncomingTransfer] =
    range(0, MaxIncomingTransfers).foldl(Map(), (acc, i) =>
      acc.put(i, List({ from: MkServiceId(0), amount: 1000 + i, toSupervisorBalance: false, memo: 0 })))

  pure val fullTransferQueue: ParachainServiceState = {
    ...initialState,
    incomingTransfers: fullIncomingTransfersFixture,
    incomingTransferBuckets:
      Some({ firstBucket: 0, lastBucket: MaxIncomingTransfers - 1,
             count: MaxIncomingTransfers })
  }

  /// One bucket short of the reservation: the queue can still hold one more.
  pure val almostFullTransferQueue: ParachainServiceState = {
    ...fullTransferQueue,
    incomingTransfers: fullTransferQueue.incomingTransfers.mapRemove(MaxIncomingTransfers - 1),
    incomingTransferBuckets:
      Some({ firstBucket: 0, lastBucket: MaxIncomingTransfers - 2,
             count: MaxIncomingTransfers - 1 })
  }

  /// Refine a candidate for `paraId` declaring a STALE parent head: the sentinel
  /// head the para never has (`StaleParentHead`), as both parent and new head,
  /// carrying `msgs` as its upward messages. Accumulate's parent-head check (§5.1
  /// step 3) must reject it, so neither its head write nor its replayed messages
  /// (§5.1 step 7) take effect. The new head is deliberately NOT `current + 1`:
  /// if the check were broken and the candidate applied, the head would jump to
  /// an obviously-invalid sentinel value, so the "head unchanged" assertion can
  /// never pass by coincidence.
  action applyStaleParentCandidate(
    paraId: ParaId, codeHash: ValidationCodeHash, msgs: List[UpwardMessage]
  ): bool = {
    val blockNow = now + 1
    val pkg: WorkPackage = {
      authorizedParas: List(paraId),
      items: List({ validationCode: { hash: codeHash, len: FixedCodeLen }, pov: 0 })
    }
    val wr: WorkResult = {
      result: refine(
        svc, pkg, false, PvfOk,
        Some(headHash(StaleParentHead)), HeadDeclared(StaleParentHead), msgs, 0
      )[0],
      authTrace: 0
    }
    val afterAlwaysAccumulate = applyAlwaysAccumulate(svc, blockNow)
    val out = accumulateOnePackage(
      afterAlwaysAccumulate.state, wr, solicitedSet, None, FixedCodeLen, blockNow,
      foreignServices
    )
    all {
      svc' = out.state,
      now' = blockNow,
      jamStagingSet' = match out.jamDesignate {
        | Some(v) => v
        | None => jamStagingSet
      },
      solicitedSet' = out.solicited,
      prevSvc' = svc,
      lastStepWorkResults' = List(wr),
      lastStepAssigns' = afterAlwaysAccumulate.assigns.concat(out.jamAssigns),
      lastHeadRoot' = headCommitment(svc, out.state),
      foreignServices' = out.foreign,
      logPrunedBelow' = logPrunedBelow
    }
  }

  /// A fresh `AccumulateOutcome` over `s`, for replaying one upward message.
  pure def accOverWith(
    s: ParachainServiceState, fs: ForeignServices
  ): AccumulateOutcome = {
    state: s, jamDesignate: None, jamSolicit: Set(),
    jamForget: Set(), jamAssigns: List(), logs: List(), foreign: fs
  }

  pure def accOver(s: ParachainServiceState): AccumulateOutcome =
    accOverWith(s, Map())

  pure def replay(
    s: ParachainServiceState, caller: ParaId, now: Timeslot, m: UpwardMessage
  ): AccumulateOutcome =
    applyUpwardMessage(accOver(s), caller, FixedCodeLen, now, now, m)

  /// Replay one upward message against a given set of supervised services.
  pure def replayForeign(
    fs: ForeignServices, caller: ParaId, now: Timeslot, m: UpwardMessage
  ): AccumulateOutcome =
    applyUpwardMessage(accOverWith(initialState, fs), caller, FixedCodeLen, now, now, m)

  /// Refine one otherwise well-formed candidate for `caller`, carrying `msgs`.
  pure def refineWithMsgs(
    s: ParachainServiceState, caller: ParaId, msgs: List[UpwardMessage]
  ): WorkExecResult = {
    val pi = s.paraInfo(caller)
    val pkg: WorkPackage = {
      authorizedParas: List(caller),
      items: List({
        validationCode: { hash: activeCodeHashOr(pi, NoCodeHash), len: FixedCodeLen },
        pov: 0
      })
    }
    refine(s, pkg, false, PvfOk,
      Some(headHash(pi.headData)), HeadDeclared(pi.headData + 1), msgs, 0)[0]
  }

  pure def isOkDigest(r: WorkExecResult): bool =
    match r { | WorkOk(d) => match d { | Ok(_) => true | Err(_) => false } | WorkErr => false }

  pure def activeCodePinned(pi: ParaInfo): bool =
    match pi.validationCode { | Some(vc) => vc.pinned | None => false }

  /// The endpoint invariant itself must reject a hole in the bucket ids. Asset
  /// Hub reads the queue by walking `firstBucket ..= lastBucket`, so a missing id
  /// is a hole in that walk, and it leaves bucket 3's transfer charged for but
  /// unreachable. The endpoint checks alone do not catch it: both endpoints are
  /// present and every id lies between them, so only the contiguity conjunct
  /// rules it out.
  pure val oneTransfer: List[IncomingTransfer] =
    List({ from: MkServiceId(0), amount: 1, toSupervisorBalance: false, memo: 0 })

  pure val gappedBucketsState: ParachainServiceState = {
    ...initialState,
    incomingTransfers: Map(
      1 -> oneTransfer,
      2 -> oneTransfer,
      4 -> oneTransfer),
    incomingTransferBuckets: Some({ firstBucket: 1, lastBucket: 4, count: 3 })
  }

  pure val contiguousBucketsState: ParachainServiceState = {
    ...initialState,
    incomingTransfers: Map(
      1 -> oneTransfer,
      2 -> oneTransfer,
      3 -> oneTransfer),
    incomingTransferBuckets: Some({ firstBucket: 1, lastBucket: 3, count: 3 })
  }
}
