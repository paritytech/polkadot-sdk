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
  pure val initialState: ParachainServiceState = {
    val coretimeId: ParaId = MkParaId(1)
    val assetHubId: ParaId = MkParaId(2)
    val coretimeCode: ValidationCodeHash = { vchBytes: 1 }
    val assetHubCode: ValidationCodeHash = { vchBytes: 2 }
    val s1 = parachainSetStateBalance(emptyState, coretimeId, BaselineFootprint * 4).state
    val s2 = parachainSetValidationCode(s1, coretimeId, coretimeCode, FixedCodeLen, FixedCodeLen).state
    val s3 = parachainSetHead(s2, coretimeId, 0).state
    val s4 = parachainSetStateBalance(s3, assetHubId, BaselineFootprint * 4).state
    val s5 = parachainSetValidationCode(s4, assetHubId, assetHubCode, FixedCodeLen, FixedCodeLen).state
    parachainSetHead(s5, assetHubId, 0).state
  }

  // --- Init --------------------------------------------------------------

  action init = all {
    svc' = initialState,
    now' = 0,
    jamStagingSet' = List(),
    coretimeChain' = MkParaId(1),
    assetHub' = MkParaId(2),
    solicitedSet' = Set(),
    prevSvc' = initialState,
    lastStepWorkResults' = List(),
    lastStepBouncedTransfers' = List()
  }

  // --- Step (a): refine + accumulate one work-report --------------------

  /// Construct one nondet work package's inputs against `svc`. Used twice
  /// in `stepRefineAccumulate` to model 1 or 2 work packages per block,
  /// from the same or different paras, with mixed valid/invalid outcomes.
  pure def buildPackage(
    s: ParachainServiceState,
    paraId: ParaId,
    candidate: ParachainCandidate,
    m1: UpwardMessage,
    m2: UpwardMessage,
    numMsgs: int,
    itemCount: int
  ): { pkg: WorkPackage, msgs: List[UpwardMessage], newHead: HeadData } = {
    val msgs =
      if (numMsgs == 0) List()
      else if (numMsgs == 1) List(m1)
      else List(m1, m2)
    val pkg: WorkPackage =
      if (itemCount == 0) { authorizedParas: List(paraId), items: List() }
      else if (itemCount == 1) { authorizedParas: List(paraId), items: List(candidate) }
      else { authorizedParas: List(paraId, paraId), items: List(candidate, candidate) }
    { pkg: pkg, msgs: msgs, newHead: s.paraInfo(paraId).headData + 1 }
  }

  action stepRefineAccumulate = {
    // First work package
    nondet wp1Para = oneOf(svc.parachains.keys())
    nondet wp1Candidate = oneOf(arbitraryCandidates(svc, wp1Para))
    nondet wp1PvfPanic = oneOf(Set(false, true))
    val wp1Pool = arbitraryMessages(wp1Para, svc.paraInfo(wp1Para).validationCode.hash)
    nondet wp1M1 = oneOf(wp1Pool)
    nondet wp1M2 = oneOf(wp1Pool)
    nondet wp1NumMsgs = oneOf(Set(0, 1, 2))
    nondet wp1ItemCount = oneOf(Set(0, 1, 2))
    nondet wp1AuthMalformed = oneOf(Set(false, true))
    val wp1 = buildPackage(svc, wp1Para, wp1Candidate, wp1M1, wp1M2, wp1NumMsgs, wp1ItemCount)
    val wp1Result = refine(svc, wp1.pkg, wp1AuthMalformed, wp1PvfPanic, wp1.newHead, wp1.msgs, assetHub, coretimeChain)[0]

    // Second work package (possibly same para — captures multi-package mix)
    nondet wp2Para = oneOf(svc.parachains.keys())
    nondet wp2Candidate = oneOf(arbitraryCandidates(svc, wp2Para))
    nondet wp2PvfPanic = oneOf(Set(false, true))
    val wp2Pool = arbitraryMessages(wp2Para, svc.paraInfo(wp2Para).validationCode.hash)
    nondet wp2M1 = oneOf(wp2Pool)
    nondet wp2M2 = oneOf(wp2Pool)
    nondet wp2NumMsgs = oneOf(Set(0, 1, 2))
    nondet wp2ItemCount = oneOf(Set(0, 1, 2))
    nondet wp2AuthMalformed = oneOf(Set(false, true))

    // 0 packages models a pure always-accumulate block (the service still
    // accumulates with no work packages); 1 or 2 exercise the normal path.
    nondet numPkgs = oneOf(Set(0, 1, 2))

    val passThrough: PerPackageOutcome =
      { state: svc, solicited: solicitedSet, jamDesignate: None }
    val out1 =
      if (numPkgs == 0) passThrough
      else accumulateOnePackage(svc, wp1Result, solicitedSet, None, FixedCodeLen, now, assetHub, coretimeChain)
    // wp2 refines against the post-wp1 state so a second package for the
    // SAME para chains off wp1's new head; different para → identical to
    // refining against `svc`.
    val wp2 = buildPackage(out1.state, wp2Para, wp2Candidate, wp2M1, wp2M2, wp2NumMsgs, wp2ItemCount)
    val wp2Result = refine(out1.state, wp2.pkg, wp2AuthMalformed, wp2PvfPanic, wp2.newHead, wp2.msgs, assetHub, coretimeChain)[0]
    val out2 =
      if (numPkgs == 2)
        accumulateOnePackage(out1.state, wp2Result, out1.solicited, out1.jamDesignate, FixedCodeLen, now, assetHub, coretimeChain)
      else out1

    val sFinal = applyAlwaysAccumulate(out2.state, now)
    val newStaging = match out2.jamDesignate {
      | Some(v) => v
      | None => jamStagingSet
    }

    val results =
      if (numPkgs == 0) List()
      else if (numPkgs == 1) List(wp1Result)
      else List(wp1Result, wp2Result)

    all {
      svc' = sFinal,
      jamStagingSet' = newStaging,
      now' = now,
      coretimeChain' = coretimeChain,
      assetHub' = assetHub,
      solicitedSet' = out2.solicited,
      prevSvc' = svc,
      lastStepWorkResults' = results,
      lastStepBouncedTransfers' = List()
    }
  }

  // --- Step (b): time advance -------------------------------------------

  action stepTick = all {
    now' = now + 1,
    svc' = svc,
    jamStagingSet' = jamStagingSet,
    coretimeChain' = coretimeChain,
    assetHub' = assetHub,
    solicitedSet' = solicitedSet,
    prevSvc' = svc,
    lastStepWorkResults' = List(),
    lastStepBouncedTransfers' = List()
  }

  // --- Step (c): another service sends an OnTransfer into Asset Hub ----
  //
  /// Models JAM-level OnTransfer ingress (§5.1 step 2): append to
  /// `incoming_transfers` if it has space, otherwise record a bounce in
  /// `lastStepBouncedTransfers` (the Parachain Service must emit a
  /// matching outgoing JAM transfer back to the sender with the same memo).
  action stepIncomingTransfer = {
    nondet senderId = oneOf(Set(MkServiceId(0), MkServiceId(3), MkServiceId(7)))
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
      coretimeChain' = coretimeChain,
      assetHub' = assetHub,
      solicitedSet' = solicitedSet,
      prevSvc' = svc,
      lastStepWorkResults' = List(),
      lastStepBouncedTransfers' = bounced
    }
  }

  // --- Step composition --------------------------------------------------

  action step = any { stepRefineAccumulate, stepTick, stepIncomingTransfer }

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
      ParachainCleanUp(para_id),
      ParachainSetStateBalance({ paraId: para_id, newTotal: 0 }),
      // Coretime-only targeting OTHER paras — registers new paras and
      // performs forced recovery / clean-up across the model's paraId space.
      ParachainSetStateBalance({ paraId: MkParaId(3), newTotal: BaselineFootprint * 2 }),
      ParachainSetStateBalance({ paraId: MkParaId(4), newTotal: BaselineFootprint * 2 }),
      ParachainSetStateBalance({ paraId: MkParaId(5), newTotal: BaselineFootprint * 2 }),
      ParachainSetHead({ paraId: MkParaId(3), newHead: 7 }),
      ParachainSetHead({ paraId: MkParaId(4), newHead: 7 }),
      ParachainSetValidationCode({ paraId: MkParaId(3), newValidationCodeHash: codeHashB, newValidationCodeLen: FixedCodeLen }),
      ParachainCleanUp(MkParaId(3)),
      ParachainCleanUp(MkParaId(4))
    )
  }

}
