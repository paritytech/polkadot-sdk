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

  /// Per-para baseline footprint: 4 205 B ParaInfo + 94 469 B log reserve.
  /// Full byte-by-byte derivation lives in design §6.1.
  pure val BaselineFootprint: int = 98674

  /// The single-referencer footprint a parachain pays for a solicited
  /// preimage of length `len`. Combines the entry-side cost (hash, len,
  /// singleton referencer set) with the underlying preimage data the
  /// Parachain Service is itself billed for by JAM. The exact figure here
  /// is an abstract proxy — what matters for the model is that it is a
  /// pure function of `len`, so two referencers of the same preimage pay
  /// identical amounts.
  pure def preimageFootprint(len: int): int =
  // 32 (hash key) + 4 (len field) + 4 (singleton referencer SCALE) +
  // 4 (entry length prefix) + len (preimage data)
  32 + 4 + 4 + 4 + len

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
  pure def initParaInfo(newTotal: Balance): Option[ParaInfo] =
  if (newTotal < BaselineFootprint)
  None
  else
  Some({
      headData: 0,
      validationCode: { hash: { vchBytes: 0 }, pinned: false },
      pendingUpgrade: None,
      totalStateBalance: newTotal,
      usedStateBalance: BaselineFootprint
  })

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
    if (s.hasReferencer(h, para_id)) {
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
        log: Some(InsufficientStateBalance(FromSolicit({ hash: h })))
      }
    } else {
      val wasEmpty = not(s.preimageRegistry.keys().contains(h)) or
      s.preimageRegistry.get(h).referencers == Set()
      val newEntry =
      if (s.preimageRegistry.keys().contains(h)) {
        val e = s.preimageRegistry.get(h)
        { ...e, referencers: e.referencers.union(Set(para_id)) }
      } else {
        { len: len, referencers: Set(para_id) }
      }
      {
        state: {
          ...s,
          parachains: s.parachains.put(para_id, pi.addUsed(delta)),
          preimageRegistry: s.preimageRegistry.put(h, newEntry)
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
    if (not(s.hasReferencer(h, para_id))) {
      // Idempotent: not a referencer, no-op (design §4.3 `forget`).
      { state: s, jamSolicit: false, jamForget: false, log: None }
    } else {
      val pi = s.paraInfo(para_id)
      val delta = preimageFootprint(len)
      val e = s.preimageRegistry.get(h)
      val rest = e.referencers.exclude(Set(para_id))
      val (registry, jamForget) =
      if (rest == Set())
      (s.preimageRegistry.mapRemove(h), true)
      else
      (s.preimageRegistry.put(h, { ...e, referencers: rest }), false)
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
}
