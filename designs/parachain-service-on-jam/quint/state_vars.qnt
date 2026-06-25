// Shared state-variable declarations.
//
// Quint vars are module-scoped, but importers see the same underlying
// instance. Declaring all state variables in one module gives every
// consumer (the driver in `main.qnt`, the invariants in `invariants.qnt`,
// and any future state-reading helpers) a single point of truth and lets
// invariants be nullary `val: bool` definitions — directly targetable as
// `--invariant=<name>` on the CLI without per-state wrapper boilerplate.
module state_vars {
  import types.* from "./types"
  import messages.* from "./messages"
  import state.* from "./state"

  var svc: ParachainServiceState
  var now: Timeslot

  /// JAM `stagingset` mirror so we can observe `designate` effects.
  var jamStagingSet: List[ValidatorKey]

  /// Privileged service IDs (modeled as parachain IDs in this skeleton).
  var coretimeChain: ParaId
  var assetHub: ParaId

  /// Ghost state: every `(para, hash)` for which Accumulate has processed
  /// a `Solicit` upward message without a matching `Forget`. Used only by
  /// `solicit_implies_registry`. Not part of the protocol.
  var solicitedSet: Set[(ParaId, Hash)]

  /// Ghost state: the service state as it was *before* the current step.
  /// Lets invariants relate consecutive states (transition invariants),
  /// e.g. `code_activation_releases_old`. Not part of the protocol.
  var prevSvc: ParachainServiceState

  /// Ghost state: the (ordered) refine work results from the last refine
  /// step. Empty for non-refine steps. Lets invariants check that the
  /// state reflects what the work results declared — e.g. a para whose
  /// head changed must have a matching `Ok` result. The Rust harness has
  /// the same data from the trace, so invariants over this ghost transfer
  /// directly. Not part of the protocol.
  var lastStepWorkResults: List[ParachainWorkResult]
}
