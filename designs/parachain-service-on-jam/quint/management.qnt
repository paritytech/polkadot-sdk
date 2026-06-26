// Coretime-chain-only host calls for parachain lifecycle (design §6).
//
// All four are idempotent and the Parachain Service performs no rights-
// checking of its own — the Coretime chain is the sole authority on which
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
    log: Option[AccumulateLog]
  }

  /// §6.1 — sole creator of `ParaInfo`. On an unused ParaId, creates with
  /// the baseline footprint pre-charged. On an existing ParaId, overwrites
  /// `totalStateBalance` iff `new_total ≥ used_state_balance`.
  pure def parachainSetStateBalance(
    s: ParachainServiceState,
    para_id: ParaId,
    newTotal: Balance
  ): MgmtOutcome = {
    if (not(s.isLive(para_id))) {
      match initParaInfo(newTotal) {
        | None =>
        // Rejected — new_total below baseline.
        {
          state: s,
          jamSolicit: Set(),
          jamForget: Set(),
          log: Some(StateBalanceUpdateRejected({
                attempted: newTotal,
                currentTotal: 0,
                currentUsed: BaselineFootprint
          }))
        }
        | Some(pi) =>
        {
          state: { ...s, parachains: s.parachains.put(para_id, pi) },
          jamSolicit: Set(),
          jamForget: Set(),
          log: None
        }
      }
    } else {
      val pi = s.paraInfo(para_id)
      if (newTotal < pi.usedStateBalance) {
        {
          state: s,
          jamSolicit: Set(),
          jamForget: Set(),
          log: Some(StateBalanceUpdateRejected({
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
          log: None
        }
      }
    }
  }

  /// §6.2 / §6.3 — upsert head data. No-op on a ParaId whose ParaInfo
  /// doesn't exist yet (Coretime must call set_state_balance first).
  pure def parachainSetHead(
    s: ParachainServiceState,
    para_id: ParaId,
    newHead: HeadData
  ): MgmtOutcome = {
    if (not(s.isLive(para_id))) {
      { state: s, jamSolicit: Set(), jamForget: Set(), log: None }
    } else {
      val pi = s.paraInfo(para_id)
      {
        state: { ...s, parachains: s.parachains.put(para_id, { ...pi, headData: newHead }) },
        jamSolicit: Set(),
        jamForget: Set(),
        log: None
      }
    }
  }

  /// §6.2 / §6.3 — upsert validation code (forced replacement; bypasses the
  /// normal upgrade lifecycle). Solicits the new code if not already
  /// solicited, releases the old active code and any pending-upgrade code —
  /// each only when the parachain isn't independently soliciting it
  /// (`pinned`; §5.2) and it differs from the new code — and clears
  /// the pending upgrade.
  pure def parachainSetValidationCode(
    s: ParachainServiceState,
    para_id: ParaId,
    newHash: ValidationCodeHash,
    codeLen: int,
    oldCodeLen: int
  ): MgmtOutcome = {
    if (not(s.isLive(para_id))) {
      { state: s, jamSolicit: Set(), jamForget: Set(), log: None }
    } else {
      val pi = s.paraInfo(para_id)
      val oldHash = pi.validationCode.hash
      val newEqualsPending =
        match pi.pendingUpgrade {
          | None => false
          | Some(pair) => pair._1.hash == newHash
        }
      // The parachain independently solicited newHash iff it references it
      // for a reason other than being the current active or pending code.
      val parachainHadIt =
        s.hasReferencer(vchAsHash(newHash), para_id) and
        newHash != oldHash and not(newEqualsPending)
      // Acquire the new referencer (no charge if already solicited); reject
      // if no headroom.
      val acquired = s.addReferencer(para_id, vchAsHash(newHash), codeLen)
      match acquired.log {
        | Some(_) =>
        { state: s, jamSolicit: Set(), jamForget: Set(), log: acquired.log }
        | None =>
        // Release the old active code unless it is the uninitialised
        // sentinel, equals the new code, or is independently solicited.
        val afterOld =
          if (oldHash == { vchBytes: 0 } or oldHash == newHash or pi.validationCode.pinned)
          { state: acquired.state, jamSolicit: false, jamForget: false, log: None }
          else
          acquired.state.removeReferencer(para_id, vchAsHash(oldHash), oldCodeLen)
        // Clear any pending upgrade, releasing its code unless it equals the
        // new code or is independently solicited.
        val afterPending =
          match pi.pendingUpgrade {
            | None =>
              { state: afterOld.state, jamSolicit: false, jamForget: false, log: None }
            | Some(pair) =>
              if (pair._1.hash == newHash or pair._1.pinned)
                { state: afterOld.state, jamSolicit: false, jamForget: false, log: None }
              else
                afterOld.state.removeReferencer(para_id, vchAsHash(pair._1.hash), oldCodeLen)
          }
        val pi2 = afterPending.state.paraInfo(para_id)
        {
          state: {
            ...afterPending.state,
            parachains: afterPending.state.parachains.put(
              para_id,
              { ...pi2,
                validationCode: {
                  hash: newHash,
                  pinned:
                    if (newHash == oldHash) pi.validationCode.pinned else parachainHadIt
                },
                pendingUpgrade: None
              }
            )
          },
          jamSolicit: if (acquired.jamSolicit) Set(vchAsHash(newHash)) else Set(),
          jamForget:
            (if (afterOld.jamForget) Set(vchAsHash(oldHash)) else Set())
              .union(match pi.pendingUpgrade {
                | None => Set()
                | Some(pair) => if (afterPending.jamForget) Set(vchAsHash(pair._1.hash)) else Set()
              }),
          log: None
        }
      }
    }
  }

  /// §6.4 — remove all per-parachain state. Releases every preimage the
  /// parachain references; JAM `forget` is invoked for each preimage that
  /// becomes referencerless. Also clears any service-level state that's
  /// logically Asset-Hub-only (notably `stagedValidatorKeys`) when Asset Hub
  /// (`AssetHubParaId`) itself is being cleaned up.
  pure def parachainCleanUp(
    s: ParachainServiceState,
    para_id: ParaId
  ): ParachainServiceState = {
    if (not(s.isLive(para_id))) {
      s
    } else {
      // Remove para_id from every referencer set, dropping entries that
      // go empty.
      val regAfter: Hash -> PreimageEntry =
      s.preimageRegistry.keys()
      .filter(h => {
          val e = s.preimageRegistry.get(h)
          e.referencers.exclude(Set(para_id)) != Set()
      })
      .mapBy(h => {
          val e = s.preimageRegistry.get(h)
          { ...e, referencers: e.referencers.exclude(Set(para_id)) }
      })
      // Drop any Asset-Hub-specific service-level state if Asset Hub
      // itself is being unregistered.
      val stagedAfter =
      if (para_id == AssetHubParaId) List() else s.stagedValidatorKeys
      {
        ...s,
        preimageRegistry: regAfter,
        parachains: s.parachains.mapRemove(para_id),
        parachainLog: s.parachainLog.mapRemove(para_id),
        stagedValidatorKeys: stagedAfter
      }
    }
  }
}
