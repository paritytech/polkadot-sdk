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
          Some(headHash(pi.headData)), Some(pi.headData + 1), p._3, 0
        )[0],
        authTrace: 0
      }
      acc.append(wr)
    })
    // Run the exact per-block Accumulate pipeline the model step runs.
    val blockOut = accumulateBlock(
      svc, blockNow, jamStagingSet, solicitedSet, FixedCodeLen, packages)
    all {
      svc' = blockOut.state,
      now' = blockNow,
      jamStagingSet' = blockOut.staging,
      solicitedSet' = blockOut.solicited,
      prevSvc' = svc,
      lastStepWorkResults' = blockOut.results,
      lastStepAssigns' = blockOut.assigns,
      lastHeadRoot' = blockOut.headRoot,
      logPrunedBelow' =
        blockOut.logPruned.foldl(logPrunedBelow, (m, pair) => m.put(pair._1, pair._2))
    }
  }

  /// Singular convenience wrapper: apply one happy-path work package.
  action applyHappyPath(
    paraId: ParaId, codeHash: ValidationCodeHash, msgs: List[UpwardMessage]
  ): bool = applyHappyPaths(List((paraId, codeHash, msgs)))

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
    logPrunedBelow' = logPrunedBelow
  }

  /// `incoming_transfers` at capacity, maximally fragmented: one transfer per
  /// slot in slots `0 .. MaxIncomingTransfers`, chained head to tail.
  pure val fullIncomingTransfersFixture: Timeslot -> {
    transfers: List[{ from: ServiceId, amount: Balance, memo: Memo }],
    nextSlot: Option[Timeslot]
  } =
    range(0, MaxIncomingTransfers).foldl(Map(), (acc, i) =>
      acc.put(i, {
        transfers: List({ from: MkServiceId(0), amount: 1000 + i, memo: 0 }),
        nextSlot: if (i == MaxIncomingTransfers - 1) None else Some(i + 1)
      }))

  pure val fullTransferQueue: ParachainServiceState = {
    ...initialState,
    incomingTransfers: fullIncomingTransfersFixture,
    incomingTransferChain:
      Some({ firstSlot: 0, lastSlot: MaxIncomingTransfers - 1,
             count: MaxIncomingTransfers })
  }

  /// One slot short of the reservation: the queue can still hold one more.
  pure val almostFullTransferQueue: ParachainServiceState = {
    ...fullTransferQueue,
    incomingTransfers: fullTransferQueue.incomingTransfers.mapRemove(MaxIncomingTransfers - 1),
    incomingTransferChain:
      Some({ firstSlot: 0, lastSlot: MaxIncomingTransfers - 2,
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
        Some(headHash(StaleParentHead)), Some(StaleParentHead), msgs, 0
      )[0],
      authTrace: 0
    }
    val afterAlwaysAccumulate = applyAlwaysAccumulate(svc, blockNow)
    val out = accumulateOnePackage(
      afterAlwaysAccumulate.state, wr, solicitedSet, None, FixedCodeLen, blockNow
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
      logPrunedBelow' = logPrunedBelow
    }
  }

  /// A fresh `AccumulateOutcome` over `s`, for replaying one upward message.
  pure def accOver(s: ParachainServiceState): AccumulateOutcome = {
    state: s, jamDesignate: None, jamSolicit: Set(),
    jamForget: Set(), jamAssigns: List(), logs: List()
  }

  pure def replay(
    s: ParachainServiceState, caller: ParaId, now: Timeslot, m: UpwardMessage
  ): AccumulateOutcome =
    applyUpwardMessage(accOver(s), caller, FixedCodeLen, now, now, m)

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
      Some(headHash(pi.headData)), Some(pi.headData + 1), msgs, 0)[0]
  }

  pure def isOkDigest(r: WorkExecResult): bool =
    match r { | WorkOk(d) => match d { | Ok(_) => true | Err(_) => false } | WorkErr => false }

  pure def activeCodePinned(pi: ParaInfo): bool =
    match pi.validationCode { | Some(vc) => vc.pinned | None => false }

  /// The chain invariant itself must reject a bucket no predecessor points at.
  /// Buckets 1 and 2 both link to 3, so walking from `firstSlot` goes 1 -> 3 and
  /// never visits 2, leaving it charged for but unreachable by
  /// `consume_transfers_up_to`. Every per-bucket conjunct still holds (endpoints
  /// present, keys in range, only the tail terminating, successors later and
  /// occupied), so only the successor-coverage conjunct catches it.
  pure val oneTransfer: List[{ from: ServiceId, amount: Balance, memo: Memo }] =
    List({ from: MkServiceId(0), amount: 1, memo: 0 })

  pure val skippingChainState: ParachainServiceState = {
    ...initialState,
    incomingTransfers: Map(
      1 -> { transfers: oneTransfer, nextSlot: Some(3) },
      2 -> { transfers: oneTransfer, nextSlot: Some(3) },
      3 -> { transfers: oneTransfer, nextSlot: None }),
    incomingTransferChain: Some({ firstSlot: 1, lastSlot: 3, count: 3 })
  }

  pure val linkedChainState: ParachainServiceState = {
    ...initialState,
    incomingTransfers: Map(
      1 -> { transfers: oneTransfer, nextSlot: Some(2) },
      2 -> { transfers: oneTransfer, nextSlot: Some(3) },
      3 -> { transfers: oneTransfer, nextSlot: None }),
    incomingTransferChain: Some({ firstSlot: 1, lastSlot: 3, count: 3 })
  }
}
