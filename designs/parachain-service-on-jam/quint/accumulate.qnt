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
  import head_commitment.* from "./head_commitment"
  import foreign_services.* from "./foreign_services"

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
    logs: List[AccumulateLog],
    // Supervised JAM services, which the §6.5 operations mutate.
    foreign: ForeignServices
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
    logPruned: Option[(ParaId, Timeslot)],
    foreign: ForeignServices
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
    logPruned: List[(ParaId, Timeslot)],
    foreign: ForeignServices
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
    results: List[WorkResult],
    // The 32-byte value this accumulate invocation returns: the head
    // commitment (§5.5). `None` when no head changed and nothing is returned.
    headRoot: Option[MerkleHash],
    foreign: ForeignServices
  }

  // ---- Replay a single upward message -----------------------------------

  pure def applyUpwardMessage(
    acc: AccumulateOutcome,
    para_id: ParaId,
    codeLen: int,
    now: Timeslot,
    lookupAnchor: Timeslot,
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
          logs: acc.logs.appendOption(d.log),
          foreign: acc.foreign
      }
      | Solicit(payload) =>
      match payload.target {
      | Service(s) =>
      val o = foreignSolicit(acc.foreign, s, payload.hash, payload.len, now)
      { ...acc, foreign: o.services, logs: acc.logs.appendOption(o.log) }
      | Parachain(target) =>
      // §3.3: no-op for a target that is not registered or is deregistering
      // (§6.4), so a Coretime-delegated solicit cannot add state to a
      // half-removed para.
      if (not(acc.state.isLive(target)) or acc.state.paraInfo(target).isDeregistering) acc
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
        // The active code is already solicited by the service, so soliciting
        // it costs no extra state balance; just record the parachain's own
        // solicit so the referencer survives the code ceasing to be active
        // (§5.2 `pinned`).
        { ...acc,
          state: { ...acc.state,
            parachains: acc.state.parachains.put(
              target,
              { ...pi, validationCode:
                match activeVc { | Some(vc) => Some({ ...vc, pinned: true }) | None => activeVc } }) } }
      } else if (isPending) {
        { ...acc,
          state: { ...acc.state,
            parachains: acc.state.parachains.put(
              target,
              { ...pi, pendingUpgrade:
                match pi.pendingUpgrade {
                  | None => None
                  | Some(pair) => Some(({ ...pair._1, pinned: true }, pair._2))
                } }) } }
      } else {
        val d = acc.state.addReferencer(target, payload.hash, payload.len)
        {
          state: d.state,
          jamDesignate: acc.jamDesignate,
          jamSolicit: acc.jamSolicit.union(optionToSet(d.jamSolicit)),
          jamForget: acc.jamForget,
          jamAssigns: acc.jamAssigns,
          logs: acc.logs.appendOption(d.log),
          foreign: acc.foreign
        }
      }
      }
      }
      | EjectService(payload) =>
      val o = foreignEject(acc.foreign, payload.service, now)
      { ...acc, foreign: o.services, logs: acc.logs.appendOption(o.log) }
      | CreateService(payload) =>
      val o = foreignCreate(
        acc.foreign, payload.codeHash, payload.len, payload.id, payload.desiredId,
        now)
      { ...acc, foreign: o.services, logs: acc.logs.appendOption(o.log) }
      | SetServiceSupervisor(payload) =>
      val o = foreignSetSupervisor(
        acc.foreign, payload.service, payload.newSupervisor)
      { ...acc, foreign: o.services, logs: acc.logs.appendOption(o.log) }
      | Forget(payload) =>
      match payload.target {
      | Service(s) =>
      val o = foreignForget(acc.foreign, s, payload.hash, payload.len, now)
      { ...acc, foreign: o.services, logs: acc.logs.appendOption(o.log) }
      | Parachain(target) =>
      // §5.4: Asset Hub holds the service's own code, so its forget of the
      // running code is never forwarded. Once expunged, JAM's `accumulate`
      // would have no code to run.
      if (target == AssetHubParaId and payload.hash == acc.state.serviceCodeHash) acc
      else if (not(acc.state.isLive(target))) acc
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
            logs: acc.logs.appendOption(d.log),
            foreign: acc.foreign
          }
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
        logs: acc.logs.appendOption(o.log),
        foreign: acc.foreign
      }
      | ParachainSetStateBalance(payload) =>
      // §6.1: only legal from Coretime chain (rights check is in §6).
      // Here we just apply the underlying primitive.
      val o = parachainSetStateBalance(acc.state, payload.paraId, payload.newTotal)
      {
        state: o.state,
        jamDesignate: acc.jamDesignate,
        jamSolicit: acc.jamSolicit,
        jamForget: acc.jamForget,
        jamAssigns: acc.jamAssigns,
        logs: acc.logs.concat(o.logs),
        foreign: acc.foreign
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
        logs: acc.logs.concat(o.logs),
        foreign: acc.foreign
      }
      | ParachainCleanUp(paraId) =>
      val o = parachainCleanUp(acc.state, paraId, now)
      {
        state: o.state,
        jamDesignate: acc.jamDesignate,
        jamSolicit: acc.jamSolicit,
        jamForget: acc.jamForget.union(o.jamForget),
        jamAssigns: acc.jamAssigns,
        logs: acc.logs.concat(o.logs),
        foreign: acc.foreign
      }
      | TransferOut(payload) =>
      if (payload.source.optionGetOr(ParachainServiceId) != ParachainServiceId)
        acc
      else if (payload.sourceSupervisorBalance)
        acc
      else match payload.deferred {
        | None =>
          if (payload.dest == ParachainServiceId and payload.destSupervisorBalance)
            acc
          else
            { ...acc, logs: acc.logs.append(
              TransferFailed({ id: payload.id, error: DestinationNotSupervised })) }
        | Some(_) => acc
      }
      | AssignCore(payload) =>
      // §5.1 step 7 (replay). Caches the entry for its `jamSlot`. The inline
      // case exists because always-accumulate ran before this replay, so a
      // `jamSlot` already due would otherwise wait a block; it fires here
      // instead, then settles into the same post-fire state as the flush.
      val queueLen = payload.queue.length()
      // Refine already rejected an empty queue, so this is defensive only.
      if (queueLen == 0)
        acc
      else if (payload.jamSlot <= now)
        { ...acc,
          jamAssigns: acc.jamAssigns.append({
            core: payload.core,
            assigner: payload.newAssigner.optionGetOr(ParachainServiceId),
            queue: assignQueueFor(payload.queue)
          }),
          state: { ...acc.state,
            pendingAssigns:
              if (fillsDirectly(queueLen)) acc.state.pendingAssigns.mapRemove(payload.core)
              else acc.state.pendingAssigns.put(payload.core, {
                queue: advanceAssignQueue(payload.queue),
                assigner: payload.newAssigner
              }),
            pendingAssignCores:
              if (fillsDirectly(queueLen)) acc.state.pendingAssignCores.mapRemove(payload.core)
              else acc.state.pendingAssignCores.put(
                payload.core, now + AuthorizerQueueLength)
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
      | CleanUpBucketsUpTo(bucketId) =>
      // Asset Hub removes whole buckets up to and including `bucketId`. Both
      // Refine and `accumulateOkPrefix`'s service-restriction check (§4.3)
      // reject non-Asset-Hub origins before we reach this branch.
      {
        val before = queuedTransferCount(acc.state)
        val cleaned = cleanUpBucketsUpTo(acc.state, bucketId)
        { ...acc,
          state: reattributeTransferQueue(cleaned, before, queuedTransferCount(cleaned)) }
      }
      | UpgradeService(payload) =>
      // §5.4: Asset Hub-initiated service code upgrade. Accumulate verifies
      // the new code's preimage is in the service's preimage store before
      // forwarding to JAM `upgrade`; otherwise rejects with a log entry.
      // §5.4: the new code must be AVAILABLE for lookup, not merely solicited.
      // JAM's `upgrade` performs no such check, so switching to a hash whose blob
      // was never supplied would leave the service with no code to run.
      if (acc.state.preimageAvailable(payload.codeHash, payload.len))
        { ...acc, state: { ...acc.state, serviceCodeHash: payload.codeHash } }
      else
        { ...acc,
          logs: acc.logs.append(ServiceUpgradePreimageMissing({ codeHash: payload.codeHash }))
        }
      | SetKV(payload) =>
      // §3.3: no-op for an unregistered or deregistering para (§6.4). Step 1
      // already rejects its work packages, so this only matters if `SetKV`
      // ever names a para.
      if (not(acc.state.isLive(para_id)) or acc.state.paraInfo(para_id).isDeregistering) acc
      else {
        val d = applySetKV(acc.state, para_id, payload.key, payload.value)
        {
          ...acc,
          state: d.state,
          logs: acc.logs.appendOption(d.log)
        }
      }
      | RemoveServiceStorage(payload) =>
      val o = foreignRemoveStorage(acc.foreign, payload.service, payload.key)
      { ...acc, foreign: o.services, logs: acc.logs.appendOption(o.log) }
      | RemoveKV(payload) =>
      // No-op for an unregistered or deregistering para (§6.4).
      if (not(acc.state.isLive(payload.paraId))
          or acc.state.paraInfo(payload.paraId).isDeregistering) acc
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
    // (§6.4) is treated as if it no longer exists: silently drop, no log.
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
        // A rejected candidate changes NOTHING: `reaped` is computed only so the
        // code check sees the post-reap view (a candidate carrying a timed-out
        // pending code must fail), but its state is discarded on rejection along
        // with any forget it would have forwarded.
        if (not(codeOk)) {
          { state: s, rejected: true, jamSolicit: Set(), jamForget: Set(), logs: List() }
        } else if (violatesParachainRestrictions(r.upwardMessages, r.paraId)) {
          // §4.3 defense-in-depth: even if Refine should already have
          // rejected this, Accumulate independently verifies and refuses
          // to apply a work-package whose upward messages include
          // restricted host calls from the wrong parachain.
          { state: s, rejected: true, jamSolicit: Set(), jamForget: Set(), logs: List() }
        } else {
          // §5.1: the candidate is accepted, so its lookup-anchor prune runs before
          // any of its own effects are applied. Doing it first also frees space that
          // would otherwise be reclaimed by evicting entries this prune removes anyway.
          val pruned = pruneLogBelow(reaped.state, r.paraId, r.lookupAnchor)
          // Step 4a: head update.
          val pi2 = pruned.paraInfo(r.paraId)
          val sAfterHead = {
            ...pruned,
            parachains: pruned.parachains.put(r.paraId, { ...pi2, headData: r.headData })
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
    now: Timeslot,
    foreign: ForeignServices
  ): PerPackageOutcome = {
    match wr.result {
      // §3.3: gray paper `WorkExecResult::Error`: JAM substituted an
      // error for this work-item before the service's refine ran.
      // Skip entirely: no log entry, no state change, no prune (no candidate).
      | WorkErr => {
          state: state,
          solicited: solicited,
          jamDesignate: prevDesignate,
          jamAssigns: List(),
          logPruned: None,
          foreign: foreign
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
              logPruned: None,
              foreign: foreign
            }
          | Ok(ok) =>
            val pre = accumulateOkPrefix(state, ok, codeLen, now)
            if (pre.rejected) {
              // §5.1: a rejected candidate changes nothing at all. In particular it
              // does not prune: pruning is justified by the parachain having consumed
              // its log up to the candidate's lookup-anchor, which a candidate failing
              // the parent-head or code check demonstrates nothing about, and pruning
              // here would let anyone holding coretime erase a para's log at any rank
              // by submitting candidates that are then rejected.
              {
                state: pre.state,
                solicited: solicited,
                jamDesignate: prevDesignate,
                jamAssigns: List(),
                logPruned: None,
                foreign: foreign
              }
            } else {
              val baseAcc = {
                state: pre.state,
                jamDesignate: prevDesignate,
                jamSolicit: pre.jamSolicit,
                jamForget: pre.jamForget,
                jamAssigns: List(),
                logs: pre.logs,
                foreign: foreign
              }
              val acc = ok.upwardMessages.foldl(baseAcc, (a, m) =>
                applyUpwardMessage(a, ok.paraId, codeLen, now, ok.lookupAnchor, m))
              // `Solicit` is always for the caller itself; `Forget` names its
              // target, so a Coretime-driven forget clears the TARGET's entry.
              val newSolicited = ok.upwardMessages.foldl(solicited, (s, m) => match m {
                // Records the TARGET, so a Coretime-delegated solicit lands on
                // the para actually charged. A Service target is not in our
                // registry at all.
                | Solicit(p) => match p.target {
                  | Parachain(target) => s.union(Set((target, p.hash, p.len)))
                  | Service(_) => s
                }
                | Forget(p) => match p.target {
                  | Parachain(target) => s.exclude(Set((target, p.hash, p.len)))
                  // Not our store, so the ghost set is untouched.
                  | Service(_) => s
                }
                | _ => s
              })
              // The lookup-anchor prune already ran inside `accumulateOkPrefix`,
              // ahead of every effect of this candidate.
              {
                state: acc.state.appendAccumulateLogs(ok.paraId, now, acc.logs),
                solicited: newSolicited,
                jamDesignate: acc.jamDesignate,
                jamAssigns: acc.jamAssigns,
                logPruned: Some((ok.paraId, ok.lookupAnchor)),
                foreign: acc.foreign
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
    packages: List[WorkResult],
    foreign: ForeignServices
  ): MultiPackageOutcome = {
    packages.foldl(
      { state: state,
        solicited: solicited,
        jamDesignate: prevDesignate,
        jamAssigns: List(),
        logPruned: List(),
        foreign: foreign },
      (acc, p) => {
        val out = accumulateOnePackage(
          acc.state, p, acc.solicited, acc.jamDesignate, codeLen, now, acc.foreign)
        { state: out.state,
          solicited: out.solicited,
          jamDesignate: out.jamDesignate,
          jamAssigns: acc.jamAssigns.concat(out.jamAssigns),
          logPruned: acc.logPruned.appendOption(out.logPruned),
          foreign: out.foreign
        }
      })
  }

  // ---- Always-accumulate (§5.1, general work) ---------------------------

  /// Fire pending `assign`s that are due (§4.3). Gating reads ONLY the
  /// dirty-core index; the much larger payload is read just for a core that is
  /// actually due, and not-yet-due cores are retained unchanged. A stored
  /// `assigner` of `None` resolves to this service, since JAM's `assign` always
  /// writes one.
  pure def applyAlwaysAccumulate(
    s: ParachainServiceState,
    now: Timeslot
  ): { state: ParachainServiceState, assigns: List[AssignCall] } = {
    s.pendingAssignCores.keys().fold(
      { state: s, assigns: List() },
      (acc, c) => {
        val dueAt = s.pendingAssignCores.get(c)
        if (now < dueAt)
          acc
        else {
          val pending = s.pendingAssigns.get(c)
          val queueLen = pending.queue.length()
          { state: { ...acc.state,
              pendingAssigns:
                if (fillsDirectly(queueLen)) acc.state.pendingAssigns.mapRemove(c)
                else acc.state.pendingAssigns.put(c, {
                  ...pending, queue: advanceAssignQueue(pending.queue)
                }),
              pendingAssignCores:
                if (fillsDirectly(queueLen)) acc.state.pendingAssignCores.mapRemove(c)
                else acc.state.pendingAssignCores.put(c, now + AuthorizerQueueLength)
            },
            assigns: acc.assigns.append({
              core: c,
              assigner: pending.assigner.optionGetOr(ParachainServiceId),
              queue: assignQueueFor(pending.queue)
            })
          }
        }
      }
    )
  }

  /// §5.1: admit one transfer into `open`, the bucket the current accumulate
  /// invocation is filling (`None` before it has opened one). Returns the new
  /// state paired with the bucket left open for the next arrival.
  pure def admitIncomingTransfer(
    s: ParachainServiceState,
    open: Option[BucketId],
    t: IncomingTransfer
  ): (ParachainServiceState, Option[BucketId]) = {
    val count = queuedTransferCount(s)
    // §5.1: inside the reservation the entry is already paid for.
    val reserved = count < MaxIncomingTransfers
    val admitted = reserved or t.amount >= IncomingTransferEntryFootprint
    if (not(admitted)) (s, open)
    else {
      // The invocation's own bucket takes it while below capacity; filling it
      // closes it, so the next arrival opens the following id. Buckets left by
      // earlier invocations are already closed and never reopened.
      val target =
        match open {
          | Some(b) =>
            if (s.incomingTransfers.get(b).length() < MaxTransfersPerBucket) b
            else nextBucketId(s)
          | None => nextBucketId(s)
        }
      val bucket =
        if (s.incomingTransfers.keys().contains(target)) s.incomingTransfers.get(target)
        else List()
      // `target` is always the tail: either the bucket this invocation opened
      // last, or the fresh id one past it.
      val queued = {
        ...s,
        incomingTransfers: s.incomingTransfers.put(target, bucket.append(t)),
        incomingTransferBuckets: Some({
          firstBucket:
            match s.incomingTransferBuckets {
              | None => target
              | Some(bk) => bk.firstBucket
            },
          lastBucket: target,
          count: count + 1
        })
      }
      val charged =
        if (reserved) queued
        else queued.reattributeTransferQueue(count, count + 1)
      (charged, Some(target))
    }
  }

  /// §5.1 incoming-transfer processing for ONE accumulate invocation: admit `ts`
  /// in order, packing the admitted ones into buckets this invocation opens.
  pure def applyIncomingTransfers(
    s: ParachainServiceState,
    ts: List[IncomingTransfer]
  ): ParachainServiceState = {
    val start: (ParachainServiceState, Option[BucketId]) = (s, None)
    ts.foldl(start, (acc, t) => admitIncomingTransfer(acc._1, acc._2, t))._1
  }

  /// One transfer arriving on its own, i.e. an invocation carrying just it.
  pure def applyIncomingTransfer(
    s: ParachainServiceState,
    t: IncomingTransfer
  ): ParachainServiceState = applyIncomingTransfers(s, List(t))

  /// The full per-block Accumulate pipeline (§5.1): first run always-accumulate
  /// (flush due authorizer queues), then accumulate the block's work packages.
  pure def accumulateBlock(
    state: ParachainServiceState,
    now: Timeslot,
    staging: List[ValidatorKey],
    solicited: Set[(ParaId, Hash, int)],
    codeLen: int,
    packages: List[WorkResult],
    foreign: ForeignServices
  ): BlockOutcome = {
    val flushed = applyAlwaysAccumulate(state, now)
    val acc =
      accumulateAll(flushed.state, solicited, None, codeLen, now, packages, foreign)
    {
      state: acc.state,
      staging: match acc.jamDesignate {
        | Some(v) => v
        | None => staging
      },
      solicited: acc.solicited,
      assigns: flushed.assigns.concat(acc.jamAssigns),
      logPruned: acc.logPruned,
      results: packages,
      headRoot: headCommitment(state, acc.state),
      foreign: acc.foreign
    }
  }
}
