// Shared state-variable declarations.
//
// Quint vars are module-scoped, but importers see the same underlying
// instance. Declaring all state variables in one module gives every
// consumer (the driver in `main.qnt`, the invariants in `invariants.qnt`,
// and any future state-reading helpers) a single point of truth and lets
// invariants be nullary `val: bool` definitions, directly targetable as
// `--invariant=<name>` on the CLI without per-state wrapper boilerplate.
module state_vars {
  import types.* from "./types"
  import messages.* from "./messages"
  import state.* from "./state"

  var svc: ParachainServiceState
  var now: Timeslot

  /// JAM `stagingset` mirror so we can observe `designate` effects.
  var jamStagingSet: List[ValidatorKey]


  /// Ghost state: every `(para_id, hash, len)` for which Accumulate has
  /// processed a `Solicit` upward message without a matching `Forget`.
  /// Stale entries for cleaned-up paras are intentionally retained. Used
  /// only by `solicit_implies_registry`. Not part of the protocol.
  var solicitedSet: Set[(ParaId, Hash, int)]

  /// Ghost state: the service state as it was *before* the current step.
  /// Lets invariants relate consecutive states (transition invariants),
  /// e.g. `code_activation_releases_old`. Not part of the protocol.
  var prevSvc: ParachainServiceState

  /// Ghost state: the (ordered) refine work results from the last refine
  /// step. Empty for non-refine steps. Lets invariants check that the
  /// state reflects what the work results declared, e.g. a para whose
  /// head changed must have a matching `Ok` result. The Rust harness has
  /// the same data from the trace, so invariants over this ghost transfer
  /// directly. Not part of the protocol.
  var lastStepWorkResults: List[WorkResult]

  /// Ghost state: JAM `assign` calls made during the last step (`set_assigner`
  /// updates and `set_authorizer_queue` flushes at their `jam_slot`). Lets
  /// invariants verify `new_assigner` is forwarded. Not part of the protocol.
  var lastStepAssigns: List[AssignCall]

  /// The 32-byte value `accumulate` returned for the last step: the head
  /// commitment of §5.5, or `None` when no head changed. Not ghost state: it
  /// is the service's actual accumulate output, but only the most recent one
  /// is retained, which is all `head_commitment_matches_changed_heads` needs.
  var lastHeadRoot: Option[MerkleHash]

  /// Ghost state: per-para lookup-anchor timeslot the `parachain_log` was
  /// last pruned against (§5.1). After a candidate for `p` is accumulated,
  /// entries below `logPrunedBelow[p]` have been dropped, so every remaining
  /// entry's timeslot is `>= logPrunedBelow[p]`. Checked by
  /// `parachain_log_above_lookup`. Not part of the protocol (the service
  /// prunes against the candidate's anchor each block without storing it).
  var logPrunedBelow: ParaId -> Timeslot
}
