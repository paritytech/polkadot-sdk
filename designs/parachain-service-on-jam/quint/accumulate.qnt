// Accumulate: on-chain integration (design §5.1).
//
// Per accumulated work-package, in order:
// 1. Parent-head check (§5.1 step 1).
// 2. Lazy reap of timed-out pending upgrade (§5.1 step 2).
// 3. Authoritative validation-code check (§5.1 step 3).
// 4. Head-data update + code-upgrade activation (§5.1 step 4).
// 5. Replay upward messages (§5.1 step 5).
//
// Plus the always-accumulate work per block (§5.1 "general work"):
// - Apply pending authorizer queues.
// - Append incoming transfers received this block.
module accumulate {
  import types.* from "./types"
  import messages.* from "./messages"
  import state.* from "./state"
  import state_balance.* from "./state_balance"
  import code_upgrades.* from "./code_upgrades"
  import validator_keys.* from "./validator_keys"
  import management.* from "./management"

  /// Per-work-package accumulate outcome.
  type AccumulateOutcome = {
    state: ParachainServiceState,
    // The vector to pass to JAM `designate`, if any.
    jamDesignate: Option[List[ValidatorKey]],
    // Hashes for which JAM `solicit` / `forget` should be invoked.
    jamSolicit: Set[Hash],
    jamForget: Set[Hash],
    // Log entries to append against the para's `parachain_log`.
    logs: List[AccumulateLog]
  }

  /// Result of accumulating ONE work package's `ParachainWorkResult` against
  /// the current state. Multi-package steps thread this forward across
  /// packages by calling `accumulateOnePackage` in sequence with the
  /// previous call's `state`, `solicited`, and `jamDesignate`.
  type PerPackageOutcome = {
    state: ParachainServiceState,
    solicited: Set[(ParaId, Hash)],
    jamDesignate: Option[List[ValidatorKey]]
  }

  pure val emptyOutcome: AccumulateOutcome = {
    state: emptyState,
    jamDesignate: None,
    jamSolicit: Set(),
    jamForget: Set(),
    logs: List()
  }

  // ---- Replay a single upward message -----------------------------------

  pure def applyUpwardMessage(
    acc: AccumulateOutcome,
    para_id: ParaId,
    codeLen: int,
    assetHub: ParaId,
    now: Timeslot,
    m: UpwardMessage
  ): AccumulateOutcome = {
    match m {
      | RequestCodeUpgrade(payload) =>
        val d = requestCodeUpgrade(acc.state, now, para_id, payload.hash, payload.len)
        {
          state: d.state,
          jamDesignate: acc.jamDesignate,
          jamSolicit: if (d.jamSolicit) acc.jamSolicit.union(Set(vchAsHash(payload.hash))) else acc.jamSolicit,
        jamForget: acc.jamForget,
        logs: match d.log {
          | Some(e) => acc.logs.append(e)
          | None => acc.logs
        }
      }
      | Solicit(payload) =>
      val pi = acc.state.paraInfo(para_id)
      val isActive = payload.hash == vchAsHash(pi.validationCode.hash)
      val isPending =
        match pi.pendingUpgrade {
          | None => false
          | Some(pair) => payload.hash == vchAsHash(pair._1.hash)
        }
      if (isActive) {
        // The active code is already solicited by the service, so soliciting
        // it costs no extra state balance — just record the parachain's own
        // solicit so the referencer survives the code ceasing to be active
        // (§5.2 `pinned`).
        { ...acc,
          state: { ...acc.state,
            parachains: acc.state.parachains.put(
              para_id,
              { ...pi, validationCode: { ...pi.validationCode, pinned: true } }) } }
      } else if (isPending) {
        { ...acc,
          state: { ...acc.state,
            parachains: acc.state.parachains.put(
              para_id,
              { ...pi, pendingUpgrade:
                match pi.pendingUpgrade {
                  | None => None
                  | Some(pair) => Some(({ ...pair._1, pinned: true }, pair._2))
                } }) } }
      } else {
        val d = acc.state.addReferencer(para_id, payload.hash, payload.len)
        {
          state: d.state,
          jamDesignate: acc.jamDesignate,
          jamSolicit: if (d.jamSolicit) acc.jamSolicit.union(Set(payload.hash)) else acc.jamSolicit,
          jamForget: acc.jamForget,
          logs: match d.log {
            | Some(e) => acc.logs.append(e)
            | None => acc.logs
          }
        }
      }
      | Forget(payload) =>
      val pi = acc.state.paraInfo(para_id)
      val isActive = payload.hash == vchAsHash(pi.validationCode.hash)
      val isPending =
        match pi.pendingUpgrade {
          | None => false
          | Some(pair) => payload.hash == vchAsHash(pair._1.hash)
        }
      if (isActive) {
        // The parachain forgets its own solicit of the active code: clear the
        // `pinned` bit, but the service still references the code (it
        // is active), so the referencer and balance stay and the JAM forget
        // is NOT forwarded (§5.2).
        { ...acc,
          state: { ...acc.state,
            parachains: acc.state.parachains.put(
              para_id,
              { ...pi, validationCode: { ...pi.validationCode, pinned: false } }) } }
      } else if (isPending) {
        { ...acc,
          state: { ...acc.state,
            parachains: acc.state.parachains.put(
              para_id,
              { ...pi, pendingUpgrade:
                match pi.pendingUpgrade {
                  | None => None
                  | Some(pair) => Some(({ ...pair._1, pinned: false }, pair._2))
                } }) } }
      } else {
        val d = acc.state.removeReferencer(para_id, payload.hash, payload.len)
        {
          state: d.state,
          jamDesignate: acc.jamDesignate,
          jamSolicit: acc.jamSolicit,
          jamForget: if (d.jamForget) acc.jamForget.union(Set(payload.hash)) else acc.jamForget,
          logs: acc.logs
        }
      }
      | SetValidatorKeys(payload) =>
      // Both Refine and `accumulateOkPrefix`'s service-restriction check
      // (§4.3) reject upward messages from non-Asset-Hub paras before we
      // reach this branch, so `para_id == assetHub` holds by construction.
      val o = applySetValidatorKeys(acc.state, assetHub, payload.keys, payload.isLast)
      {
        state: o.state,
        jamDesignate:
        if (o.jamDesignate) Some(o.designateVec) else acc.jamDesignate,
        jamSolicit: acc.jamSolicit,
        jamForget: acc.jamForget,
        logs: match o.log {
          | Some(e) => acc.logs.append(e)
          | None => acc.logs
        }
      }
      | ParachainSetStateBalance(payload) =>
      // §6.1 — only legal from Coretime chain (rights check is in §6).
      // Here we just apply the underlying primitive.
      val o = parachainSetStateBalance(acc.state, payload.paraId, payload.newTotal)
      {
        state: o.state,
        jamDesignate: acc.jamDesignate,
        jamSolicit: acc.jamSolicit,
        jamForget: acc.jamForget,
        logs: match o.log {
          | Some(e) => acc.logs.append(e)
          | None => acc.logs
        }
      }
      | ParachainSetHead(payload) =>
      val o = parachainSetHead(acc.state, payload.paraId, payload.newHead)
      { ...acc, state: o.state }
      | ParachainSetValidationCode(payload) =>
      val o = parachainSetValidationCode(
        acc.state, payload.paraId, payload.newValidationCodeHash, codeLen, codeLen
      )
      {
        state: o.state,
        jamDesignate: acc.jamDesignate,
        jamSolicit: acc.jamSolicit.union(o.jamSolicit),
        jamForget: acc.jamForget.union(o.jamForget),
        logs: match o.log {
          | Some(e) => acc.logs.append(e)
          | None => acc.logs
        }
      }
      | ParachainCleanUp(paraId) =>
      { ...acc, state: parachainCleanUp(acc.state, paraId, assetHub) }
      | TransferOut(_) =>
      // Asset-Hub-only outbound transfer. Modeled abstractly: the
      // top-level `step` action routes the transfer into the destination
      // service's incoming queue. Here we no-op.
      acc
      | SetAuthorizerQueue(payload) =>
      // §5.1 step 5: queue updates go through the authorizer cache; the
      // immediate flag and pending-authorizer flush are handled in the
      // always-accumulate path (see `applyAlwaysAccumulate`).
      { ...acc,
        state: { ...acc.state,
          pendingAuthorizerQueues:
          acc.state.pendingAuthorizerQueues.put(payload.core, payload.queue)
        }
      }
      | ConsumeTransfersUpTo(idx) =>
      // Asset Hub prunes the consumed prefix. Both Refine and
      // `accumulateOkPrefix`'s service-restriction check (§4.3) reject
      // non-Asset-Hub origins before we reach this branch.
      { ...acc,
        state: { ...acc.state,
          incomingTransfers:
          acc.state.incomingTransfers.slice(idx, acc.state.incomingTransfers.length())
        }
      }
      | UpgradeService(payload) =>
      // §5.4: Asset Hub-initiated service code upgrade. Accumulate verifies
      // the new code's preimage is in the service's preimage store before
      // forwarding to JAM `upgrade`; otherwise rejects with a log entry.
      val preimagePresent =
        acc.state.preimageRegistry.keys().contains(payload.codeHash) and
        acc.state.preimageRegistry.get(payload.codeHash).referencers != Set()
      if (preimagePresent)
        { ...acc, state: { ...acc.state, serviceCodeHash: payload.codeHash } }
      else
        { ...acc,
          logs: acc.logs.append(ServiceUpgradePreimageMissing({ codeHash: payload.codeHash }))
        }
    }
  }

  // ---- Per-work-package accumulation (§5.1) -----------------------------

  /// Implements §5.1 steps 1–5 for a single `ParachainWorkResult::Ok`.
  /// Step 5 (replay upward messages) is iterated by the caller.
  pure def accumulateOkPrefix(
    s: ParachainServiceState,
    r: ParachainWorkResultOk,
    codeLen: int,
    now: Timeslot,
    assetHub: ParaId,
    coretimeChain: ParaId
  ): {
    state: ParachainServiceState,
    rejected: bool,
    // Hashes for solicit/forget collected by the code-upgrade reap
    jamSolicit: Set[Hash],
    jamForget: Set[Hash],
    logs: List[AccumulateLog]
  } = {
    // Step 1: parent-head check.
    if (not(s.isLive(r.paraId))) {
      // Para not registered — silently drop.
      { state: s, rejected: true, jamSolicit: Set(), jamForget: Set(), logs: List() }
    } else {
      val pi0 = s.paraInfo(r.paraId)
      if (r.parentHeadHash != headHash(pi0.headData)) {
        { state: s, rejected: true, jamSolicit: Set(), jamForget: Set(), logs: List() }
      } else {
        // Step 2: lazy reap of timed-out pending upgrade.
        val reaped = reapTimedOutUpgrade(s, now, r.paraId, codeLen)
        val pi1 = reaped.state.paraInfo(r.paraId)
        // Step 3: authoritative validation-code check.
        val codeOk =
        r.validationCodeHash == pi1.validationCode.hash or
        (match pi1.pendingUpgrade {
            | None => false
            | Some(pair) => r.validationCodeHash == pair._1.hash
        })
        if (not(codeOk)) {
          {
            state: reaped.state,
            rejected: true,
            jamSolicit: Set(),
            jamForget: if (reaped.jamForget) Set({ hashBytes: 0 }) else Set(),
            logs: List(InvalidCodeHashAcc(r.validationCodeHash))
          }
        } else if (violatesParachainRestrictions(r.upwardMessages, r.paraId, assetHub, coretimeChain)) {
          // §4.3 defense-in-depth: even if Refine should already have
          // rejected this, Accumulate independently verifies and refuses
          // to apply a work-package whose upward messages include
          // restricted host calls from the wrong parachain.
          {
            state: reaped.state,
            rejected: true,
            jamSolicit: Set(),
            jamForget: if (reaped.jamForget) Set({ hashBytes: 0 }) else Set(),
            logs: List(InvalidCodeHashAcc(r.validationCodeHash))
          }
        } else {
          // Step 4a: head update.
          val pi2 = reaped.state.paraInfo(r.paraId)
          val sAfterHead = {
            ...reaped.state,
            parachains: reaped.state.parachains.put(r.paraId, { ...pi2, headData: r.headData })
          }
          // Step 4b: activate code upgrade if this candidate used the pending hash.
          val activated = activateUpgradeIfMatch(sAfterHead, r.paraId, r.validationCodeHash, codeLen)
          val solicits =
          if (reaped.jamSolicit) Set({ hashBytes: 0 }) else Set()
          val forgets = (
            if (reaped.jamForget) Set({ hashBytes: 0 }) else Set()
          ).union(
            if (activated.jamForget) Set({ hashBytes: 0 }) else Set()
          )
          {
            state: activated.state,
            rejected: false,
            jamSolicit: solicits,
            jamForget: forgets,
            logs: List()
          }
        }
      }
    }
  }

  /// Accumulate one work package against the current state, threading
  /// `solicited` (ghost) and `jamDesignate` (last designate so far) forward.
  /// Used by multi-package steps to apply work packages sequentially.
  pure def accumulateOnePackage(
    state: ParachainServiceState,
    result: ParachainWorkResult,
    solicited: Set[(ParaId, Hash)],
    prevDesignate: Option[List[ValidatorKey]],
    codeLen: int,
    now: Timeslot,
    assetHub: ParaId,
    coretimeChain: ParaId
  ): PerPackageOutcome = {
    match result {
      | Err(_) => {
        state: state,
        solicited: solicited,
        jamDesignate: prevDesignate
      }
      | Ok(ok) =>
        val pre = accumulateOkPrefix(state, ok, codeLen, now, assetHub, coretimeChain)
        if (pre.rejected) {
          state: pre.state,
          solicited: solicited,
          jamDesignate: prevDesignate
        } else {
          val baseAcc = {
            state: pre.state,
            jamDesignate: prevDesignate,
            jamSolicit: pre.jamSolicit,
            jamForget: pre.jamForget,
            logs: pre.logs
          }
          val acc = ok.upwardMessages.foldl(baseAcc, (a, m) =>
            applyUpwardMessage(a, ok.paraId, codeLen, assetHub, now, m))
          val newSolicited = ok.upwardMessages.foldl(solicited, (s, m) => match m {
            | Solicit(p) => s.union(Set((ok.paraId, p.hash)))
            | Forget(p) => s.exclude(Set((ok.paraId, p.hash)))
            | _ => s
          })
          {
            state: acc.state,
            solicited: newSolicited,
            jamDesignate: acc.jamDesignate
          }
        }
    }
  }

  // ---- Always-accumulate (§5.1, general work) ---------------------------

  /// Apply any pending authorizer queues whose current 80-slot queue has been
  /// exhausted (tracked via `lastAuthorizerAssignment`). Modeled simply:
  /// any core whose cached queue is non-empty is flushed and the cache
  /// cleared. The "queue exhausted" predicate is deliberately abstract here.
  pure def applyAlwaysAccumulate(
    s: ParachainServiceState,
    now: Timeslot
  ): ParachainServiceState = {
    s.pendingAuthorizerQueues.keys().fold(s, (acc, c) => {
        if (acc.pendingAuthorizerQueues.get(c) == List())
        acc
        else
        // In the real protocol this calls JAM `assign(core, queue)`; the
        // model just records the assignment timestamp and clears the cache.
        { ...acc,
          lastAuthorizerAssignment: acc.lastAuthorizerAssignment.put(c, now),
          pendingAuthorizerQueues: acc.pendingAuthorizerQueues.put(c, List())
        }
    })
  }
}
