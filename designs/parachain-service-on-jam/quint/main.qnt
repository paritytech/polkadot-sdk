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
    nondet msgs = oneOf(arbitraryUpwardBundles(para_id, assetHub, svc.paraInfo(para_id).validationCode.hash))
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
    nondet para_id = oneOf(Set(MkParaId(1), MkParaId(2), MkParaId(3), MkParaId(4), MkParaId(5)))
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

  /// A small menu of upward-message bundles a PVF might emit during Refine.
  /// For Asset Hub the menu includes well-formed `SetValidatorKeys` chunks
  /// (partial / finalize / abort), and intentionally ill-formed bundles that
  /// the Refine wrapper rejects via `wellFormedUpwardMessages` (multiple
  /// `SetValidatorKeys` per Refine — caught by `setValidatorKeysCount ≤ 1`).
  pure def arbitraryUpwardBundles(
    para_id: ParaId, assetHubId: ParaId, activeCode: ValidationCodeHash
  ): Set[List[UpwardMessage]] = {
    val baseBundles = Set(
      List(),
      List(RequestCodeUpgrade({ hash: { vchBytes: 101 }, len: FixedCodeLen })),
      // A second distinct upgrade hash so the overwrite path (superseding an
      // in-flight pending upgrade with a different code) gets exercised.
      List(RequestCodeUpgrade({ hash: { vchBytes: 102 }, len: FixedCodeLen })),
      // Plain solicit/forget pair — exercises `solicit_implies_registry`
      // and the registry's referencer-multiplexing on a non-code preimage.
      List(Solicit({ hash: { hashBytes: 42 }, len: 1024 })),
      List(Forget({ hash: { hashBytes: 42 }, len: 1024 })),
      // Parachain soliciting / forgetting its OWN active code — exercises the
      // `pinned` path: solicit sets the bit (no extra balance), forget
      // clears it without forwarding a JAM forget (the code is still active).
      List(Solicit({ hash: vchAsHash(activeCode), len: FixedCodeLen })),
      List(Forget({ hash: vchAsHash(activeCode), len: FixedCodeLen }))
    )
    if (para_id == assetHubId)
    baseBundles.union(Set(
        // §5.3 partial chunk: 6 keys (|ValCount| min), not last.
        List(SetValidatorKeys({ keys: List(0, 0, 0, 0, 0, 0), isLast: false })),
        // §5.3 finalize: append 6 keys with isLast — designate fires
        // (length 6 is in ValCount).
        List(SetValidatorKeys({ keys: List(0, 0, 0, 0, 0, 0), isLast: true })),
        // §5.3 abort: empty chunk with isLast — designate rejects the empty
        // vector, buffer is cleared.
        List(SetValidatorKeys({ keys: List(), isLast: true })),
        // §4.3 violation: two `SetValidatorKeys` in one Refine — caught by
        // the well-formedness rule and turned into `Err(Opaque)`.
        List(
          SetValidatorKeys({ keys: List(0), isLast: false }),
          SetValidatorKeys({ keys: List(0), isLast: false })
        ),
        // §5.4 happy path: upgrade to a hash whose preimage is already in
        // the registry (AH's own active validation code).
        List(UpgradeService({ codeHash: vchAsHash(activeCode), minItemGas: 0, minMemoGas: 0 })),
        // §5.4 missing-preimage path: upgrade to an unsolicited hash —
        // rejected with `ServiceUpgradePreimageMissing`.
        List(UpgradeService({ codeHash: { hashBytes: 99999 }, minItemGas: 0, minMemoGas: 0 }))
    ))
    else
    // Non-Asset-Hub paras may still attempt restricted host calls —
    // exercise the service-restriction Refine rejection (§4.3).
    baseBundles.union(Set(
        List(SetValidatorKeys({ keys: List(0), isLast: false }))
    ))
  }
}
