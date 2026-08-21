// Refine: in-core, stateless PVF execution (design §4).
//
// We do NOT model PVF semantics; instead we model Refine as a non-
// deterministic producer of `ParachainWorkDigest`s subject to the
// structural rules from §4.1–§4.3:
//
// - The `paraId` of each work-item is sourced from the authorizer config's
// `authorized_paras` prefix, not from the extrinsic.
// - The PVF must call `set_parent_head_hash` exactly once (modeled as a
// well-formedness check on `Ok` results).
// - Refine returns `Err(InvalidCodeHash)` iff `historical_lookup` for the
// candidate's `validation_code_hash` returns None.
// - `set_validator_keys` may appear at most once in the emitted
//   `upwardMessages` list, and any chunk must contain at most 30 keys.
//
// The `parent_head_hash` field is mandatory but is *not* validated against
// service state here; that check lives in Accumulate (§5.1, step 1).
module refine {
  import types.* from "./types"
  import messages.* from "./messages"
  import state.* from "./state"

  /// Per-work-package extrinsic: one parachain candidate per item (design §3.2).
  type ParachainCandidate = {
    validationCode: ValidationCodeRef,
    // PoV is opaque; the model treats it as a nondeterministic input.
    pov: int
  }

  type WorkPackage = {
    // Authorizer config prefix binding work-item index → ParaId (design §3.2, §7.1).
    authorizedParas: List[ParaId],
    // One candidate per work item. Currently `length(items) == 1` per design §3.2.
    items: List[ParachainCandidate]
  }

  // --- Upward-message validity rules (Refine-side enforcement) ------------

  /// Per §4.3: `set_validator_keys` may appear at most once per Refine.
  pure def setValidatorKeysCount(msgs: List[UpwardMessage]): int =
  msgs.foldl(0, (acc, m) => match m {
      | SetValidatorKeys(_) => acc + 1
      | _ => acc
  })

  /// Per §4.3: each `set_validator_keys` chunk contains at most 30 keys.
  pure def setValidatorKeysChunkOk(msgs: List[UpwardMessage]): bool =
  msgs.foldl(true, (acc, m) => match m {
      | SetValidatorKeys(payload) =>
      acc and (payload.keys.length() <= SetValidatorKeysMaxKeys)
      | _ => acc
  })

  /// Per §4.3: an `assign_core` queue holds 1 to `AuthorizerQueueLength` hashes.
  /// A queue that hands the core to another service must be exactly
  /// `AuthorizerQueueLength`: the service can no longer re-present a short one
  /// after the handoff, so the installed queue has to cycle on its own.
  pure def wellFormedAssignQueues(msgs: List[UpwardMessage]): bool =
  msgs.foldl(true, (acc, m) => match m {
    | AssignCore(p) => acc and (match p.newAssigner {
        | None => p.queue.length() > 0 and p.queue.length() <= AuthorizerQueueLength
        | Some(_) => p.queue.length() == AuthorizerQueueLength
      })
    | _ => acc
  })

  /// Combined Refine-side well-formedness for emitted upward messages.
  pure def wellFormedUpwardMessages(msgs: List[UpwardMessage]): bool =
  setValidatorKeysCount(msgs) <= 1 and
  setValidatorKeysChunkOk(msgs)

  /// Actual SCALE-encoded size of one upward message, used by the 48 KiB
  /// result-blob budget check (§4.1 step 7). Every variant is measured from
  /// its real payload, except `ParachainSetHead`: `HeadData` is opaque in the
  /// model (an `int`, not a byte vector), so its `newHead` has no measurable
  /// length and is charged its worst case (2 B compact len + 4096 B max).
  // 1 B enum discriminant (the message tag) + the variant's payload size.
  pure def upwardMessageSize(m: UpwardMessage): int =
  1 + match m {
    // hash 32 + Compact<u32> len.
    | RequestCodeUpgrade(p) => 32 + compactLen(p.len)
    | Solicit(p) => 32 + compactLen(p.len)
    // paraId 4 + hash 32 + Compact<u32> len.
    | Forget(p) => 36 + compactLen(p.len)
    | SetKV(p) =>
      compactLen(p.key.length()) + p.key.length()
        + compactLen(p.value.length()) + p.value.length()
    // paraId 4 + Vec<u8> key (compact len prefix + bytes).
    | RemoveKV(p) => 4 + compactLen(p.key.length()) + p.key.length()
    // dest 4 + two balance selectors 2 + amount Compact<u64> + id Compact<u64>
    // + Option<ServiceId> source (1 tag + 4 when Some)
    // + Option<(Memo, gas)> (1 tag + 128 memo + 8 gas when Some).
    | TransferOut(p) =>
      6 + compactLen(p.amount) + compactLen(p.id)
        + (match p.source { | Some(_) => 5 | None => 1 })
        + (match p.deferred { | Some(_) => 137 | None => 1 })
    // core 4 + jamSlot 4 + Vec<AuthorizerHash> (compact len + 32 per entry)
    // + Option<ServiceId> (1 tag + 4 when Some).
    | AssignCore(p) =>
      8 + compactLen(p.queue.length()) + p.queue.length() * 32
        + (match p.newAssigner { | Some(_) => 5 | None => 1 })
    // Vec<ValidatorKey> (compact len + 336 per key) + isLast 1.
    | SetValidatorKeys(p) =>
      1 + compactLen(p.keys.length()) + p.keys.length() * 336
    | ConsumeTransfersUpTo(_) => 4
    | ParachainSetHead(_) => 4102
    // paraId 4 + hash 32 + Compact<u32> len.
    | ParachainSetValidationCode(p) => 36 + compactLen(p.newValidationCodeLen)
    | ParachainCleanUp(_) => 4
    // paraId 4 + newTotal Compact<u64>.
    | ParachainSetStateBalance(p) => 4 + compactLen(p.newTotal)
    // codeHash 32 + Compact<u32> len + minAccGas 8 + minMemoGas 8.
    | UpgradeService(p) => 48 + compactLen(p.len)
  }

  /// `true` iff the work result (`paraId` + `vch` + `parentHeadHash` +
  /// worst-case `head_data` + upward messages) plus a worst-case auth
  /// trace would exceed `MaxReportVarSize`.
  pure def resultExceedsBudget(msgs: List[UpwardMessage]): bool = {
    val headDataMax = 4098    // BoundedVec<u8, 4096> worst case
    val authTraceMax = 256
    val resultBase = 4 + 32 + 32 + headDataMax
    val msgsSize = msgs.foldl(2, (acc, m) => acc + upwardMessageSize(m))
    resultBase + msgsSize + authTraceMax > MaxReportVarSize
  }

  // --- Abstract Refine ----------------------------------------------------
  //
  // `refineItem` models §4.1 abstractly: given a candidate and the service's
  // view of the active+pending validation code for that para, produce
  // either an `Ok` with a list of upward messages (any from §4.3 that the
  // PVF chose to emit, subject to the well-formedness rules above) or an
  // `Err(InvalidCodeHash)` when the lookup fails.
  //
  // For the model, we nondeterministically pick the upward messages in
  // the spec's top-level `step` action; see `main.qnt`.

  pure def refineErr(paraId: ParaId, e: RefineLog): ParachainWorkDigest =
  Err({ paraId: paraId, error: e })

  pure def refineOk(
    paraId: ParaId,
    ref: ValidationCodeRef,
    parentHead: HeadHash,
    newHead: HeadData,
    msgs: List[UpwardMessage],
    lookupAnchor: Timeslot
  ): ParachainWorkDigest =
  Ok({
      paraId: paraId,
      validationCode: ref,
      parentHeadHash: parentHead,
      headData: newHead,
      upwardMessages: msgs,
      lookupAnchor: lookupAnchor
  })

  /// `true` iff the code identified by `ref` has a non-empty referencer set
  /// in the preimage registry, i.e. the `(hash, len)` tuple key exists.
  pure def codeAvailable(s: ParachainServiceState, ref: ValidationCodeRef): bool = {
    val key = (vchAsHash(ref.hash), ref.len)
    s.preimageRegistry.keys().contains(key) and
    s.preimageRegistry.get(key).referencers != Set()
  }

  /// How the PVF terminated (design §4.2). The service treats these differently:
  ///   - `PvfOk`: ran to completion; the wrapper's own checks decide the outcome.
  ///   - `PvfReportError(payload)`: the PVF aborted itself with
  ///     `report_error(data)`. That call IS the abort, and it is the only way a
  ///     PVF records a reason for its failure (`RefineLog::Opaque`).
  ///   - `PvfPanic`: any other abnormal exit. Deliberately NOT caught, so the
  ///     service's whole `refine` fails with it and there is no
  ///     `ParachainWorkDigest` at all; the work-digest result is a gray-paper
  ///     work error, which Accumulate skips entirely (§3.3).
  type PvfOutcome =
    | PvfOk
    | PvfReportError(int)
    | PvfPanic

  /// Per-item Refine semantics (§4.1). The PVF is modeled as an oracle whose
  /// visible effects are how it terminated (`pvf`), the `set_parent_head_hash`
  /// and `set_head` declarations (`parentHead` / `newHead`, both `None` if the
  /// PVF never called them, §4.2), and the emitted upward `msgs`. The wrapper
  /// applies the §4.1/§4.3 checks in order and otherwise forwards the
  /// declarations verbatim; the authoritative parent-head match and the head
  /// write happen in Accumulate (§5.1).
  ///
  /// `None` means the invocation produced no digest at all (`PvfPanic`).
  ///
  /// Refine is stateless: it reads no `ParaInfo` (registration is checked in
  /// Accumulate, §5.1 step 1); the only state it touches is the preimage
  /// store, via `codeAvailable`.
  pure def refineItem(
    s: ParachainServiceState,
    para_id: ParaId,
    candidate: ParachainCandidate,
    pvf: PvfOutcome,
    parentHead: Option[HeadHash],
    newHead: Option[HeadData],
    msgs: List[UpwardMessage],
    lookupAnchor: Timeslot
  ): Option[ParachainWorkDigest] = {
    if (not(codeAvailable(s, candidate.validationCode)))
    Some(refineErr(para_id, InvalidCodeHash))
    else
    match pvf {
      | PvfPanic => None
      | PvfReportError(payload) => Some(refineErr(para_id, Opaque(payload)))
      | PvfOk =>
        if (msgs.length() > MaxUpwardMessages)
        Some(refineErr(para_id, TooManyUpwardMessages))
        else if (not(wellFormedUpwardMessages(msgs)))
        Some(refineErr(para_id, SetValidatorKeysTooManyKeys))
        else if (not(wellFormedAssignQueues(msgs)))
        Some(refineErr(para_id, InvalidAuthorizerQueue))
        else if (violatesParachainRestrictions(msgs, para_id))
        Some(refineErr(para_id, RestrictedHostFunction))
        else if (resultExceedsBudget(msgs))
        Some(refineErr(para_id, RefineOutputTooLarge))
        else
        // `set_parent_head_hash` / `set_head` are both mandatory (§4.2): a
        // missing declaration (`None`) is rejected with `MissingHeadDeclaration`.
        match parentHead {
          | None => Some(refineErr(para_id, MissingHeadDeclaration))
          | Some(ph) =>
            match newHead {
              | None => Some(refineErr(para_id, MissingHeadDeclaration))
              | Some(nh) =>
                Some(refineOk(para_id, candidate.validationCode, ph, nh, msgs, lookupAnchor))
            }
        }
    }
  }

  /// Per-work-package Refine entry point (§4.1).
  ///
  /// Every package-level failure panics, because `para_id` is only authoritative
  /// from §4.1 step 2 onward and a `RefineLog` with no para to land in is
  /// useless. Otherwise each item is refined via `refineItem`, with its
  /// `para_id` sourced from `authorized_paras`.
  ///
  /// Single-item packages only (per §3.2): `pvf`, `parentHead`, `newHead`, and
  /// `msgs` are per-item scalars declared by the PVF. `parentHead` / `newHead`
  /// are `None` when the PVF skipped the corresponding mandatory host call
  /// (§4.2).
  pure def refine(
    s: ParachainServiceState,
    pkg: WorkPackage,
    // The config parsed for the authorizer but carries no `Vec<ParaId>` prefix,
    // so the service cannot source a `para_id` from it (§4.1 step 1).
    authParasPrefixMissing: bool,
    pvf: PvfOutcome,
    parentHead: Option[HeadHash],
    newHead: Option[HeadData],
    msgs: List[UpwardMessage],
    lookupAnchor: Timeslot
  ): List[WorkExecResult] = {
    // Package-level failures are settled before §4.1 step 2 fixes an
    // authoritative `para_id`, so none can name a para to log against and all
    // panic (§4.2): a missing `Vec<ParaId>` prefix, a package that is not
    // single-item (§3.2), and an `authorized_paras` length that disagrees with
    // the item count. A config that will not decode at all never gets here:
    // `is_authorized` fails on it first, so it never becomes a work report (§7).
    if (authParasPrefixMissing
        or pkg.items.length() != 1
        or pkg.authorizedParas.length() != pkg.items.length())
    List(WorkErr)
    else
    match refineItem(
      s, pkg.authorizedParas[0], pkg.items[0],
      pvf, parentHead, newHead, msgs, lookupAnchor
    ) {
      | Some(d) => List(WorkOk(d))
      | None => List(WorkErr)
    }
  }
}
