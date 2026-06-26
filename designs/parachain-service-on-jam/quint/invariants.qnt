// State-invariant catalogue for the Parachain Service spec.
//
// Each invariant is a nullary `val: bool` reading the shared state vars
// declared in `state_vars.qnt`. The CLI flag `--invariant=<name>` targets
// any of them directly.
module invariants {
  import types.* from "./types"
  import state.* from "./state"
  import state_balance.* from "./state_balance"
  import state_vars.* from "./state_vars"

  /// For every live para, `0 ≤ used_state_balance ≤ total_state_balance`.
  val balance_invariant: bool =
  svc.parachains.keys().forall(para_id => {
      val pi = svc.paraInfo(para_id)
      0 <= pi.usedStateBalance and pi.usedStateBalance <= pi.totalStateBalance
  })

  /// A ParaId appears in `preimage_registry[h].referencers` iff its
  /// referencer set is non-empty. (The "iff" is structural — an empty
  /// referencer set should not be persisted; see `removeReferencer`.)
  val preimage_referencer_consistency: bool =
  svc.preimageRegistry.keys().forall(h =>
    svc.preimageRegistry.get(h).referencers != Set()
  )

  /// `designate` is invoked iff Accumulate just processed a
  /// `SetValidatorKeys { isLast: true }` whose assembled length ∈ ValCount.
  /// We over-approximate by checking the stagingset, when non-empty, is in
  /// `ValCount`.
  val designate_only_valid_lengths: bool =
  jamStagingSet == List() or ValCount.contains(jamStagingSet.length())

  /// No pending upgrade points at the currently active code.
  val no_pending_when_code_matches: bool =
  svc.parachains.keys().forall(para_id => {
      val pi = svc.paraInfo(para_id)
      match pi.pendingUpgrade {
        | None => true
        | Some(pair) => pair._1.hash != pi.validationCode.hash
      }
  })

  /// Staged validator keys live entirely on Asset Hub's books: if the
  /// buffer is non-empty, Asset Hub must be live and have room for it.
  val staged_keys_owned_by_asset_hub: bool =
  svc.stagedValidatorKeys == List() or (
    svc.isLive(AssetHubParaId) and
    svc.paraInfo(AssetHubParaId).usedStateBalance >=
    ValidatorKeySize * svc.stagedValidatorKeys.length()
  )

  /// Every live parachain's active validation code (when set) is a
  /// referencer in `preimage_registry`. The all-zero hash is the
  /// uninitialised sentinel between `parachain_set_state_balance` and
  /// the first `parachain_set_validation_code` in the registration
  /// sequence (see §6.2).
  val validation_code_in_registry: bool =
  svc.parachains.keys().forall(para_id => {
      val pi = svc.paraInfo(para_id)
      pi.validationCode.hash == { vchBytes: 0 } or
      svc.hasReferencer(vchAsHash(pi.validationCode.hash), para_id)
  })

  /// Every live parachain's pending-upgrade code (when present) is also
  /// a referencer in `preimage_registry` — same lifecycle as any other
  /// solicited preimage (§5.2).
  val pending_code_in_registry: bool =
  svc.parachains.keys().forall(para_id => {
      val pi = svc.paraInfo(para_id)
      match pi.pendingUpgrade {
        | None => true
        | Some(pair) => svc.hasReferencer(vchAsHash(pair._1.hash), para_id)
      }
  })

  /// Every `(para, hash)` for which Accumulate has processed a `Solicit`
  /// upward message without a subsequent `Forget` must still be
  /// referenced in `preimage_registry`. Guarded by `isLive` because
  /// `parachainCleanUp` releases the registry slot but leaves the ghost
  /// entry stale by design — we only care that live paras' solicits are
  /// reflected.
  val solicit_implies_registry: bool =
  solicitedSet.forall(pair => {
      val para_id = pair._1
      val h = pair._2
      not(svc.isLive(para_id)) or svc.hasReferencer(h, para_id)
  })

  /// Every live para's `usedStateBalance` is at least the baseline footprint.
  /// After registration the baseline is pre-charged and never released
  /// (§6.1); only preimage referencer churn and staged-key churn move the
  /// figure above baseline.
  val used_above_baseline: bool =
  svc.parachains.keys().forall(para_id =>
    svc.paraInfo(para_id).usedStateBalance >= BaselineFootprint)

  /// Full footprint accounting (§6.1). For every live para,
  /// `usedStateBalance` equals
  ///     `BaselineFootprint`
  ///   + Σ `preimageFootprint(len)` over every preimage the para references
  ///   + (Asset Hub only) `ValidatorKeySize * |stagedValidatorKeys|`.
  /// Catches off-by-one bugs in `addReferencer` / `removeReferencer` /
  /// `applySetValidatorKeys` / `parachainSetValidationCode`'s swap.
  val used_balance_consistency: bool =
  svc.parachains.keys().forall(para_id => {
      val pi = svc.paraInfo(para_id)
      val refSum = svc.preimageRegistry.keys().fold(0, (acc, h) => {
          val e = svc.preimageRegistry.get(h)
          if (e.referencers.contains(para_id)) acc + preimageFootprint(e.len)
          else acc
      })
      val stagedFootprint =
      if (para_id == AssetHubParaId)
      ValidatorKeySize * svc.stagedValidatorKeys.length()
      else 0
      pi.usedStateBalance == BaselineFootprint + refSum + stagedFootprint
  })

  /// Every referencer recorded in `preimage_registry` is a currently-live
  /// `ParaId`. `parachainCleanUp` (§6.4) must remove the cleaning-up para
  /// from every referencer set; this invariant catches any cleanup path
  /// that forgets to.
  val referencers_are_live: bool =
  svc.preimageRegistry.keys().forall(h =>
    svc.preimageRegistry.get(h).referencers.forall(para_id =>
      svc.isLive(para_id)))

  /// Every `parachain_log` key is a currently-live `ParaId`. Together with
  /// `referencers_are_live` and `staged_keys_owned_by_asset_hub` this
  /// captures the post-cleanup property: no per-parachain state remains
  /// anywhere in service storage for an unregistered para.
  val parachain_log_only_for_live: bool =
  svc.parachainLog.keys().forall(para_id => svc.isLive(para_id))

  /// Transition invariant (§5.2 Phase 5a). When a code upgrade is
  /// *activated* — para `para_id`'s validation code changes to what was its
  /// pending-upgrade hash — the old code's referencer slot for that para is
  /// released, UNLESS one of:
  ///   - the parachain was independently soliciting the old code
  ///     (its `validationCode.pinned` bit), so the slot survives; or
  ///   - the same block re-requested an upgrade back to the old code (it is
  ///     re-referenced as the new pending; §5.2 allows activation + a
  ///     different request, processed in that order).
  /// `prevSvc` holds the pre-step state. Exact balance accounting is pinned
  /// separately by `used_balance_consistency`.
  val code_activation_releases_old: bool =
    svc.parachains.keys().forall(para_id => {
      if (not(prevSvc.isLive(para_id))) true
      else {
        val before = prevSvc.paraInfo(para_id)
        val after = svc.paraInfo(para_id)
        val activated =
          match before.pendingUpgrade {
            | None => false
            | Some(pair) =>
              after.validationCode.hash == pair._1.hash and
              after.validationCode.hash != before.validationCode.hash
          }
        val oldHash = vchAsHash(before.validationCode.hash)
        val reRequested =
          match after.pendingUpgrade {
            | None => false
            | Some(pair) => pair._1.hash == before.validationCode.hash
          }
        not(activated) or
          not(svc.hasReferencer(oldHash, para_id)) or
          before.validationCode.pinned or
          reRequested
      }
    })

  /// Every `headData` change between `prevSvc` and `svc` must be backed
  /// by an `Ok` work result that claims that new head — either directly
  /// (the Ok's own `headData` for its `paraId`) or indirectly via a
  /// `ParachainSetHead` upward message. Holds universally, including
  /// always-accumulate blocks with no work packages. Uses only data also
  /// available to a Rust trace harness.
  val head_state_matches_outcomes: bool =
    prevSvc.parachains.keys().forall(p =>
      // Guarded explicitly because Quint's `or` is eager — `svc.paraInfo(p)`
      // would crash on a `ParachainCleanUp`-removed para.
      if (not(svc.parachains.keys().contains(p))) true
      else if (svc.paraInfo(p).headData == prevSvc.paraInfo(p).headData) true
      else lastStepWorkResults.foldl(false, (acc, r) => match r.result {
        | WorkErr => acc
        | WorkOk(digest) =>
          match digest {
            | Ok(ok) =>
              val direct = ok.paraId == p
                and ok.headData == svc.paraInfo(p).headData
              val viaMsg = ok.upwardMessages.foldl(false, (a, m) => match m {
                | ParachainSetHead(payload) =>
                  a or (payload.paraId == p
                    and payload.newHead == svc.paraInfo(p).headData)
                | _ => a
              })
              acc or direct or viaMsg
            | Err(_) => acc
          }
      })
    )

  /// When Refine fails for a *registered* para, Accumulate must record a
  /// `RefineLogEntry` carrying that error in the para's `parachain_log`
  /// (design §3.3). This invariant checks that record exists after a step.
  ///
  /// Two facts let the check find the entry by reading `parachain_log[p]`
  /// at `now`:
  ///   - `parachain_log[p]` is a `Map[Timeslot, LogEntry]`, and Accumulate
  ///     files each entry under the timeslot it ran in.
  ///   - `stepRefineAccumulate` does not advance time, so the timeslot the
  ///     entry was filed under equals the value of `now` we read here.
  /// Hence: the matching entry, if present, lives at `parachain_log[p][now]`.
  ///
  /// Two cases are deliberately NOT required to hold:
  ///   - Unregistered-para errors: there is no `parachain_log[p]` to append
  ///     to (§5.1 step 1), so they are exempt.
  ///   - Each timeslot holds at most one `LogEntry` (§3.1
  ///     `CountedMap[Timeslot, LogEntry, 8]`), so when a para has several
  ///     results in one step the *last* writer wins that slot. We therefore
  ///     only require the entry for the para's LAST result: if that result
  ///     is an `Err`, the slot must hold a matching `RefineLogEntry`. A
  ///     later `Ok` for the same para legitimately overwrites an earlier
  ///     refine entry with an accumulate one — covered by `acc_error_logged`.
  val refine_error_logged: bool = {
    val erredLiveParas =
      lastStepWorkResults.foldl(Set(), (acc, r) => match r.result {
        | WorkErr => acc
        | WorkOk(digest) => match digest {
          | Ok(_) => acc
          | Err(e) => if (svc.isLive(e.paraId)) acc.union(Set(e.paraId)) else acc
        }
      })
    erredLiveParas.forall(p => {
      // Last result for this para; `None` once any later result exists that
      // is not an Err for `p` (an Ok write would have superseded the slot).
      val lastForPara =
        lastStepWorkResults.foldl(None, (acc, r) => match r.result {
          | WorkErr => acc
          | WorkOk(digest) => match digest {
            | Ok(ok) => if (ok.paraId == p) None else acc
            | Err(e) => if (e.paraId == p) Some(e.error) else acc
          }
        })
      match lastForPara {
        | None => true  // superseded by a later Ok for the same para
        | Some(err) =>
          val log = svc.paraLog(p)
          log.keys().contains(now) and
          (match log.get(now) {
            | RefineLogEntry(entry) => entry.error == err
            | AccumulateLogEntry(_) => false
          })
      }
    })
  }

  /// An `Ok` candidate that fails the authoritative validation-code check
  /// (§5.1 step 3 — its `validationCodeHash` matches neither the active nor
  /// the pending code) must surface an `InvalidCodeHash` accumulate event in
  /// the para's `parachain_log` at the current timeslot (§5.1, §3.1). We
  /// check this against `prevSvc` — the pre-step code state the check ran
  /// against — and only for the para's LAST result this step (last writer
  /// wins per slot). Paras whose code check would have passed are exempt;
  /// other accumulate-log causes (insufficient balance, designate rejection)
  /// are pinned by `used_balance_consistency` / `designate_only_valid_lengths`
  /// rather than re-derived here.
  val acc_error_logged: bool = {
    val okLiveParas =
      lastStepWorkResults.foldl(Set(), (acc, r) => match r.result {
        | WorkErr => acc
        | WorkOk(digest) => match digest {
          | Err(_) => acc
          | Ok(ok) => if (prevSvc.isLive(ok.paraId)) acc.union(Set(ok.paraId)) else acc
        }
      })
    okLiveParas.forall(p => {
      // Last result for this para, and whether it is an Ok with a code
      // hash matching neither active nor pending in the PRE-step state.
      val lastBadCode =
        lastStepWorkResults.foldl(None, (acc, r) => match r.result {
          | WorkErr => acc
          | WorkOk(digest) => match digest {
            | Err(_) => acc
            | Ok(ok) =>
              if (ok.paraId != p) acc
              else {
                val pi = prevSvc.paraInfo(p)
                val matchesActive = ok.validationCodeHash == pi.validationCode.hash
                // The lazy reap (§5.1 step 2) runs BEFORE the code check and
                // can drop a timed-out pending upgrade. If the pre-step pending
                // deadline had passed we cannot decide the post-reap code state
                // from `prevSvc` alone, so we exempt that case (`None`).
                val reapable =
                  match pi.pendingUpgrade {
                    | None => false
                    | Some(pair) => pair._2 <= now
                  }
                val matchesPending =
                  match pi.pendingUpgrade {
                    | None => false
                    | Some(pair) => ok.validationCodeHash == pair._1.hash
                  }
                if (reapable) None
                else Some(not(matchesActive) and not(matchesPending))
              }
          }
        })
      match lastBadCode {
        | None => true
        | Some(bad) =>
          not(bad) or {
            val log = svc.paraLog(p)
            log.keys().contains(now) and
            (match log.get(now) {
              | AccumulateLogEntry(es) =>
                es.foldl(false, (a, e) => match e {
                  | InvalidCodeHashAcc(_) => true
                  | _ => a
                })
              | RefineLogEntry(_) => false
            })
          }
      }
    })
  }

  /// `incoming_transfers` must never exceed its cap; overflow is bounced
  /// at ingress in `stepIncomingTransfer`.
  val incoming_transfers_bounded: bool =
    svc.incomingTransfers.length() <= IncomingTransfersMaxLen

  /// Each pending authorizer queue must fit within JAM's queue length.
  val pending_authorizer_queue_bounded: bool =
    svc.pendingAuthorizerQueues.keys().forall(c =>
      svc.pendingAuthorizerQueues.get(c).length() <= AuthorizerQueueLength)

  /// `staged_validator_keys` must never exceed the worst-case staging
  /// length (`MaxStagedValidatorKeys = 1023`).
  val staged_validator_keys_bounded: bool =
    svc.stagedValidatorKeys.length() <= MaxStagedValidatorKeys

  /// Composite invariant — every invariant above must hold simultaneously.
  val invariants: bool = and {
    balance_invariant,
    preimage_referencer_consistency,
    designate_only_valid_lengths,
    no_pending_when_code_matches,
    staged_keys_owned_by_asset_hub,
    validation_code_in_registry,
    pending_code_in_registry,
    solicit_implies_registry,
    used_above_baseline,
    used_balance_consistency,
    referencers_are_live,
    parachain_log_only_for_live,
    code_activation_releases_old,
    head_state_matches_outcomes,
    refine_error_logged,
    acc_error_logged,
    incoming_transfers_bounded,
    pending_authorizer_queue_bounded,
    staged_validator_keys_bounded
  }
}
