// Coretime-chain-only host calls for parachain lifecycle (design §6).
//
// All four are idempotent and the Parachain Service performs no rights-
// checking of its own: the Coretime chain is the sole authority on which
// ParaIds are live and who owns them. The model exposes them as separate
// actions so we can study any ordering the Coretime chain might choose.
module management {
  import types.* from "./types"
  import messages.* from "./messages"
  import state.* from "./state"
  import state_balance.* from "./state_balance"

  type MgmtOutcome = {
    state: ParachainServiceState,
    // Hashes for which JAM `solicit` / `forget` should be invoked.
    jamSolicit: Set[Hash],
    jamForget: Set[Hash],
    // Log entries to append.
    logs: List[AccumulateLog]
  }

  /// §6.1: sole creator of `ParaInfo`. On an unused ParaId, creates with
  /// the baseline footprint pre-charged. On an existing ParaId, overwrites
  /// `totalStateBalance` iff `new_total ≥ used_state_balance`.
  pure def parachainSetStateBalance(
    s: ParachainServiceState,
    para_id: ParaId,
    newTotal: Balance
  ): MgmtOutcome = {
    if (not(s.isLive(para_id))) {
      match initParaInfo(para_id, newTotal) {
        | None =>
        // Rejected: new_total below baseline.
        {
          state: s,
          jamSolicit: Set(),
          jamForget: Set(),
          logs: List(StateBalanceUpdateRejected({
                attempted: newTotal,
                currentTotal: 0,
                currentUsed: baselineFor(para_id)
          }))
        }
        | Some(pi) =>
        {
          state: { ...s, parachains: s.parachains.put(para_id, pi) },
          jamSolicit: Set(),
          jamForget: Set(),
          logs: List()
        }
      }
    } else {
      val pi = s.paraInfo(para_id)
      if (newTotal < pi.usedStateBalance) {
        {
          state: s,
          jamSolicit: Set(),
          jamForget: Set(),
          logs: List(StateBalanceUpdateRejected({
                attempted: newTotal,
                currentTotal: pi.totalStateBalance,
                currentUsed: pi.usedStateBalance
          }))
        }
      } else {
        {
          state: { ...s, parachains: s.parachains.put(para_id, pi.setTotal(newTotal)) },
          jamSolicit: Set(),
          jamForget: Set(),
          logs: List()
        }
      }
    }
  }

  /// §6.2 / §6.3: upsert head data. No-op on a ParaId whose ParaInfo
  /// doesn't exist yet (Coretime must call set_state_balance first).
  pure def parachainSetHead(
    s: ParachainServiceState,
    para_id: ParaId,
    newHead: HeadData
  ): MgmtOutcome = {
    if (not(s.isLive(para_id))) {
      { state: s, jamSolicit: Set(), jamForget: Set(), logs: List() }
    } else {
      val pi = s.paraInfo(para_id)
      {
        state: { ...s, parachains: s.parachains.put(para_id, { ...pi, headData: newHead }) },
        jamSolicit: Set(),
        jamForget: Set(),
        logs: List()
      }
    }
  }

  /// §6.2 / §6.3: upsert validation code (forced replacement; bypasses the
  /// normal upgrade lifecycle). Solicits the new code if not already
  /// solicited, releases the old active code and any pending-upgrade code,
  /// each only when the parachain isn't independently soliciting it
  /// (`pinned`; §5.2) and it differs from the new code, and clears
  /// the pending upgrade.
  pure def parachainSetValidationCode(
    s: ParachainServiceState,
    para_id: ParaId,
    newHash: ValidationCodeHash,
    codeLen: int,
    now: Timeslot
  ): MgmtOutcome = {
    if (not(s.isLive(para_id))) {
      { state: s, jamSolicit: Set(), jamForget: Set(), logs: List() }
    } else {
      val pi = s.paraInfo(para_id)
      val newEqualsPending =
        match pi.pendingUpgrade {
          | None => false
          | Some(pair) => pair._1.ref.hash == newHash
        }
      // The parachain independently solicited newHash iff it references it
      // for a reason other than being the current active or pending code.
      val activeEqualsNew =
        match pi.validationCode { | Some(vc) => vc.ref.hash == newHash | None => false }
      val parachainHadIt =
        s.hasReferencer(vchAsHash(newHash), codeLen, para_id) and
        not(activeEqualsNew) and not(newEqualsPending)
      // Acquire the new referencer (no charge if already solicited); reject
      // if no headroom.
      val acquired = s.addReferencer(para_id, vchAsHash(newHash), codeLen)
      match acquired.log {
        | Some(_) =>
        { state: s, jamSolicit: Set(), jamForget: Set(), logs: List().appendOption(acquired.log) }
        | None =>
        // Release the old active code, if any - unless it equals the new code
        // or is independently solicited (a para with no active code yet has
        // nothing to release).
        val afterOld =
          match pi.validationCode {
            | None =>
              { state: acquired.state, jamSolicit: None, jamForget: None, retained: false, log: None }
            | Some(vc) =>
              if (vc.ref.hash == newHash or vc.pinned)
                { state: acquired.state, jamSolicit: None, jamForget: None, retained: false, log: None }
              else
                acquired.state.removeReferencer(para_id, vchAsHash(vc.ref.hash), vc.ref.len, now)
          }
        // Clear any pending upgrade, releasing its code unless it equals the
        // new code or is independently solicited.
        val afterPending =
          match pi.pendingUpgrade {
            | None =>
              { state: afterOld.state, jamSolicit: None, jamForget: None, retained: false, log: None }
            | Some(pair) =>
              if (pair._1.ref.hash == newHash or pair._1.pinned)
                { state: afterOld.state, jamSolicit: None, jamForget: None, retained: false, log: None }
              else
                afterOld.state.removeReferencer(para_id, vchAsHash(pair._1.ref.hash), pair._1.ref.len, now)
          }
        val pi2 = afterPending.state.paraInfo(para_id)
        {
          state: {
            ...afterPending.state,
            parachains: afterPending.state.parachains.put(
              para_id,
              { ...pi2,
                // If the new hash equals the existing active code, preserve its
                // `pinned` bit; otherwise the new code's bit is `parachainHadIt`.
                validationCode: Some({
                  ref: { hash: newHash, len: codeLen },
                  pinned:
                    match pi.validationCode {
                      | Some(vc) => if (vc.ref.hash == newHash) vc.pinned else parachainHadIt
                      | None => parachainHadIt
                    }
                }),
                pendingUpgrade: None
              }
            )
          },
          jamSolicit: optionToSet(acquired.jamSolicit),
          jamForget: optionToSet(afterOld.jamForget).union(optionToSet(afterPending.jamForget)),
          // Both the old active and the pending code can enter limbo with
          // distinct deadlines.
          logs: List().appendOption(afterOld.log).appendOption(afterPending.log)
        }
      }
    }
  }

   /// The exact `usedStateBalance` a para may hold at clean-up: its baseline
   /// plus the footprints of its active validation code and (if any) pending
   /// upgrade code: nothing else. Any excess means it still holds other
   /// solicited preimages or `keyValueStorage`, which it must release first.
  pure def cleanUpAllowedBalance(pi: ParaInfo, para_id: ParaId): int = {
    val activeFp =
      match pi.validationCode {
        | None => 0
        | Some(vc) => preimageFootprint(vc.ref.len)
      }
    val pendingFp = match pi.pendingUpgrade {
      | None => 0
      | Some(pair) => preimageFootprint(pair._1.ref.len)
    }
    baselineFor(para_id) + activeFp + pendingFp
  }

   /// §6.4: deregister a parachain. Rejects with `TooMuchStateHeld` unless the
   /// para holds only its baseline and validation code(s) (it must release all
   /// other solicited preimages and `keyValueStorage` first). Otherwise forgets
   /// the validation code(s) via the two-step forget (§6.1). If any code cannot
   /// be expunged yet, sets `isDeregistering = true` and keeps the entry; the
   /// Coretime chain needs to retry once the logged `due` is reached. Once every code
   /// is expunged, drops all per-para state.
  pure def parachainCleanUp(
    s: ParachainServiceState,
    para_id: ParaId,
    now: Timeslot
  ): MgmtOutcome = {
    if (not(s.isLive(para_id))) {
      { state: s, jamSolicit: Set(), jamForget: Set(), logs: List() }
    } else {
      val pi = s.paraInfo(para_id)
      if (pi.usedStateBalance > cleanUpAllowedBalance(pi, para_id)) {
        // Still holds extra state: reject, no change.
        { state: s, jamSolicit: Set(), jamForget: Set(), logs: List(TooMuchStateHeld) }
      } else {
        // Forget the active code (if any), then the pending code, via two-step forget.
        val afterActive =
          match pi.validationCode {
            | None => { state: s, jamSolicit: None, jamForget: None, retained: false, log: None }
            | Some(vc) => s.removeReferencer(para_id, vchAsHash(vc.ref.hash), vc.ref.len, now)
          }
        val afterPending = match pi.pendingUpgrade {
          | None =>
            { state: afterActive.state, jamSolicit: None, jamForget: None, retained: false, log: None }
          | Some(pair) =>
            afterActive.state.removeReferencer(
              para_id, vchAsHash(pair._1.ref.hash), pair._1.ref.len, now)
        }
        val s1 = afterPending.state
        val jamForgets =
          optionToSet(afterActive.jamForget).union(optionToSet(afterPending.jamForget))

        val stillHeld = afterActive.retained or afterPending.retained
        if (stillHeld) {
          // Mark deregistering and keep the entry; retry completes it later.
          {
            state: { ...s1,
              parachains: s1.parachains.put(para_id, { ...s1.paraInfo(para_id), isDeregistering: true }) },
            jamSolicit: Set(),
            jamForget: jamForgets,
            // Both the active and the pending code can enter limbo with distinct
            // deadlines.
            logs: List().appendOption(afterActive.log).appendOption(afterPending.log)
          }
        } else {
          // Fully expunged: drop all per-para state.
          val stagedAfter =
            if (para_id == AssetHubParaId) List() else s1.stagedValidatorKeys
          {
            state: { ...s1,
              parachains: s1.parachains.mapRemove(para_id),
              parachainLog: s1.parachainLog.mapRemove(para_id),
              stagedValidatorKeys: stagedAfter,
              keyValueStorage: s1.keyValueStorage.mapFilterKeys(key => key._1 != para_id) },
            jamSolicit: Set(),
            jamForget: jamForgets,
            logs: List()
          }
        }
      }
    }
  }

}
