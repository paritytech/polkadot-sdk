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
    // `None` for a freshly-registered para that has no validation code yet
    // (before the first `parachain_set_validation_code`); `Some` once set.
    validationCode: Option[ValidationCode],
    // Some((newCode, deadlineTimeslot)) when a code upgrade is in flight.
    // The pending code's `pinned` bit carries into `validationCode`
    // on activation.
    pendingUpgrade: Option[(ValidationCode, Timeslot)],
    totalStateBalance: Balance,
    usedStateBalance: Balance,
    // Set once `parachain_clean_up` has begun deregistering this parachain
    // but some validation code still awaits its second, expunging `forget`.
    // While `true` the service rejects all work packages for it (design §6.4).
    isDeregistering: bool
  }

  // --- Shared preimage registry (design §3.1, §6.1) ----------------------

  type PreimageEntry = {
    referencers: Set[ParaId]
  }

  /// JAM's `query(hash, len)` request status. The service reaches every state
  // except that it / never carries the `provided_since`/`rerequested_since`
  // slots - only the / `unrequested_since` slot that gates the next `forget`.
  ///   - `Unprovided`: solicited, preimage never submitted. A `forget` drops
  ///     it in one step.
  ///   - `Provided`: the preimage is available. A `forget` unrequests it (does
  ///     NOT expunge) and moves it to `Unrequested`.
  ///   - `Unrequested(since)`: provided then forgotten once; `since` is the
  ///     unrequest slot. A second `forget` expunges only once
  ///     `now > since + ExpungePeriod`.
  ///     A `solicit` while `Unrequested` RESCUES the still-held blob back to
  ///     available, moving it to `Rerequested`.
  ///   - `Rerequested(since)`: unrequested then re-solicited — available again.
  ///     `since` is the ORIGINAL `unrequested_since`, which still gates the
  ///     next `forget` (JAM `Rerequested → Unrequest` requires
  ///     `now > since + ExpungePeriod`; otherwise `NotYetUnrequest`).
  type RequestStatus =
    | Unprovided
    | Provided
    | Unrequested(Timeslot)
    | Rerequested(Timeslot)

  // --- Per-parachain log (design §3.1, capacity-bounded) -----------------
  type ParachainLog = List[(Timeslot, LogEntry)]

  // --- Top-level service state -------------------------------------------

  type ParachainServiceState = {
    parachains: ParaId -> ParaInfo,
    // Incoming transfers for Asset Hub, bucketed by arrival timeslot (§3.1).
    // Arrivals sharing a slot append to the same bucket, so they add no storage
    // item. `nextSlot` chains a bucket to the next occupied one, `None` at the
    // tail: JAM storage has no prefix iteration, so the order cannot be
    // recovered from key enumeration.
    incomingTransfers: Timeslot -> {
        transfers: List[{ from: ServiceId, amount: Balance, memo: Memo }],
        nextSlot: Option[Timeslot]
    },
    // Head and tail of the bucket chain, `None` when the queue is empty.
    incomingTransferChain: Option[{ firstSlot: Timeslot, lastSlot: Timeslot }],
    parachainLog: ParaId -> ParachainLog,
    preimageRegistry: (Hash, int) -> PreimageEntry,
    // Scheduled-but-unapplied `assign` payloads, keyed by core: the queue to
    // install and who owns the core afterwards (`None` = this service,
    // unchanged). JAM's `assign` writes both atomically, so they are cached
    // together. Cold: read only for a core that is actually due.
    pendingAssigns: CoreIndex -> {
        queue: List[AuthorizerHash],
        assigner: Option[ServiceId]
    },
    // Dirty-core index: each core with a pending assign, paired with the
    // `jam_slot` at which it becomes due. Lets the always-accumulate path find
    // and gate due entries without reading the (much larger) payloads above.
    // See `applyAlwaysAccumulate`.
    pendingAssignCores: CoreIndex -> Timeslot,
    // Validator-key buffer (design §5.3) — appended chunk by chunk by
    // Asset Hub via `set_validator_keys`; flushed to JAM `designate` on
    // `is_last`.
    stagedValidatorKeys: List[ValidatorKey],
    // JAM `sa.codehash` for the Parachain Service itself. Mutated by the
    // `parachain_service_upgrade` upward message (design §5.4).
    serviceCodeHash: Hash,
    // Per-parachain key/value store (design §3.1, §6.1). Keyed by
    // (ParaId, user_key); each entry is billed under the sole-user rule.
    keyValueStorage: (ParaId, List[int]) -> List[int],
    // MODEL-ONLY ghost state, NOT part of the service's real storage or
    // footprint. The spec keeps no bookkeeping for the two-step forget and
    // reads request status from JAM's `query` (§6.1); the model stands that in
    // with this map, mirroring JAM's `query(hash, len)` for every solicited
    // preimage. Its domain always equals `preimageRegistry`'s.
    preimageStatus: (Hash, int) -> RequestStatus
  }

  // --- Constructors ------------------------------------------------------

  pure val emptyState: ParachainServiceState = {
    parachains: Map(),
    incomingTransfers: Map(),
    incomingTransferChain: None,
    parachainLog: Map(),
    preimageRegistry: Map(),
    pendingAssigns: Map(),
    pendingAssignCores: Map(),
    stagedValidatorKeys: List(),
    serviceCodeHash: { hashBytes: 0 },
    keyValueStorage: Map(),
    preimageStatus: Map()
  }

  // --- Read helpers ------------------------------------------------------

  pure def isLive(s: ParachainServiceState, para_id: ParaId): bool =
  s.parachains.keys().contains(para_id)

  pure def paraInfo(s: ParachainServiceState, para_id: ParaId): ParaInfo =
  s.parachains.get(para_id)

  /// The para's active validation-code hash, or `fallback` if it has no code
  /// yet (`validationCode == None`).
  pure def activeCodeHashOr(
    pi: ParaInfo, fallback: ValidationCodeHash
  ): ValidationCodeHash =
  pi.validationCode.optionMap(vc => vc.ref.hash).optionGetOr(fallback)

  /// Total transfers held across every bucket (§5.1 admission counts transfers,
  /// not buckets).
  pure def queuedTransferCount(s: ParachainServiceState): int =
  s.incomingTransfers.keys().fold(0, (acc, k) =>
    acc + s.incomingTransfers.get(k).transfers.length())

  /// Walk the bucket chain from `firstSlot`, dropping every bucket whose slot is
  /// `<= slot` (§5.1 `consume_transfers_up_to`). Returns the surviving map and
  /// the new chain endpoints, `None` once the chain is exhausted.
  pure def dropBucketsUpTo(
    s: ParachainServiceState, slot: Timeslot
  ): ParachainServiceState = {
    val survivors = s.incomingTransfers.mapFilterKeys(k => k > slot)
    match s.incomingTransferChain {
      | None => s
      | Some(chain) =>
        if (survivors.keys() == Set())
          { ...s, incomingTransfers: Map(), incomingTransferChain: None }
        else {
          // The chain is ordered, so the first survivor is the smallest key left.
          val newFirst =
            survivors.keys().fold(chain.lastSlot, (m, k) => if (k < m) k else m)
          { ...s,
            incomingTransfers: survivors,
            incomingTransferChain: Some({ ...chain, firstSlot: newFirst }) }
        }
    }
  }

  pure def hasReferencer(
    s: ParachainServiceState, h: Hash, len: int, para_id: ParaId
  ): bool =
  s.preimageRegistry.keys().contains((h, len)) and
  s.preimageRegistry.get((h, len)).referencers.contains(para_id)

  /// Whether JAM's `query(hash, len)` reports the preimage as AVAILABLE for
  /// lookup. Strictly stronger than being solicited: `Unprovided` means the
  /// service asked for the blob but nobody ever supplied it, and `Unrequested`
  /// is the two-step-forget limbo where JAM still holds the blob but the request
  /// no longer serves lookups (§6.1).
  pure def preimageAvailable(s: ParachainServiceState, h: Hash, len: int): bool =
  s.preimageStatus.keys().contains((h, len)) and
  (match s.preimageStatus.get((h, len)) {
    | Provided => true
    | Rerequested(_) => true
    | Unprovided => false
    | Unrequested(_) => false
  })

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
  /// Hash/ValidationCodeHash = 32 B, u32 = 4 B; balances are `Compact<u128>`,
  /// charged at their actual `compactLen`. The InsufficientStateBalance body is
  /// its reason's 1 B discriminant plus a 32 B hash (FromSolicit's preimage
  /// hash, FromSetKV's key hash). `StagedValidatorKeysOverflow` is nullary.
  pure def accumulateLogSize(e: AccumulateLog): int =
  1 + match e {
    | InsufficientStateBalance(r) =>
      1 + match r {
        // hash 32 + Compact<u32> len.
        | FromSolicit(p) => 32 + compactLen(p.len)
        | FromSetKV(_) => 32
      }
    | StateBalanceUpdateRejected(p) =>
      compactLen(p.attempted) + compactLen(p.currentTotal) + compactLen(p.currentUsed)
    | DesignateRejected(p) => compactLen(p.len)
    | StagedValidatorKeysOverflow => 0
    | ServiceUpgradePreimageMissing(_) => 32
    // id Compact<u64> + TransferError tag 1.
    | TransferFailed(p) => compactLen(p.id) + 1
    // hash 32 + Compact<u32> len + due (Timeslot, fixed u32).
    | ForgetAgainAt(p) => 32 + compactLen(p.len) + 4
    | TooMuchStateHeld => 0
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

  /// Eviction rank of a log entry (§5.1). Lower ranks are discarded first.
  ///   0 - a refine error other than `Opaque`: useful for debugging, but a fixed
  ///       structural failure carrying no parachain-supplied detail, and the rank
  ///       into which every failure a coretime buyer can provoke falls. The most
  ///       disposable of the three.
  ///   1 - `Opaque`: the payload the parachain's own PVF chose to report (§4.2),
  ///       the only refine entry carrying context the parachain can act on.
  ///   2 - an accumulate event: an actual on-chain state change.
  pure def entryRank(e: (Timeslot, LogEntry)): int =
  match e._2 {
    | RefineLogEntry(r) => match r.error { | Opaque(_) => 1 | _ => 0 }
    | AccumulateLogEntry(_) => 2
  }

  /// Drop the single best eviction candidate on behalf of an incoming entry of
  /// rank `rank`: the OLDEST entry of the lowest occupied rank at or below
  /// `rank`. Nothing above `rank` is ever touched, so a low-ranked newcomer can
  /// never displace a higher-ranked entry; if no candidate exists the log is
  /// returned unchanged, which `pushLogEntry` turns into dropping the newcomer.
  pure def evictOneFor(log: ParachainLog, rank: int): ParachainLog = {
    // The victim is the lowest rank at or below `rank` and, within it, the
    // earliest timeslot. Age is read from the stored timeslot rather than list
    // position, so eviction does not depend on the log being kept in order.
    // `None` when every entry present outranks the newcomer.
    val victim: Option[(int, Timeslot)] = log.foldl(None, (best, e) => {
      val r = entryRank(e)
      if (r > rank) best
      else match best {
        | None => Some((r, e._1))
        | Some(b) =>
          if (r < b._1 or (r == b._1 and e._1 < b._2)) Some((r, e._1)) else best
      }
    })
    match victim {
      | None => log
      | Some(v) =>
        // Remove one entry with that (rank, timeslot); the flag stops it there,
        // so entries sharing a timeslot lose exactly one of their number.
        log.foldl((List(), false), (acc, e) =>
          if (not(acc._2) and entryRank(e) == v._1 and e._1 == v._2) (acc._1, true)
          else (acc._1.append(e), acc._2))._1
    }
  }

  /// Append `e`, then evict by rank until `parachainLogSize` is within
  /// `ParachainLogByteCap`. Uses a bounded fold (at most `log.length()+1`
  /// iterations) since Quint has no while-loop; iterations that find the log
  /// already under cap are no-ops.
  ///
  /// The newcomer is itself an eviction candidate at its own rank, so when it is
  /// the only entry at or below that rank it evicts itself — exactly the
  /// "incoming entry is dropped instead" rule. The final guard restores the
  /// original log if no amount of permitted eviction makes room.
  pure def pushLogEntry(log: ParachainLog, e: (Timeslot, LogEntry)): ParachainLog = {
    val rank = entryRank(e)
    val appended = log.append(e)
    val shrunk = appended.foldl(appended, (cur, _) =>
      if (parachainLogSize(cur) <= ParachainLogByteCap) cur
      else evictOneFor(cur, rank))
    if (parachainLogSize(shrunk) <= ParachainLogByteCap) shrunk else log
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

  /// Append one batched `AccumulateLogEntry` at timeslot `now` for a live para
  /// (design §5.1). The batch's total size is bounded only by the 64 KiB
  /// `parachain_log` byte cap enforced in `pushLogEntry`. No-op for an unregistered
  /// para and for an empty `entries` list.
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
