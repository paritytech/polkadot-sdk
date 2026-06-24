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

  // --- Init --------------------------------------------------------------

  action init = all {
    svc' = emptyState,
    now' = 0,
    jamStagingSet' = List(),
    coretimeChain' = MkParaId(1),
    assetHub' = MkParaId(2),
    solicitedSet' = Set(),
    prevSvc' = emptyState
  }

  // --- Step (a): refine + accumulate one work-report --------------------

  action stepRefineAccumulate = {
    // Refine phase (§4.1): pick a live parachain, an authored candidate
    // (which may carry the active code, the pending-upgrade code, or some
    // unrelated hash), an abstract PVF outcome, and a bundle of upward
    // messages. Then apply the Refine wrapper rules to assemble a
    // ParachainWorkResult.
    nondet para_id = oneOf(svc.parachains.keys())
    nondet candidate = oneOf(arbitraryCandidates(svc, para_id))
    nondet pvfPanic = oneOf(Set(false, true))
    val activeHash = svc.paraInfo(para_id).validationCode.hash
    val msgPool = arbitraryMessages(para_id, activeHash)
    nondet m1 = oneOf(msgPool)
    nondet m2 = oneOf(msgPool)
    nondet numMsgs = oneOf(Set(0, 1, 2))
    val msgs =
      if (numMsgs == 0) List()
      else if (numMsgs == 1) List(m1)
      else List(m1, m2)
    nondet itemCount = oneOf(Set(0, 1, 2))
    nondet authConfigMalformed = oneOf(Set(false, true))
    val newHead = svc.paraInfo(para_id).headData + 1
    val pkg: WorkPackage =
      if (itemCount == 0) { authorizedParas: List(para_id), items: List() }
      else if (itemCount == 1) { authorizedParas: List(para_id), items: List(candidate) }
      else { authorizedParas: List(para_id, para_id), items: List(candidate, candidate) }
    val result = refine(svc, pkg, authConfigMalformed, pvfPanic, newHead, msgs, assetHub, coretimeChain)[0]

    // Accumulate phase (§5.1): registration check, parent-head check, code
    // check, code-upgrade activation, then upward-message replay.
    val accIn = { ...emptyOutcome, state: svc }
    val accOut = match result {
      | Err(_) =>
      // Refine failure: in the real protocol Accumulate would append a
      // RefineLogEntry against the para's log. Modeled here as a no-op
      // on service state.
      accIn
      | Ok(ok) =>
      val pre = accumulateOkPrefix(accIn.state, ok, FixedCodeLen, now, assetHub, coretimeChain)
      if (pre.rejected)
      { state: pre.state,
        jamDesignate: None,
        jamSolicit: pre.jamSolicit,
        jamForget: pre.jamForget,
        logs: pre.logs
      }
      else
      ok.upwardMessages.foldl(
        { ...accIn,
          state: pre.state,
          jamSolicit: pre.jamSolicit,
          jamForget: pre.jamForget,
          logs: pre.logs
        },
        (acc, m) => applyUpwardMessage(acc, ok.paraId, FixedCodeLen, assetHub, now, m)
      )
    }

    // Always-accumulate work.
    val sFinal = applyAlwaysAccumulate(accOut.state, now)
    val newStaging = match accOut.jamDesignate {
      | Some(v) => v
      | None => jamStagingSet
    }
    // Ghost-state update: mirror the Solicit/Forget upward messages that
    // were actually processed (Ok result + accumulate not rejected).
    val nextSolicited = match result {
      | Err(_) => solicitedSet
      | Ok(ok) =>
      val pre = accumulateOkPrefix(svc, ok, FixedCodeLen, now, assetHub, coretimeChain)
      if (pre.rejected) solicitedSet
      else ok.upwardMessages.foldl(solicitedSet, (acc, m) => match m {
          | Solicit(payload) => acc.union(Set((ok.paraId, payload.hash)))
          | Forget(payload) => acc.exclude(Set((ok.paraId, payload.hash)))
          | _ => acc
      })
    }
    all {
      svc' = sFinal,
      jamStagingSet' = newStaging,
      now' = now,
      coretimeChain' = coretimeChain,
      assetHub' = assetHub,
      solicitedSet' = nextSolicited,
      prevSvc' = svc
    }
  }

  // --- Step (b): Coretime-driven management primitive -------------------
  //
  /// Any of the four §6 primitives, applied to a nondeterministically chosen
  /// ParaId with nondeterministically chosen arguments (within reasonable
  /// bounds for exploration).
  action stepManagement = {
    nondet paraIdInt = oneOf(Set(1, 2, 3, 4, 5))
    val para_id: ParaId = MkParaId(paraIdInt)
    nondet choice = oneOf(Set("set_balance", "set_head", "set_code", "clean_up"))
    nondet total = oneOf(Set(BaselineFootprint, BaselineFootprint * 2, BaselineFootprint * 4))
    nondet headBlob = oneOf(Set(1, 2, 3, 4, 5, 6))
    nondet codeHash = oneOf(Set(
        { vchBytes: 101 }, { vchBytes: 102 }, { vchBytes: 103 }
    ))
    val newSvc =
    if (choice == "set_balance") parachainSetStateBalance(svc, para_id, total).state
    else if (choice == "set_head") parachainSetHead(svc, para_id, headBlob).state
    else if (choice == "set_code")
    parachainSetValidationCode(svc, para_id, codeHash, FixedCodeLen, FixedCodeLen).state
    else parachainCleanUp(svc, para_id, assetHub)
    all {
      svc' = newSvc,
      now' = now,
      jamStagingSet' = jamStagingSet,
      coretimeChain' = coretimeChain,
      assetHub' = assetHub,
      solicitedSet' = solicitedSet,
      prevSvc' = svc
    }
  }

  // --- Step (c): time advance -------------------------------------------

  action stepTick = all {
    now' = now + 1,
    svc' = svc,
    jamStagingSet' = jamStagingSet,
    coretimeChain' = coretimeChain,
    assetHub' = assetHub,
    solicitedSet' = solicitedSet,
    prevSvc' = svc
  }

  // --- Step composition --------------------------------------------------

  action step = any { stepRefineAccumulate, stepManagement, stepTick }

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
      ParachainSetValidationCode({ paraId: para_id, newValidationCodeHash: codeHashA }),
      ParachainCleanUp(para_id),
      ParachainSetStateBalance({ paraId: para_id, newTotal: 0 })
    )
  }
}
