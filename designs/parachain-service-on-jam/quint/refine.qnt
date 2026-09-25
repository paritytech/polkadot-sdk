// Refine: in-core, stateless validation-code execution (design §4).
//
// We do NOT model validation-code semantics; instead we model Refine as a non-
// deterministic producer of `ParachainWorkDigest`s subject to the
// structural rules from §4.1–§4.3:
//
// - The `paraId` of each work-item is sourced from the authorizer config's
// `authorized_paras` prefix, not from the extrinsic.
// - The validation code must call `set_parent_head_hash` and `set_head` exactly once.
// - Refine returns `Err(InvalidCodeHash)` iff `historical_lookup` for the
// candidate's `validation_code_hash` returns None.
// - The first host call that breaks a §4.3 rule aborts Refine with that rule's
//   `RefineLog`.
//
// The `parent_head_hash` field is mandatory but is *not* validated against
// service state here; that check lives in Accumulate (§5.1, step 1).
module refine {
  import types.* from "./types"
  import messages.* from "./messages"
  import state.* from "./state"

  /// The work-item payload: one parachain candidate per item (design §3.2).
  type ParachainCandidate = {
    validationCode: ValidationCodeRef
  }

  type WorkPackage = {
    // Authorizer config prefix binding work-item index → ParaId (design §3.2, §7.1).
    authorizedParas: List[ParaId],
    // One candidate per work item. Currently `length(items) == 1` per design §3.2.
    items: List[ParachainCandidate]
  }

  // --- Upward-message validity rules (Refine-side enforcement) ------------

  /// Actual SCALE-encoded size of one upward message, used by the 48 KiB
  /// result-blob budget check (§4.1 step 7). Every variant is measured from
  /// its real payload, except `ParachainSetHead`: `HeadData` is opaque in the
  /// model (an `int`, not a byte vector), so its `newHead` has no measurable
  /// length and is charged its worst case (2 B compact len + 4096 B max).
  // 1 B enum discriminant (the message tag) + the variant's payload size.
  pure def upwardMessageSize(m: UpwardMessage): int =
  1 + match m {
    // hash 32 + Compact<u32> len + 1 B phase tag.
    | RequestCodeUpgrade(p) => 32 + compactLen(p.len) + 1
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
  // either an `Ok` with the upward messages the validation code emitted or
  // an `Err` naming the first rule it broke.
  //
  // For the model, we nondeterministically pick the host calls in
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
    val key = (ref.hash.asPreimage(), ref.len)
    s.preimageRegistry.keys().contains(key) and
    s.preimageRegistry.get(key).referencers != Set()
  }

  /// One host call the validation code makes, in the order it makes them
  /// (§4.2, §4.3). A trace that ends without `ReportError` or `Panic` means the
  /// code ran to completion.
  ///   - `SetHeadOversized`: `set_head` with head data past the 4 KiB `HeadData`
  ///     cap. `HeadData` is opaque in the model (types.qnt), so the violation is
  ///     a case of its own rather than a measured length.
  ///   - `ReportError(payload)`: `report_error(data)`, the only way a validation
  ///     code records a reason for its failure (`RefineLog::Opaque`).
  ///   - `Panic`: any other abnormal exit. Deliberately NOT caught, so the
  ///     service's whole `refine` fails with it and there is no
  ///     `ParachainWorkDigest` at all (§3.3).
  type CodeCall =
    | SetParentHeadHash(HeadHash)
    | SetHead(HeadData)
    | SetHeadOversized
    | Send(UpwardMessage)
    | ReportError(int)
    | Panic

  /// `Send` for each of `msgs`, in order.
  pure def sends(msgs: List[UpwardMessage]): List[CodeCall] =
  msgs.foldl(List(), (acc, m) => acc.append(Send(m)))

  type RunStop =
    | Running
    | Aborted(RefineLog)
    | Panicked

  /// What the Refine wrapper has collected from the calls made so far.
  type RefineRun = {
    parent: Option[HeadHash],
    head: Option[HeadData],
    msgs: List[UpwardMessage],
    msgsSize: int,
    keysSent: bool,
    stop: RunStop
  }

  pure val freshRun: RefineRun = {
    parent: None, head: None, msgs: List(), msgsSize: 0, keysSent: false, stop: Running
  }

  /// The rule `send_upward_message(m)` breaks, checked in the §4.3 order: the
  /// para restrictions, the variant's own requirements, the 40 KiB budget, the
  /// message count.
  pure def sendViolation(r: RefineRun, para_id: ParaId, m: UpwardMessage): Option[RefineLog] = {
    val variantViolation: Option[RefineLog] = match m {
      | AssignCore(p) =>
        if (not(wellFormedAssignQueue(p.queue, p.newAssigner))) Some(InvalidAuthorizerQueue)
        else if (not(validCoreIndex(p.core))) Some(InvalidCoreIndex)
        else None
      | SetValidatorKeys(p) =>
        if (p.keys.length() > SetValidatorKeysMaxKeys) Some(TooManyValidatorKeys)
        else if (r.keysSent) Some(SetValidatorKeysRepeated)
        else None
      | _ => None
    }
    if (violatesParachainRestrictions(List(m), para_id)) Some(RestrictedHostFunction)
    else if (variantViolation != None) variantViolation
    else if (r.msgsSize + upwardMessageSize(m) > MaxUpwardMessagesSize)
    Some(UpwardMessagesTooLarge)
    else if (r.msgs.length() >= MaxUpwardMessages) Some(TooManyUpwardMessages)
    else None
  }

  /// Apply one host call. The first call that breaks a rule aborts Refine, and
  /// every later call is ignored. Calling `set_parent_head_hash` or `set_head`
  /// a second time is as invalid as never calling it (§4.2).
  pure def applyCall(r: RefineRun, para_id: ParaId, c: CodeCall): RefineRun =
  if (r.stop != Running) r
  else match c {
    | SetParentHeadHash(h) =>
      if (r.parent != None) { ...r, stop: Aborted(InvalidHeadDeclaration) }
      else { ...r, parent: Some(h) }
    | SetHead(h) =>
      if (r.head != None) { ...r, stop: Aborted(InvalidHeadDeclaration) }
      else { ...r, head: Some(h) }
    | SetHeadOversized =>
      if (r.head != None) { ...r, stop: Aborted(InvalidHeadDeclaration) }
      else { ...r, stop: Aborted(HeadDataTooLarge) }
    | Send(m) =>
      match sendViolation(r, para_id, m) {
        | Some(e) => { ...r, stop: Aborted(e) }
        | None => {
          ...r,
          msgs: r.msgs.append(m),
          msgsSize: r.msgsSize + upwardMessageSize(m),
          keysSent: r.keysSent or (match m { | SetValidatorKeys(_) => true | _ => false })
        }
      }
    | ReportError(payload) => { ...r, stop: Aborted(Opaque(payload)) }
    | Panic => { ...r, stop: Panicked }
  }

  /// Per-item Refine semantics (§4.1). The validation code is modeled as an
  /// oracle whose visible effects are its host `calls`, in order. The wrapper
  /// applies the §4.2/§4.3 rules call by call, then the checks that need the
  /// whole run, and otherwise forwards the declarations verbatim. The
  /// authoritative parent-head match and the head write happen in Accumulate
  /// (§5.1).
  ///
  /// `None` means the invocation produced no digest at all (`Panic`).
  ///
  /// Refine is stateless: it reads no `ParaInfo` (registration is checked in
  /// Accumulate, §5.1 step 1). The only state it touches is the preimage
  /// store, via `codeAvailable`.
  pure def refineItem(
    s: ParachainServiceState,
    para_id: ParaId,
    candidate: ParachainCandidate,
    calls: List[CodeCall],
    lookupAnchor: Timeslot
  ): Option[ParachainWorkDigest] = {
    if (not(codeAvailable(s, candidate.validationCode)))
    Some(refineErr(para_id, InvalidCodeHash))
    else {
      val r = calls.foldl(freshRun, (acc, c) => applyCall(acc, para_id, c))
      match r.stop {
        | Panicked => None
        | Aborted(e) => Some(refineErr(para_id, e))
        | Running =>
          match r.parent {
            | None => Some(refineErr(para_id, InvalidHeadDeclaration))
            | Some(ph) =>
              match r.head {
                | None => Some(refineErr(para_id, InvalidHeadDeclaration))
                | Some(nh) =>
                  if (refineOutputExceedsBudget(r.msgs))
                  Some(refineErr(para_id, RefineOutputTooLarge))
                  else
                  Some(refineOk(para_id, candidate.validationCode, ph, nh, r.msgs, lookupAnchor))
              }
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
  /// Single-item packages only (per §3.2): `calls` are the host calls of that
  /// item's validation code.
  pure def refine(
    s: ParachainServiceState,
    pkg: WorkPackage,
    // The config parsed for the authorizer but carries no `Vec<ParaId>` prefix,
    // so the service cannot source a `para_id` from it (§4.1 step 1).
    authParasPrefixMissing: bool,
    calls: List[CodeCall],
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
      calls, lookupAnchor
    ) {
      | Some(d) => List(WorkOk(d))
      | None => List(WorkErr)
    }
  }
}
