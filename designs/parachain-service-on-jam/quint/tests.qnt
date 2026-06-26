// Quint `run` tests. Driven via `quint test --backend=typescript tests.qnt`.
//
// Tests live here so they can be exercised independently of the main
// model; they import the same state, types, and actions as `main.qnt`.
module tests {
  import types.* from "./types"
  import messages.* from "./messages"
  import state.* from "./state"
  import state_vars.* from "./state_vars"
  import main.* from "./main"

  // --- Bounce-on-full -----------------------------------------------------

  /// Pre-built `incoming_transfers` at capacity for `testBounceOnFull`.
  /// Order is unspecified (Set.fold); the test only asserts on length and
  /// on the bounce ghost.
  pure val fullIncomingTransfersFixture: List[{
    from: ServiceId, amount: Balance, memo: Memo
  }] =
    0.to(IncomingTransfersMaxLen - 1).fold(List(), (acc, i) =>
      acc.append({ from: MkServiceId(0), amount: i, memo: 0 }))

  /// Sets `svc.incomingTransfers` to the full fixture without going through
  /// `stepIncomingTransfer` (which would take 1 000 steps).
  action fillIncomingTransfers = all {
    svc' = { ...svc, incomingTransfers: fullIncomingTransfersFixture },
    now' = now,
    jamStagingSet' = jamStagingSet,
    coretimeChain' = coretimeChain,
    assetHub' = assetHub,
    solicitedSet' = solicitedSet,
    prevSvc' = svc,
    lastStepWorkResults' = lastStepWorkResults,
    lastStepBouncedTransfers' = lastStepBouncedTransfers
  }

  /// Guard-only action wrapping the assertion (plain `assert` has
  /// incompatible effects with state-updating actions in `then`).
  action expectBounceOnFull = all {
    svc.incomingTransfers.length() == IncomingTransfersMaxLen,
    lastStepBouncedTransfers.length() == 1,
    svc' = svc,
    now' = now,
    jamStagingSet' = jamStagingSet,
    coretimeChain' = coretimeChain,
    assetHub' = assetHub,
    solicitedSet' = solicitedSet,
    prevSvc' = prevSvc,
    lastStepWorkResults' = lastStepWorkResults,
    lastStepBouncedTransfers' = lastStepBouncedTransfers
  }

  /// With `incoming_transfers` at capacity, the next ingress must bounce:
  /// the queue length must NOT grow, and exactly one entry must appear
  /// in `lastStepBouncedTransfers`.
  run testBounceOnFull =
    init
      .then(fillIncomingTransfers)
      .then(stepIncomingTransfer)
      .then(expectBounceOnFull)
}
