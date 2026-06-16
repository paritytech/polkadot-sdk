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
  //
  /// Modeled as a map keyed by timeslot; the bound (ParachainLogCapacity = 8)
  /// is enforced by an invariant in `main.qnt`, not by the type.
  type ParachainLog = Timeslot -> LogEntry

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
    stagedValidatorKeys: List[ValidatorKey]
  }

  /// The "global" state we track alongside the service state: the current
  /// JAM timeslot, the JAM `stagingset` (so we can observe `designate`
  /// effects), and which service ids are which special chain.
  type Globals = {
    now: Timeslot,
    jamStagingSet: List[ValidatorKey],
    // The service IDs of the two privileged chains (design §6, §7).
    coretimeChain: ServiceId,
    assetHub: ServiceId
  }

  // --- Constructors ------------------------------------------------------

  pure val emptyState: ParachainServiceState = {
    parachains: Map(),
    incomingTransfers: List(),
    parachainLog: Map(),
    preimageRegistry: Map(),
    pendingAuthorizerQueues: Map(),
    lastAuthorizerAssignment: Map(),
    stagedValidatorKeys: List()
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
}
