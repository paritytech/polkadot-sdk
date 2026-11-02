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
      svc.preimageRegistry.get(key).referencers != Set())

  /// `designate` is invoked iff Accumulate just processed a
  /// `SetValidatorKeys { isLast: true }` whose assembled length ∈ ValCount.
  /// We over-approximate by checking the stagingset, when non-empty, is in
  /// `ValCount`.
  val designate_only_valid_lengths: bool =
    jamStagingSet == List() or ValCount.contains(jamStagingSet.length())

  /// An announced upgrade requires an active validation code to upgrade FROM,
  /// and must differ from it. A para with no validation code has every candidate
  /// rejected (§5.1 step 2), so it never reaches the replay that could announce.
  val announcement_requires_different_active_code: bool =
    svc.parachains.keys().forall(para_id => {
      val pi = svc.paraInfo(para_id)
      match pi.announcedUpgrade {
        | None => true
        | Some(announced) =>
          match pi.validationCode {
            | None => false
            | Some(vc) => announced != vc
          }
      }
    })

  /// Staged validator keys live entirely on Asset Hub's books: if the
  /// buffer is non-empty, Asset Hub must be live and have room for it.
  val staged_keys_owned_by_asset_hub: bool =
    svc.stagedValidatorKeys == List() or (svc.isLive(AssetHubParaId) and
      svc.paraInfo(AssetHubParaId).usedStateBalance >=
        ValidatorKeySize * svc.stagedValidatorKeys.length())

  /// Every live parachain's active validation code (when set) is a
  /// referencer in `preimage_registry`. `None` is a freshly-registered para
  /// between `parachain_set_state_balance` and the first
  /// `parachain_set_validation_code` in the registration sequence (see §6.2).
  /// A deregistering para (§6.4) is exempt: clean-up already forgot its
  /// codes, and a rescue by another para drops it as referencer while its
  /// retained record still names them.
  val validation_code_in_registry: bool =
    svc.parachains.keys().forall(para_id => {
      val pi = svc.paraInfo(para_id)
      if (pi.isDeregistering) true
      else match pi.validationCode {
        | None => true
        | Some(vc) => svc.hasReferencer(vc.hash.asPreimage(), vc.len, para_id)
      }
    })

  /// Every live parachain's announced-upgrade code (when present) is also
  /// a referencer in `preimage_registry`, same lifecycle as any other
  /// solicited preimage (§5.2). Deregistering paras are exempt for the same
  /// reason as in `validation_code_in_registry`.
  val announced_code_in_registry: bool =
    svc.parachains.keys().forall(para_id => {
      val pi = svc.paraInfo(para_id)
      if (pi.isDeregistering) true
      else match pi.announcedUpgrade {
        | None => true
        | Some(announced) =>
          svc.hasReferencer(
            announced.hash.asPreimage(),
            announced.len,
            para_id
          )
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
      }))

  /// Parent-head continuity (§5.1 step 4): replay the block's head-affecting
  /// operations in order and confirm the result matches the stored heads. Start
  /// from the pre-block heads; for each `Ok` work result, apply it ONLY if its
  /// para is live, its declared parent matches that para's current head
  /// (step 4), and its `messages_requires_roots` settle against the rings as
  /// of that digest (step 5, §8). If accepted, write its new
  /// head and push its `messages_streams_root` (step 6), then replay its
  /// `parachain_set_head` messages (step 7), which move heads only and leave
  /// every ring alone (§8.1). A rejected candidate (stale parent or missed
  /// settlement) is ignored entirely (its head update, its push, AND its
  /// upward messages), just as Accumulate never applies a rejected candidate.
  /// This is what lets a forced `parachain_set_head` survive a same-block
  /// candidate built on a superseded head, while a candidate legitimately
  /// continuing the current head is allowed.
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
            else if (not(settlementCheck(acc._2, ok.messagesRequiresRoots))) acc
            else {
              // Accepted: write the new head and push the candidate's
              // `messages_streams_root` (step 6), then replay its forced
              // `parachain_set_head` messages (step 7), each a no-op for a
              // non-live target and none touching a ring (§8.1).
              val pushed = match ok.messagesStreamsRoot {
                | Some(root) => settlementRingPush(acc._2, ok.paraId, root)
                | None => acc._2
              }
              val heads = ok.upwardMessages.foldl(
                acc._1.put(ok.paraId, ok.headData), (h, m) =>
                match m {
                  | ParachainSetHead(pl) =>
                    if (h.keys().contains(pl.paraId)) h.put(pl.paraId, pl.newHead)
                    else h
                  | _ => h
                })
              (heads, pushed)
            }
        }
      })
    prevSvc.parachains.keys().forall(p =>
      if (not(svc.parachains.keys().contains(p))) true
      else replayed._1.get(p) == svc.paraInfo(p).headData)
  }

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
  /// empty queue is a no-op and is never cached (§3.3).
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

  /// A core whose `assign` fired last step is not left due: firing drops its
  /// entry or re-schedules a rotating queue 80 blocks out. A due entry may only
  /// remain cached when the gas ran out before reaching it (§5.1).
  val fired_assign_not_left_due: bool =
    lastStepAssigns.foldl(true, (acc, a) =>
      acc and (not(svc.pendingAssignCores.keys().contains(a.core)) or
        svc.pendingAssignCores.get(a.core) > now))

  /// Every `assign` recorded last step targets a valid core index (< CoreCount).
  /// (A structural sanity check that the assign-recording is well-formed.)
  val assign_calls_well_formed: bool =
    lastStepAssigns.foldl(true, (acc, a) =>
      acc and a.core >= 0 and a.core < CoreCount)

  /// No tracked JAM account is ever overdrawn: every spend is checked against
  /// the balance it draws on.
  val jam_balances_nonnegative: bool = and {
    svc.jamAccount.balance >= 0,
    svc.jamAccount.supervisorBalance >= 0,
    foreignServices.keys().forall(s =>
      foreignServices.get(s).account.balance >= 0 and
        foreignServices.get(s).account.supervisorBalance >= 0)
  }

  /// `staged_validator_keys` must never exceed the worst-case staging
  /// length (`MaxStagedValidatorKeys = 1023`).
  val staged_validator_keys_bounded: bool =
    svc.stagedValidatorKeys.length() <= MaxStagedValidatorKeys

  /// Every batched `Accumulate` in a parachain's log is non-empty.
  /// `appendAccumulateLogs` short-circuits an empty `entries` list; this pins
  /// that. The batch's total size is bounded by the 64 KiB byte cap
  /// (`parachain_log_within_capacity`), not a separate event count.
  val accumulate_log_batch_nonempty: bool =
    svc.parachainLog.keys().forall(p =>
      svc.paraLog(p).foldl(true, (acc, pair) =>
        acc and match pair._2 {
          | Accumulate(evs) => evs != List()
          | Refine(_) => true
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

  /// §8.1 settlement-ring shape. For every para with a cursor, `len` is in
  /// `1..MaxSettlementRingCapacity`; following `len` links from `oldest`
  /// visits `len` distinct roots and ends at `newest`, which links to itself;
  /// and the visited roots are exactly the para's `messagesMember` keys, so
  /// no entry is stranded off the list. Every member entry belongs to a para
  /// with a cursor.
  val settlement_ring_coherent: bool = and {
    svc.messagesCursor.keys().forall(p => {
      val c = svc.messagesCursor.get(p)
      if (c.len < 1 or c.len > MaxSettlementRingCapacity) false
      else {
        val roots = ringRoots(svc, p)
        val rootSet = roots.foldl(Set(), (acc, r) => acc.union(Set(r)))
        and {
          rootSet.size() == c.len,
          roots[c.len - 1] == c.newest,
          svc.messagesMember.keys().contains((p, c.newest)),
          svc.messagesMember.keys().filter(k => k._1 == p) == rootSet.map(r => (p, r)),
        }
      }
    }),
    svc.messagesCursor.keys().forall(p =>
      not(svc.messagesMember.keys().contains((p, svc.messagesCursor.get(p).newest)))
        or svc.messagesMember.get((p, svc.messagesCursor.get(p).newest))
          == svc.messagesCursor.get(p).newest),
    svc.messagesMember.keys().forall(k => svc.messagesCursor.keys().contains(k._1)),
  }

  /// §8.1: a settlement ring exists only for a live parachain. Deletion runs
  /// inside `parachainCleanUp`, so a removed para must leave no ring behind.
  /// (A live para without a cursor is fine: the cursor is absent until the
  /// para's first push, §8.1.)
  val settlement_ring_only_for_live: bool =
    svc.messagesCursor.keys().forall(p => svc.parachains.keys().contains(p))

  /// Composite invariant: every invariant above must hold simultaneously.
  val invariants: bool = and {
    balance_invariant,
    preimage_referencer_consistency,
    preimage_status_domain,
    unrequested_is_singleton_retention,
    designate_only_valid_lengths,
    announcement_requires_different_active_code,
    staged_keys_owned_by_asset_hub,
    validation_code_in_registry,
    announced_code_in_registry,
    solicit_implies_registry,
    used_above_baseline,
    used_balance_consistency,
    referencers_are_live,
    parachain_log_only_for_live,
    jam_balances_nonnegative,
    head_state_matches_outcomes,
    head_commitment_matches_changed_heads,
    foreign_supervisor_is_us_or_self,
    foreign_excludes_self,
    parent_head_continuity,
    incoming_transfer_count_consistent,
    incoming_transfers_buckets_consistent,
    incoming_transfer_bucket_bounded,
    pending_authorizer_queue_bounded,
    pending_authorizer_cores_consistent,
    fired_assign_not_left_due,
    assign_calls_well_formed,
    staged_validator_keys_bounded,
    accumulate_log_batch_nonempty,
    parachain_log_within_capacity,
    parachain_log_above_lookup,
    settlement_ring_coherent,
    settlement_ring_only_for_live
  }
}
