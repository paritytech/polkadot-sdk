// Incoming-transfer admission and bucketing, outgoing transfer
// supervision, and incoming-transfer chain integrity (design 5.1).
module transfers_test {
  import types.* from "../types"
  import messages.* from "../messages"
  import state.* from "../state"
  import state_balance.* from "../state_balance"
  import accumulate.* from "../accumulate"
  import state_vars.* from "../state_vars"
  import main.* from "../main"
  import invariants.* from "../invariants"
  import support.* from "./support"

  /// `MaxIncomingTransfers` is a reservation, not a cap (§5.1). Below it a
  /// transfer is recorded whatever it carries. At it the slot is unreserved, so
  /// only a transfer covering its own entry cost is recorded, and the queue then
  /// grows past the bound.
  run reservedPortionThenSelfFundedTest = {
    val full = fullTransferQueue
    val at = MaxIncomingTransfers   // a fresh slot past the fixture's tail
    // One short of the reservation: still inside it, so the entry is free and
    // fills it exactly.
    val edge = applyIncomingTransfer(almostFullTransferQueue, MaxIncomingTransfers - 1,
      { from: MkServiceId(0), amount: 0, memo: 0 })
    // Below the bound: a zero-amount transfer is still recorded.
    val below = applyIncomingTransfer(initialState, 7,
      { from: MkServiceId(0), amount: 0, memo: 0 })
    // At the bound: too little to pay for its own slot, so not recorded.
    val short = applyIncomingTransfer(full, at,
      { from: MkServiceId(0), amount: IncomingTransferEntryFootprint - 1, memo: 0 })
    // At the bound: exactly covers its slot, so recorded past the reserved portion.
    val paid = applyIncomingTransfer(full, at,
      { from: MkServiceId(0), amount: IncomingTransferEntryFootprint, memo: 0 })
    all {
      // One short of the reservation the entry is free, filling it exactly.
      assert(queuedTransferCount(edge) == MaxIncomingTransfers),
      // Empty queue: the new bucket is both endpoints.
      assert(below.incomingTransferChain == Some({ firstSlot: 7, lastSlot: 7, count: 1 })),
      assert(queuedTransferCount(below) == 1),
      assert(short == full),
      // Past the bound: a new bucket, chained from the old tail.
      assert(queuedTransferCount(paid) == MaxIncomingTransfers + 1),
      assert(paid.incomingTransferChain ==
        Some({ firstSlot: 0, lastSlot: at, count: MaxIncomingTransfers + 1 })),
      assert(paid.incomingTransfers.get(at - 1).nextSlot == Some(at)),
      assert(paid.incomingTransfers.get(at).nextSlot == None),
      init
    }
  }

  /// Arrivals sharing a slot append to the same bucket, adding no storage item.
  run sameSlotSharesBucketTest = {
    val a = applyIncomingTransfer(initialState, 5,
      { from: MkServiceId(0), amount: 500, memo: 0 })
    val b = applyIncomingTransfer(a, 5, { from: MkServiceId(3), amount: 600, memo: 0 })
    all {
      assert(b.incomingTransfers.keys() == Set(5)),
      assert(b.incomingTransfers.get(5).transfers.length() == 2),
      assert(b.incomingTransferChain == Some({ firstSlot: 5, lastSlot: 5, count: 2 })),
      assert(queuedTransferCount(b) == 2),
      init
    }
  }

  /// Every `TransferFailed` payload in `paraId`'s accumulate log, oldest first.
  pure def transferFailures(s: ParachainServiceState, paraId: ParaId):
    List[{ id: int, error: TransferError }] =
  s.paraLog(paraId).foldl(List(), (acc, pair) => match pair._2 {
    | AccumulateLogEntry(evs) => evs.foldl(acc, (a, e) => match e {
      | TransferFailed(p) => a.append(p)
      | _ => a
    })
    | RefineLogEntry(_) => acc
  })

  run transferOutPlainMoveNeedsSupervisionTest = {
    val assetHubCode: ValidationCodeHash = { vchBytes: 2 }
    val dest = MkServiceId(5)
    init
      // Plain move to another service → rejected.
      .then(applyHappyPaths(List((AssetHubParaId, assetHubCode,
        List(TransferOut({ source: None, dest: dest, amount: 100, id: 77,
          sourceSupervisorBalance: false, destSupervisorBalance: false,
          deferred: None }))))))
      // The caller-supplied id is echoed back untouched.
      .then(expectThat(svc.transferFailures(AssetHubParaId) == List(
        { id: 77, error: DestinationNotSupervised })))
      // Deferred transfer → no supervision needed, so no new failure appears.
      .then(applyHappyPaths(List((AssetHubParaId, assetHubCode,
        List(TransferOut({ source: None, dest: dest, amount: 100, id: 78,
          sourceSupervisorBalance: false, destSupervisorBalance: false,
          deferred: Some((7, 1000)) }))))))
      .then(expectThat(svc.transferFailures(AssetHubParaId).length() == 1))
      // Self-move into the service's own supervisor balance → permitted, so no
      // new failure appears.
      .then(applyHappyPaths(List((AssetHubParaId, assetHubCode,
        List(TransferOut({ source: None, dest: ParachainServiceId, amount: 100,
          id: 79, sourceSupervisorBalance: false, destSupervisorBalance: true,
          deferred: None }))))))
      .then(expectThat(svc.transferFailures(AssetHubParaId).length() == 1))
      // The reverse self-move spends the supervisor balance, so its outcome
      // depends on self-supervision the model abstracts away; no failure here.
      .then(applyHappyPaths(List((AssetHubParaId, assetHubCode,
        List(TransferOut({ source: None, dest: ParachainServiceId, amount: 100,
          id: 80, sourceSupervisorBalance: true, destSupervisorBalance: false,
          deferred: None }))))))
      .then(expectThat(svc.transferFailures(AssetHubParaId).length() == 1))
      // A foreign source always fails, but `UnknownSource` vs
      // `SourceNotSupervised` turns on existence the model abstracts away.
      .then(applyHappyPaths(List((AssetHubParaId, assetHubCode,
        List(TransferOut({ source: Some(dest), dest: MkServiceId(6),
          amount: 100, id: 81, sourceSupervisorBalance: false,
          destSupervisorBalance: false, deferred: None }))))))
      .then(expectThat(svc.transferFailures(AssetHubParaId).length() == 1))
      // Both selectors naming the regular balance on a self target is an
      // identity write.
      .then(applyHappyPaths(List((AssetHubParaId, assetHubCode,
        List(TransferOut({ source: None, dest: ParachainServiceId, amount: 100,
          id: 82, sourceSupervisorBalance: false, destSupervisorBalance: false,
          deferred: None }))))))
      .then(expectThat(svc.transferFailures(AssetHubParaId) == List(
        { id: 77, error: DestinationNotSupervised },
        { id: 82, error: DestinationNotSupervised })))
  }

  /// §5.1: draining refunds exactly what admission charged, so an unreserved
  /// entry cannot ratchet `used_state_balance` upward over time.
  run unreservedTransfersAreSelfFundedTest = {
    val ah = AssetHubParaId
    val ahCode: ValidationCodeHash = { vchBytes: 2 }
    val big = IncomingTransferEntryFootprint
    val full = fullTransferQueue
    val at = MaxIncomingTransfers
    val used0 = full.paraInfo(ah).usedStateBalance
    val total0 = full.paraInfo(ah).totalStateBalance
    // Deliberately far more than one entry costs, to pin that the surplus is not
    // credited to the allowance.
    val paid = applyIncomingTransfer(full, at, { from: MkServiceId(3), amount: big * 100, memo: 0 })
    // Drain the whole queue back out through the real accumulate path.
    val ok: ParachainWorkDigestOk = {
      paraId: ah,
      validationCode: { hash: ahCode, len: FixedCodeLen },
      parentHeadHash: headHash(paid.paraInfo(ah).headData),
      headData: paid.paraInfo(ah).headData + 1,
      upwardMessages: List(ConsumeTransfersUpTo(at)),
      lookupAnchor: at
    }
    val out = accumulateOnePackage(
      paid, { result: WorkOk(Ok(ok)), authTrace: 0 }, Set(), None, FixedCodeLen, at + 1, Map())
    all {
      // The reserved portion cost nothing beyond the baseline.
      assert(excessTransferFootprint(MaxIncomingTransfers) == 0),
      // Both move by the per-bucket cost, never by the transfer's `amount`.
      assert(paid.paraInfo(ah).usedStateBalance == used0 + big),
      assert(paid.paraInfo(ah).totalStateBalance == total0 + big),
      // Draining refunds exactly the charge on both, leaving no residue.
      assert(queuedTransferCount(out.state) == 0),
      assert(out.state.paraInfo(ah).usedStateBalance == used0),
      assert(out.state.paraInfo(ah).totalStateBalance == total0),
      init
    }
  }

  run chainMustNotSkipBucketTest =
    init
      // The skipping chain satisfies every per-bucket check...
      .then(setSvc(skippingChainState))
      .then(expectThat(all {
        svc.incomingTransfers.keys().forall(k =>
          (k == 3) == (svc.incomingTransfers.get(k).nextSlot == None)),
        svc.incomingTransfers.keys().forall(k =>
          match svc.incomingTransfers.get(k).nextSlot {
            | None => true
            | Some(nxt) => nxt > k and svc.incomingTransfers.keys().contains(nxt)
          }),
        // ...but the invariant rejects it, because bucket 2 is unreachable.
        not(incoming_transfers_chain_consistent)
      }))
      // A properly linked chain over the same buckets is accepted.
      .then(setSvc(linkedChainState))
      .then(expectThat(incoming_transfers_chain_consistent))
}
