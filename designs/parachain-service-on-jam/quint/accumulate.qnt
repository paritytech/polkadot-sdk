// Accumulate: on-chain integration (design §5.1).
//
// All always-accumulate work runs first (before work packages): apply due
// authorizer queues, then incoming-transfer processing. Then per accumulated
// work-package, in order (§5.1):
// 1. Registration check.
// 2. Refine-result dispatch (log a failed digest; else continue).
// 3. Parent-head check.
// 4. Reap timed-out pending upgrade.
// 5. Authoritative validation-code check.
// 6. Head-data update + code-upgrade activation.
// 7. Replay upward messages.
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
    // JAM `assign` host-function calls made while replaying upward messages.
    jamAssigns: List[AssignCall],
    // Log entries to append against the para's `parachain_log`.
    logs: List[AccumulateLog]
  }

  /// Result of accumulating ONE work package's `ParachainWorkDigest` against
  /// the current state. Multi-package steps thread this forward across
  /// packages by calling `accumulateOnePackage` in sequence with the
  /// previous call's `state`, `solicited`, and `jamDesignate`.
  type PerPackageOutcome = {
    state: ParachainServiceState,
    solicited: Set[(ParaId, Hash, int)],
    jamDesignate: Option[List[ValidatorKey]],
    jamAssigns: List[AssignCall],
    // The `(para, lookup-anchor)` this package pruned its log against, if it
    // processed a candidate (`None` for a JAM-substituted `WorkErr`). The
    // caller folds it into the ghost `logPrunedBelow`.
    logPruned: Option[(ParaId, Timeslot)]
  }

  // ---- Replay a single upward message -----------------------------------

  pure def applyUpwardMessage(
    acc: AccumulateOutcome,
    para_id: ParaId,
    codeLen: int,
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
          jamAssigns: acc.jamAssigns,
          logs: match d.log {
            | Some(e) => acc.logs.append(e)
            | None => acc.logs
          }
      }
      | Solicit(payload) =>
      val pi = acc.state.paraInfo(para_id)
      val isActive = payload.hash == vchAsHash(pi.validationCode.ref.hash)
      val isPending =
        match pi.pendingUpgrade {
          | None => false
          | Some(pair) => payload.hash == vchAsHash(pair._1.ref.hash)
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
          jamAssigns: acc.jamAssigns,
          logs: match d.log {
            | Some(e) => acc.logs.append(e)
            | None => acc.logs
          }
        }
      }
      | Forget(payload) =>
      val pi = acc.state.paraInfo(para_id)
      val isActive = payload.hash == vchAsHash(pi.validationCode.ref.hash)
      val isPending =
        match pi.pendingUpgrade {
          | None => false
          | Some(pair) => payload.hash == vchAsHash(pair._1.ref.hash)
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
          jamAssigns: acc.jamAssigns,
          logs: acc.logs
        }
      }
      | SetValidatorKeys(payload) =>
      // Both Refine and `accumulateOkPrefix`'s service-restriction check
      // (§4.3) reject upward messages from non-Asset-Hub paras before we
      // reach this branch, so `para_id == AssetHubParaId` holds by construction.
      val o = applySetValidatorKeys(acc.state, payload.keys, payload.isLast)
      {
        state: o.state,
        jamDesignate:
        if (o.jamDesignate) Some(o.designateVec) else acc.jamDesignate,
        jamSolicit: acc.jamSolicit,
        jamForget: acc.jamForget,
        jamAssigns: acc.jamAssigns,
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
        jamAssigns: acc.jamAssigns,
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
        acc.state, payload.paraId, payload.newValidationCodeHash,
        payload.newValidationCodeLen
      )
      {
        state: o.state,
        jamDesignate: acc.jamDesignate,
        jamSolicit: acc.jamSolicit.union(o.jamSolicit),
        jamForget: acc.jamForget.union(o.jamForget),
        jamAssigns: acc.jamAssigns,
        logs: match o.log {
          | Some(e) => acc.logs.append(e)
          | None => acc.logs
        }
      }
      | ParachainCleanUp(paraId) =>
      { ...acc, state: parachainCleanUp(acc.state, paraId) }
      | TransferOut(_) =>
      // Asset-Hub-only outbound transfer. Modeled abstractly: the
      // top-level `step` action routes the transfer into the destination
      // service's incoming queue. Here we no-op.
      acc
      | SetAuthorizerQueue(payload) =>
      // §5.1 step 7 (replay). Schedules the authorizer QUEUE for `jam_slot`: the
      // queue is cached and `applyAlwaysAccumulate` emits the `assign` once the
      // timeslot reaches `jam_slot`. An empty `queue` cancels any cached queue for
      // the core (dropped, not applied). If `jam_slot` is already due
      // (`jam_slot <= now`, the same condition always-accumulate flushes on), the
      // queue is applied inline right away (assign now, superseding any cached
      // queue) — always-accumulate ran before this replay, so caching would
      // otherwise defer it to the next block. The Coretime chain chooses
      // `jam_slot` so the pool fills without overwriting the currently active
      // authorizers.
      if (payload.queue.length() == 0)
        { ...acc,
          state: { ...acc.state,
            pendingAuthorizerQueues:
              acc.state.pendingAuthorizerQueues.mapRemove(payload.core),
            pendingAuthorizerCores:
              acc.state.pendingAuthorizerCores.mapRemove(payload.core)
          }
        }
      else if (payload.jamSlot <= now)
        { ...acc,
          jamAssigns: acc.jamAssigns.append(QueueSet({ core: payload.core })),
          state: { ...acc.state,
            pendingAuthorizerQueues:
              acc.state.pendingAuthorizerQueues.mapRemove(payload.core),
            pendingAuthorizerCores:
              acc.state.pendingAuthorizerCores.mapRemove(payload.core)
          }
        }
      else
        { ...acc,
          state: { ...acc.state,
            pendingAuthorizerQueues:
              acc.state.pendingAuthorizerQueues.put(payload.core, payload.queue),
            pendingAuthorizerCores:
              acc.state.pendingAuthorizerCores.put(payload.core, payload.jamSlot)
          }
        }
      | SetAssigner(payload) =>
      // Change the core's assigner (§7.1) — applied directly via JAM `assign`,
      // independent of the queue. `Some(s)` sets it to `s`; `None` resets it.
      { ...acc,
        jamAssigns: acc.jamAssigns.append(AssignerSet({ core: payload.core, newAssigner: payload.newAssigner }))
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
        acc.state.preimageRegistry.keys().contains((payload.codeHash, payload.len)) and
        acc.state.preimageRegistry.get((payload.codeHash, payload.len)).referencers != Set()
      if (preimagePresent)
        { ...acc, state: { ...acc.state, serviceCodeHash: payload.codeHash } }
      else
        { ...acc,
          logs: acc.logs.append(ServiceUpgradePreimageMissing({ codeHash: payload.codeHash }))
        }
    }
  }

  // ---- Per-work-package accumulation (§5.1) -----------------------------

  /// Implements §5.1 steps 3–7 for a single `ParachainWorkDigest::Ok`.
  /// Step 7 (replay upward messages) is iterated by the caller.
  pure def accumulateOkPrefix(
    s: ParachainServiceState,
    r: ParachainWorkDigestOk,
    codeLen: int,
    now: Timeslot
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
        r.validationCode == pi1.validationCode.ref or
        (match pi1.pendingUpgrade {
            | None => false
            | Some(pair) => r.validationCode == pair._1.ref
        })
        if (not(codeOk)) {
          {
            state: reaped.state,
            rejected: true,
            jamSolicit: Set(),
            jamForget: if (reaped.jamForget) Set({ hashBytes: 0 }) else Set(),
            logs: List(InvalidCodeHashAcc(r.validationCode.hash))
          }
        } else if (violatesParachainRestrictions(r.upwardMessages, r.paraId)) {
          // §4.3 defense-in-depth: even if Refine should already have
          // rejected this, Accumulate independently verifies and refuses
          // to apply a work-package whose upward messages include
          // restricted host calls from the wrong parachain.
          {
            state: reaped.state,
            rejected: true,
            jamSolicit: Set(),
            jamForget: if (reaped.jamForget) Set({ hashBytes: 0 }) else Set(),
            logs: List(InvalidCodeHashAcc(r.validationCode.hash))
          }
        } else {
          // Step 4a: head update.
          val pi2 = reaped.state.paraInfo(r.paraId)
          val sAfterHead = {
            ...reaped.state,
            parachains: reaped.state.parachains.put(r.paraId, { ...pi2, headData: r.headData })
          }
          // Step 4b: activate code upgrade if this candidate used the pending hash.
          val activated = activateUpgradeIfMatch(sAfterHead, r.paraId, r.validationCode.hash, codeLen)
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
    wr: WorkResult,
    solicited: Set[(ParaId, Hash, int)],
    prevDesignate: Option[List[ValidatorKey]],
    codeLen: int,
    now: Timeslot,
    lookupAnchor: Timeslot
  ): PerPackageOutcome = {
    match wr.result {
      // §3.3: gray paper `WorkExecResult::Error` — JAM substituted an
      // error for this work-item before the service's refine ran.
      // Skip entirely: no log entry, no state change, no prune (no candidate).
      | WorkErr => {
          state: state,
          solicited: solicited,
          jamDesignate: prevDesignate,
          jamAssigns: List(),
          logPruned: None
        }
      | WorkOk(digest) =>
        match digest {
          | Err(e) =>
            // §3.3: a failed Refine digest is logged with the work-report's
            // authorizer trace, truncated to `StoredAuthTraceCap` before
            // storing to keep the log bounded (the Gray Paper only caps the
            // 48 KiB combined budget). No-op inside `appendRefineLog` for an
            // unregistered para (§5.1 step 1). §5.1: prune the para's log
            // below the candidate's lookup-anchor before appending this entry.
            {
              state: pruneLogBelow(state, e.paraId, lookupAnchor)
                .appendRefineLog(e.paraId, now, e.error, truncateAuthTrace(wr.authTrace)),
              solicited: solicited,
              jamDesignate: prevDesignate,
              jamAssigns: List(),
              logPruned: Some((e.paraId, lookupAnchor))
            }
          | Ok(ok) =>
            val pre = accumulateOkPrefix(state, ok, codeLen, now)
            if (pre.rejected) {
              // §5.1: a candidate rejected at the parent-head or validation-code
              // check may still record an accumulate event (e.g. `InvalidCodeHash`).
              // Prune below the lookup-anchor before persisting it.
              {
                state: pruneLogBelow(pre.state, ok.paraId, lookupAnchor)
                  .appendAccumulateLogs(ok.paraId, now, pre.logs),
                solicited: solicited,
                jamDesignate: prevDesignate,
                jamAssigns: List(),
                logPruned: Some((ok.paraId, lookupAnchor))
              }
            } else {
              val baseAcc = {
                state: pre.state,
                jamDesignate: prevDesignate,
                jamSolicit: pre.jamSolicit,
                jamForget: pre.jamForget,
                jamAssigns: List(),
                logs: pre.logs
              }
              val acc = ok.upwardMessages.foldl(baseAcc, (a, m) =>
                applyUpwardMessage(a, ok.paraId, codeLen, now, m))
              val newSolicited = ok.upwardMessages.foldl(solicited, (s, m) => match m {
                | Solicit(p) => s.union(Set((ok.paraId, p.hash, p.len)))
                | Forget(p) => s.exclude(Set((ok.paraId, p.hash, p.len)))
                | _ => s
              })
              // §5.1: prune below the candidate's lookup-anchor before
              // appending this candidate's collected accumulate events.
              {
                state: pruneLogBelow(acc.state, ok.paraId, lookupAnchor)
                  .appendAccumulateLogs(ok.paraId, now, acc.logs),
                solicited: newSolicited,
                jamDesignate: acc.jamDesignate,
                jamAssigns: acc.jamAssigns,
                logPruned: Some((ok.paraId, lookupAnchor))
              }
            }
        }
    }
  }

  // ---- Always-accumulate (§5.1, general work) ---------------------------

  /// Apply pending authorizer queues that are due: for each core whose scheduled
  /// `jam_slot` has arrived (`now >= jam_slot`), emit the `assign` and drop the
  /// cached queue. Not-yet-due entries are retained unchanged.
  pure def applyAlwaysAccumulate(
    s: ParachainServiceState,
    now: Timeslot
  ): { state: ParachainServiceState, assigns: List[AssignCall] } = {
    s.pendingAuthorizerCores.keys().fold(
      { state: { ...s, pendingAuthorizerCores: Map() }, assigns: List() },
      (acc, c) => {
        val jamSlot = s.pendingAuthorizerCores.get(c)
        if (now < jamSlot)
          { ...acc,
            state: { ...acc.state,
              pendingAuthorizerCores: acc.state.pendingAuthorizerCores.put(c, jamSlot)
            }
          }
        else
          { state: { ...acc.state,
              pendingAuthorizerQueues: acc.state.pendingAuthorizerQueues.mapRemove(c)
            },
            assigns: acc.assigns.append(QueueSet({ core: c }))
          }
      }
    )
  }
}
