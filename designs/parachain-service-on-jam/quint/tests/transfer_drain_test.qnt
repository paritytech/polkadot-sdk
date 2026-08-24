// Draining the incoming-transfer queue: `consume_transfers_up_to`,
// lookup-anchor clamping, and the refund side of admission (design 5.1).
module transfer_drain_test {
  import types.* from "../types"
  import messages.* from "../messages"
  import state.* from "../state"
  import state_balance.* from "../state_balance"
  import accumulate.* from "../accumulate"
  import main.* from "../main"
  import support.* from "./support"

  /// `consume_transfers_up_to(slot)` drops whole buckets up to and including
  /// `slot`, re-heads the chain, and clears it once nothing survives.
  run consumeDrainsBucketsUpToSlotTest = {
    val a = applyIncomingTransfer(initialState, 1,
      { from: MkServiceId(0), amount: 500, memo: 0 })
    val b = applyIncomingTransfer(a, 4, { from: MkServiceId(0), amount: 500, memo: 0 })
    val c = applyIncomingTransfer(b, 9, { from: MkServiceId(0), amount: 500, memo: 0 })
    val partial = dropBucketsUpTo(c, 4, 9)  // anchor 9: no clamp, drops slots 1 and 4
    val everything = dropBucketsUpTo(c, 9, 9)  // drains the chain
    all {
      assert(c.incomingTransferChain == Some({ firstSlot: 1, lastSlot: 9, count: 3 })),
      assert(partial.incomingTransfers.keys() == Set(9)),
      assert(partial.incomingTransferChain == Some({ firstSlot: 9, lastSlot: 9, count: 1 })),
      assert(everything.incomingTransfers.keys() == Set()),
      assert(everything.incomingTransferChain == None),
      init
    }
  }

  /// §5.1: `consume_transfers_up_to` is clamped to the candidate's lookup-anchor.
  /// Asset Hub cannot have read a bucket newer than the anchor it built on, so a
  /// slot beyond it must not drain buckets it never observed. Buckets at or below
  /// the anchor are still drained normally.
  run consumeIsClampedToAnchorTest = {
    val ah = AssetHubParaId
    val ahCode: ValidationCodeHash = { vchBytes: 2 }
    val big = IncomingTransferEntryFootprint
    val from = MkServiceId(3)
    // Two buckets: one at slot 1, one at slot 9.
    val seeded = applyIncomingTransfer(
      applyIncomingTransfer(initialState, 1, { from: from, amount: big, memo: 0 }),
      9, { from: from, amount: big, memo: 0 })
    // A candidate anchored at 5 asking to consume everything up to 9.
    val ok: ParachainWorkDigestOk = {
      paraId: ah,
      validationCode: { hash: ahCode, len: FixedCodeLen },
      parentHeadHash: headHash(seeded.paraInfo(ah).headData),
      headData: seeded.paraInfo(ah).headData + 1,
      upwardMessages: List(ConsumeTransfersUpTo(9)),
      lookupAnchor: 5
    }
    val out = accumulateOnePackage(
      seeded, { result: WorkOk(Ok(ok)), authTrace: 0 }, Set(), None, FixedCodeLen, 10)
    all {
      assert(seeded.incomingTransfers.keys() == Set(1, 9)),
      // Clamped to 5: bucket 1 drained, bucket 9 survives.
      assert(out.state.incomingTransfers.keys() == Set(9)),
      init
    }
  }

  /// §5.1: removing the unreserved transfers restores the state balance to
  /// exactly its pre-charge value, while the reserved queue stays untouched.
  /// §5.1: full queue, three more entries pushed, then the three oldest entries
  /// drained on the next round. The queue is full again and the state balance is
  /// back to its pre-charge value.
  run drainingUnreservedRestoresBalanceTest = {
    val ah = AssetHubParaId
    val ahCode: ValidationCodeHash = { vchBytes: 2 }
    val big = IncomingTransferEntryFootprint
    val full = fullTransferQueue
    val used0 = full.paraInfo(ah).usedStateBalance
    val total0 = full.paraInfo(ah).totalStateBalance
    // Full queue (count `MaxIncomingTransfers`), then three more entries, each
    // in its own fresh slot past the tail.
    val seeded = List(
      { from: MkServiceId(3), amount: big * 100, memo: 0 },
      { from: MkServiceId(4), amount: big * 100, memo: 0 },
      { from: MkServiceId(5), amount: big * 100, memo: 0 })
      .foldl(full, (acc, t) =>
        applyIncomingTransfer(acc, queuedTransferCount(acc), t))
    // Next round, drain the first three entries: `consume_transfers_up_to`
    // removes buckets from the oldest slot up, so slots 0..2 go and the queue
    // returns to the reservation size.
    val ok: ParachainWorkDigestOk = {
      paraId: ah,
      validationCode: { hash: ahCode, len: FixedCodeLen },
      parentHeadHash: headHash(seeded.paraInfo(ah).headData),
      headData: seeded.paraInfo(ah).headData + 1,
      upwardMessages: List(ConsumeTransfersUpTo(2)),
      lookupAnchor: 2
    }
    val out = accumulateOnePackage(
      seeded, { result: WorkOk(Ok(ok)), authTrace: 0 }, Set(), None, FixedCodeLen, 3)
    all {
      // The three extra entries were charged their per-bucket cost.
      assert(seeded.paraInfo(ah).usedStateBalance == used0 + 3 * big),
      assert(seeded.paraInfo(ah).totalStateBalance == total0 + 3 * big),
      // Draining the first three leaves the queue full again...
      assert(queuedTransferCount(out.state) == MaxIncomingTransfers),
      // ...with no extra state usage.
      assert(out.state.paraInfo(ah).usedStateBalance == used0),
      assert(out.state.paraInfo(ah).totalStateBalance == total0),
      init
    }
  }

  /// §5.1: admitting and draining move `used` and `total` by the same per-bucket
  /// cost, so the available state balance never changes and draining frees nothing
  /// for the rest of the candidate. Replay order therefore does not matter, and no
  /// ordering rule is needed. Breaks if the credit is ever tied to the transfer's
  /// `amount` again.
  run consumeTransfersOrderIsUnobservableTest = {
    val ah = AssetHubParaId
    val ahCode: ValidationCodeHash = { vchBytes: 2 }
    val big = IncomingTransferEntryFootprint
    val h: Hash = { hashBytes: 41 }
    val len = 512
    // One unreserved entry, charged to Asset Hub, then pin the allowance so the
    // solicit only fits once that entry is refunded.
    val paid = applyIncomingTransfer(
      fullTransferQueue, MaxIncomingTransfers, { from: MkServiceId(3), amount: big, memo: 0 })
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
        Set(), None, FixedCodeLen, MaxIncomingTransfers + 1).state
    val consumeFirst =
      replay(List(ConsumeTransfersUpTo(MaxIncomingTransfers), Solicit({ hash: h, len: len })))
    val solicitFirst =
      replay(List(Solicit({ hash: h, len: len }), ConsumeTransfersUpTo(MaxIncomingTransfers)))
    all {
      // The allowance is one short of the solicit, and draining does not help.
      assert(not(tight.paraInfo(ah).hasHeadroom(preimageFootprint(len)))),
      assert(not(consumeFirst.hasReferencer(h, len, ah))),
      // Both emission orders land on exactly the same state.
      assert(consumeFirst == solicitFirst),
      init
    }
  }
}
