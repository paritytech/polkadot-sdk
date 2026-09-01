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
  import head_commitment.* from "./head_commitment"
  import foreign_services.* from "./foreign_services"

  /// §6.5: every service this model created is supervised by the Parachain
  /// Service, unless supervision was explicitly handed away, in which case the
  /// service supervises itself. No third party ever appears, because only
  /// `set_service_supervisor` can move the link and the model only ever names
  /// the service itself.
  val foreign_supervisor_is_us_or_self: bool =
  foreignServices.keys().forall(s =>
    effectiveSupervisor(foreignServices, s) == ParachainServiceId
      or effectiveSupervisor(foreignServices, s) == s)

  /// The Parachain Service never appears among the services it supervises.
  /// `eject_service` refuses to name self, and a `desired_id` naming our own
  /// index is refused as `IdTaken` even though it sits in the protected range.
  val foreign_excludes_self: bool =
  not(foreignServices.keys().contains(ParachainServiceId))

  /// The hash `accumulate` returned is exactly the commitment over the heads
  /// that changed in the step (§5.5): `None` when none changed, the single
  /// leaf's hash when one did, the Merkle root otherwise. Also pins the steps
  /// that return no hash: if one ever started moving a head without
  /// committing to it, this fails.
  val head_commitment_matches_changed_heads: bool =
  lastHeadRoot == headCommitment(prevSvc, svc)

  /// For every live para, `0 ≤ used_state_balance ≤ total_state_balance`.
  val balance_invariant: bool =
  svc.parachains.keys().forall(para_id => {
      val pi = svc.paraInfo(para_id)
      0 <= pi.usedStateBalance and pi.usedStateBalance <= pi.totalStateBalance
  })

  /// A ParaId appears in `preimage_registry[(h, len)].referencers` iff its
  /// referencer set is non-empty. (The "iff" is structural: an empty
  /// referencer set should not be persisted; see `removeReferencer`.)
  val preimage_referencer_consistency: bool =
  svc.preimageRegistry.keys().forall(key =>
    svc.preimageRegistry.get(key).referencers != Set()
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
        | Some(pair) =>
          match pi.validationCode {
            | None => true
            | Some(vc) => pair._1.ref != vc.ref
          }
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
  /// referencer in `preimage_registry`. `None` is a freshly-registered para
  /// between `parachain_set_state_balance` and the first
  /// `parachain_set_validation_code` in the registration sequence (see §6.2).
  val validation_code_in_registry: bool =
  svc.parachains.keys().forall(para_id => {
      val pi = svc.paraInfo(para_id)
      match pi.validationCode {
        | None => true
        | Some(vc) => svc.hasReferencer(vchAsHash(vc.ref.hash), vc.ref.len, para_id)
      }
  })

  /// Every live parachain's pending-upgrade code (when present) is also
  /// a referencer in `preimage_registry`, same lifecycle as any other
  /// solicited preimage (§5.2).
  val pending_code_in_registry: bool =
  svc.parachains.keys().forall(para_id => {
      val pi = svc.paraInfo(para_id)
      match pi.pendingUpgrade {
        | None => true
        | Some(pair) =>
          svc.hasReferencer(
            vchAsHash(pair._1.ref.hash),
            pair._1.ref.len,
            para_id)
      }
  })

  /// Every `(para, hash, len)` for which Accumulate has processed a `Solicit`
  /// upward message without a subsequent `Forget` must still be
  /// referenced in `preimage_registry`. Guarded by `isLive` because
  /// `parachainCleanUp` releases the registry slot but leaves the ghost
  /// entry stale by design; we only care that live paras' solicits are
  /// reflected.
  val solicit_implies_registry: bool =
  solicitedSet.forall(triple => {
      val para_id = triple._1
      val h = triple._2
      val len = triple._3
      not(svc.isLive(para_id)) or svc.hasReferencer(h, len, para_id)
  })

  /// Every live para's `usedStateBalance` is at least its baseline footprint
  /// After registration the baseline is pre-charged and never released; only
  /// preimage referencer churn moves the figure above baseline.
  val used_above_baseline: bool =
  svc.parachains.keys().forall(para_id =>
    svc.paraInfo(para_id).usedStateBalance >= baselineFor(para_id))

  /// Full footprint accounting (§6.1). For every live para, `usedStateBalance`
  /// equals its baseline footprint plus Σ `preimageFootprint(len)` over
  /// every preimage the para references. Catches off-by-one bugs in
  /// `addReferencer` / `removeReferencer` / `parachainSetValidationCode`'s swap.
  val used_balance_consistency: bool =
  svc.parachains.keys().forall(para_id => {
      val pi = svc.paraInfo(para_id)
      val refSum = svc.preimageRegistry.keys().fold(0, (acc, key) => {
          val e = svc.preimageRegistry.get(key)
          if (e.referencers.contains(para_id)) acc + preimageFootprint(key._2)
          else acc
      })
      val kvSum = svc.keyValueStorage.keys().fold(0, (acc, key) => {
          if (key._1 == para_id) {
              val v = svc.keyValueStorage.get(key)
              acc + kvEntryFootprint(key._2.length(), v.length())
          } else acc
      })
      pi.usedStateBalance == baselineFor(para_id) + refSum + kvSum
  })

  /// Every referencer recorded in `preimage_registry` is a currently-live
  /// `ParaId`. `parachainCleanUp` (§6.4) must remove the cleaning-up para
  /// from every referencer set; this invariant catches any cleanup path
  /// that forgets to.
  val referencers_are_live: bool =
  svc.preimageRegistry.keys().forall(key =>
    svc.preimageRegistry.get(key).referencers.forall(para_id =>
      svc.isLive(para_id)))

  /// Every `parachain_log` key is a currently-live `ParaId`. Together with
  /// `referencers_are_live` and `staged_keys_owned_by_asset_hub` this
  /// captures the post-cleanup property: no per-parachain state remains
  /// anywhere in service storage for an unregistered para.
  val parachain_log_only_for_live: bool =
  svc.parachainLog.keys().forall(para_id => svc.isLive(para_id))

  /// Transition invariant (§5.2 Phase 5a). When a code upgrade is
  /// *activated*: para `para_id`'s validation code changes to what was its
  /// pending-upgrade ref; the old code's referencer slot for that para is
  /// released, UNLESS one of:
  ///   - the parachain was independently soliciting the old code
  ///     (its `validationCode.pinned` bit), so the slot survives;
  ///   - the same block re-requested an upgrade back to the old code (it is
  ///     re-referenced as the new pending; §5.2 allows activation + a
  ///     different request, processed in that order); or
  ///   - a `Solicit` upward message for the old code's hash was processed
  ///     (now in `solicitedSet`), re-referencing it as a plain data preimage
  ///     independent of the code slot.
  /// `prevSvc` holds the pre-step state. Exact balance accounting is pinned
  /// separately by `used_balance_consistency`.
  val code_activation_releases_old: bool =
    svc.parachains.keys().forall(para_id => {
      if (not(prevSvc.isLive(para_id))) true
      else {
        val before = prevSvc.paraInfo(para_id)
        val after = svc.paraInfo(para_id)
        // A para with no old active code has nothing to release, so the
        // invariant holds vacuously there.
        match before.validationCode {
          | None => true
          | Some(beforeVc) => {
            val activated =
              match before.pendingUpgrade {
                | None => false
                | Some(pair) =>
                  match after.validationCode {
                    | None => false
                    | Some(afterVc) =>
                      afterVc.ref == pair._1.ref and afterVc.ref != beforeVc.ref
                  }
              }
            val oldHash = vchAsHash(beforeVc.ref.hash)
            val oldLen = beforeVc.ref.len
            val reRequested =
              match after.pendingUpgrade {
                | None => false
                | Some(pair) => pair._1.ref == beforeVc.ref
              }
            // The two-step forget (§6.1) deliberately RETAINS the last
            // referencer when the code was still `Provided`: that first forget
            // only unrequests it, and the slot is freed by a second forget past
            // the logged `ForgetAgainAt` deadline. A referencer surviving in
            // that limbo is correct, not a leaked slot.
            val inForgetLimbo =
              svc.preimageStatus.keys().contains((oldHash, oldLen)) and
              (match svc.preimageStatus.get((oldHash, oldLen)) {
                | Unrequested(_) => true
                | Rerequested(_) => true
                | _ => false
              })
            not(activated) or
              not(svc.hasReferencer(oldHash, oldLen, para_id)) or
              beforeVc.pinned or
              reRequested or
              inForgetLimbo or
              solicitedSet.contains((para_id, oldHash, oldLen))
          }
        }
      }
    })

  /// Every `headData` change between `prevSvc` and `svc` must be backed
  /// by an `Ok` work result that claims that new head, either directly
  /// (the Ok's own `headData` for its `paraId`) or indirectly via a
  /// `ParachainSetHead` upward message. Holds universally, including
  /// always-accumulate blocks with no work packages. Uses only data also
  /// available to a Rust trace harness.
  val head_state_matches_outcomes: bool =
    prevSvc.parachains.keys().forall(p =>
      // Guarded explicitly because Quint's `or` is eager; `svc.paraInfo(p)`
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

  /// Parent-head continuity (§5.1 step 3): replay the block's head-affecting
  /// operations in order and confirm the result matches the stored heads. Start
  /// from the pre-block heads; for each `Ok` work result, apply it ONLY if its
  /// para is live, its declared parent matches that para's current head
  /// (step 3), and its `Requires` entries settle against the ring as of that
  /// digest (step 6, §8.3) — settlement is the one Accumulate rejection Refine
  /// cannot pre-filter, so the mirror must carry the ring: pushes from earlier
  /// accepted digests in the same block included, threaded through the real
  /// ring functions so eviction stays exact. If accepted, write its new head
  /// and push its `Provides` root (step 6), then replay its
  /// `parachain_set_head` messages (step 7), each clearing its target's ring
  /// (§6, §8.3). A rejected candidate (stale parent or missed settlement) is
  /// ignored entirely (its head update, its push, AND its upward messages),
  /// just as Accumulate never applies a rejected candidate. This is what lets a
  /// forced `parachain_set_head` survive a same-block candidate built on a
  /// superseded head, while a candidate legitimately continuing the current head
  /// is allowed.
  ///
  /// `ParachainCleanUp`'s ring teardown is deliberately NOT mirrored: the
  /// step's menu only cleans up paras the requires menu never names as a
  /// source (see `arbitraryMessages` / `specMsgRequiresMenu`), so the ring a
  /// clean-up tears down can never decide a later settlement here.
  val parent_head_continuity: bool = {
    val initialHeads = prevSvc.parachains.keys().mapBy(p => prevSvc.paraInfo(p).headData)
    val replayed =
      lastStepWorkResults.foldl((initialHeads, prevSvc), (acc, r) => match r.result {
        | WorkErr => acc
        | WorkOk(digest) => match digest {
          | Err(_) => acc
          | Ok(ok) =>
            if (not(acc._1.keys().contains(ok.paraId))) acc
            else if (ok.parentHeadHash != headHash(acc._1.get(ok.paraId))) acc
            else if (not(specMsgSettles(acc._2, ok.specMsgRequires))) acc
            else {
              // Accepted: write the new head and push the `Provides` root
              // (step 6), then replay its forced `parachain_set_head`
              // messages (step 7), each a no-op for a non-live target.
              val pushed = match ok.specMsgProvides {
                | Some(root) => specMsgPush(acc._2, ok.paraId, root)
                | None => acc._2
              }
              ok.upwardMessages.foldl(
                (acc._1.put(ok.paraId, ok.headData), pushed), (h, m) =>
                match m {
                  | ParachainSetHead(pl) =>
                    if (h._1.keys().contains(pl.paraId))
                      (h._1.put(pl.paraId, pl.newHead),
                       specMsgClearRing(h._2, pl.paraId))
                    else h
                  | _ => h
                })
            }
        }
      })
    prevSvc.parachains.keys().forall(p =>
      if (not(svc.parachains.keys().contains(p))) true
      else replayed._1.get(p) == svc.paraInfo(p).headData)
  }

  /// For every Refine Err in lastStepWorkResults targeting a registered para,
  /// a matching RefineLogEntry must exist in the flat log at timeslot now. The
  /// flat vec supports multiple entries per timeslot so ALL errors must be
  /// present; no last-writer-wins.
  val refine_error_logged: bool =
    lastStepWorkResults.foldl(true, (acc, r) => acc and match r.result {
      | WorkErr => true
      | WorkOk(digest) => match digest {
        | Ok(_) => true
        | Err(e) =>
          if (not(svc.isLive(e.paraId))) true
          else svc.paraLog(e.paraId).foldl(false, (found, pair) =>
            found or (pair._1 == now and match pair._2 {
              | RefineLogEntry(entry) => entry.error == e.error
              | AccumulateLogEntry(_) => false
            }))
      }
    })

  /// The maintained `count` equals the transfers actually held across the buckets.
  /// It is incremented on append and decremented on clean-up rather than derived,
  /// so this is what stops the two drifting apart.
  val incoming_transfer_count_consistent: bool =
    queuedTransferCount(svc) == queuedTransferCountWalked(svc)

  /// The bucket endpoints are well formed: the occupied ids are exactly the
  /// contiguous range they bound. Contiguity is what lets Asset Hub enumerate the
  /// queue from the two endpoints alone, with no per-bucket successor pointer, and
  /// what makes `cleanUpBucketsUpTo`'s cut a prefix.
  val incoming_transfers_buckets_consistent: bool =
    match svc.incomingTransferBuckets {
      // No endpoints iff no buckets.
      | None => svc.incomingTransfers.keys() == Set()
      | Some(buckets) =>
        buckets.firstBucket <= buckets.lastBucket and
        svc.incomingTransfers.keys() ==
          buckets.firstBucket.to(buckets.lastBucket)
    }

  /// §3.1: every bucket holds at least one transfer and never more than
  /// `MaxTransfersPerBucket`, which is what lets Asset Hub bound the cost of
  /// reading any single bucket. Buckets are never empty either: one is only
  /// created by the arrival it holds, and clean-up removes whole buckets.
  val incoming_transfer_bucket_bounded: bool =
    svc.incomingTransfers.keys().forall(k => {
      val held = svc.incomingTransfers.get(k).length()
      held > 0 and held <= MaxTransfersPerBucket
    })


  /// A cached assign holds a non-empty queue within JAM's queue length.
  /// Non-emptiness is what makes `assignQueueFor`'s division well-defined; an
  /// empty queue is a no-op and is never cached (§4.3).
  val pending_authorizer_queue_bounded: bool =
    svc.pendingAssigns.keys().forall(c => {
      val queueLen = svc.pendingAssigns.get(c).queue.length()
      queueLen > 0 and queueLen <= AuthorizerQueueLength
    })

  /// The dirty-core index tracks exactly the cores with a pending assign
  /// payload. Since gating reads only the index, a divergence would either
  /// strand a payload that never fires or index a core with no payload.
  val pending_authorizer_cores_consistent: bool =
    svc.pendingAssignCores.keys() == svc.pendingAssigns.keys()

  /// Every cached assign is scheduled strictly in the future: a due entry is
  /// never left cached; always-accumulate fires due entries at block start
  /// (dropping them, or re-scheduling a rotating queue 80 blocks out), and an
  /// `assign_core` whose `jamSlot` is already due is applied inline during
  /// replay.
  val pending_authorizer_apply_at_future: bool =
    svc.pendingAssignCores.keys().forall(c =>
      svc.pendingAssignCores.get(c) > now)

  /// Every `assign` recorded last step targets a valid core index (< CoreCount).
  /// (A structural sanity check that the assign-recording is well-formed.)
  val assign_calls_well_formed: bool =
    lastStepAssigns.foldl(true, (acc, a) =>
      acc and a.core >= 0 and a.core < CoreCount)

  /// `staged_validator_keys` must never exceed the worst-case staging
  /// length (`MaxStagedValidatorKeys = 1023`).
  val staged_validator_keys_bounded: bool =
    svc.stagedValidatorKeys.length() <= MaxStagedValidatorKeys

  /// Every batched `AccumulateLogEntry` in a parachain's log is non-empty.
  /// `appendAccumulateLogs` short-circuits an empty `entries` list; this pins
  /// that. The batch's total size is bounded by the 64 KiB byte cap
  /// (`parachain_log_within_capacity`), not a separate event count.
  val accumulate_log_batch_nonempty: bool =
    svc.parachainLog.keys().forall(p =>
      svc.paraLog(p).foldl(true, (acc, pair) =>
        acc and match pair._2 {
          | AccumulateLogEntry(evs) => evs != List()
          | RefineLogEntry(_) => true
        }))

  /// Every para's log total encoded size stays within the 64 KiB byte cap.
  val parachain_log_within_capacity: bool =
    svc.parachainLog.keys().forall(p =>
      parachainLogSize(svc.paraLog(p)) <= ParachainLogByteCap)

  /// §5.1 lookup-anchor pruning: every entry left in a parachain's log has a
  /// timeslot `>= logPrunedBelow[para]`, the lookup-anchor it was last pruned
  /// against (0 if never pruned). This holds through byte-cap eviction too,
  /// which only removes entries (oldest-first) and never lowers a timeslot.
  val parachain_log_above_lookup: bool =
    svc.parachainLog.keys().forall(p => {
      val threshold =
        if (logPrunedBelow.keys().contains(p)) logPrunedBelow.get(p) else 0
      svc.paraLog(p).select(e => e._1 < threshold).length() == 0
    })

  /// The ghost `preimageStatus` (JAM `query`) tracks exactly the solicited
  /// preimages: its domain always equals `preimage_registry`'s. Catches any
  /// path that drops one map but not the other.
  val preimage_status_domain: bool =
    svc.preimageStatus.keys() == svc.preimageRegistry.keys()

  /// An `Unrequested(_)` entry has exactly one referencer: the retained last
  /// one awaiting the expunging second `forget` (§6.1).
  val unrequested_is_singleton_retention: bool =
    svc.preimageRegistry.keys().forall(key =>
      match svc.preimageStatus.get(key) {
        | Unrequested(_) => svc.preimageRegistry.get(key).referencers.size() == 1
        | _ => true
      })

  /// §8.3 settlement-ring shape. For every para with a cursor, the window is
  /// within `SpecMsgRingCapacity` and `specMsgQueue` holds exactly positions
  /// `tail .. head-1`; every queue or member entry belongs to a para with a
  /// cursor; membership is exactly the set of roots in the live window, each
  /// member entry pointing at a live queue position that holds its root
  /// (the newest one, per the §8.3 re-push rule).
  val spec_msg_ring_coherent: bool = and {
    svc.specMsgCursor.keys().forall(p => {
      val c = svc.specMsgCursor.get(p)
      and {
        c.tail <= c.head,
        c.head - c.tail <= SpecMsgRingCapacity,
        svc.specMsgQueue.keys().filter(k => k._1 == p)
          == c.tail.to(c.head - 1).map(i => (p, i))
      }
    }),
    svc.specMsgQueue.keys().forall(k => svc.specMsgCursor.keys().contains(k._1)),
    svc.specMsgMember.keys().forall(k => svc.specMsgCursor.keys().contains(k._1)),
    svc.specMsgMember.keys().forall(k => {
      val seq = svc.specMsgMember.get(k).seq
      svc.specMsgQueue.keys().contains((k._1, seq)) and
      svc.specMsgQueue.get((k._1, seq)) == k._2
    }),
    svc.specMsgQueue.keys().forall(k =>
      svc.specMsgMember.keys().contains((k._1, svc.specMsgQueue.get(k))))
  }

  /// §8.3: a settlement ring exists only for a live parachain. Teardown runs
  /// inside `parachainCleanUp`, so a removed para must leave no ring behind
  /// (a live para with an EMPTY ring is fine: forced `set_head` clears it).
  val spec_msg_ring_only_for_live: bool =
    svc.specMsgCursor.keys().forall(p => svc.parachains.keys().contains(p))

  /// Composite invariant: every invariant above must hold simultaneously.
  val invariants: bool = and {
    balance_invariant,
    preimage_referencer_consistency,
    preimage_status_domain,
    unrequested_is_singleton_retention,
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
    head_commitment_matches_changed_heads,
    foreign_supervisor_is_us_or_self,
    foreign_excludes_self,
    parent_head_continuity,
    refine_error_logged,
    incoming_transfer_count_consistent,
    incoming_transfers_buckets_consistent,
    incoming_transfer_bucket_bounded,
    pending_authorizer_queue_bounded,
    pending_authorizer_cores_consistent,
    pending_authorizer_apply_at_future,
    assign_calls_well_formed,
    staged_validator_keys_bounded,
    accumulate_log_batch_nonempty,
    parachain_log_within_capacity,
    parachain_log_above_lookup,
    spec_msg_ring_coherent,
    spec_msg_ring_only_for_live
  }
}
