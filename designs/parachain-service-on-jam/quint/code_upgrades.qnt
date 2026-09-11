// PVF code-upgrade lifecycle (design §5.2).
//
// Phase 1 Request: PVF emits `RequestCodeUpgrade(new_hash)`.
// Phase 2 Solicit + pendingUpgrade tracking (this module).
// Phase 3 Preimage submitted out-of-band by anyone.
// Phase 4 Dual-code window: Refine accepts either active or pending hash.
// Phase 5 Activation on first block validated with `new_hash`, OR rejection
// if the deadline passes (lazy reap on next accumulate; see §5.1
// step 2).
module code_upgrades {
  import types.* from "./types"
  import messages.* from "./messages"
  import state.* from "./state"
  import state_balance.* from "./state_balance"

  /// Release a code's referencer slot for `para_id`, UNLESS the parachain is
  /// independently soliciting it (`pinned`). The service itself solicits
  /// active/pending code on the parachain's behalf, so the referencer is held
  /// for two possible reasons: "it is the active/pending code" and "the
  /// parachain solicited it" (`pinned`). When the first reason ends
  /// (activation, reap, supersede), the slot is dropped only if the second
  /// reason is also absent. See §5.2.
  pure def releaseCodeIfNotPinned(
    s: ParachainServiceState,
    para_id: ParaId,
    ref: ValidationCodeRef,
    pinned: bool,
    now: Timeslot
  ): PreimageDelta =
    if (pinned)
      { state: s, jamSolicit: None, jamForget: None, retained: false, log: None }
    else
      s.removeReferencer(para_id, vchAsHash(ref.hash), ref.len, now)

  /// Phase 2: solicit the new code (if not already solicited) and arm
  /// `pendingUpgrade` with a deadline.
  ///
  /// Invoked from the Parachain Service's `accumulate` step when replaying
  /// a `RequestCodeUpgrade` upward message produced by the PVF during the
  /// preceding Refine, i.e. this runs on-chain, with full access to
  /// service storage, NOT inside Refine itself.
  ///
  /// The service solicits the new code only if it isn't already solicited; if
  /// the parachain had solicited it itself, no extra state balance is charged
  /// and the pending code's `pinned` bit is set so the slot survives a later
  /// supersede/reap. Overwriting a different in-flight upgrade is allowed:
  /// the superseded code is released unless the parachain solicited it.
  /// Requesting the already-active code is a no-op.
  pure def requestCodeUpgrade(
    s: ParachainServiceState,
    now: Timeslot,
    para_id: ParaId,
    newHash: ValidationCodeHash,
    // The new code's length, supplied by the PVF as part of the
    // `RequestCodeUpgrade { hash, len }` upward message (§3.3, §4.3).
    codeLen: int
  ): PreimageDelta = {
    val pi = s.paraInfo(para_id)
    val deadline = now + UpgradeTimeoutTimeslots
    val isSamePending =
      match pi.pendingUpgrade {
        | None => false
        | Some(pair) => pair._1.ref.hash == newHash
      }
    val activeHash = pi.validationCode.optionMap(vc => vc.ref.hash)
    if (activeHash == Some(newHash)) {
      // Can't "upgrade" to the code already running: no-op.
      { state: s, jamSolicit: None, jamForget: None, retained: false, log: None }
    } else if (isSamePending) {
      // Re-request of the same pending code: refresh the deadline only,
      // preserving the pending code's pinned bit.
      val keep =
        match pi.pendingUpgrade {
          | None => { ref: { hash: newHash, len: codeLen }, pinned: false }
          | Some(pair) => pair._1
        }
      {
        state: {
          ...s,
          parachains: s.parachains.put(
            para_id, { ...pi, pendingUpgrade: Some((keep, deadline)) }
          )
        },
        jamSolicit: None, jamForget: None, retained: false, log: None
      }
    } else {
      // The parachain independently solicited this code already iff it is a
      // referencer (it is neither the active nor the current pending code
      // here). In that case the service charges no extra balance.
      val parachainHadIt = s.hasReferencer(vchAsHash(newHash), codeLen, para_id)
      val acquired = s.addReferencer(para_id, vchAsHash(newHash), codeLen)
      match acquired.log {
        | Some(_) =>
          acquired  // insufficient balance; pending untouched
        | None =>
          // Supersede a different pending code: release it unless the
          // parachain is independently soliciting it.
          val released =
            match pi.pendingUpgrade {
              | None =>
                { state: acquired.state, jamSolicit: None, jamForget: None, retained: false, log: None }
              | Some(pair) =>
                releaseCodeIfNotPinned(
                  acquired.state, para_id, pair._1.ref, pair._1.pinned, now
                )
            }
          val pi2 = released.state.paraInfo(para_id)
          {
            state: {
              ...released.state,
              parachains: released.state.parachains.put(
                para_id,
                { ...pi2,
                  pendingUpgrade:
                    Some((
                      { ref: { hash: newHash, len: codeLen }, pinned: parachainHadIt },
                      deadline
                    ))
                }
              )
            },
            jamSolicit: acquired.jamSolicit,
            jamForget: released.jamForget,
            retained: released.retained,
            log: None
          }
      }
    }
  }

  /// Phase 5(b): lazy reap on the next per-work-package accumulate
  /// (§5.1, step 2). If the deadline has passed, release the pending code
  /// (unless the parachain solicited it) and clear `pendingUpgrade`.
  pure def reapTimedOutUpgrade(
    s: ParachainServiceState,
    now: Timeslot,
    para_id: ParaId,
    codeLen: int
  ): PreimageDelta = {
    val pi = s.paraInfo(para_id)
    match pi.pendingUpgrade {
      | None => { state: s, jamSolicit: None, jamForget: None, retained: false, log: None }
      | Some(pair) =>
      val deadline = pair._2
      if (deadline > now) {
        { state: s, jamSolicit: None, jamForget: None, retained: false, log: None }
      } else {
        val released =
          releaseCodeIfNotPinned(s, para_id, pair._1.ref, pair._1.pinned, now)
        val pi2 = released.state.paraInfo(para_id)
        {
          ...released,
          state: {
            ...released.state,
            parachains: released.state.parachains.put(
              para_id,
              { ...pi2, pendingUpgrade: None }
            )
          }
        }
      }
    }
  }

  /// Phase 5(a): activate the new code when this candidate was validated
  /// with the pending hash (§5.1, step 4). The old active code's referencer
  /// is released unless the parachain solicited it; the pending code's
  /// `pinned` bit carries over to the now-active code.
  pure def activateUpgradeIfMatch(
    s: ParachainServiceState,
    para_id: ParaId,
    candidateCodeHash: ValidationCodeHash,
    codeLen: int,
    now: Timeslot
  ): PreimageDelta = {
    val pi = s.paraInfo(para_id)
    match pi.pendingUpgrade {
      | None => { state: s, jamSolicit: None, jamForget: None, retained: false, log: None }
      | Some(pair) =>
      if (candidateCodeHash != pair._1.ref.hash) {
        // Validated with old code; pending stays armed.
        { state: s, jamSolicit: None, jamForget: None, retained: false, log: None }
      } else {
        val released =
          match pi.validationCode {
            | Some(vc) => releaseCodeIfNotPinned(s, para_id, vc.ref, vc.pinned, now)
            | None => { state: s, jamSolicit: None, jamForget: None, retained: false, log: None }
          }
        val pi2 = released.state.paraInfo(para_id)
        {
          ...released,
          state: {
            ...released.state,
            parachains: released.state.parachains.put(
              para_id,
              { ...pi2,
                // The pending code becomes active, carrying its extra bit.
                validationCode: Some({ ref: pair._1.ref, pinned: pair._1.pinned }),
                pendingUpgrade: None
              }
            )
          }
        }
      }
    }
  }
}
