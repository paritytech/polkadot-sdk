// State-invariant catalogue for the Parachain Service spec.
//
// Each invariant is a nullary `val: bool` reading the shared state vars
// declared in `state_vars.qnt`. The CLI flag `--invariant=<name>` targets
// any of them directly.
module invariants {
  import types.* from "./types"
  import messages.* from "./messages"
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

  /// The last step's work results whose effects it kept: those before the
  /// package during which it ran out of gas (§5.1).
  val lastStepKeptWorkResults =
    match lastStepOutOfGas {
      | InPackage(i) => lastStepWorkResults.slice(0, i)
      | _ => lastStepWorkResults
    }

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
      else lastStepKeptWorkResults.foldl(false, (acc, r) => match r.result {
        | WorkErr => acc
        | WorkOk(digest) =>
          match digest {
            | Ok(ok) =>
              val direct = ok.paraId == p
                and ok.headData == svc.paraInfo(p).headData
              val viaMsg = ok.upwardMessages.foldl(false, (a, m) => match m._2 {
                | ParachainSetHead(payload) =>
                  a or (payload.paraId == p
                    and payload.newHead == svc.paraInfo(p).headData)
                | _ => a
              })
              acc or direct or viaMsg
            | Err(_) => acc
          }
      }))

  /// Helpers for `parent_head_continuity`.
  pure def cleanUpCount(oks: List[ParachainWorkDigestOk], q: ParaId): int =
    oks.foldl(0, (n, ok) => ok.upwardMessages.foldl(n, (c, m) => match m._2 {
      | ParachainCleanUp(x) => if (x == q) c + 1 else c
      | _ => c
    }))
  pure def reRegistered(oks: List[ParachainWorkDigestOk], q: ParaId): bool =
    oks.foldl(false, (b, ok) => ok.upwardMessages.foldl(b, (c, m) => match m._2 {
      | ParachainSetStateBalance(pl) => c or pl.paraId == q
      | _ => c
    }))
  /// The paras whose head or ring a candidate's upward messages can change: the
  /// targets of its `parachain_set_head` and `parachain_clean_up` messages.
  pure def headTargets(ok: ParachainWorkDigestOk): Set[ParaId] =
    ok.upwardMessages.foldl(Set(), (t, m) => match m._2 {
      | ParachainSetHead(pl) => t.union(Set(pl.paraId))
      | ParachainCleanUp(q) => t.union(Set(q))
      | _ => t
    })
  pure def dropHeads(h: ParaId -> HeadData, qs: Set[ParaId]): ParaId -> HeadData =
    h.keys().exclude(qs).mapBy(k => h.get(k))
  /// The paras whose active validation code a candidate's upward messages can
  /// change: the target of a `parachain_set_validation_code`, and its own para
  /// for an `Apply` (§5.2). An `Announcement` never moves the active code.
  pure def codeTargets(ok: ParachainWorkDigestOk): Set[ParaId] =
    ok.upwardMessages.foldl(Set(), (t, m) => match m._2 {
      | ParachainSetValidationCode(pl) => t.union(Set(pl.paraId))
      | RequestCodeUpgrade(pl) => match pl.phase {
          | Apply => t.union(Set(ok.paraId))
          | Announcement => t
        }
      | _ => t
    })
  pure def codeChangeCount(oks: List[ParachainWorkDigestOk], q: ParaId): int =
    oks.foldl(0, (n, ok) => ok.upwardMessages.foldl(n, (c, m) => match m._2 {
      | ParachainSetValidationCode(pl) => if (pl.paraId == q) c + 1 else c
      | RequestCodeUpgrade(pl) => match pl.phase {
          | Apply => if (ok.paraId == q) c + 1 else c
          | Announcement => c
        }
      | _ => c
    }))
  /// Re-derive the known code of each of `qs` after a candidate that may change
  /// it: `settled` holds the end-of-step code of every para whose code only one
  /// message in the block can change; any other target's code is forgotten.
  pure def recode(
    c: ParaId -> Option[ValidationCodeHash], qs: Set[ParaId],
    settled: ParaId -> Option[ValidationCodeHash]
  ): ParaId -> Option[ValidationCodeHash] =
    qs.fold(c, (m, q) =>
      if (settled.keys().contains(q)) m.put(q, settled.get(q))
      else m.keys().exclude(Set(q)).mapBy(k => m.get(k)))

  /// The replay state of `parent_head_continuity`. `heads` holds the heads it
  /// still tracks and `rings` the settlement rings. `unknown` holds the paras
  /// whose head and ring it can no longer predict: their heads are dropped, and
  /// whether their candidates, or candidates requiring one of their roots,
  /// enact is unknown. `frozen` holds the paras known to be deregistering at
  /// this point of the block. `codes` holds the active validation code hashes
  /// it still knows (§5.1 step 2). A code change is classified from the
  /// end-of-step code when it is the target's only one in the block and the
  /// target is neither removed nor re-registered; any other leaves the target
  /// missing from `codes`. `codeSets` records, for `validation_code_matches_outcomes`,
  /// the code each `parachain_set_validation_code` of an accepted Coretime
  /// candidate set on a target that was tracked, live and not deregistering,
  /// with the candidate's position in `stepOkDigests` and the call's `MessageId`.
  type ContinuityReplay = {
    heads: ParaId -> HeadData,
    rings: ParachainServiceState,
    unknown: Set[ParaId],
    frozen: Set[ParaId],
    codes: ParaId -> Option[ValidationCodeHash],
    codeSets: ParaId -> (int, MessageId, ValidationCodeRef)
  }

  /// A candidate the replay cannot decide may have enacted, so its own para and
  /// every para it forces a head on or cleans up become unknown, and their heads
  /// are dropped. A frozen para keeps its head unless the candidate cleans it
  /// up: a forced head on it is a no-op either way, and only a clean-up of it
  /// can remove it so that a re-registration gives it a fresh head. Every para
  /// whose code the candidate may change has its code re-derived (`recode`).
  pure def markUnknown(
    acc: ContinuityReplay, ok: ParachainWorkDigestOk,
    settled: ParaId -> Option[ValidationCodeHash]
  ): ContinuityReplay = {
    val touched = Set(ok.paraId).union(headTargets(ok))
    val cleanedUp = ok.upwardMessages.foldl(Set(), (t, m) => match m._2 {
      | ParachainCleanUp(q) => t.union(Set(q))
      | _ => t
    })
    val kept = acc.frozen.exclude(cleanedUp)
    {
      ...acc,
      heads: dropHeads(acc.heads, touched.exclude(kept)),
      unknown: acc.unknown.union(touched),
      codes: recode(acc.codes, codeTargets(ok), settled)
    }
  }

  /// Parent-head continuity (§5.1 step 4): replay the block's head-affecting
  /// operations in order and confirm the result matches the stored heads. Start
  /// from the pre-block heads, with the deregistering paras frozen. Only the
  /// step's kept work results count: a package rolled back by running out of
  /// gas, and every later one, never took effect (§5.1). For each kept
  /// `Ok` work result, apply it ONLY if its para is live and not frozen (step 1,
  /// §6.4), it names its para's active code (step 2), its declared parent matches that para's current head (step 4), and
  /// its `messages_requires_roots` settle against the rings as of that digest
  /// (step 5, §8). If accepted, write its new head and push its
  /// `messages_streams_root` (step 6), then replay its upward messages (step 7):
  /// a `parachain_set_head` moves its para's head unless that para is frozen,
  /// and leaves every ring alone (§8.1). A rejected candidate is ignored
  /// entirely (its head update, its push, AND its upward messages), just as
  /// Accumulate never applies a rejected candidate. This is what lets a forced
  /// `parachain_set_head` survive a same-block candidate built on a superseded
  /// head, while a candidate legitimately continuing the current head is allowed.
  /// A `parachain_clean_up(q)` is classified from the end-of-step state when it
  /// is q's only clean-up in the block and nothing re-registers q: q gone means
  /// fully removed (drop its head and delete its ring, §8.1 "Ring Cleanup"), q
  /// deregistering means partial (freeze q, keep its head and ring), q still
  /// live means rejected (no change). Any other clean-up makes q unknown. A
  /// candidate the replay would otherwise apply, but whose para is unknown, whose
  /// para's code is no longer known (one of several code changes to it this
  /// block), or which requires a root of an unknown para, goes
  /// through `markUnknown`.
  /// Frozen paras stay in the end-of-step comparison, so nothing may move a
  /// deregistering para's head before it is removed; one removed and
  /// re-registered (§6.2) in this step is a fresh entry, and unknown.
  val parent_head_continuity: bool =
    continuityReplay.heads.keys().forall(p =>
      if (not(svc.parachains.keys().contains(p))) true
      else continuityReplay.heads.get(p) == svc.paraInfo(p).headData)

  /// The `Ok` digests of the last step's kept work results, in order. A package
  /// rolled back by running out of gas (§5.1) takes no effect, so neither the
  /// replay nor the code checks see it.
  val stepOkDigests: List[ParachainWorkDigestOk] =
    lastStepKeptWorkResults.foldl(List(), (l, r) => match r.result {
      | WorkErr => l
      | WorkOk(d) => match d { | Ok(ok) => l.append(ok) | Err(_) => l }
    })

  /// The replay behind `parent_head_continuity` (documented there).
  val continuityReplay: ContinuityReplay = {
    val okDigests = stepOkDigests
    val initial: ContinuityReplay = {
      heads: prevSvc.parachains.keys().mapBy(p => prevSvc.paraInfo(p).headData),
      rings: prevSvc,
      unknown: Set(),
      frozen: prevSvc.parachains.keys().filter(p => prevSvc.paraInfo(p).isDeregistering),
      codes: prevSvc.parachains.keys().mapBy(p =>
        prevSvc.paraInfo(p).validationCode.optionMap(vc => vc.hash)),
      codeSets: Map()
    }
    val settled = okDigests.foldl(Set(), (t, ok) => t.union(codeTargets(ok)))
      .filter(q => codeChangeCount(okDigests, q) == 1 and
        svc.parachains.keys().contains(q) and not(reRegistered(okDigests, q)))
      .mapBy(q => svc.paraInfo(q).validationCode.optionMap(vc => vc.hash))
    val replayed = range(0, okDigests.length()).foldl(initial, (acc, i) => {
      val ok = okDigests[i]
      if (acc.unknown.contains(ok.paraId)) markUnknown(acc, ok, settled)
      // Removed or deregistering: Accumulate drops it (step 1).
      else if (not(acc.heads.keys().contains(ok.paraId)) or acc.frozen.contains(ok.paraId)) acc
      // Names a code other than its para's known active code (step 2).
      else if (acc.codes.keys().contains(ok.paraId) and
          acc.codes.get(ok.paraId) != Some(ok.validationCode)) acc
      else if (ok.parentHeadHash != headHash(acc.heads.get(ok.paraId))) acc
      else if (not(acc.codes.keys().contains(ok.paraId))) markUnknown(acc, ok, settled)
      else if (ok.messagesRequiresRoots.foldl(false, (b, e) => b or acc.unknown.contains(e._1)))
        markUnknown(acc, ok, settled)
      else if (not(settlementCheck(acc.rings, ok.messagesRequiresRoots))) acc
      else {
        // Accepted: write the new head and push the candidate's
        // `messages_streams_root` (step 6), then replay its upward messages
        // (step 7): forced heads, which leave every ring alone (§8.1), and
        // clean-ups, classified as above.
        val pushed = match ok.messagesStreamsRoot {
          | Some(root) => settlementRingPush(acc.rings, ok.paraId, root)
          | None => acc.rings
        }
        // Its code changes (step 7) are re-derived up front: only the
        // candidate-level checks read `codes`.
        ok.upwardMessages.foldl(
          {
            ...acc,
            heads: acc.heads.put(ok.paraId, ok.headData),
            rings: pushed,
            codes: recode(acc.codes, codeTargets(ok), settled)
          }, (a, m) =>
          match m._2 {
            | ParachainSetHead(pl) =>
              if (a.heads.keys().contains(pl.paraId) and not(a.frozen.contains(pl.paraId)))
                { ...a, heads: a.heads.put(pl.paraId, pl.newHead) }
              else a
            | ParachainCleanUp(q) =>
              if (cleanUpCount(okDigests, q) != 1 or reRegistered(okDigests, q))
                { ...a, heads: dropHeads(a.heads, Set(q)), unknown: a.unknown.union(Set(q)) }
              else if (not(svc.parachains.keys().contains(q)))
                { ...a, heads: dropHeads(a.heads, Set(q)), rings: settlementRingDelete(a.rings, q) }
              else if (svc.paraInfo(q).isDeregistering) { ...a, frozen: a.frozen.union(Set(q)) }
              else a
            | ParachainSetValidationCode(pl) =>
              if (ok.paraId == CoretimeParaId and a.heads.keys().contains(pl.paraId)
                  and not(a.frozen.contains(pl.paraId)))
                { ...a, codeSets: a.codeSets.put(pl.paraId, (i, m._1, pl.newValidationCode)) }
              else a
            | _ => a
          })
      }
    })
    replayed
  }

  /// Whether the `parachain_set_validation_code` sent with `id` by the `k`-th
  /// `Ok` digest of the step was rejected for lack of headroom (§6.3): that
  /// candidate's replay logged `InsufficientStateBalance` for `id`. That
  /// rejection is the only way the call leaves a live, non-deregistering para's
  /// code unchanged. Coretime's `parachain_log` alone cannot tell: its other
  /// candidates of the step log at the same timeslot, may reuse `id`, and only
  /// those that log anything append an entry, so neither the id nor the entry's
  /// position names the candidate. `lastStepOkLogs` keeps each candidate's
  /// events apart. Every generator sends a candidate's messages with distinct
  /// ids, so `id` names the call within the candidate.
  pure def setCodeMayHaveFailed(
    okLogs: List[List[(MessageId, AccumulateLog)]], k: int, id: MessageId
  ): bool =
    if (k >= okLogs.length()) false
    else okLogs[k].foldl(false, (x, l) => x or l == (id, InsufficientStateBalance))

  /// Validation-code outcomes (§5.1 step 7, §5.2, §6.3), the code counterpart of
  /// `head_state_matches_outcomes`. Checked against the end-of-step state, so a
  /// model that gets a code change's outcome wrong cannot hide it behind
  /// `parent_head_continuity`, which takes that outcome from the end-of-step state.
  /// 1. Every change of a para's `validation_code` over the step (para neither
  ///    removed nor re-registered) is claimed by an `Ok` digest: a Coretime
  ///    `parachain_set_validation_code` of exactly that code, or an `Apply` from
  ///    the para itself of code that was announced, before the step or by an
  ///    `Announcement` of the para in it (§5.2: an unannounced `Apply` is rejected).
  /// 2. A `parachain_set_validation_code(q, c)` that the replay accepted while q
  ///    was live and not deregistering, and that is q's only code change in the
  ///    block, DID set q's code to c unless its candidate logged
  ///    `InsufficientStateBalance` for it (§6.3).
  val validation_code_matches_outcomes: bool = {
    // Guarded explicitly because Quint's `or` is eager (`svc.paraInfo` crashes
    // on a removed para).
    def tracked(q: ParaId): bool = svc.parachains.keys().contains(q) and
      not(reRegistered(stepOkDigests, q))
    and {
      prevSvc.parachains.keys().forall(p =>
        if (not(tracked(p))) true
        else svc.paraInfo(p).validationCode == prevSvc.paraInfo(p).validationCode
          or codeChangeClaimed(stepOkDigests, prevSvc, p, svc.paraInfo(p).validationCode)),
      continuityReplay.codeSets.keys().forall(q => {
        val cs = continuityReplay.codeSets.get(q)
        if (not(tracked(q)) or codeChangeCount(stepOkDigests, q) != 1) true
        else svc.paraInfo(q).validationCode == Some(cs._3) or
          setCodeMayHaveFailed(lastStepOkLogs, cs._1, cs._2)
      })
    }
  }

  /// Whether an `Ok` digest of the step claims `p`'s new code `vc` (see
  /// `validation_code_matches_outcomes`, part 1).
  pure def codeChangeClaimed(
    oks: List[ParachainWorkDigestOk], prev: ParachainServiceState, p: ParaId,
    vc: Option[ValidationCodeRef]
  ): bool =
    oks.foldl(false, (b, ok) => ok.upwardMessages.foldl(b, (x, m) => match m._2 {
      | ParachainSetValidationCode(pl) =>
        x or (ok.paraId == CoretimeParaId and pl.paraId == p and vc == Some(pl.newValidationCode))
      | RequestCodeUpgrade(pl) => match pl.phase {
          | Apply =>
            val c = { hash: pl.hash, len: pl.len }
            x or (ok.paraId == p and vc == Some(c) and
              (prev.paraInfo(p).announcedUpgrade == Some(c) or announcedIn(oks, p, c)))
          | Announcement => x
        }
      | _ => x
    }))

  /// Whether one of `p`'s `Ok` digests announces `c` (§5.2).
  pure def announcedIn(
    oks: List[ParachainWorkDigestOk], p: ParaId, c: ValidationCodeRef
  ): bool =
    oks.foldl(false, (b, ok) => ok.upwardMessages.foldl(b, (x, m) => match m._2 {
      | RequestCodeUpgrade(pl) => match pl.phase {
          | Announcement => x or (ok.paraId == p and { hash: pl.hash, len: pl.len } == c)
          | Apply => x
        }
      | _ => x
    }))

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
  /// Non-emptiness is what makes `assignQueueFor`'s modulo well-defined; an
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
  /// entry or re-arms it 80 slots after its due slot. A due entry may only
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
    validation_code_matches_outcomes,
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
