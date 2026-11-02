// The Parachain Service's on-chain state (design §3.1).
//
// All state mutations happen through Accumulate; Refine is stateless and only
// reads through host-function lookups. The shape here mirrors the conceptual
// Rust layout in design §3.1, translated to Quint records.
module state {
  import types.* from "./types"
  import messages.* from "./messages"

  // --- JAM service accounts (model-only ghost state) ----------------------

  /// The part of a JAM service account the model tracks: its two balances, and
  /// the gas a deferred transfer to it must carry.
  type JamAccount = {
    balance: Balance,
    supervisorBalance: Balance,
    minMemoGas: int
  }

  /// `a` with `amount` taken from its supervisor balance if `supervisor`, else
  /// from its regular balance.
  pure def debit(a: JamAccount, amount: Balance, supervisor: bool): JamAccount =
    if (supervisor) { ...a, supervisorBalance: a.supervisorBalance - amount }
    else { ...a, balance: a.balance - amount }

  pure def credit(a: JamAccount, amount: Balance, supervisor: bool): JamAccount =
    debit(a, 0 - amount, supervisor)

  /// JAM rejects an operation that would leave an account with a negative
  /// balance, or with less than its threshold balance across both.
  pure def isOverdrawn(after: JamAccount, threshold: Balance): bool =
    after.balance < 0 or after.supervisorBalance < 0 or
      after.balance + after.supervisorBalance < threshold

  // --- Per-parachain record (design §3.1) --------------------------------

  type ParaInfo = {
    headData: HeadData,
    // `None` for a freshly-registered para that has no validation code yet
    // (before the first `parachain_set_validation_code`); `Some` once set.
    validationCode: Option[ValidationCodeRef],
    // Announced code upgrade (§5.2).
    announcedUpgrade: Option[ValidationCodeRef],
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

  /// JAM's `query(hash, len)` request status, with the slots `query` reports.
  ///   - `Unprovided`: solicited, preimage never submitted. A `forget` drops
  ///     it in one step.
  ///   - `Provided(since)`: available from slot `since` on. A `forget`
  ///     unrequests it (does NOT expunge) and moves it to `Unrequested`.
  ///   - `Unrequested`: provided then forgotten once, available from
  ///     `providedSince` until before `unrequestedSince`. A second `forget`
  ///     expunges only once `now > unrequestedSince + ExpungePeriod`.
  ///     A `solicit` while `Unrequested` RESCUES the still-held blob back to
  ///     available, moving it to `Rerequested`.
  ///   - `Rerequested`: unrequested then re-solicited at `rerequestedSince`,
  ///     available again from then on. `unrequestedSince` still gates the
  ///     next `forget` (JAM `Rerequested → Unrequest` requires
  ///     `now > unrequestedSince + ExpungePeriod`; otherwise
  ///     `NotYetUnrequest`), which leaves it `Unrequested` with
  ///     `providedSince = rerequestedSince`.
  type RequestStatus =
    | Unprovided
    | Provided(Timeslot)
    | Unrequested({ providedSince: Timeslot, unrequestedSince: Timeslot })
    | Rerequested({
      providedSince: Timeslot,
      unrequestedSince: Timeslot,
      rerequestedSince: Timeslot
    })

  // --- Per-parachain log (design §3.1, capacity-bounded) -----------------
  type ParachainLog = List[(Timeslot, LogEntry)]

  /// One recorded incoming transfer (§3.1). `toSupervisorBalance` is JAM's
  /// `destsupervisor` flag: the amount went to the supervisor balance.
  type IncomingTransfer =
    { source: ServiceId, amount: Balance, toSupervisorBalance: bool, memo: Memo }

  // --- Settlement ring (design §8.1) --------------------------------------

  /// Ends and length of a parachain's settlement ring: `oldest` is the root
  /// evicted next, `newest` the most recently pushed root, and `len` the
  /// number of live roots, in `1..MaxSettlementRingCapacity`.
  type SettlementCursor = { oldest: StreamsRoot, newest: StreamsRoot, len: int }

  // --- Top-level service state -------------------------------------------

  type ParachainServiceState = {
    parachains: ParaId -> ParaInfo,
    // Incoming transfers for Asset Hub, held in fixed-size buckets keyed by an
    // allocated bucket id (§3.1). Arrivals append to the bucket the current
    // accumulate invocation opened. A bucket is closed once it holds
    // `MaxTransfersPerBucket` transfers or that invocation ends; the next arrival
    // then opens the following id. Ids are contiguous, so the queue order is the
    // id order and enumerating it needs no per-bucket successor pointer, which
    // JAM storage could not substitute for anyway: it has no prefix iteration.
    incomingTransfers: BucketId -> List[IncomingTransfer],
    // First and last bucket id, `None` when the queue is empty — in real state
    // that is the storage key being absent, not an encoded `Option` tag. Ids
    // restart at 0 once a clean-up empties the queue, so they stay small.
    // `count` is the total number of queued transfers across every bucket. It is
    // maintained on append and clean-up so the §5.1 admission bound is an O(1)
    // read; deriving it would mean reading every bucket, defeating the bucketing.
    incomingTransferBuckets: Option[{ firstBucket: BucketId, lastBucket: BucketId, count: int }],
    parachainLog: ParaId -> ParachainLog,
    // Scheduled-but-unapplied `assign` payloads, keyed by core: the queue to
    // install and who owns the core afterwards (`None` = this service,
    // unchanged). A queue that must keep cycling is stored already rotated to
    // where the next cycle starts, so its own order carries the schedule (§7.1).
    // Cold: read only for a core that is due.
    pendingAssigns: CoreIndex -> {
      queue: List[AuthorizerHash],
      assigner: Option[ServiceId]
    },
    // Dirty-core index: each core with a pending assign, paired with the
    // timeslot at which it is due. Sole home of the due time, so the
    // always-accumulate path can gate without reading the payloads above.
    pendingAssignCores: CoreIndex -> Timeslot,
    preimageRegistry: (Hash, int) -> PreimageEntry,
    // Validator-key buffer (design §5.3), appended chunk by chunk by
    // Asset Hub via `set_validator_keys`; flushed to JAM `designate` on
    // `is_last`.
    stagedValidatorKeys: List[ValidatorKey],
    // Per-parachain key/value store (design §3.1, §6.1). Keyed by
    // (ParaId, user_key); each entry is billed under the sole-user rule.
    keyValueStorage: (ParaId, List[int]) -> List[int],
    // §8.1 settlement ring, a singly linked list mirroring the two storage
    // maps under tags `0x09` and `0x0a`. `messagesCursor` holds the list's
    // ends and length, absent until the parachain's first push and never
    // read by the settlement check. `messagesMember` is present iff the root
    // is one of the sender's `MaxSettlementRingCapacity` most recent distinct
    // enacted roots, and is the ONLY entry the settlement check reads. Its
    // value is the next root in insertion order; the newest root maps to
    // itself. The ring at capacity is reserved in the baseline footprint
    // (§6.1, `SettlementRingFootprint`), so pushes are never charged.
    messagesCursor: ParaId -> SettlementCursor,
    messagesMember: (ParaId, StreamsRoot) -> StreamsRoot,
    // JAM `sa.codehash` for the Parachain Service itself, and the length of that
    // code. Both are set by `UpgradeService` (design §5.4).
    serviceCodeHash: Hash,
    serviceCodeLen: int,
    // MODEL-ONLY ghost state, NOT part of the service's real storage or
    // footprint. The spec keeps no bookkeeping for the two-step forget and
    // reads request status from JAM's `query` (§6.1); the model stands that in
    // with this map, mirroring JAM's `query(hash, len)` for every solicited
    // preimage. Its domain always equals `preimageRegistry`'s.
    preimageStatus: (Hash, int) -> RequestStatus,
    // MODEL-ONLY ghost state, NOT part of the service's real storage or
    // footprint. JAM holds each core's assigner (gray paper `Omega_A` writes it
    // alongside the queue) and rejects an `assign` from anyone else, so the
    // model needs it to tell an attempted assign from a successful one (§7.1).
    // A core absent from the map is still assigned to this service. Handing a
    // core away is one-way here: only the new assigner could give it back, and
    // nothing this service does can cause that.
    jamCoreAssigners: CoreIndex -> ServiceId,
    // MODEL-ONLY ghost state, NOT part of the service's real storage or
    // footprint: this service's own JAM account, which JAM debits and credits
    // as it transfers, creates and ejects services.
    jamAccount: JamAccount
  }

  // --- Constructors ------------------------------------------------------

  pure val emptyState: ParachainServiceState = {
    parachains: Map(),
    incomingTransfers: Map(),
    incomingTransferBuckets: None,
    parachainLog: Map(),
    pendingAssigns: Map(),
    pendingAssignCores: Map(),
    preimageRegistry: Map(),
    stagedValidatorKeys: List(),
    keyValueStorage: Map(),
    messagesCursor: Map(),
    messagesMember: Map(),
    serviceCodeHash: { hashBytes: 0 },
    serviceCodeLen: 0,
    preimageStatus: Map(),
    jamCoreAssigners: Map(),
    jamAccount: { balance: 0, supervisorBalance: 0, minMemoGas: 0 }
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
    pi.validationCode.optionMap(vc => vc.hash).optionGetOr(fallback)

  /// Total transfers held across every bucket (§5.1 admission counts transfers,
  /// not buckets).
  pure def queuedTransferCount(s: ParachainServiceState): int =
    match s.incomingTransferBuckets { | None => 0 | Some(b) => b.count }

  /// The same total computed the slow way, by reading every bucket. Used only by
  /// the invariant that pins the counter against the buckets it summarises.
  pure def queuedTransferCountWalked(s: ParachainServiceState): int =
    s.incomingTransfers.keys().fold(0, (acc, k) =>
      acc + s.incomingTransfers.get(k).length())

  /// The bucket id the next arrival opens: one past the tail, or 0 on an empty
  /// queue, since a clean-up that empties it clears the endpoints (§5.1).
  pure def nextBucketId(s: ParachainServiceState): BucketId =
    match s.incomingTransferBuckets { | None => 0 | Some(b) => b.lastBucket + 1 }

  /// Remove every bucket up to and including `upTo` (§5.1
  /// `clean_up_buckets_up_to`), re-heading the queue at the first survivor and
  /// clearing the endpoints once nothing remains.
  pure def cleanUpBucketsUpTo(
    s: ParachainServiceState, upTo: BucketId
  ): ParachainServiceState = {
    val survivors = s.incomingTransfers.mapFilterKeys(k => k > upTo)
    match s.incomingTransferBuckets {
      | None => s
      | Some(buckets) =>
        if (survivors.keys() == Set())
          { ...s, incomingTransfers: Map(), incomingTransferBuckets: None }
        else {
          // What survives is a suffix, so its first id is the smallest left.
          val newFirst =
            survivors.keys().fold(buckets.lastBucket, (m, k) => if (k < m) k else m)
          val removed = s.incomingTransfers.keys().fold(0, (acc, k) =>
            if (k <= upTo) acc + s.incomingTransfers.get(k).length() else acc)
          {
            ...s,
            incomingTransfers: survivors,
            incomingTransferBuckets:
              Some({
                ...buckets,
                firstBucket: newFirst,
                count: buckets.count - removed
              })
          }
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
        | Provided(_) => true
        | Rerequested(_) => true
        | Unprovided => false
        | Unrequested(_) => false
      })

  /// Whether this service is still `core`'s assigner, i.e. whether JAM accepts
  /// an `assign` for it (§7.1). Cores it never handed away are absent from the
  /// ghost map and still belong to it.
  pure def isAssignerOf(s: ParachainServiceState, core: CoreIndex): bool =
    not(s.jamCoreAssigners.keys().contains(core)) or
      s.jamCoreAssigners.get(core) == ParachainServiceId

  /// This service's threshold balance, as the model approximates it: JAM's base
  /// deposit plus the footprint its parachains are billed for (§6.1). Billing
  /// each para as the sole user over-counts shared entries, so the model may
  /// refuse a spend JAM would allow, never the reverse.
  pure def serviceThreshold(s: ParachainServiceState): Balance =
    s.parachains.keys().fold(BaseDeposit, (acc, p) => acc + s.paraInfo(p).usedStateBalance)

  /// Record the assigner JAM wrote for `core` while applying an `assign`.
  pure def recordAssigner(
    s: ParachainServiceState, core: CoreIndex, assigner: ServiceId
  ): ParachainServiceState =
    { ...s, jamCoreAssigners: s.jamCoreAssigners.put(core, assigner) }

  // --- Log-append helpers (design §3.1, §3.3, §5.1) ----------------------

  // --- Authorizer-queue fill (§7.1) -------------------------------------

  /// `l` repeated `n` times.
  pure def repeatList(l: List[AuthorizerHash], n: int): List[AuthorizerHash] =
    range(0, n).foldl(List(), (acc, _) => acc.concat(l))

  /// The first `k` elements of `l`.
  pure def takeFirst(k: int, l: List[AuthorizerHash]): List[AuthorizerHash] =
    range(0, k).foldl(List(), (acc, i) => acc.append(l.nth(i)))

  /// `l` re-ordered to start at index `k`, wrapping the earlier elements to the
  /// end.
  pure def rotateList(l: List[AuthorizerHash], k: int): List[AuthorizerHash] =
    if (l.length() == 0) l
    else {
      val o = k % l.length()
      l.slice(o, l.length()).concat(l.slice(0, o))
    }

  /// True iff `x` authorizers tile JAM's 80 slots exactly, so the installed
  /// queue cycles correctly forever and never needs re-presenting.
  pure def fillsDirectly(queueLen: int): bool =
    queueLen == AuthorizerQueueLength or (AuthorizerQueueLength % queueLen) == 0

  /// The 80 slots JAM sees: `q` repeated and cut to
  /// `AuthorizerQueueLength`. `q` is stored already rotated to where this cycle
  /// starts, so no separate offset is needed (§7.1).
  pure def assignQueueFor(q: List[AuthorizerHash]): List[AuthorizerHash] =
    takeFirst(AuthorizerQueueLength, repeatList(q, AuthorizerQueueLength / q.length() + 1))

  /// The queue to store for the cycle after `q`: rotated on by the 80 slots just
  /// consumed, so the next cycle resumes where this one stopped.
  pure def advanceAssignQueue(q: List[AuthorizerHash]): List[AuthorizerHash] =
    rotateList(q, AuthorizerQueueLength)

  pure def paraLog(s: ParachainServiceState, para_id: ParaId): ParachainLog =
    if (s.parachainLog.keys().contains(para_id)) s.parachainLog.get(para_id)
    else List()

  /// Byte width of a SCALE compact-encoded integer `n`: 1 B for n < 64, 2 B for
  /// n < 2^14, 4 B for n < 2^30, then big-integer mode — a 1 B header plus `n`'s
  /// minimal byte length, so 5 B below 2^32 up to 9 B at the `u64` ceiling.
  pure def compactLen(n: int): int =
    if (n < 64) 1
    else if (n < 16384) 2
    else if (n < 1073741824) 4
    else if (n < 4294967296) 5
    else if (n < 1099511627776) 6
    else if (n < 281474976710656) 7
    else if (n < 72057594037927936) 8
    else 9

  /// Exact SCALE size of one AccumulateLog event: 1 B discriminant + body.
  /// Hash/ValidationCodeHash = 32 B, u32 = 4 B; balances are `Compact<u64>`,
  /// charged at their actual `compactLen`. The InsufficientStateBalance body is
  /// its reason's 1 B discriminant plus a 32 B hash (FromSolicit's preimage
  /// hash, FromSetKV's key hash). `StagedValidatorKeysOverflow` is nullary.

  /// Payload octets a `ServiceCreationResult` carries beyond its discriminant:
  /// the assigned ServiceId on `Created`, nothing on the failures.
  pure def creationPayload(r: ServiceCreationResult): int =
    match r { | Created(_) => 4 | _ => 0 }

  pure def accumulateLogSize(e: AccumulateLog): int =
    1 + match e {
      | InsufficientStateBalance(r) =>
        1 + match r {
          // hash 32 + Compact<u32> len.
          | FromSolicit(p) => 32 + compactLen(p.len)
          | FromSetKV(_) => 32
        }
      // ParaId 4 + Compact attempted + reason discriminant 1 + reason body.
      | StateBalanceUpdateRejected(p) =>
        4 + compactLen(p.attempted) + 1 + match p.reason {
          | BelowUsed(r) => compactLen(r.currentTotal) + compactLen(r.currentUsed)
          | ParachainIsDeregistering => 0
        }
      | DesignateRejected(p) => compactLen(p.len)
      | StagedValidatorKeysOverflow => 0
      | ServiceUpgradePreimageMissing(_) => 32
      // hash 32 + Compact<u32> len.
      | CodeUpgradeNotAvailable(p) => 32 + compactLen(p.len)
      | CodeUpgradeNotAnnounced(p) => 32 + compactLen(p.len)
      | CanNotRemoveCode(p) => 32 + compactLen(p.len)
      // core index, a fixed u16.
      | CoreNotAssignable(_) => 2
      // id Compact<u64> + TransferError tag 1.
      | TransferFailed(p) => compactLen(p.id) + 1
      // Target (1 tag + 4 id) + hash 32 + Compact<u32> len + due (Timeslot, fixed u32).
      | ForgetAgainAt(p) => 5 + 32 + compactLen(p.len) + 4
      // service 4 + error tag 1, for each of the three failure entries.
      | ServiceSolicitFailed(_) => 5
      | ServiceEjectFailed(_) => 5
      | ServiceSupervisorFailed(_) => 5
      // service 4 + hash 32 + Compact<u32> len.
      | ServiceForgetNotRequested(p) => 36 + compactLen(p.len)
      // service 4 + key hash 32.
      | ServiceKeyNotFound(_) => 36
      // service 4 + Compact<Balance> credited.
      | ServiceEjected(p) => 4 + compactLen(p.credited)
      // id Compact<u64> + result (1 tag, + 4 when Created).
      | ServiceCreation(p) => compactLen(p.id) + 1 + creationPayload(p.result)
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
  /// Refine body: refineLogSize(error) + auth_trace compact-len + authTrace.
  /// Accumulate body: 1 (vec compact-len prefix) + Σ accumulateLogSize per event.
  /// authTrace is an int (abstract length proxy) bounded to 256 by its type.
  pure def logEntrySize(e: (Timeslot, LogEntry)): int =
    4 + 1 + match e._2 {
      | Refine(r) =>
        refineLogSize(r.error) + compactLen(r.authTrace) + r.authTrace
      | Accumulate(evs) =>
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
  ///   1 - `Opaque`: the payload the parachain's own validation code chose to report (§4.2),
  ///       the only refine entry carrying context the parachain can act on.
  ///   2 - an accumulate event: an actual on-chain state change.
  pure def entryRank(e: (Timeslot, LogEntry)): int =
    match e._2 {
      | Refine(r) => match r.error { | Opaque(_) => 1 | _ => 0 }
      | Accumulate(_) => 2
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
  /// the only entry at or below that rank it evicts itself, exactly the
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

  /// Append a `Refine` for a failed Refine at timeslot `now`
  /// (design §3.3). No-op for an unregistered para: there is no
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
      val entry = (now, Refine({ error: error, authTrace: authTrace }))
      { ...s, parachainLog: s.parachainLog.put(para_id, pushLogEntry(log, entry)) }
    }

  /// Append one batched `Accumulate` at timeslot `now` for a live para
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
      { ...s, parachainLog: s.parachainLog.put(para_id, pushLogEntry(log, (now, Accumulate(entries)))) }
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

  // --- Messaging settlement ring (§8.1) -----------------------------------

  /// The roots of `para`'s ring from oldest to newest, found by following
  /// `messagesMember` links `len` times from `oldest`. Empty without a cursor.
  /// A missing link repeats the last root, which `settlement_ring_coherent`
  /// then rejects as a duplicate.
  pure def ringRoots(s: ParachainServiceState, para: ParaId): List[StreamsRoot] =
    if (not(s.messagesCursor.keys().contains(para))) List()
    else {
      val c = s.messagesCursor.get(para)
      range(0, c.len).foldl((List(), c.oldest), (acc, _) => {
        val link =
          if (s.messagesMember.keys().contains((para, acc._2)))
            s.messagesMember.get((para, acc._2))
          else acc._2
        (acc._1.append(acc._2), link)
      })._1
    }

  /// Settlement check (§5.1 step 5).
  /// Ensures every required `(source, root)` exists in `messagesMember`.
  /// Always returns `true` for empty requirement lists.
  pure def settlementCheck(
    s: ParachainServiceState, requires: List[(ParaId, StreamsRoot)]
  ): bool =
    requires.foldl(true, (acc, e) => acc and s.messagesMember.keys().contains(e))

  /// Appends a new `messages_streams_root` upon candidate enactment, following
  /// §8.1 "Pushing a Root".
  ///
  /// - Skips a root already in the ring (step 1). The skip is required: a
  ///   re-pushed live root would overwrite its link and strand every root
  ///   after it. A repeated root keeps its original eviction schedule.
  /// - The first push creates the cursor with a self-linked root (step 2).
  /// - At capacity, evicts `oldest`; its link names the next oldest (step 3).
  /// - Links the previous `newest` to the new root, which links to itself
  ///   (steps 4 and 5), then rewrites the cursor (step 6).
  pure def settlementRingPush(
    s: ParachainServiceState, para: ParaId, root: StreamsRoot
  ): ParachainServiceState =
    if (s.messagesMember.keys().contains((para, root))) s
    else if (not(s.messagesCursor.keys().contains(para)))
      { ...s,
        messagesMember: s.messagesMember.put((para, root), root),
        messagesCursor: s.messagesCursor.put(para, { oldest: root, newest: root, len: 1 }) }
    else {
      val c = s.messagesCursor.get(para)
      val atCapacity = c.len == MaxSettlementRingCapacity
      val oldest = if (atCapacity) s.messagesMember.get((para, c.oldest)) else c.oldest
      val members =
        if (atCapacity) s.messagesMember.mapRemove((para, c.oldest)) else s.messagesMember
      { ...s,
        messagesMember: members.put((para, c.newest), root).put((para, root), root),
        messagesCursor: s.messagesCursor.put(para, {
          oldest: oldest,
          newest: root,
          len: if (atCapacity) c.len else c.len + 1
        }) }
    }

  /// Destroys a parachain's entire settlement ring (cursor and member list).
  ///
  /// Triggered exclusively by `parachain_clean_up` (§6.4). Note: forced head updates
  /// bypass this. While atomic in this model, the service walks the list from
  /// `oldest` (§8.1 "Ring Cleanup") and needs gas for `1 + MaxSettlementRingCapacity`
  /// reads and `1 + MaxSettlementRingCapacity` deletions in the worst case.
  pure def settlementRingDelete(
    s: ParachainServiceState, para: ParaId
  ): ParachainServiceState =
    { ...s,
      messagesCursor: s.messagesCursor.mapRemove(para),
      messagesMember: s.messagesMember.mapFilterKeys(k => k._1 != para) }
}
