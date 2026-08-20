// State-balance accounting (design §6.1).
//
// JAM bills the Parachain Service for everything it holds in state. The
// service re-attributes that obligation per parachain via `usedStateBalance`,
// using an "as if sole user" rule for every shared structure: each referencer
// pays the full encoded cost of a `(key, value)` pair containing only its own
// contribution. Existing parachains' contributions never need recomputing
// when set membership changes.
module state_balance {
  import types.* from "./types"
  import messages.* from "./messages"
  import state.* from "./state"

  // --- Per-parachain footprint primitives --------------------------------

   /// Per-para baseline footprint, in BALANCE UNITS: 4 262 (ParaInfo entry)
   /// + 65 585 (log reserve). Each is one storage item, so each carries
   /// `C_itemdeposit = 10` on top of its octets (4 252 / 65 575). Derivation in
   /// §6.1. Balances are `Compact<u128>` sized at their 17 B worst case here.
   pure val BaselineFootprint: int = 69847

  /// Octet overhead of one `BoundedVec` general-storage entry: JAM's 34 B, a
  /// 1 B map-tag key, and a 2 B compact length prefix (§6.1).
  pure val VecEntryOverhead: int = 34 + 1 + 2

  /// Asset Hub additionally owns the service-global state items (staged
  /// validator keys, pending assigns + their dirty-core index, incoming
  /// transfers). Asset Hub reserves this in its baseline, so those items are
  /// pre-provisioned rather than charged to `usedStateBalance` as they grow.
  pure val AssetHubGlobalItemsFootprint: int = {
    val stagedKeys = VecEntryOverhead + MaxStagedValidatorKeys * ValidatorKeySize
    // One storage entry per core: 34 + 3 (tag + u16 CoreIndex key) + 2 (compact
    // len) + the authorizer queue + 5 (Option<ServiceId>).
    val pendingAssigns =
      CoreCount * (34 + 3 + 2 + AuthorizerQueueLength * 32 + 5)
    // (u16 CoreIndex 2 + Timeslot 4) per core.
    val pendingAssignCores = VecEntryOverhead + CoreCount * 6
    // Head/tail pointer: 34 + 1 (key) + 1 (Option tag) + first_slot 4 + last_slot 4
    // + count 4.
    val transferChain = 34 + 1 + 1 + 4 + 4 + 4
    // Fixed storage items: the two BoundedVecs, the chain pointer, and one per
    // core for pendingAssigns. The transfer buckets are counted per transfer below.
    val fixedItems = 3 + CoreCount
    val fixed =
      stagedKeys + pendingAssigns + pendingAssignCores + transferChain + fixedItems * 10
    // Worst case one bucket per transfer, each costing a full bucket.
    fixed + MaxIncomingTransfers * IncomingTransferEntryFootprint
  }

  /// Asset Hub's baseline footprint: the generic per-para baseline plus its
  /// reserved service-global items (§6.1).
  pure val AssetHubBaselineFootprint: int = BaselineFootprint + AssetHubGlobalItemsFootprint

  /// Registration baseline for a para: Asset Hub reserves its service-global
  /// items on top of the generic baseline; every other para uses the generic
  /// baseline (§6.1).
  pure def baselineFor(paraId: ParaId): int =
    if (paraId == AssetHubParaId) AssetHubBaselineFootprint else BaselineFootprint

  /// The single-referencer footprint a parachain pays for a solicited preimage
  /// of length `len`, billed "as if sole user" (design §6.1), in BALANCE UNITS.
  /// Two JAM state contributions (Gray Paper, *Account Footprint and Threshold
  /// Balance*), each charged its items AND its octets:
  ///   a_requests (preimage lookup, keyed by (hash, len)):
  ///     2 items = 20, octets 81 + len
  ///   a_storage  (referencer-map entry): 1 item = 10, octets
  ///     34 + 5 (singleton {ParaId} set) + 37 (key = 1 B map tag + 32 B hash + 4 B len)
  /// Total = 187 + len balance units (octet subtotal 157 + len, items 30).
  pure def preimageFootprint(len: int): int =
  (20 + 81 + len) + (10 + 34 + 5 + 37)

  /// What Asset Hub is charged for the unreserved part of the transfer queue
  /// (§5.1), priced per worst-case bucket as §6.1 sizes the reservation itself.
  pure def excessTransferFootprint(count: int): int =
    if (count <= MaxIncomingTransfers) 0
    else (count - MaxIncomingTransfers) * IncomingTransferEntryFootprint

  /// Apply the change in that charge after `count` moved. The transfer funds the
  /// entry, so Asset Hub's allowance rises with the charge and falls with the
  /// refund: both move by the per-bucket cost and nothing else. Admission and
  /// draining both go through here, so the two cannot drift.
  pure def reattributeTransferQueue(
    s: ParachainServiceState, oldCount: int, newCount: int
  ): ParachainServiceState = {
    val delta = excessTransferFootprint(newCount) - excessTransferFootprint(oldCount)
    if (delta == 0) s
    else {
      val pi = s.paraInfo(AssetHubParaId)
      { ...s,
        parachains: s.parachains.put(AssetHubParaId,
          { ...pi,
            usedStateBalance: pi.usedStateBalance + delta,
            totalStateBalance: pi.totalStateBalance + delta }) }
    }
  }

  // --- Update primitives --------------------------------------------------

  pure def setUsed(pi: ParaInfo, newUsed: Balance): ParaInfo =
  { ...pi, usedStateBalance: newUsed }

  pure def setTotal(pi: ParaInfo, newTotal: Balance): ParaInfo =
  { ...pi, totalStateBalance: newTotal }

  pure def addUsed(pi: ParaInfo, delta: int): ParaInfo =
  setUsed(pi, pi.usedStateBalance + delta)

  pure def hasHeadroom(pi: ParaInfo, delta: int): bool =
  pi.usedStateBalance + delta <= pi.totalStateBalance

  // --- ParaInfo construction at registration -----------------------------
  //
  /// `parachain_set_state_balance` on a previously-unused ParaId creates the
  /// entry with the baseline footprint pre-charged (design §6.1, §6.2).
  pure def initParaInfo(paraId: ParaId, newTotal: Balance): Option[ParaInfo] = {
    val baseline = baselineFor(paraId)
    if (newTotal < baseline)
      None
    else
      Some({
        headData: 0,
        validationCode: None,
        pendingUpgrade: None,
        totalStateBalance: newTotal,
        usedStateBalance: baseline,
        isDeregistering: false
      })
  }

  // --- Preimage-registry referencer sharing (§6.1) ------------------------
  //
  // `solicit` is forwarded to JAM iff the referencer set transitions from
  // empty to non-empty. `forget` is forwarded iff the set transitions back
  // to empty. The model records `jamSolicit`/`jamForget` effects so the
  // top-level invariant can check JAM-side bookkeeping.

  type PreimageDelta = {
    // The mutated service state.
    state: ParachainServiceState,
    // `Some(hash)` iff JAM `solicit` should be invoked for that hash.
    jamSolicit: Option[Hash],
    // `Some(hash)` iff JAM `forget` should be invoked for that hash.
    jamForget: Option[Hash],
    // `true` iff `removeReferencer` kept the para as a referencer awaiting the
    // second, expunging forget (first forget of the last referencer, §6.1).
    retained: bool,
    // Optional log entry (`InsufficientStateBalance`).
    log: Option[AccumulateLog]
  }

  pure def addReferencer(
    s: ParachainServiceState, para_id: ParaId, h: Hash, len: int
  ): PreimageDelta = {
    val pi = s.paraInfo(para_id)
    val delta = preimageFootprint(len)
    // A `solicit` while the request is `Unrequested(y)` RESCUES the still-held
    // blob back to available (JAM `Unrequested → Rerequested`), keeping the
    // original `y` for the next forget's gate. This fires a real JAM `solicit`.
    // `rescueSlot` is `Some(y)` exactly when this solicit is a rescue, so it
    // doubles as the rescue flag and carries `y` for the status transition.
    val rescueSlot: Option[Timeslot] =
      if (s.preimageStatus.keys().contains((h, len)))
        match s.preimageStatus.get((h, len)) {
          | Unrequested(y) => Some(y)
          | _ => None
        }
      else None
    val isRescue = rescueSlot != None
    val rescuedStatus =
      match rescueSlot {
        | Some(y) => s.preimageStatus.put((h, len), Rerequested(y))
        | None => s.preimageStatus
      }
    if (s.hasReferencer(h, len, para_id)) {
      // Already a referencer: idempotent, needs no extra state balance
      // (footprint already charged). But if the request is `Unrequested` (the
      // retained para re-soliciting its own code in limbo), the service still
      // must fire JAM `solicit` to rescue it, else the para cannot look it up.
      if (isRescue)
        { state: { ...s, preimageStatus: rescuedStatus },
          jamSolicit: Some(h), jamForget: None, retained: false, log: None }
      else
        { state: s, jamSolicit: None, jamForget: None, retained: false, log: None }
    } else if (not(pi.hasHeadroom(delta))) {
      // Insufficient headroom for a genuinely new reference: no state
      // change, log it.
      {
        state: s,
        jamSolicit: None,
        jamForget: None,
        retained: false,
        log: Some(InsufficientStateBalance(FromSolicit({ hash: h, len: len })))
      }
    } else {
      val wasEmpty = not(s.preimageRegistry.keys().contains((h, len))) or
      s.preimageRegistry.get((h, len)).referencers == Set()
      // On a rescue, the existing referencers are only the retained ghost(s)
      // from the two-step forget (`Unrequested` has no live referencer). The
      // soliciting para becomes the sole LIVE referencer, so refund and drop
      // every ghost, keeping them charged would double-count one JAM request
      // entry (§6.1). A fresh solicit / non-rescue join has no ghosts to clear.
      val ghosts =
        if (isRescue) s.preimageRegistry.get((h, len)).referencers else Set()
      val parasAfterRefund =
        ghosts.fold(s.parachains, (acc, g) =>
          acc.put(g, acc.get(g).addUsed(-preimageFootprint(len))))
      // Rescue drops the ghost(s), leaving the soliciting para alone. Otherwise
      // add the para to whatever set already exists (or a fresh singleton).
      val existing =
        if (s.preimageRegistry.keys().contains((h, len)))
          s.preimageRegistry.get((h, len)).referencers
        else Set()
      val newEntry =
        { referencers: (if (isRescue) Set() else existing).union(Set(para_id)) }
      // A fresh JAM `solicit` (empty → non-empty) starts `Unprovided`. A new
      // referencer joining an `Unrequested(y)` entry rescues it to
      // `Rerequested(y)` (fires JAM `solicit`). Any other join leaves the
      // status untouched (already `Unprovided`/`Provided`/`Rerequested`).
      val newStatus =
        if (wasEmpty) s.preimageStatus.put((h, len), Unprovided)
        else rescuedStatus
      {
        state: {
          ...s,
          parachains: parasAfterRefund.put(para_id, parasAfterRefund.get(para_id).addUsed(delta)),
          preimageRegistry: s.preimageRegistry.put((h, len), newEntry),
          preimageStatus: newStatus
        },
        jamSolicit: if (wasEmpty or isRescue) Some(h) else None,
        jamForget: None,
        retained: false,
        log: None
      }
    }
  }

  // Release a referencer via JAM's `forget` (design §6.1). `now` is the current
  // timeslot, needed to gate the expunging second forget. 
  pure def removeReferencer(
    s: ParachainServiceState, para_id: ParaId, h: Hash, len: int, now: Timeslot
  ): PreimageDelta = {
    if (not(s.hasReferencer(h, len, para_id))) {
      // Idempotent: not a referencer, no-op (design §4.3 `forget`).
      { state: s, jamSolicit: None, jamForget: None, retained: false, log: None }
    } else {
      val pi = s.paraInfo(para_id)
      val delta = preimageFootprint(len)
      val e = s.preimageRegistry.get((h, len))
      val rest = e.referencers.exclude(Set(para_id))
      if (rest != Set()) {
        // A non-last referencer leaves immediately: the remaining referencers
        // still cover the live JAM request, so refund now, no JAM forget.
        {
          state: {
            ...s,
            parachains: s.parachains.put(para_id, pi.addUsed(-delta)),
            preimageRegistry: s.preimageRegistry.put((h, len), { ...e, referencers: rest })
          },
          jamSolicit: None,
          jamForget: None,
          retained: false,
          log: None
        }
      } else {
        // Last referencer leaves: JAM `forget` behaviour depends on whether the
        // preimage was ever provided (its `query` status).
        val expunged = {
          // Drop / Expunge: JAM removes the request. Refund and drop the entry.
          state: {
            ...s,
            parachains: s.parachains.put(para_id, pi.addUsed(-delta)),
            preimageRegistry: s.preimageRegistry.mapRemove((h, len)),
            preimageStatus: s.preimageStatus.mapRemove((h, len))
          },
          jamSolicit: None,
          jamForget: Some(h),
          retained: false,
          log: None
        }
        match s.preimageStatus.get((h, len)) {
          | Unprovided =>
            // Never provided → single `forget` drops it, deposit lifted now.
            expunged
          | Provided =>
            // Available → first `forget` only unrequests (JAM keeps it for the
            // turnaround). Keep the referencer (still charged), record the
            // unrequest slot, log the expunge deadline.
            {
              state: { ...s, preimageStatus: s.preimageStatus.put((h, len), Unrequested(now)) },
              jamSolicit: None,
              jamForget: Some(h),
              retained: true,
              log: Some(ForgetAgainAt({ hash: h, len: len, due: now + ExpungePeriod }))
            }
          | Unrequested(since) =>
            // Second forget: expunge only once strictly past the turnaround
            // (JAM `now > since + min_turnaround`); otherwise a no-op retry.
            val due = since + ExpungePeriod
            if (now > due) expunged
            else {
              state: s,
              jamSolicit: None,
              jamForget: None,
              retained: true,
              log: Some(ForgetAgainAt({ hash: h, len: len, due: due }))
            }
          | Rerequested(since) =>
            // Rescued preimage. JAM gates `forget` against the ORIGINAL
            // `unrequested_since`: past the turnaround it unrequests again
            // (→ `Unrequested(now)`, fresh clock); otherwise a no-op retry
            // (JAM `NotYetUnrequest`). Either way it does NOT expunge here.
            val due = since + ExpungePeriod
            if (now > due) {
              state: { ...s, preimageStatus: s.preimageStatus.put((h, len), Unrequested(now)) },
              jamSolicit: None,
              jamForget: Some(h),
              retained: true,
              log: Some(ForgetAgainAt({ hash: h, len: len, due: now + ExpungePeriod }))
            } else {
              state: s,
              jamSolicit: None,
              jamForget: None,
              retained: true,
              log: Some(ForgetAgainAt({ hash: h, len: len, due: due }))
            }
        }
      }
    }
  }

  // --- Parachain-storage KV accounting (§6.1) ----------------------------

  /// Per-entry footprint for a `key_value_storage` write of a `kLen`-byte key
  /// and `vLen`-byte value, per §6.1, in balance units:
  /// 10 (1 storage item)
  /// + 34 (JAM overhead) + compactLen(vLen) + vLen (SCALE Vec<u8> value)
  /// + 1 (map tag 0x07) + 4 (ParaId) + compactLen(kLen) + kLen (key)
  /// = 49 + compactLen(kLen) + kLen + compactLen(vLen) + vLen
  pure def kvEntryFootprint(kLen: int, vLen: int): int =
    10 + 39 + compactLen(kLen) + kLen + compactLen(vLen) + vLen

  type KVDelta = {
    state: ParachainServiceState,
    log: Option[AccumulateLog]
  }

  /// Replay a `SetKV`: upsert `keyValueStorage[(para_id, key)]`, delta-charged
  /// per §6.1. Rejected (state unchanged, `FromSetKV` logged) when a positive
  /// delta exceeds headroom.
  pure def applySetKV(
    s: ParachainServiceState, para_id: ParaId, key: List[int], value: List[int]
  ): KVDelta = {
    val pi = s.paraInfo(para_id)
    val newVBytes = compactLen(value.length()) + value.length()
    val entryExists = s.keyValueStorage.keys().contains((para_id, key))
    val delta =
      if (not(entryExists))
        kvEntryFootprint(key.length(), value.length())
      else {
        val oldV = s.keyValueStorage.get((para_id, key))
        newVBytes - (compactLen(oldV.length()) + oldV.length())
      }
    if (delta > 0 and not(pi.hasHeadroom(delta))) {
      {
        state: s,
        log: Some(InsufficientStateBalance(FromSetKV({
          keyHash: listHash(key)
        })))
      }
    } else {
      {
        state: {
          ...s,
          parachains: s.parachains.put(para_id, pi.addUsed(delta)),
          keyValueStorage: s.keyValueStorage.put((para_id, key), value)
        },
        log: None
      }
    }
  }

  /// Replay a `RemoveKV`: drop `keyValueStorage[(para_id, key)]` and refund its
  /// `kvEntryFootprint`. No-op on absent keys.
  pure def applyRemoveKV(
    s: ParachainServiceState, para_id: ParaId, key: List[int]
  ): KVDelta = {
    if (not(s.keyValueStorage.keys().contains((para_id, key)))) {
      { state: s, log: None }
    } else {
      val pi = s.paraInfo(para_id)
      val v = s.keyValueStorage.get((para_id, key))
      val refund = kvEntryFootprint(key.length(), v.length())
      {
        state: {
          ...s,
          parachains: s.parachains.put(para_id, pi.addUsed(-refund)),
          keyValueStorage: s.keyValueStorage.mapRemove((para_id, key))
        },
        log: None
      }
    }
  }
}
