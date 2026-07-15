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

   /// Per-para baseline footprint: 4 248 B ParaInfo + 65 575 B log reserve.
   /// Derivation in §6.1.
   pure val BaselineFootprint: int = 69823

  /// Asset Hub additionally owns the service-global state items (staged
  /// validator keys, pending authorizer queues + cores, incoming transfers),
  /// worst-cased at 1 381 508 B in §6.1. Asset Hub reserves this in its baseline,
  /// so those items are pre-provisioned rather than charged to
  /// `usedStateBalance` as they grow.
  pure val AssetHubGlobalItemsFootprint: int = 1381508

  /// Asset Hub's baseline footprint: the generic per-para baseline plus its
  /// reserved service-global items (§6.1).
  pure val AssetHubBaselineFootprint: int = BaselineFootprint + AssetHubGlobalItemsFootprint

  /// Registration baseline for a para: Asset Hub reserves its service-global
  /// items on top of the generic baseline; every other para uses the generic
  /// baseline (§6.1).
  pure def baselineFor(paraId: ParaId): int =
    if (paraId == AssetHubParaId) AssetHubBaselineFootprint else BaselineFootprint

  /// The single-referencer footprint a parachain pays for a solicited
  /// preimage of length `len`, billed "as if sole user" (design §6.1).
  /// Two JAM state contributions (Gray Paper, *Account Footprint and
  /// Threshold Balance*):
  ///   a_requests (preimage lookup, keyed by (hash, len)): 81 + len
  ///   a_storage  (referencer-map entry): 34 + |value| + |key|
  ///              = 34 + 5 (singleton {ParaId} set) + 37 (storage key = 1 B map tag + 32 B hash + 4 B len)
  /// Total = 157 + len bytes. Byte term only; JAM's per-item balance
  /// term (B_I * items) is not modelled — matching the baseline byte proxy.
  pure def preimageFootprint(len: int): int =
  (81 + len) + (34 + 5 + 37)

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
        validationCode: { ref: { hash: { vchBytes: 0 }, len: 0 }, pinned: false },
        pendingUpgrade: None,
        totalStateBalance: newTotal,
        usedStateBalance: baseline
      })
  }

  // --- Preimage-registry referencer multiplexing (§6.1) -------------------
  //
  // `solicit` is forwarded to JAM iff the referencer set transitions from
  // empty to non-empty. `forget` is forwarded iff the set transitions back
  // to empty. The model records `jamSolicit`/`jamForget` effects so the
  // top-level invariant can check JAM-side bookkeeping.

  type PreimageDelta = {
    // The mutated service state.
    state: ParachainServiceState,
    // Whether JAM `solicit` should be invoked for `hash`.
    jamSolicit: bool,
    // Whether JAM `forget` should be invoked for `hash`.
    jamForget: bool,
    // Optional log entry (`InsufficientStateBalance`).
    log: Option[AccumulateLog]
  }

  pure def addReferencer(
    s: ParachainServiceState, para_id: ParaId, h: Hash, len: int
  ): PreimageDelta = {
    val pi = s.paraInfo(para_id)
    val delta = preimageFootprint(len)
    if (s.hasReferencer(h, len, para_id)) {
      // Already a referencer — idempotent, and requires no extra state
      // balance (the footprint is already charged). Checked before the
      // headroom test so a para at its balance limit can still re-reference
      // a preimage it already holds (design §6.1).
      { state: s, jamSolicit: false, jamForget: false, log: None }
    } else if (not(pi.hasHeadroom(delta))) {
      // Insufficient headroom for a genuinely new reference — no state
      // change, log it.
      {
        state: s,
        jamSolicit: false,
        jamForget: false,
        log: Some(InsufficientStateBalance(FromSolicit({ hash: h, len: len })))
      }
    } else {
      val wasEmpty = not(s.preimageRegistry.keys().contains((h, len))) or
      s.preimageRegistry.get((h, len)).referencers == Set()
      val newEntry =
      if (s.preimageRegistry.keys().contains((h, len))) {
        val e = s.preimageRegistry.get((h, len))
        { ...e, referencers: e.referencers.union(Set(para_id)) }
      } else {
        { referencers: Set(para_id) }
      }
      {
        state: {
          ...s,
          parachains: s.parachains.put(para_id, pi.addUsed(delta)),
          preimageRegistry: s.preimageRegistry.put((h, len), newEntry)
        },
        jamSolicit: wasEmpty,
        jamForget: false,
        log: None
      }
    }
  }

  pure def removeReferencer(
    s: ParachainServiceState, para_id: ParaId, h: Hash, len: int
  ): PreimageDelta = {
    if (not(s.hasReferencer(h, len, para_id))) {
      // Idempotent: not a referencer, no-op (design §4.3 `forget`).
      { state: s, jamSolicit: false, jamForget: false, log: None }
    } else {
      val pi = s.paraInfo(para_id)
      val delta = preimageFootprint(len)
      val e = s.preimageRegistry.get((h, len))
      val rest = e.referencers.exclude(Set(para_id))
      val (registry, jamForget) =
      if (rest == Set())
      (s.preimageRegistry.mapRemove((h, len)), true)
      else
      (s.preimageRegistry.put((h, len), { ...e, referencers: rest }), false)
      {
        state: {
          ...s,
          parachains: s.parachains.put(para_id, pi.addUsed(-delta)),
          preimageRegistry: registry
        },
        jamSolicit: false,
        jamForget: jamForget,
        log: None
      }
    }
  }

  // --- Parachain-storage KV accounting (§6.1) ----------------------------

  /// Per-entry footprint for a `key_value_storage` write of a `kLen`-byte
  /// key and `vLen`-byte value, per §6.1:
  /// 34 (JAM overhead) + compactLen(vLen) + vLen (SCALE Vec<u8> value)
  /// + 1 (map tag 0x07) + 4 (ParaId) + compactLen(kLen) + kLen (key)
  /// = 39 + compactLen(kLen) + kLen + compactLen(vLen) + vLen
  pure def kvEntryFootprint(kLen: int, vLen: int): int =
    39 + compactLen(kLen) + kLen + compactLen(vLen) + vLen

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
