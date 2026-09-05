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

  /// Per §4.3: a `SetValidatorKeys` chunk holds at most
  /// `SetValidatorKeysMaxKeys` keys. Refine checks the length explicitly and
  /// rejects a longer chunk with `TooManyValidatorKeys`.
  pure def setValidatorKeysWithinCap(msgs: List[UpwardMessage]): bool =
  msgs.foldl(true, (acc, m) => match m {
    | SetValidatorKeys(payload) =>
    acc and (payload.keys.length() <= SetValidatorKeysMaxKeys)
    | _ => acc
  })

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
    // Target (1 tag + 4 id) + hash 32 + Compact<u32> len.
    | Solicit(p) => 37 + compactLen(p.len)
    // service 4.
    | EjectService(_) => 4
    // service 4 + newSupervisor 4.
    | SetServiceSupervisor(_) => 8
    // codeHash 32 + Compact<u32> len + two u64 gas limits 16 + id Compact<u64>
    // + Option<ServiceId> desiredId (1 tag + 4 when Some)
    // + two balance selectors 2.
    | CreateService(p) =>
      50 + compactLen(p.len) + compactLen(p.id)
        + (match p.desiredId { | Some(_) => 5 | None => 1 })
    // Target (1 tag + 4 id) + hash 32 + Compact<u32> len.
    | Forget(p) => 37 + compactLen(p.len)
    // service 4 + Vec<u8> key (compact len prefix + bytes).
    | RemoveServiceStorage(p) => 4 + compactLen(p.key.length()) + p.key.length()
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
    // bucket_id u64.
    | CleanUpBucketsUpTo(_) => 8
    | ParachainSetHead(_) => 4102
    // paraId 4 + hash 32 + Compact<u32> len.
    | ParachainSetValidationCode(p) => 36 + compactLen(p.newValidationCodeLen)
    | ParachainCleanUp(_) => 4
    // paraId 4 + newTotal Compact<u64>.
    | ParachainSetStateBalance(p) => 4 + compactLen(p.newTotal)
    // codeHash 32 + Compact<u32> len + minAccGas 8 + minMemoGas 8.
    | UpgradeService(p) => 48 + compactLen(p.len)
  }

  /// Summed SCALE-encoded size of the upward messages themselves, excluding the
  /// enclosing vector's length prefix.
  pure def upwardMessagesTotalSize(msgs: List[UpwardMessage]): int =
  msgs.foldl(0, (acc, m) => acc + upwardMessageSize(m))

  /// `true` iff the upward messages exceed the parachain's 40 KiB budget. The
  /// vector's length prefix is not charged against the budget.
  pure def exceedsUpwardMessagesBudget(msgs: List[UpwardMessage]): bool =
  upwardMessagesTotalSize(msgs) > MaxUpwardMessagesSize

  /// `true` iff the whole Refine output (`paraId` + `vch` + `parentHeadHash` +
  /// worst-case `head_data` + upward messages) plus a worst-case auth trace
  /// would exceed `MaxReportVarSize`. Backstop on the 48 KiB Gray Paper limit:
  /// `MaxUpwardMessagesSize` is chosen so this cannot trigger, so it fires only
  /// if that budget or one of the sizes below is ever set inconsistently.
  pure def refineOutputExceedsBudget(msgs: List[UpwardMessage]): bool = {
    val headDataMax = 4098    // BoundedVec<u8, 4096> worst case
    val authTraceMax = 256
    val resultBase = 4 + 32 + 32 + headDataMax
    // The encoded blob does carry the vector's length prefix, 2 B worst case.
    val msgsSize = 2 + upwardMessagesTotalSize(msgs)
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

  /// What the PVF's mandatory `set_head` call amounts to (§4.2):
  ///   - `HeadUndeclared`: never called, rejected with `MissingHeadDeclaration`.
  ///   - `HeadOversized`: called with head data past the 4 KiB `HeadData` cap,
  ///     rejected with `HeadDataTooLarge` (§4.3). `HeadData` is opaque in the
  ///     model (types.qnt), so the violation is a case of its own rather than a
  ///     measured length.
  ///   - `HeadDeclared(h)`: a well-formed declaration of `h`.
  type HeadDeclaration =
    | HeadUndeclared
    | HeadOversized
    | HeadDeclared(HeadData)

  /// Per-item Refine semantics (§4.1). The PVF is modeled as an oracle whose
  /// visible effects are how it terminated (`pvf`), the `set_parent_head_hash`
  /// and `set_head` declarations (`parentHead` `None` / `newHead`
  /// `HeadUndeclared` if the PVF never called them, §4.2), and the emitted
  /// upward `msgs`. The wrapper applies the §4.1/§4.3 checks in order and
  /// otherwise forwards the declarations verbatim; the authoritative
  /// parent-head match and the head write happen in Accumulate (§5.1).
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
    newHead: HeadDeclaration,
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
        else if (setValidatorKeysCount(msgs) > 1)
        Some(refineErr(para_id, SetValidatorKeysRepeated))
        else if (not(setValidatorKeysWithinCap(msgs)))
        Some(refineErr(para_id, TooManyValidatorKeys))
        else if (not(wellFormedAssignQueues(msgs)))
        Some(refineErr(para_id, InvalidAuthorizerQueue))
        else if (violatesParachainRestrictions(msgs, para_id))
        Some(refineErr(para_id, RestrictedHostFunction))
        else if (exceedsUpwardMessagesBudget(msgs))
        Some(refineErr(para_id, UpwardMessagesTooLarge))
        else if (refineOutputExceedsBudget(msgs))
        Some(refineErr(para_id, RefineOutputTooLarge))
        else
        // `set_parent_head_hash` / `set_head` are both mandatory (§4.2): a
        // missing declaration is rejected with `MissingHeadDeclaration`. An
        // oversized head is checked first because `set_head` aborts in the call
        // itself, before the wrapper can observe what else the PVF skipped.
        match newHead {
          | HeadOversized => Some(refineErr(para_id, HeadDataTooLarge))
          | HeadUndeclared => Some(refineErr(para_id, MissingHeadDeclaration))
          | HeadDeclared(nh) =>
            match parentHead {
              | None => Some(refineErr(para_id, MissingHeadDeclaration))
              | Some(ph) =>
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
  /// `msgs` are per-item scalars declared by the PVF. `parentHead` is `None` and
  /// `newHead` is `HeadUndeclared` when the PVF skipped the corresponding
  /// mandatory host call (§4.2).
  pure def refine(
    s: ParachainServiceState,
    pkg: WorkPackage,
    // The config parsed for the authorizer but carries no `Vec<ParaId>` prefix,
    // so the service cannot source a `para_id` from it (§4.1 step 1).
    authParasPrefixMissing: bool,
    pvf: PvfOutcome,
    parentHead: Option[HeadHash],
    newHead: HeadDeclaration,
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
