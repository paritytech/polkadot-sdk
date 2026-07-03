// Top-level composition for the Parachain Service on JAM Quint spec.
//
// The state machine has three classes of step:
// (a) Refine + Accumulate of one parachain work-report.
// (b) Coretime-chain-driven management operations.
// (c) Time advance (epoch / timeslot tick).
//
// Each step is followed by the always-accumulate work (§5.1, general work)
// and a possible JAM `designate` invocation if a `SetValidatorKeys`
// finalized this block.
module main {
  import types.* from "./types"
  import messages.* from "./messages"
  import state.* from "./state"
  import state_balance.* from "./state_balance"
  import code_upgrades.* from "./code_upgrades"
  import validator_keys.* from "./validator_keys"
  import refine.* from "./refine"
  import accumulate.* from "./accumulate"
  import management.* from "./management"
  import state_vars.* from "./state_vars"
  import invariants.* from "./invariants"

  /// Abstract per-test fixed code length (so the upgrade math is concrete).
  pure val FixedCodeLen: int = 64 * 1024

  /// Initial state with the two privileged chains pre-registered, so that
  /// every state mutation in the model flows through `stepRefineAccumulate`
  /// — Coretime and Asset Hub act as ordinary parachains whose PVFs emit
  /// privileged upward messages (Coretime: `ParachainSet*`, Asset Hub:
  /// `SetValidatorKeys` / `UpgradeService` / `TransferOut` / `ConsumeTransfersUpTo`).
  /// Per-para balance budget at init: baseline + headroom for the active
  /// code preimage + headroom for one pending-upgrade code preimage + a
  /// small buffer for other state (staged validator keys, solicited
  /// preimages used by tests, etc.).
  pure val InitialParaBalance: int =
    BaselineFootprint + preimageFootprint(FixedCodeLen) * 2 + BaselineFootprint

  pure val initialState: ParachainServiceState = {
    val coretimeCode: ValidationCodeHash = { vchBytes: 1 }
    val assetHubCode: ValidationCodeHash = { vchBytes: 2 }
    val s1 = parachainSetStateBalance(emptyState, CoretimeParaId, InitialParaBalance).state
    val s2 = parachainSetValidationCode(s1, CoretimeParaId, coretimeCode, FixedCodeLen, FixedCodeLen).state
    val s3 = parachainSetHead(s2, CoretimeParaId, 0).state
    val s4 = parachainSetStateBalance(s3, AssetHubParaId, InitialParaBalance).state
    val s5 = parachainSetValidationCode(s4, AssetHubParaId, assetHubCode, FixedCodeLen, FixedCodeLen).state
    parachainSetHead(s5, AssetHubParaId, 0).state
  }

  // --- Init --------------------------------------------------------------

  action init = all {
    svc' = initialState,
    now' = 0,
    jamStagingSet' = List(),
    solicitedSet' = Set(),
    prevSvc' = initialState,
    lastStepWorkResults' = List(),
    lastStepBouncedTransfers' = List(),
    lastStepAssigns' = List()
  }

  // --- Step (a): refine + accumulate one work-report --------------------

  /// Construct one nondet work package's inputs against `svc`. Used twice
  /// in `stepRefineAccumulate` to model 1 or 2 work packages per block,
  /// from the same or different paras, with mixed valid/invalid outcomes.
  /// `declParent` / `declHead` model whether the PVF called
  /// `set_parent_head_hash` / `set_head`; when false the corresponding
  /// declaration is `None`, which Refine rejects with
  /// `MissingHeadDeclaration` (§4.2).
  pure def buildPackage(
    s: ParachainServiceState,
    paraId: ParaId,
    candidate: ParachainCandidate,
    m1: UpwardMessage,
    m2: UpwardMessage,
    numMsgs: int,
    itemCount: int,
    declParent: bool,
    declHead: bool
  ): {
    pkg: WorkPackage, msgs: List[UpwardMessage],
    parentHead: Option[Hash], newHead: Option[HeadData]
  } = {
    val msgs =
      if (numMsgs == 0) List()
      else if (numMsgs == 1) List(m1)
      else List(m1, m2)
    val pkg: WorkPackage =
      if (itemCount == 0) { authorizedParas: List(paraId), items: List() }
      else if (itemCount == 1) { authorizedParas: List(paraId), items: List(candidate) }
      else { authorizedParas: List(paraId, paraId), items: List(candidate, candidate) }
    // The collator builds on the para's current head. For a live para that is
    // `headHash(headData)` and the new head is `headData + 1`, so the result
    // chains correctly and passes Accumulate's parent-head check (§5.1 step 3).
    // For a non-live para (e.g. cleaned up by an earlier package this block)
    // there is no head; we declare an arbitrary one — the candidate is dropped
    // at Accumulate (§5.1 step 1) regardless.
    val curHead = if (s.isLive(paraId)) s.paraInfo(paraId).headData else 0
    {
      pkg: pkg, msgs: msgs,
      parentHead: if (declParent) Some(headHash(curHead)) else None,
      newHead: if (declHead) Some(curHead + 1) else None
    }
  }

  action stepRefineAccumulate = {
    // A block advances the clock to its own timeslot (nondet gap >= 1 models
    // empty slots between blocks), then runs always-accumulate at that slot —
    // so `now` only ever takes block-timeslot values and every block flushes
    // due authorizer queues.
    nondet blockGap = oneOf(Set(1, 2))
    val blockNow = now + blockGap
    val afterAlwaysAccumulate = applyAlwaysAccumulate(svc, blockNow)

    // First work package
    nondet wp1Para = oneOf(afterAlwaysAccumulate.state.parachains.keys())
    nondet wp1Candidate = oneOf(arbitraryCandidates(afterAlwaysAccumulate.state, wp1Para))
    nondet wp1PvfPanic = oneOf(Set(false, true))
    val wp1Pool = arbitraryMessages(
      wp1Para, afterAlwaysAccumulate.state.paraInfo(wp1Para).validationCode.hash)
    nondet wp1M1 = oneOf(wp1Pool)
    nondet wp1M2 = oneOf(wp1Pool)
    nondet wp1NumMsgs = oneOf(Set(0, 1, 2))
    nondet wp1ItemCount = oneOf(Set(0, 1, 2))
    nondet wp1AuthMalformed = oneOf(Set(false, true))
    nondet wp1AuthTrace = oneOf(Set(0, 1, 2))
    // Gray paper `WorkExecResult::Error` — JAM substituted an error for this
    // work-item before the service's refine ran. Accumulate sees the no-op
    // case (§3.3).
    nondet wp1IsJamSkip = oneOf(Set(false, true))
    // Whether the PVF declared its parent head / new head (§4.2). When
    // false, Refine rejects with `MissingHeadDeclaration`.
    nondet wp1DeclParent = oneOf(Set(false, true))
    nondet wp1DeclHead = oneOf(Set(false, true))
    val wp1 = buildPackage(
      afterAlwaysAccumulate.state, wp1Para, wp1Candidate, wp1M1, wp1M2,
      wp1NumMsgs, wp1ItemCount, wp1DeclParent, wp1DeclHead
    )
    val wp1Wr: WorkResult = {
      result:
        if (wp1IsJamSkip) WorkErr
        else WorkOk(refine(
          afterAlwaysAccumulate.state, wp1.pkg, wp1AuthMalformed, wp1PvfPanic,
          wp1.parentHead, wp1.newHead, wp1.msgs
        )[0]),
      authTrace: wp1AuthTrace
    }

    // Second work package (possibly same para — captures multi-package mix)
    nondet wp2Para = oneOf(afterAlwaysAccumulate.state.parachains.keys())
    nondet wp2Candidate = oneOf(arbitraryCandidates(afterAlwaysAccumulate.state, wp2Para))
    nondet wp2PvfPanic = oneOf(Set(false, true))
    val wp2Pool = arbitraryMessages(
      wp2Para, afterAlwaysAccumulate.state.paraInfo(wp2Para).validationCode.hash)
    nondet wp2M1 = oneOf(wp2Pool)
    nondet wp2M2 = oneOf(wp2Pool)
    nondet wp2NumMsgs = oneOf(Set(0, 1, 2))
    nondet wp2ItemCount = oneOf(Set(0, 1, 2))
    nondet wp2AuthMalformed = oneOf(Set(false, true))
    nondet wp2AuthTrace = oneOf(Set(0, 1, 2))
    nondet wp2IsJamSkip = oneOf(Set(false, true))
    nondet wp2DeclParent = oneOf(Set(false, true))
    nondet wp2DeclHead = oneOf(Set(false, true))

    // 0 packages models a pure always-accumulate block (the service still
    // accumulates with no work packages); 1 or 2 exercise the normal path.
    nondet numPkgs = oneOf(Set(0, 1, 2))

    val passThrough: PerPackageOutcome =
      {
        state: afterAlwaysAccumulate.state,
        solicited: solicitedSet,
        jamDesignate: None,
        jamAssigns: List()
      }
    val out1 =
      if (numPkgs == 0) passThrough
      else accumulateOnePackage(afterAlwaysAccumulate.state, wp1Wr, solicitedSet, None, FixedCodeLen, blockNow)
    // wp2 refines against the post-wp1 state so a second package for the
    // SAME para chains off wp1's new head; different para → identical to
    // refining against `afterAlwaysAccumulate.state`. wp1 may have cleaned up `wp2Para`
    // (it was chosen from the post-flush `afterAlwaysAccumulate.state.parachains`);
    // `buildPackage` and `refine` both guard the resulting unregistered para
    // (it fails Refine with `InvalidCodeHash` and is dropped at Accumulate).
    val wp2 = buildPackage(
      out1.state, wp2Para, wp2Candidate, wp2M1, wp2M2,
      wp2NumMsgs, wp2ItemCount, wp2DeclParent, wp2DeclHead
    )
    val wp2Wr: WorkResult = {
      result:
        if (wp2IsJamSkip) WorkErr
        else WorkOk(refine(
          out1.state, wp2.pkg, wp2AuthMalformed, wp2PvfPanic,
          wp2.parentHead, wp2.newHead, wp2.msgs
        )[0]),
      authTrace: wp2AuthTrace
    }
    val out2 =
      if (numPkgs == 2) {
        val second = accumulateOnePackage(
          out1.state, wp2Wr, out1.solicited, out1.jamDesignate, FixedCodeLen, blockNow)
        { ...second, jamAssigns: out1.jamAssigns.concat(second.jamAssigns) }
      } else out1

    val newStaging = match out2.jamDesignate {
      | Some(v) => v
      | None => jamStagingSet
    }

    val results =
      if (numPkgs == 0) List()
      else if (numPkgs == 1) List(wp1Wr)
      else List(wp1Wr, wp2Wr)

    all {
      svc' = out2.state,
      jamStagingSet' = newStaging,
      now' = blockNow,
      solicitedSet' = out2.solicited,
      prevSvc' = svc,
      lastStepWorkResults' = results,
      lastStepBouncedTransfers' = List(),
      lastStepAssigns' = afterAlwaysAccumulate.assigns.concat(out2.jamAssigns)
    }
  }

  // --- Step (b): another service sends an OnTransfer into Asset Hub ----
  //
  /// Models JAM-level OnTransfer ingress (§5.1, incoming-transfer phase):
  /// append to `incoming_transfers` if it has space, otherwise record a bounce
  /// in `lastStepBouncedTransfers` (the Parachain Service must emit a matching
  /// outgoing JAM transfer back to the sender with the same memo).
  action stepIncomingTransfer = {
    val senderPool: Set[ServiceId] = Set(MkServiceId(0), MkServiceId(3), MkServiceId(7))
    nondet senderId = oneOf(senderPool)
    nondet amount = oneOf(Set(100, 1000, 10000))
    nondet memo = oneOf(Set(0, 1, 2))
    val incoming = { from: senderId, amount: amount, memo: memo }
    val full = svc.incomingTransfers.length() >= IncomingTransfersMaxLen
    val newSvc =
      if (full) svc
      else { ...svc, incomingTransfers: svc.incomingTransfers.append(incoming) }
    val bounced = if (full) List(incoming) else List()
    all {
      svc' = newSvc,
      now' = now,
      jamStagingSet' = jamStagingSet,
      solicitedSet' = solicitedSet,
      prevSvc' = svc,
      lastStepWorkResults' = List(),
      lastStepBouncedTransfers' = bounced,
      lastStepAssigns' = List()
    }
  }

  // --- Step composition --------------------------------------------------

  action step = any { stepRefineAccumulate, stepIncomingTransfer }

  // --- Auxiliary: nondeterministic refine-input generators --------------

  /// A small menu of candidates a collator might produce: one carrying the
  /// active validation code (the happy path), one carrying the pending
  /// upgrade code (the dual-code window of §5.2), and one carrying an
  /// unsolicited hash (which Refine must reject as `InvalidCodeHash`).
  pure def arbitraryCandidates(
    s: ParachainServiceState, para_id: ParaId
  ): Set[ParachainCandidate] = {
    val pi = s.paraInfo(para_id)
    val active = pi.validationCode.hash
    val pending = match pi.pendingUpgrade {
      | None => active
      | Some(pair) => pair._1.hash
    }
    Set(
      { validationCodeHash: active, pov: 0 },
      { validationCodeHash: pending, pov: 0 },
      { validationCodeHash: { vchBytes: 99999 }, pov: 0 }
    )
  }

  /// Menu of individual upward messages a PVF might emit during Refine.
  /// Every parachain draws from the SAME menu — restricted host calls
  /// (AH-only, Coretime-only) are included for all paras so the Refine
  /// wrapper's `RestrictedHostFunction` rejection (§4.3) is exercised
  /// for each one. Bundles of multiple messages are composed nondet in
  /// the step action, so any combination/ordering is reachable.
  pure def arbitraryMessages(
    para_id: ParaId, activeCode: ValidationCodeHash
  ): Set[UpwardMessage] = {
    val anyHash: Hash = { hashBytes: 42 }
    val unsolicitedHash: Hash = { hashBytes: 99999 }
    val codeHashA: ValidationCodeHash = { vchBytes: 101 }
    val codeHashB: ValidationCodeHash = { vchBytes: 102 }
    val anyAuthHash: AuthorizerHash = { authBytes: 0 }
    Set(
      RequestCodeUpgrade({ hash: codeHashA, len: FixedCodeLen }),
      RequestCodeUpgrade({ hash: codeHashB, len: FixedCodeLen }),
      Solicit({ hash: anyHash, len: 1024 }),
      Forget({ hash: anyHash, len: 1024 }),
      // Solicit/forget of the para's OWN active code — exercises the
      // `pinned` path (§5.2).
      Solicit({ hash: vchAsHash(activeCode), len: FixedCodeLen }),
      Forget({ hash: vchAsHash(activeCode), len: FixedCodeLen }),
      // AH-only — Refine rejects from non-AH callers (§4.3).
      TransferOut({ dest: MkServiceId(0), amount: 100, memo: 0 }),
      SetValidatorKeys({ keys: List(0, 0, 0, 0, 0, 0), isLast: false }),
      SetValidatorKeys({ keys: List(0, 0, 0, 0, 0, 0), isLast: true }),
      SetValidatorKeys({ keys: List(), isLast: true }),
      ConsumeTransfersUpTo(0),
      UpgradeService({ codeHash: vchAsHash(activeCode), minItemGas: 0, minMemoGas: 0 }),
      UpgradeService({ codeHash: unsolicitedHash, minItemGas: 0, minMemoGas: 0 }),
      // Coretime-only — Refine rejects from non-Coretime callers.
      SetAuthorizerQueue({
        core: 0, queue: List(anyAuthHash), immediate: false, newAssigner: None
      }),
      ParachainSetHead({ paraId: para_id, newHead: 0 }),
      ParachainSetValidationCode({ paraId: para_id, newValidationCodeHash: codeHashA, newValidationCodeLen: FixedCodeLen }),
      ParachainSetStateBalance({ paraId: para_id, newTotal: 0 }),
      // Coretime-only targeting OTHER paras — registers new paras and
      // performs forced recovery / clean-up across the model's paraId space.
      ParachainSetStateBalance({ paraId: MkParaId(3), newTotal: BaselineFootprint * 2 }),
      ParachainSetStateBalance({ paraId: MkParaId(4), newTotal: BaselineFootprint * 2 }),
      ParachainSetStateBalance({ paraId: MkParaId(5), newTotal: BaselineFootprint * 2 }),
      ParachainSetHead({ paraId: MkParaId(3), newHead: 7 }),
      ParachainSetHead({ paraId: MkParaId(4), newHead: 7 }),
      ParachainSetValidationCode({ paraId: MkParaId(3), newValidationCodeHash: codeHashB, newValidationCodeLen: FixedCodeLen }),
      // `ParachainCleanUp` is Coretime-only, but the real Coretime chain
      // never cleans up the privileged chains (1 = Coretime, 2 = Asset Hub)
      // — that would brick the control plane. We only generate clean-up of
      // non-privileged test paras (3, 4), never `para_id` (which is the
      // originating para, e.g. Coretime cleaning up itself).
      ParachainCleanUp(MkParaId(3)),
      ParachainCleanUp(MkParaId(4))
    )
  }

}
