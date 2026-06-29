// The Parachain Service's on-chain state (design §3.1).
//
// All state mutations happen through Accumulate; Refine is stateless and only
// reads through host-function lookups. The shape here mirrors the conceptual
// Rust layout in design §3.1, translated to Quint records.
module state {
  import types.* from "./types"
  import messages.* from "./messages"

  // --- Per-parachain record (design §3.1) --------------------------------

  /// A validation code reference: its hash plus whether the parachain has
  /// *also* solicited it itself, on top of the service's own code-upgrade
  /// solicit (§5.2). When `pinned` is true, the code's referencer
  /// survives the code ceasing to be active/pending; when false, it is
  /// released. A parachain `forget` of an active/pending code clears the bit
  /// but does not release the referencer (the service still needs the code).
  type ValidationCode = {
    hash: ValidationCodeHash,
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
    len: int,
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
    preimageRegistry: Hash -> PreimageEntry,
    pendingAuthorizerQueues: CoreIndex -> List[AuthorizerHash],
    lastAuthorizerAssignment: CoreIndex -> Timeslot,
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
    lastAuthorizerAssignment: Map(),
    stagedValidatorKeys: List(),
    serviceCodeHash: { hashBytes: 0 }
  }

  // --- Read helpers ------------------------------------------------------

  pure def isLive(s: ParachainServiceState, para_id: ParaId): bool =
  s.parachains.keys().contains(para_id)

  pure def paraInfo(s: ParachainServiceState, para_id: ParaId): ParaInfo =
  s.parachains.get(para_id)

  pure def hasReferencer(
    s: ParachainServiceState, h: Hash, para_id: ParaId
  ): bool =
  s.preimageRegistry.keys().contains(h) and
  s.preimageRegistry.get(h).referencers.contains(para_id)

  // --- Log-append helpers (design §3.1, §3.3, §5.1) ----------------------

  pure def paraLog(s: ParachainServiceState, para_id: ParaId): ParachainLog =
  if (s.parachainLog.keys().contains(para_id)) s.parachainLog.get(para_id)
  else List()

  /// Append `e`; if that exceeds `ParachainLogCapacity`, evict one entry,
  /// preferring the oldest `RefineLogEntry` so `AccumulateLogEntry`s are
  /// retained. Only when no refine entry remains is the oldest entry dropped.
  pure def pushLogEntry(log: ParachainLog, e: (Timeslot, LogEntry)): ParachainLog = {
    val appended = log.append(e)
    if (appended.length() <= ParachainLogCapacity) appended
    else {
      val refineIdx = range(0, appended.length()).foldl(-1, (acc, i) =>
        if (acc >= 0) acc
        else match appended.nth(i)._2 {
          | RefineLogEntry(_) => i
          | AccumulateLogEntry(_) => acc
        })
      val dropIdx = if (refineIdx >= 0) refineIdx else 0
      range(0, appended.length()).foldl(List(), (acc, i) =>
        if (i == dropIdx) acc else acc.append(appended.nth(i)))
    }
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
}
