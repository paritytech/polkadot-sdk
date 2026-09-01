// Incoming-transfer admission and bucketing, outgoing transfer
// supervision, and incoming-transfer bucket integrity (design 5.1).
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
  /// transfer is recorded whatever it carries. At it the entry is unreserved, so
  /// only a transfer covering its own entry cost is recorded, and the queue then
  /// grows past the bound.
  run reservedPortionThenSelfFundedTest = {
    val full = fullTransferQueue
    // The fixture holds ids 0 .. MaxIncomingTransfers - 1, so a fresh arrival
    // opens exactly this id.
    val fresh = MaxIncomingTransfers
    // One short of the reservation: still inside it, so the entry is free and
    // fills it exactly.
    val edge = applyIncomingTransfer(almostFullTransferQueue,
      { from: MkServiceId(0), amount: 0, memo: 0 })
    // Below the bound: a zero-amount transfer is still recorded.
    val below = applyIncomingTransfer(initialState,
      { from: MkServiceId(0), amount: 0, memo: 0 })
    // At the bound: too little to pay for its own entry, so not recorded.
    val short = applyIncomingTransfer(full,
      { from: MkServiceId(0), amount: IncomingTransferEntryFootprint - 1, memo: 0 })
    // At the bound: exactly covers its entry, so recorded past the reserved portion.
    val paid = applyIncomingTransfer(full,
      { from: MkServiceId(0), amount: IncomingTransferEntryFootprint, memo: 0 })
    all {
      // One short of the reservation the entry is free, filling it exactly.
      assert(queuedTransferCount(edge) == MaxIncomingTransfers),
      // Empty queue: ids restart at 0, and the new bucket is both endpoints.
      assert(below.incomingTransferBuckets ==
        Some({ firstBucket: 0, lastBucket: 0, count: 1 })),
      assert(queuedTransferCount(below) == 1),
      assert(short == full),
      // Past the bound: the next id past the old tail.
      assert(queuedTransferCount(paid) == MaxIncomingTransfers + 1),
      assert(paid.incomingTransferBuckets ==
        Some({ firstBucket: 0, lastBucket: fresh, count: MaxIncomingTransfers + 1 })),
      assert(paid.incomingTransfers.get(fresh).length() == 1),
      init
    }
  }

  /// Transfers arriving in ONE accumulate invocation share the bucket it opens,
  /// adding no storage item. A later invocation cannot reopen that bucket, so its
  /// arrivals get a fresh id: that is what closes a bucket and makes clean-up safe.
  run oneInvocationSharesBucketTest = {
    val t1 = { from: MkServiceId(0), amount: 500, memo: 0 }
    val t2 = { from: MkServiceId(3), amount: 600, memo: 0 }
    // One invocation carrying both.
    val together = applyIncomingTransfers(initialState, List(t1, t2))
    // Two invocations carrying one each.
    val apart = applyIncomingTransfer(applyIncomingTransfer(initialState, t1), t2)
    all {
      assert(together.incomingTransfers == Map(0 -> List(t1, t2))),
      assert(together.incomingTransferBuckets ==
        Some({ firstBucket: 0, lastBucket: 0, count: 2 })),
      assert(queuedTransferCount(together) == 2),
      // Closed by the invocation boundary, so `t2` lands in bucket 1.
      assert(apart.incomingTransfers == Map(0 -> List(t1), 1 -> List(t2))),
      assert(apart.incomingTransferBuckets ==
        Some({ firstBucket: 0, lastBucket: 1, count: 2 })),
      init
    }
  }

  /// §3.1: a bucket never holds more than `MaxTransfersPerBucket`, so a single
  /// invocation delivering more than that spills into fresh ids. This is what
  /// bounds the cost of reading any one bucket, and why a bucket id cannot be the
  /// arrival timeslot.
  run bucketSpillsAtCapacityTest = {
    val batch = range(0, MaxTransfersPerBucket + 1).foldl(List(), (acc, i) =>
      acc.append({ from: MkServiceId(0), amount: 500, memo: i }))
    val filled = applyIncomingTransfers(initialState, batch)
    all {
      // One past capacity in a single invocation: two buckets.
      assert(filled.incomingTransfers.keys() == Set(0, 1)),
      assert(filled.incomingTransfers.get(0).length() == MaxTransfersPerBucket),
      assert(filled.incomingTransfers.get(1).length() == 1),
      assert(filled.incomingTransferBuckets ==
        Some({ firstBucket: 0, lastBucket: 1, count: MaxTransfersPerBucket + 1 })),
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

  /// §5.1: cleaning up refunds exactly what admission charged, so an unreserved
  /// entry cannot ratchet `used_state_balance` upward over time.
  run unreservedTransfersAreSelfFundedTest = {
    val ah = AssetHubParaId
    val ahCode: ValidationCodeHash = { vchBytes: 2 }
    val big = IncomingTransferEntryFootprint
    val full = fullTransferQueue
    // The fixture holds ids 0 .. MaxIncomingTransfers - 1, so the arrival below
    // opens this id and cleaning up to it empties the queue.
    val fresh = MaxIncomingTransfers
    val used0 = full.paraInfo(ah).usedStateBalance
    val total0 = full.paraInfo(ah).totalStateBalance
    // Deliberately far more than one entry costs, to pin that the surplus is not
    // credited to the allowance.
    val paid = applyIncomingTransfer(full, { from: MkServiceId(3), amount: big * 100, memo: 0 })
    // Clean the whole queue back out through the real accumulate path.
    val ok: ParachainWorkDigestOk = {
      paraId: ah,
      validationCode: { hash: ahCode, len: FixedCodeLen },
      parentHeadHash: headHash(paid.paraInfo(ah).headData),
      headData: paid.paraInfo(ah).headData + 1,
      upwardMessages: List(CleanUpBucketsUpTo(fresh)),
      lookupAnchor: fresh,
      specMsgProvides: None,
      specMsgRequires: List()
    }
    val out = accumulateOnePackage(
      paid, { result: WorkOk(Ok(ok)), authTrace: 0 }, Set(), None, FixedCodeLen,
      fresh + 1, Map())
    all {
      // The reserved portion cost nothing beyond the baseline.
      assert(excessTransferFootprint(MaxIncomingTransfers) == 0),
      // Both move by the per-bucket cost, never by the transfer's `amount`.
      assert(paid.paraInfo(ah).usedStateBalance == used0 + big),
      assert(paid.paraInfo(ah).totalStateBalance == total0 + big),
      // Cleaning up refunds exactly the charge on both, leaving no residue.
      assert(queuedTransferCount(out.state) == 0),
      assert(out.state.paraInfo(ah).usedStateBalance == used0),
      assert(out.state.paraInfo(ah).totalStateBalance == total0),
      init
    }
  }

  run bucketIdsMustBeContiguousTest =
    init
      // The gapped queue satisfies every endpoint check...
      .then(setSvc(gappedBucketsState))
      .then(expectThat(all {
        svc.incomingTransfers.keys().contains(1),
        svc.incomingTransfers.keys().contains(4),
        svc.incomingTransfers.keys().forall(k => k >= 1 and k <= 4),
        // ...but the invariant rejects it: bucket 3 is missing, so a walk of
        // `firstBucket ..= lastBucket` would hit a hole.
        not(incoming_transfers_buckets_consistent)
      }))
      // A contiguous queue over the same endpoints is accepted.
      .then(setSvc(contiguousBucketsState))
      .then(expectThat(incoming_transfers_buckets_consistent))
}
