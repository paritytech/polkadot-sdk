// The Parachain Service's on-chain state (design §3.1).
//
// All state mutations happen through Accumulate; Refine is stateless and only
// reads through host-function lookups. The shape here mirrors the conceptual
// Rust layout in design §3.1, translated to Quint records.
module state {
  import types.* from "./types"
  import messages.* from "./messages"

  // --- Per-parachain record (design §3.1) --------------------------------

  /// A validation code reference plus the per-para `pinned` bookkeeping bit.
  /// `ref` carries the code's hash and SCALE-encoded byte length; `pinned`
  /// records whether the parachain has also solicited it independently.
  type ValidationCode = {
    ref: ValidationCodeRef,
    pinned: bool
  }

  type ParaInfo = {
    headData: HeadData,
    validationCode: ValidationCode,
    // Some((newCode, deadlineTimeslot)) when a code upgrade is in flight.
    // The pending code's `pinned` bit carries into `validationCode`
    // on activation.
    pendingUpgrade: Option[(ValidationCode, Timeslot)],
    totalStateBalance: Balance,
    usedStateBalance: Balance
  }

  // --- Shared preimage registry (design §3.1, §6.1) ----------------------

  type PreimageEntry = {
    referencers: Set[ParaId]
  }

  // --- Per-parachain log (design §3.1, capacity-bounded) -----------------
  type ParachainLog = List[(Timeslot, LogEntry)]

  // --- Top-level service state -------------------------------------------

  type ParachainServiceState = {
    parachains: ParaId -> ParaInfo,
    incomingTransfers: List[{
        from: ServiceId,
        amount: Balance,
        memo: Memo
    }],
    parachainLog: ParaId -> ParachainLog,
    preimageRegistry: (Hash, int) -> PreimageEntry,
    pendingAuthorizerQueues: CoreIndex -> List[AuthorizerHash],
    // Dirty-core index: maps each core with a pending queue to the JAM timeslot
    // (`jam_slot`) at which it should be applied. See `applyAlwaysAccumulate`.
    pendingAuthorizerCores: CoreIndex -> Timeslot,
    // Validator-key buffer (design §5.3) — appended chunk by chunk by
    // Asset Hub via `set_validator_keys`; flushed to JAM `designate` on
    // `is_last`.
    stagedValidatorKeys: List[ValidatorKey],
    // JAM `sa.codehash` for the Parachain Service itself. Mutated by the
    // `parachain_service_upgrade` upward message (design §5.4).
    serviceCodeHash: Hash
  }

  // --- Constructors ------------------------------------------------------

  pure val emptyState: ParachainServiceState = {
    parachains: Map(),
    incomingTransfers: List(),
    parachainLog: Map(),
    preimageRegistry: Map(),
    pendingAuthorizerQueues: Map(),
    pendingAuthorizerCores: Map(),
    stagedValidatorKeys: List(),
    serviceCodeHash: { hashBytes: 0 }
  }

  // --- Read helpers ------------------------------------------------------

  pure def isLive(s: ParachainServiceState, para_id: ParaId): bool =
  s.parachains.keys().contains(para_id)

  pure def paraInfo(s: ParachainServiceState, para_id: ParaId): ParaInfo =
  s.parachains.get(para_id)

  pure def hasReferencer(
    s: ParachainServiceState, h: Hash, len: int, para_id: ParaId
  ): bool =
  s.preimageRegistry.keys().contains((h, len)) and
  s.preimageRegistry.get((h, len)).referencers.contains(para_id)

  // --- Log-append helpers (design §3.1, §3.3, §5.1) ----------------------

  pure def paraLog(s: ParachainServiceState, para_id: ParaId): ParachainLog =
  if (s.parachainLog.keys().contains(para_id)) s.parachainLog.get(para_id)
  else List()

  /// Byte width of a SCALE compact-encoded length `n`: 1 B for n < 64,
  /// 2 B for n < 2^14, 4 B for n < 2^30, else 5+ B.
  pure def compactLen(n: int): int =
  if (n < 64) 1
  else if (n < 16384) 2
  else if (n < 1073741824) 4
  else 5

  /// Exact SCALE size of one AccumulateLog event: 1 B discriminant + body.
  /// Hash/ValidationCodeHash = 32 B, Balance = 16 B, u32 = 4 B. The
  /// InsufficientStateBalance body is its reason's 1 B discriminant plus
  /// FromSolicit's 32 B hash (FromValidatorKeys is nullary).
  pure def accumulateLogSize(e: AccumulateLog): int =
  1 + match e {
    | InvalidCodeHashAcc(_) => 32
    | InsufficientStateBalance(r) =>
      1 + match r {
        | FromSolicit(_) => 32
        | FromValidatorKeys => 0
      }
    | StateBalanceUpdateRejected(_) => 48
    | DesignateRejected(_) => 4
    | ServiceUpgradePreimageMissing(_) => 32
  }

  /// SCALE size of a RefineLog variant. Nullary variants = 1 B (discriminant only).
  /// Opaque(payload) = 1 (discriminant) + compact-len prefix + payload length.
  /// The model represents the Opaque payload as an int (the actual byte length);
  /// we measure it directly rather than charging the 1024 cap.
  pure def refineLogSize(r: RefineLog): int =
  match r {
    | Opaque(payload) => 1 + compactLen(payload) + payload
    | _ => 1
  }

  /// SCALE size of one (Timeslot, LogEntry) pair:
  ///   4 B timeslot + 1 B LogEntry discriminant + variant body.
  /// RefineLogEntry body: refineLogSize(error) + auth_trace compact-len + authTrace.
  /// AccumulateLogEntry body: 1 (vec compact-len prefix) + Σ accumulateLogSize per event.
  /// authTrace is an int (abstract length proxy) bounded to 256 by its type.
  pure def logEntrySize(e: (Timeslot, LogEntry)): int =
  4 + 1 + match e._2 {
    | RefineLogEntry(r) =>
      refineLogSize(r.error) + compactLen(r.authTrace) + r.authTrace
    | AccumulateLogEntry(evs) =>
      1 + evs.foldl(0, (a, ev) => a + accumulateLogSize(ev))
  }

  /// Total SCALE byte size of a ParachainLog: the outer `Vec` compact-length
  /// prefix (over the entry count) plus the sum of all entry sizes.
  pure def parachainLogSize(log: ParachainLog): int =
  compactLen(log.length()) + log.foldl(0, (a, e) => a + logEntrySize(e))

  /// Append `e`; then evict entries one at a time (oldest RefineLogEntry first,
  /// else oldest overall) until `parachainLogSize` is within `ParachainLogByteCap`.
  /// Uses a bounded fold (at most log.length()+1 iterations) since Quint has no
  /// while-loop; iterations that find the log already under cap are no-ops.
  pure def evictOneEntry(log: ParachainLog): ParachainLog = {
    val refineIdx = log.foldl((-1, 0), (acc, e) =>
      if (acc._1 >= 0) acc
      else match e._2 {
        | RefineLogEntry(_) => (acc._2, acc._2 + 1)
        | AccumulateLogEntry(_) => (acc._1, acc._2 + 1)
      })._1
    val dropIdx = if (refineIdx >= 0) refineIdx else 0
    log.foldl((List(), 0), (acc, e) =>
      (if (acc._2 == dropIdx) acc._1 else acc._1.append(e), acc._2 + 1))._1
  }

  pure def pushLogEntry(log: ParachainLog, e: (Timeslot, LogEntry)): ParachainLog = {
    val appended = log.append(e)
    // Evict until under the byte cap. Folding over `appended` runs the eviction
    // step exactly `appended.length()` times (the element is ignored); once under
    // cap the state passes through unchanged.
    appended.foldl(appended, (cur, _) =>
      if (parachainLogSize(cur) <= ParachainLogByteCap) cur
      else evictOneEntry(cur))
  }

  /// Append a `RefineLogEntry` for a failed Refine at timeslot `now`
  /// (design §3.3). No-op for an unregistered para — there is no
  /// `parachainLog[para_id]` to append to (§5.1 step 1).
  pure def appendRefineLog(
    s: ParachainServiceState,
    para_id: ParaId,
    now: Timeslot,
    error: RefineLog,
    authTrace: AuthTrace
  ): ParachainServiceState =
  if (not(s.isLive(para_id))) s
  else {
    val log = s.paraLog(para_id)
    val entry = (now, RefineLogEntry({ error: error, authTrace: authTrace }))
    { ...s, parachainLog: s.parachainLog.put(para_id, pushLogEntry(log, entry)) }
  }

  /// Append one batched `AccumulateLogEntry` (up to AccumulateLogCapacity
  /// events) at timeslot `now` for a live para (design §5.1). No-op for an
  /// unregistered para and for an empty `entries` list.
  pure def appendAccumulateLogs(
    s: ParachainServiceState,
    para_id: ParaId,
    now: Timeslot,
    entries: List[AccumulateLog]
  ): ParachainServiceState =
  if (not(s.isLive(para_id)) or entries == List()) s
  else {
    val log = s.paraLog(para_id)
    { ...s, parachainLog: s.parachainLog.put(para_id, pushLogEntry(log, (now, AccumulateLogEntry(entries)))) }
  }

  /// §5.1 log pruning: after a candidate for `para_id` is processed, drop
  /// `parachain_log[para_id]` entries whose inline timeslot is strictly below
  /// the candidate's lookup-anchor `threshold`. No-op for a para with no log.
  /// Only removes entries, so it never grows the log or its byte size.
  pure def pruneLogBelow(
    s: ParachainServiceState, para_id: ParaId, threshold: Timeslot
  ): ParachainServiceState =
  if (not(s.parachainLog.keys().contains(para_id))) s
  else {
    ...s,
    parachainLog: s.parachainLog.put(
      para_id, s.parachainLog.get(para_id).select(e => e._1 >= threshold)
    )
  }
}
