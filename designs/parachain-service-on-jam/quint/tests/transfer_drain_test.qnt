// Cleaning up the incoming-transfer queue: `clean_up_buckets_up_to`, bucket
// closing, and the refund side of admission (design 5.1).
module transfer_drain_test {
  import types.* from "../types"
  import messages.* from "../messages"
  import state.* from "../state"
  import state_balance.* from "../state_balance"
  import accumulate.* from "../accumulate"
  import main.* from "../main"
  import support.* from "./support"

  /// `clean_up_buckets_up_to(bucket_id)` removes whole buckets up to and
  /// including `bucket_id`, re-heads the queue, and clears the endpoints once
  /// nothing survives. Ids then restart at 0, so a drained queue leaves no
  /// numbering to carry forward.
  run cleanUpRemovesBucketsUpToIdTest = {
    // Three separate invocations, so three closed buckets: 0, 1, 2.
    val a = applyIncomingTransfer(initialState,
      { from: MkServiceId(0), amount: 500, toSupervisorBalance: false, memo: 0 })
    val b = applyIncomingTransfer(a, { from: MkServiceId(0), amount: 500, toSupervisorBalance: false, memo: 1 })
    val c = applyIncomingTransfer(b, { from: MkServiceId(0), amount: 500, toSupervisorBalance: false, memo: 2 })
    val partial = cleanUpBucketsUpTo(c, 1)
    val everything = cleanUpBucketsUpTo(c, 2)
    val refilled = applyIncomingTransfer(everything,
      { from: MkServiceId(0), amount: 500, toSupervisorBalance: false, memo: 3 })
    all {
      assert(c.incomingTransfers.keys() == Set(0, 1, 2)),
      assert(c.incomingTransferBuckets ==
        Some({ firstBucket: 0, lastBucket: 2, count: 3 })),
      assert(partial.incomingTransfers.keys() == Set(2)),
      assert(partial.incomingTransferBuckets ==
        Some({ firstBucket: 2, lastBucket: 2, count: 1 })),
      assert(everything.incomingTransfers.keys() == Set()),
      assert(everything.incomingTransferBuckets == None),
      // Fully drained, so the next arrival numbers from 0 again.
      assert(refilled.incomingTransfers.keys() == Set(0)),
      assert(refilled.incomingTransferBuckets ==
        Some({ firstBucket: 0, lastBucket: 0, count: 1 })),
      init
    }
  }

  /// §5.1: naming a bucket id cannot destroy a transfer the candidate never saw.
  /// A bucket only takes arrivals within the invocation that opened it, so
  /// anything arriving after the candidate's read lands in a strictly higher id.
  /// This is why `clean_up_buckets_up_to` needs no lookup-anchor clamp: here the
  /// candidate read buckets 0 and 1, a third arrived meanwhile, and cleaning up
  /// to 1 leaves that third one untouched.
  run cleanUpCannotReachLaterBucketsTest = {
    val ah = AssetHubParaId
    val ahCode: ValidationCodeHash = { vchBytes: 2 }
    val big = IncomingTransferEntryFootprint
    val from = MkServiceId(3)
    // What the candidate read: buckets 0 and 1.
    val read = applyIncomingTransfer(
      applyIncomingTransfer(initialState, { from: from, amount: big, toSupervisorBalance: false, memo: 0 }),
      { from: from, amount: big, toSupervisorBalance: false, memo: 1 })
    // A later invocation, after that read: bucket 2.
    val seeded = applyIncomingTransfer(read, { from: from, amount: big, toSupervisorBalance: false, memo: 2 })
    val ok: ParachainWorkDigestOk = {
      paraId: ah,
      validationCode: { hash: ahCode, len: FixedCodeLen },
      parentHeadHash: headHash(seeded.paraInfo(ah).headData),
      headData: seeded.paraInfo(ah).headData + 1,
      upwardMessages: List(CleanUpBucketsUpTo(1)),
      lookupAnchor: 5
    }
    val out = accumulateOnePackage(
      seeded, { result: WorkOk(Ok(ok)), authTrace: 0 }, Set(), None, FixedCodeLen, 10, Map())
    all {
      assert(read.incomingTransfers.keys() == Set(0, 1)),
      assert(seeded.incomingTransfers.keys() == Set(0, 1, 2)),
      // The two it read are gone; the one it never saw survives intact.
      assert(out.state.incomingTransfers.keys() == Set(2)),
      assert(out.state.incomingTransfers.get(2) ==
        List({ from: from, amount: big, toSupervisorBalance: false, memo: 2 })),
      assert(out.state.incomingTransferBuckets ==
        Some({ firstBucket: 2, lastBucket: 2, count: 1 })),
      init
    }
  }

  /// §5.1: removing the unreserved transfers restores the state balance to
  /// exactly its pre-charge value, while the reserved queue stays untouched.
  /// Full queue, three more entries pushed, then the three oldest buckets cleaned
  /// up on the next round. The queue is back to the reservation size and the state
  /// balance is back to its pre-charge value.
  run drainingUnreservedRestoresBalanceTest = {
    val ah = AssetHubParaId
    val ahCode: ValidationCodeHash = { vchBytes: 2 }
    val big = IncomingTransferEntryFootprint
    val full = fullTransferQueue
    val used0 = full.paraInfo(ah).usedStateBalance
    val total0 = full.paraInfo(ah).totalStateBalance
    // Full queue (count `MaxIncomingTransfers`), then three more entries, each
    // arriving in its own invocation so each opens its own bucket.
    val seeded = List(
      { from: MkServiceId(3), amount: big * 100, toSupervisorBalance: false, memo: 0 },
      { from: MkServiceId(4), amount: big * 100, toSupervisorBalance: false, memo: 0 },
      { from: MkServiceId(5), amount: big * 100, toSupervisorBalance: false, memo: 0 })
      .foldl(full, (acc, t) => applyIncomingTransfer(acc, t))
    // Next round, clean up the first three entries: `clean_up_buckets_up_to`
    // removes buckets from the oldest id up, so buckets 0..2 go and the queue
    // returns to the reservation size.
    val ok: ParachainWorkDigestOk = {
      paraId: ah,
      validationCode: { hash: ahCode, len: FixedCodeLen },
      parentHeadHash: headHash(seeded.paraInfo(ah).headData),
      headData: seeded.paraInfo(ah).headData + 1,
      upwardMessages: List(CleanUpBucketsUpTo(2)),
      lookupAnchor: 2
    }
    val out = accumulateOnePackage(
      seeded, { result: WorkOk(Ok(ok)), authTrace: 0 }, Set(), None, FixedCodeLen, 3, Map())
    all {
      // The three extra entries were charged their per-bucket cost.
      assert(seeded.paraInfo(ah).usedStateBalance == used0 + 3 * big),
      assert(seeded.paraInfo(ah).totalStateBalance == total0 + 3 * big),
      // Cleaning up the first three leaves the queue at the reservation again...
      assert(queuedTransferCount(out.state) == MaxIncomingTransfers),
      // ...with no extra state usage.
      assert(out.state.paraInfo(ah).usedStateBalance == used0),
      assert(out.state.paraInfo(ah).totalStateBalance == total0),
      init
    }
  }

  /// §5.1: admitting and cleaning up move `used` and `total` by the same
  /// per-bucket cost, so the available state balance never changes and cleaning up
  /// frees nothing for the rest of the candidate. Replay order therefore does not
  /// matter, and no ordering rule is needed. Breaks if the credit is ever tied to
  /// the transfer's `amount` again.
  run cleanUpOrderIsUnobservableTest = {
    val ah = AssetHubParaId
    val ahCode: ValidationCodeHash = { vchBytes: 2 }
    val big = IncomingTransferEntryFootprint
    val h: Hash = { hashBytes: 41 }
    val len = 512
    // One unreserved entry, charged to Asset Hub, then pin the allowance so the
    // solicit only fits once that entry is refunded.
    val paid = applyIncomingTransfer(
      fullTransferQueue, { from: MkServiceId(3), amount: big, toSupervisorBalance: false, memo: 0 })
    val pi = paid.paraInfo(ah)
    val tight =
      { ...paid,
        parachains: paid.parachains.put(
          ah, pi.setTotal(pi.usedStateBalance + preimageFootprint(len) - 1)) }
    pure def candidate(msgs: List[UpwardMessage]): ParachainWorkDigestOk = {
      paraId: ah,
      validationCode: { hash: ahCode, len: FixedCodeLen },
      parentHeadHash: headHash(tight.paraInfo(ah).headData),
      headData: tight.paraInfo(ah).headData + 1,
      upwardMessages: msgs,
      lookupAnchor: MaxIncomingTransfers
    }
    pure def replay(msgs: List[UpwardMessage]): ParachainServiceState =
      accumulateOnePackage(
        tight, { result: WorkOk(Ok(candidate(msgs))), authTrace: 0 },
        Set(), None, FixedCodeLen, MaxIncomingTransfers + 1, Map()).state
    val cleanFirst =
      replay(List(CleanUpBucketsUpTo(MaxIncomingTransfers),
                  Solicit({ target: Parachain(ah), hash: h, len: len })))
    val solicitFirst =
      replay(List(Solicit({ target: Parachain(ah), hash: h, len: len }),
                  CleanUpBucketsUpTo(MaxIncomingTransfers)))
    all {
      // The allowance is one short of the solicit, and cleaning up does not help.
      assert(not(tight.paraInfo(ah).hasHeadroom(preimageFootprint(len)))),
      assert(not(cleanFirst.hasReferencer(h, len, ah))),
      // Both emission orders land on exactly the same state.
      assert(cleanFirst == solicitFirst),
      init
    }
  }
}
