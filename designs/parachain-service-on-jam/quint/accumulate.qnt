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
  /// the current state. Multi-package steps combine these via `accumulateAll`,
  /// which threads `state`, `solicited`, and `jamDesignate` across packages.
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

  /// Result of accumulating ALL of a block's work packages in sequence.
  type MultiPackageOutcome = {
    state: ParachainServiceState,
    solicited: Set[(ParaId, Hash, int)],
    jamDesignate: Option[List[ValidatorKey]],
    // Every package's `assign` calls, concatenated in package order.
    jamAssigns: List[AssignCall],
    // Each candidate-bearing package's `(para, lookup-anchor)` prune record, in
    // order; the caller folds these into the ghost `logPrunedBelow`.
    logPruned: List[(ParaId, Timeslot)]
  }

  /// Outcome of a whole block's Accumulate (`accumulateBlock`): the new state
  /// plus everything a caller needs to update its ghost/step variables.
  type BlockOutcome = {
    state: ParachainServiceState,
    // New `jamStagingSet` (unchanged unless a package designated).
    staging: List[ValidatorKey],
    solicited: Set[(ParaId, Hash, int)],
    // Always-accumulate `assign`s followed by the packages' `assign`s.
    assigns: List[AssignCall],
    // Each candidate-bearing package's `(para, lookup-anchor)` prune record.
    logPruned: List[(ParaId, Timeslot)],
    // The block's work results, in order (for `lastStepWorkResults`).
    results: List[WorkResult]
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
          jamSolicit: acc.jamSolicit.union(optionToSet(d.jamSolicit)),
          jamForget: acc.jamForget,
          jamAssigns: acc.jamAssigns,
          logs: acc.logs.appendOption(d.log)
      }
      | Solicit(payload) =>
      val pi = acc.state.paraInfo(para_id)
      val activeVc = pi.validationCode
      val isActive =
        match activeVc { | Some(vc) => refIs(vc.ref, payload.hash, payload.len) | None => false }
      val isPending =
        match pi.pendingUpgrade {
          | None => false
          | Some(pair) => refIs(pair._1.ref, payload.hash, payload.len)
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
              { ...pi, validationCode:
                match activeVc { | Some(vc) => Some({ ...vc, pinned: true }) | None => activeVc } }) } }
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
          jamSolicit: acc.jamSolicit.union(optionToSet(d.jamSolicit)),
          jamForget: acc.jamForget,
          jamAssigns: acc.jamAssigns,
          logs: acc.logs.appendOption(d.log)
        }
      }
      | Forget(payload) =>
      val target = payload.paraId
      if (not(acc.state.isLive(target))) acc
      else {
        val pi = acc.state.paraInfo(target)
        val activeVc = pi.validationCode
        val isActive =
          match activeVc { | Some(vc) => refIs(vc.ref, payload.hash, payload.len) | None => false }
        val isPending =
          match pi.pendingUpgrade {
            | None => false
            | Some(pair) => refIs(pair._1.ref, payload.hash, payload.len)
          }
        if (isActive) {
          // Forgetting the target's own solicit of its active code: clear the
          // `pinned` bit, but the service still references the code (it
          // is active), so the referencer and balance stay and the JAM forget
          // is NOT forwarded (§5.2). This is what stops a forced call from
          // stripping a running PVF.
          { ...acc,
            state: { ...acc.state,
              parachains: acc.state.parachains.put(
                target,
                { ...pi, validationCode:
                  match activeVc { | Some(vc) => Some({ ...vc, pinned: false }) | None => activeVc } }) } }
        } else if (isPending) {
          { ...acc,
            state: { ...acc.state,
              parachains: acc.state.parachains.put(
                target,
                { ...pi, pendingUpgrade:
                  match pi.pendingUpgrade {
                    | None => None
                    | Some(pair) => Some(({ ...pair._1, pinned: false }, pair._2))
                  } }) } }
        } else {
          val d = acc.state.removeReferencer(target, payload.hash, payload.len, now)
          {
            state: d.state,
            jamDesignate: acc.jamDesignate,
            jamSolicit: acc.jamSolicit,
            jamForget: acc.jamForget.union(optionToSet(d.jamForget)),
            jamAssigns: acc.jamAssigns,
            logs: acc.logs.appendOption(d.log)
          }
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
        logs: acc.logs.appendOption(o.log)
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
        logs: acc.logs.concat(o.logs)
      }
      | ParachainSetHead(payload) =>
      val o = parachainSetHead(acc.state, payload.paraId, payload.newHead)
      { ...acc, state: o.state }
      | ParachainSetValidationCode(payload) =>
      val o = parachainSetValidationCode(
        acc.state, payload.paraId, payload.newValidationCodeHash,
        payload.newValidationCodeLen, now
      )
      {
        state: o.state,
        jamDesignate: acc.jamDesignate,
        jamSolicit: acc.jamSolicit.union(o.jamSolicit),
        jamForget: acc.jamForget.union(o.jamForget),
        jamAssigns: acc.jamAssigns,
        logs: acc.logs.concat(o.logs)
      }
      | ParachainCleanUp(paraId) =>
      val o = parachainCleanUp(acc.state, paraId, now)
      {
        state: o.state,
        jamDesignate: acc.jamDesignate,
        jamSolicit: acc.jamSolicit,
        jamForget: acc.jamForget.union(o.jamForget),
        jamAssigns: acc.jamAssigns,
        logs: acc.logs.concat(o.logs)
      }
      | TransferOut(_) =>
      // Asset-Hub-only outbound transfer. Modeled abstractly: the
      // top-level `step` action routes the transfer into the destination
      // service's incoming queue. Here we no-op. If the JAM `transfer`
      // call fails (destination does not exist or refused the transfer),
      // an `AccumulateLog::TransferFailed({ memoHash })` entry is appended
      // to the parachain's log — not exercised in this abstract branch.
      acc
      | AssignCore(payload) =>
      // §5.1 step 7 (replay). Schedules JAM `assign` for `jam_slot`: the entry
      // (queue + assigner) is cached and `applyAlwaysAccumulate` emits the
      // `assign` once the timeslot reaches `jam_slot`. An empty `queue` cancels
      // any cached entry for the core (dropped, no JAM call). If `jam_slot` is
      // already due (`jam_slot <= now`, the same condition always-accumulate
      // flushes on), it is applied inline right away, superseding any cached
      // entry — always-accumulate ran before this replay, so caching would
      // otherwise defer it to the next block. `newAssigner == None` keeps this
      // service as the assigner; `Some(s)` hands the core to `s` (one-way).
      if (payload.queue.length() == 0)
        { ...acc,
          state: { ...acc.state,
            pendingAssigns: acc.state.pendingAssigns.mapRemove(payload.core),
            pendingAssignCores: acc.state.pendingAssignCores.mapRemove(payload.core)
          }
        }
      else if (payload.jamSlot <= now)
        { ...acc,
          jamAssigns: acc.jamAssigns.append({
            core: payload.core,
            assigner: payload.newAssigner.optionGetOr(ParachainServiceId),
            queue: payload.queue
          }),
          state: { ...acc.state,
            pendingAssigns: acc.state.pendingAssigns.mapRemove(payload.core),
            pendingAssignCores: acc.state.pendingAssignCores.mapRemove(payload.core)
          }
        }
      else
        { ...acc,
          state: { ...acc.state,
            pendingAssigns: acc.state.pendingAssigns.put(payload.core, {
              queue: payload.queue,
              assigner: payload.newAssigner
            }),
            pendingAssignCores:
              acc.state.pendingAssignCores.put(payload.core, payload.jamSlot)
          }
        }
      | ConsumeTransfersUpTo(slot) =>
      // Asset Hub drains whole buckets up to and including `slot`. Both Refine
      // and `accumulateOkPrefix`'s service-restriction check (§4.3) reject
      // non-Asset-Hub origins before we reach this branch.
      { ...acc, state: dropBucketsUpTo(acc.state, slot) }
      | UpgradeService(payload) =>
      // §5.4: Asset Hub-initiated service code upgrade. Accumulate verifies
      // the new code's preimage is in the service's preimage store before
      // forwarding to JAM `upgrade`; otherwise rejects with a log entry.
      // §5.4: the new code must be AVAILABLE for lookup, not merely solicited.
      // JAM's `upgrade` performs no such check, so switching to a hash whose blob
      // was never supplied would leave the service with no code to run.
      val preimagePresent =
        acc.state.preimageAvailable(payload.codeHash, payload.len)
      if (preimagePresent)
        { ...acc, state: { ...acc.state, serviceCodeHash: payload.codeHash } }
      else
        { ...acc,
          logs: acc.logs.append(ServiceUpgradePreimageMissing({ codeHash: payload.codeHash }))
        }
      | SetKV(payload) =>
      val d = applySetKV(acc.state, para_id, payload.key, payload.value)
      {
        ...acc,
        state: d.state,
        logs: acc.logs.appendOption(d.log)
      }
      | RemoveKV(payload) =>
      if (not(acc.state.isLive(payload.paraId))) acc
      else {
        val d = applyRemoveKV(acc.state, payload.paraId, payload.key)
        {
          ...acc,
          state: d.state,
          logs: acc.logs.appendOption(d.log)
        }
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
    // Step 1: registration check. A not-registered OR deregistering para
    // (§6.4) is treated as if it no longer exists — silently drop, no log.
    if (not(s.isLive(r.paraId)) or s.paraInfo(r.paraId).isDeregistering) {
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
        val matchesActive =
          match pi1.validationCode { | Some(vc) => r.validationCode == vc.ref | None => false }
        val codeOk =
        matchesActive or
        (match pi1.pendingUpgrade {
            | None => false
            | Some(pair) => r.validationCode == pair._1.ref
        })
        if (not(codeOk)) {
          {
            state: reaped.state,
            rejected: true,
            jamSolicit: Set(),
            jamForget: optionToSet(reaped.jamForget),
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
            jamForget: optionToSet(reaped.jamForget),
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
          val activated = activateUpgradeIfMatch(sAfterHead, r.paraId, r.validationCode.hash, codeLen, now)
           
          {
            state: activated.state,
            rejected: false,
            jamSolicit: optionToSet(reaped.jamSolicit),
            jamForget: optionToSet(reaped.jamForget).union(optionToSet(activated.jamForget)),
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
    now: Timeslot
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
            // unregistered para (§5.1 step 1). No pruning: a Refine failure is
            // not a candidate, so it carries no lookup-anchor and does not prune
            {
              state: state
                .appendRefineLog(e.paraId, now, e.error, truncateAuthTrace(wr.authTrace)),
              solicited: solicited,
              jamDesignate: prevDesignate,
              jamAssigns: List(),
              logPruned: None
            }
          | Ok(ok) =>
            val pre = accumulateOkPrefix(state, ok, codeLen, now)
            if (pre.rejected) {
              // §5.1: a candidate rejected at the parent-head or validation-code
              // check may still record an accumulate event (e.g. `InvalidCodeHashAcc`).
              // Prune below the lookup-anchor before persisting it.
              {
                state: pruneLogBelow(pre.state, ok.paraId, ok.lookupAnchor)
                  .appendAccumulateLogs(ok.paraId, now, pre.logs),
                solicited: solicited,
                jamDesignate: prevDesignate,
                jamAssigns: List(),
                logPruned: Some((ok.paraId, ok.lookupAnchor))
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
              // `Solicit` is always for the caller itself; `Forget` names its
              // target, so a Coretime-driven forget clears the TARGET's entry.
              val newSolicited = ok.upwardMessages.foldl(solicited, (s, m) => match m {
                | Solicit(p) => s.union(Set((ok.paraId, p.hash, p.len)))
                | Forget(p) => s.exclude(Set((p.paraId, p.hash, p.len)))
                | _ => s
              })
              // §5.1: prune below the candidate's lookup-anchor before
              // appending this candidate's collected accumulate events.
              {
                state: pruneLogBelow(acc.state, ok.paraId, ok.lookupAnchor)
                  .appendAccumulateLogs(ok.paraId, now, acc.logs),
                solicited: newSolicited,
                jamDesignate: acc.jamDesignate,
                jamAssigns: acc.jamAssigns,
                logPruned: Some((ok.paraId, ok.lookupAnchor))
              }
            }
        }
    }
  }

  /// Accumulate a block's work packages in order (§5.1). 
  pure def accumulateAll(
    state: ParachainServiceState,
    solicited: Set[(ParaId, Hash, int)],
    prevDesignate: Option[List[ValidatorKey]],
    codeLen: int,
    now: Timeslot,
    packages: List[WorkResult]
  ): MultiPackageOutcome = {
    packages.foldl(
      { state: state,
        solicited: solicited,
        jamDesignate: prevDesignate,
        jamAssigns: List(),
        logPruned: List() },
      (acc, p) => {
        val out = accumulateOnePackage(
          acc.state, p, acc.solicited, acc.jamDesignate, codeLen, now)
        { state: out.state,
          solicited: out.solicited,
          jamDesignate: out.jamDesignate,
          jamAssigns: acc.jamAssigns.concat(out.jamAssigns),
          logPruned: acc.logPruned.appendOption(out.logPruned)
        }
      })
  }

  // ---- Always-accumulate (§5.1, general work) ---------------------------

  /// Apply pending `assign`s that are due. Gating reads ONLY the dirty-core
  /// index (`pendingAssignCores`): the much larger payload is read just for a
  /// core that is actually due, then both entries are dropped. Not-yet-due
  /// cores are retained unchanged. A cached `assigner` of `None` resolves to
  /// this service (the assigner is unchanged); JAM's `assign` always writes one.
  pure def applyAlwaysAccumulate(
    s: ParachainServiceState,
    now: Timeslot
  ): { state: ParachainServiceState, assigns: List[AssignCall] } = {
    s.pendingAssignCores.keys().fold(
      { state: { ...s, pendingAssignCores: Map() }, assigns: List() },
      (acc, c) => {
        val jamSlot = s.pendingAssignCores.get(c)
        if (now < jamSlot)
          { ...acc,
            state: { ...acc.state,
              pendingAssignCores: acc.state.pendingAssignCores.put(c, jamSlot)
            }
          }
        else {
          val entry = s.pendingAssigns.get(c)
          { state: { ...acc.state,
              pendingAssigns: acc.state.pendingAssigns.mapRemove(c)
            },
            assigns: acc.assigns.append({
              core: c,
              assigner: entry.assigner.optionGetOr(ParachainServiceId),
              queue: entry.queue
            })
          }
        }
      }
    )
  }

  /// §5.1 incoming-transfer processing.
  pure def applyIncomingTransfer(
    s: ParachainServiceState,
    now: Timeslot,
    t: { from: ServiceId, amount: Balance, memo: Memo }
  ): ParachainServiceState = {
    val admitted =
      queuedTransferCount(s) < MaxIncomingTransfers or
        t.amount >= IncomingTransferEntryFootprint
    if (not(admitted)) s
    else
      match s.incomingTransferChain {
        // Empty queue: `now` becomes the only bucket, so both endpoints.
        | None =>
          { ...s,
            incomingTransfers:
              s.incomingTransfers.put(now, { transfers: List(t), nextSlot: None }),
            incomingTransferChain: Some({ firstSlot: now, lastSlot: now }) }
        | Some(chain) =>
          if (chain.lastSlot == now) {
            // Same slot as the tail: append in place, no new storage item.
            val bucket = s.incomingTransfers.get(now)
            { ...s,
              incomingTransfers: s.incomingTransfers.put(
                now, { ...bucket, transfers: bucket.transfers.append(t) }) }
          } else {
            // New bucket at `now`; link the old tail to it and move `lastSlot`.
            val oldTail = s.incomingTransfers.get(chain.lastSlot)
            { ...s,
              incomingTransfers: s.incomingTransfers
                .put(chain.lastSlot, { ...oldTail, nextSlot: Some(now) })
                .put(now, { transfers: List(t), nextSlot: None }),
              incomingTransferChain: Some({ ...chain, lastSlot: now }) }
          }
      }
  }

  /// The full per-block Accumulate pipeline (§5.1): first run always-accumulate
  /// (flush due authorizer queues), then accumulate the block's work packages.
  pure def accumulateBlock(
    state: ParachainServiceState,
    now: Timeslot,
    staging: List[ValidatorKey],
    solicited: Set[(ParaId, Hash, int)],
    codeLen: int,
    packages: List[WorkResult]
  ): BlockOutcome = {
    val flushed = applyAlwaysAccumulate(state, now)
    val acc = accumulateAll(flushed.state, solicited, None, codeLen, now, packages)
    {
      state: acc.state,
      staging: match acc.jamDesignate {
        | Some(v) => v
        | None => staging
      },
      solicited: acc.solicited,
      assigns: flushed.assigns.concat(acc.jamAssigns),
      logPruned: acc.logPruned,
      results: packages
    }
  }
}
