// Authorizer queue scheduling and the cached `assign` flush (design 5.1, 7.1).
module authorizer_queue_test {
  import types.* from "../types"
  import messages.* from "../messages"
  import state.* from "../state"
  import refine.* from "../refine"
  import accumulate.* from "../accumulate"
  import state_vars.* from "../state_vars"
  import main.* from "../main"
  import support.* from "./support"

  /// A full `assign_core` queue of `h`, repeated to the length JAM requires.
  pure def fullQ(h: AuthorizerHash): List[AuthorizerHash] =
    range(0, AuthorizerQueueLength).foldl(List(), (l, _) => l.append(h))

  /// A queue that fills the 80 slots exactly, whole or tiled, fires once and
  /// leaves nothing cached. Both are due here, so both apply inline (§5.1).
  run tilingQueueForwardsImmediatelyTest = {
    val core = 5
    val baseAcc: AccumulateOutcome = {
      state: initialState,
      jamDesignate: List(),
      preimageOps: List(),
      jamAssigns: List(),
      logs: List(),
      foreign: Map()
    }
    // X = 10 tiles exactly: 8 copies fill 80.
    val ten = range(0, 10).foldl(List(), (acc, i) => acc.append({ authBytes: i }))
    val tiled = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 0, 0,
      AssignCore({ core: core, queue: ten, newAssigner: None, jamSlot: 0 })
    )
    // A full 80-hash queue is installed as-is.
    val h80: AuthorizerHash = { authBytes: 1 }
    val eighty = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 0, 0,
      AssignCore({ core: core, queue: fullQ(h80), newAssigner: None, jamSlot: 0 })
    )
    all {
      assert(tiled.jamAssigns ==
        List({ core: core, assigner: ParachainServiceId, queue: repeatList(ten, 8) })),
      assert(tiled.jamAssigns.nth(0).queue.length() == AuthorizerQueueLength),
      assert(eighty.jamAssigns ==
        List({ core: core, assigner: ParachainServiceId, queue: fullQ(h80) })),
      // Directly applied, nothing cached.
      assert(not(tiled.state.pendingAssigns.keys().contains(core))),
      assert(not(eighty.state.pendingAssignCores.keys().contains(core))),
      init
    }
  }

  /// A queue that tiles the 80 slots but is scheduled ahead is cached, fires at
  /// its `jam_slot`, and is then gone from both maps; nothing re-presents it.
  /// It is installed as it comes, although slot 50 is not a multiple of 80.
  run tilingQueueFiresOnceAtJamSlotTest = {
    val core = 4
    val ten = range(0, 10).foldl(List(), (acc, i) => acc.append({ authBytes: i }))
    val baseAcc: AccumulateOutcome = {
      state: initialState,
      jamDesignate: List(),
      preimageOps: List(),
      jamAssigns: List(),
      logs: List(),
      foreign: Map()
    }
    val cached = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 0, 0,
      AssignCore({ core: core, queue: ten, newAssigner: None, jamSlot: 50 })
    ).state
    val beforeDue = applyAlwaysAccumulate(cached, 49)
    val atDue = applyAlwaysAccumulate(cached, 50)
    all {
      assert(cached.pendingAssignCores.get(core) == 50),
      assert(beforeDue.assigns == List()),
      assert(beforeDue.state.pendingAssignCores.get(core) == 50),
      assert(atDue.assigns ==
        List({ core: core, assigner: ParachainServiceId, queue: repeatList(ten, 8) })),
      assert(not(atDue.state.pendingAssigns.keys().contains(core))),
      assert(not(atDue.state.pendingAssignCores.keys().contains(core))),
      init
    }
  }

  /// Accumulate ignores a malformed `assign_core`: no JAM call, nothing cached,
  /// any entry already stored left alone. Refine rejects all four shapes first
  /// (§3.3), so this only pins the defensive branch: an empty queue, an
  /// over-long one, a handoff carrying fewer than 80 hashes, and an unknown core.
  run malformedAssignCoreIsNoOpTest = {
    val core = 5
    val storedHash: AuthorizerHash = { authBytes: 5 }
    val seeded = {
      ...initialState,
      pendingAssigns: Map(core -> { queue: List(storedHash), assigner: None }),
      pendingAssignCores: Map(core -> 100)
    }
    val baseAcc: AccumulateOutcome = {
      state: seeded,
      jamDesignate: List(),
      preimageOps: List(),
      jamAssigns: List(),
      logs: List(),
      foreign: Map()
    }
    val empty = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 1, 1,
      AssignCore({ core: core, queue: List(), newAssigner: None, jamSlot: 0 })
    )
    val tooLong = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 1, 1,
      AssignCore({
        core: core,
        queue: fullQ({ authBytes: 1 }).append({ authBytes: 2 }),
        newAssigner: None,
        jamSlot: 0
      })
    )
    // A handoff must carry exactly 80 hashes, so a short one is malformed.
    val shortHandoff = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 1, 1,
      AssignCore({
        core: core,
        queue: List(storedHash),
        newAssigner: Some(MkServiceId(7)),
        jamSlot: 0
      })
    )
    // Due later, so nothing would call `assign` yet: only caching it is at stake.
    val unknownCore = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 1, 1,
      AssignCore({ core: CoreCount, queue: List(storedHash), newAssigner: None, jamSlot: 100 })
    )
    all {
      Set(empty, tooLong, shortHandoff, unknownCore).forall(out => and {
        out.jamAssigns == List(),
        out.logs == List(),
        out.state == seeded
      }),
      init
    }
  }

  /// A queue too short to fill the 80 slots exactly is cached until its
  /// `jam_slot`, then kept as sent and rewritten every 80 slots (§7.1). The set
  /// `[A, B, C]` due at slot 40 is the example from §7.1.
  run shortQueueStoredAndScheduledTest = {
    val core = 5
    val a: AuthorizerHash = { authBytes: 0 }
    val b: AuthorizerHash = { authBytes: 1 }
    val c: AuthorizerHash = { authBytes: 2 }
    val q = List(a, b, c)
    val baseAcc: AccumulateOutcome = {
      state: initialState,
      jamDesignate: List(),
      preimageOps: List(),
      jamAssigns: List(),
      logs: List(),
      foreign: Map()
    }
    // Cached at now = 0 for a future jam_slot of 40.
    val stored = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 0, 0,
      AssignCore({ core: core, queue: q, newAssigner: None, jamSlot: 40 })
    )
    val beforeDue = applyAlwaysAccumulate(stored.state, 39)
    val first = applyAlwaysAccumulate(stored.state, 40)
    val notDueYet = applyAlwaysAccumulate(first.state, 119)
    val second = applyAlwaysAccumulate(first.state, 120)
    val firstQueue = first.assigns.nth(0).queue
    val secondQueue = second.assigns.nth(0).queue
    all {
      // Cached at its jam_slot, nothing forwarded yet.
      assert(stored.jamAssigns == List()),
      assert(stored.state.pendingAssigns.get(core).queue == q),
      assert(stored.state.pendingAssignCores.get(core) == 40),
      assert(beforeDue.assigns == List()),
      // At jam_slot: one 80-hash queue for this service.
      assert(first.assigns.length() == 1),
      assert(first.assigns.nth(0).assigner == ParachainServiceId),
      assert(firstQueue.length() == AuthorizerQueueLength),
      // Slots 40 to 79 read entries 40 to 79, slots 80 to 119 entries 0 to 39.
      assert(firstQueue.nth(40) == b),
      assert(firstQueue.nth(79) == b),
      assert(firstQueue.nth(0) == c),
      assert(firstQueue.nth(39) == c),
      // The set is kept as sent and due again 80 slots later.
      assert(first.state.pendingAssigns.get(core).queue == q),
      assert(first.state.pendingAssignCores.get(core) == 120),
      assert(notDueYet.assigns == List()),
      // At slot 120 entry 40 comes round again and starts with A.
      assert(second.assigns.length() == 1),
      assert(secondQueue.nth(40) == a),
      assert(second.state.pendingAssigns.get(core).queue == q),
      assert(second.state.pendingAssignCores.get(core) == 200),
      init
    }
  }

  /// Slot `t` gets hash `t % X` of the set across every rewrite. With X = 11 and
  /// the first assign due at slot 13, every slot from 13 to 252 reads its hash
  /// from the queue written by the last rewrite before it (§7.1).
  run assignQueueFollowsSlotOrderTest = {
    val core = 5
    val q = range(0, 11).foldl(List(), (acc, i) => acc.append({ authBytes: i }))
    val armed = {
      ...initialState,
      pendingAssigns: Map(core -> { queue: q, assigner: None }),
      pendingAssignCores: Map(core -> 13)
    }
    val first = applyAlwaysAccumulate(armed, 13)
    val second = applyAlwaysAccumulate(first.state, 93)
    val third = applyAlwaysAccumulate(second.state, 173)
    all {
      assert(range(13, 253).foldl(true, (ok, t) => {
        val written = if (t < 93) first else if (t < 173) second else third
        ok and written.assigns.nth(0).queue.nth(t % AuthorizerQueueLength) == q.nth(t % 11)
      })),
      init
    }
  }

  /// The hash JAM reads at slot `due + i` is element `i` of the set repeated
  /// from hash `due % q.length()`, for all 80 slots from `due` on. Checked for
  /// sets that do and do not tile the 80 slots, and for `due` on, just before and
  /// just after a multiple of 80. A set that tiles the 80 slots comes out as it
  /// is, repeated from entry 0.
  run assignQueueForLayoutTest = {
    pure def hashes(n: int): List[AuthorizerHash] =
      range(0, n).foldl(List(), (acc, i) => acc.append({ authBytes: i }))
    pure def laidOut(q: List[AuthorizerHash], due: Timeslot): bool = {
      val start = due % q.length()
      val expected = repeatList(
        q.slice(start, q.length()).concat(q.slice(0, start)),
        AuthorizerQueueLength / q.length() + 1
      )
      val queue = assignQueueFor(q, due)
      queue.length() == AuthorizerQueueLength and
        range(0, AuthorizerQueueLength).foldl(true, (ok, i) =>
          ok and queue.nth((due + i) % AuthorizerQueueLength) == expected.nth(i))
    }
    all {
      assert(tuples(Set(1, 3, 7, 10, 11, 79, 80), Set(0, 13, 40, 79, 80, 173))
        .forall(c => laidOut(hashes(c._1), c._2))),
      assert(assignQueueFor(hashes(10), 50) == repeatList(hashes(10), 8)),
      assert(assignQueueFor(hashes(AuthorizerQueueLength), 5) == hashes(AuthorizerQueueLength)),
      init
    }
  }

  /// A second short assignment for a core that already has one stored replaces
  /// it wholesale (queue and due time; last write wins).
  run newAssignmentReplacesPendingTest = {
    val core = 5
    val q1 = List({ authBytes: 0 }, { authBytes: 1 }, { authBytes: 2 })
    val q2 = range(0, 11).foldl(List(), (acc, i) => acc.append({ authBytes: 10 + i }))
    val baseAcc: AccumulateOutcome = {
      state: initialState,
      jamDesignate: List(),
      preimageOps: List(),
      jamAssigns: List(),
      logs: List(),
      foreign: Map()
    }
    val after1 = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 0, 0,
      AssignCore({ core: core, queue: q1, newAssigner: None, jamSlot: 100 })
    )
    val after2 = applyUpwardMessage(
      after1, CoretimeParaId, FixedCodeLen, 5, 5,
      AssignCore({ core: core, queue: q2, newAssigner: None, jamSlot: 150 })
    )
    all {
      assert(after1.state.pendingAssigns.get(core).queue == q1),
      assert(after1.state.pendingAssignCores.get(core) == 100),
      assert(after2.state.pendingAssigns.get(core).queue == q2),
      assert(after2.state.pendingAssigns.get(core).queue == q2),
      assert(after2.state.pendingAssignCores.get(core) == 150),
      init
    }
  }

  /// `new_assigner = Some(s)` installs the exact 80-hash queue and hands the
  /// core over in the same call; nothing is kept for re-presentation, since the
  /// service is no longer the assigner. `None` keeps this service as assigner.
  run assignerHandoffTest = {
    val core = 0
    val assigner = MkServiceId(7)
    val h80 = range(0, AuthorizerQueueLength).foldl(List(), (acc, i) =>
      acc.append({ authBytes: i }))
    val three = List({ authBytes: 1 }, { authBytes: 2 }, { authBytes: 3 })
    val baseAcc: AccumulateOutcome = {
      state: initialState,
      jamDesignate: List(),
      preimageOps: List(),
      jamAssigns: List(),
      logs: List(),
      foreign: Map()
    }
    // Handoff with the required full queue, due inline.
    val out = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 0, 0,
      AssignCore({ core: core, queue: h80, newAssigner: Some(assigner), jamSlot: 0 })
    )
    // A later assign for the SAME core, which `assigner` now controls. JAM
    // rejects it: nothing takes effect or is scheduled, and it is logged.
    val after = applyUpwardMessage(
      { ...out, jamAssigns: List(), logs: List() }, CoretimeParaId, FixedCodeLen, 0, 0,
      AssignCore({ core: core, queue: three, newAssigner: None, jamSlot: 0 })
    )
    // A keep-assigner short queue is kept as usual, from a state where the core
    // was never handed away.
    val stored = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 0, 0,
      AssignCore({ core: core, queue: three, newAssigner: None, jamSlot: 0 })
    )
    all {
      assert(out.jamAssigns ==
        List({ core: core, assigner: assigner, queue: h80 })),
      // The handoff is one-shot: nothing left to re-present.
      assert(not(out.state.pendingAssigns.keys().contains(core))),
      assert(not(out.state.pendingAssignCores.keys().contains(core))),
      // ...and one-way: naming no new assigner does NOT win the core back.
      assert(after.jamAssigns == List()),
      assert(after.logs == List(CoreNotAssignable({ core: core }))),
      assert(after.state == out.state),
      // A short keep-assigner queue is kept for its rewrite, `None` resolving to
      // this service.
      assert(stored.jamAssigns ==
        List({ core: core, assigner: ParachainServiceId, queue: assignQueueFor(three, 0) })),
      assert(stored.state.pendingAssigns.get(core).assigner == None),
      assert(stored.state.pendingAssigns.get(core).queue == three),
      init
    }
  }

  /// §3.3: an `assign_core` queue holds 1 to `AuthorizerQueueLength` hashes.
  /// Both an empty and an over-long queue abort Refine the same way.
  run assignQueueLengthRangeTest = {
    val ct = CoretimeParaId
    val h: AuthorizerHash = { authBytes: 3 }
    pure def msg(q: List[AuthorizerHash], a: Option[ServiceId]): UpwardMessage =
      AssignCore({ core: 0, queue: q, newAssigner: a, jamSlot: 100 })
    all {
      // Both ends of the range are rejected with the same error.
      assert(refineWithMsgs(initialState, ct, List(msg(fullQ(h).append(h), None)))
        == refineCallsErr(initialState, ct, InvalidAuthorizerQueue)),
      assert(refineWithMsgs(initialState, ct, List(msg(List(), None)))
        == refineCallsErr(initialState, ct, InvalidAuthorizerQueue)),
      // At the maximum and at one, both refine fine.
      assert(isOkDigest(refineWithMsgs(initialState, ct, List(msg(fullQ(h), None))))),
      assert(isOkDigest(refineWithMsgs(initialState, ct, List(msg(List(h), None))))),
      // A hand-off demands an exact 80-hash queue: short is rejected...
      assert(refineWithMsgs(initialState, ct, List(msg(List(h), Some(MkServiceId(7)))))
        == refineCallsErr(initialState, ct, InvalidAuthorizerQueue)),
      // ...and 80 passes.
      assert(isOkDigest(refineWithMsgs(initialState, ct, List(msg(fullQ(h), Some(MkServiceId(7))))))),
      init
    }
  }

  /// Refine rejects a core JAM's `assign` does not know: `CoreCount` and above.
  run assignCoreIndexRangeTest = {
    val ct = CoretimeParaId
    pure def onCore(c: CoreIndex): UpwardMessage =
      AssignCore({ core: c, queue: List({ authBytes: 3 }), newAssigner: None, jamSlot: 100 })
    all {
      assert(isOkDigest(refineWithMsgs(initialState, ct, List(onCore(CoreCount - 1))))),
      assert(refineWithMsgs(initialState, ct, List(onCore(CoreCount)))
        == refineCallsErr(initialState, ct, InvalidCoreIndex)),
      init
    }
  }

  /// Ownership is only known once `assign` is called, so an `assign_core` for a
  /// future slot on a handed-away core is cached like any other. The rejection
  /// comes from the flush that makes the call at `jam_slot` (§7.1).
  run scheduledAssignRejectedAtFlushTest = {
    val core = 4
    val other = MkServiceId(7)
    val three = List({ authBytes: 1 }, { authBytes: 2 }, { authBytes: 3 })
    val handedAway = {
      ...initialState,
      jamCoreAssigners: initialState.jamCoreAssigners.put(core, other)
    }
    val baseAcc: AccumulateOutcome = {
      state: handedAway,
      jamDesignate: List(),
      preimageOps: List(),
      jamAssigns: List(),
      logs: List(),
      foreign: Map()
    }
    val scheduled = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 1, 1,
      AssignCore({ core: core, queue: three, newAssigner: None, jamSlot: 10 })
    )
    val flushed = applyAlwaysAccumulate(scheduled.state, 10)
    all {
      // Replay: cached, nothing logged, no `assign` yet.
      assert(scheduled.logs == List()),
      assert(scheduled.jamAssigns == List()),
      assert(scheduled.state.pendingAssignCores.get(core) == 10),
      // Flush at `jam_slot`: JAM rejects, the entry goes, the rejection is logged.
      assert(flushed.assigns == List()),
      assert(not(flushed.state.pendingAssigns.keys().contains(core))),
      assert(not(flushed.state.pendingAssignCores.keys().contains(core))),
      assert(flushed.logs == List(CoreNotAssignable({ core: core }))),
      init
    }
  }

  /// A rewrite left armed on a core that has since been handed away is
  /// rejected by JAM when it falls due: it takes no effect, the entry is
  /// dropped, and the rejection is logged (§7.1).
  run dueFlushDropsHandedAwayCoreTest = {
    val core = 3
    val other = MkServiceId(7)
    val three = List({ authBytes: 1 }, { authBytes: 2 }, { authBytes: 3 })
    // A rewrite armed for `core`, due now, but `other` owns the core.
    val armed = {
      ...initialState,
      pendingAssigns: initialState.pendingAssigns.put(
        core, { queue: three, assigner: None }
      ),
      pendingAssignCores: initialState.pendingAssignCores.put(core, 5),
      jamCoreAssigners: initialState.jamCoreAssigners.put(core, other)
    }
    val flushed = applyAlwaysAccumulate(armed, 10)
    // The same rewrite on a core still owned by the service does fire.
    val owned = { ...armed, jamCoreAssigners: initialState.jamCoreAssigners }
    val flushedOwned = applyAlwaysAccumulate(owned, 10)
    all {
      assert(not(armed.isAssignerOf(core))),
      assert(flushed.assigns == List()),
      // The dead entry is dropped, not left to retry every block.
      assert(not(flushed.state.pendingAssigns.keys().contains(core))),
      assert(not(flushed.state.pendingAssignCores.keys().contains(core))),
      assert(flushed.logs == List(CoreNotAssignable({ core: core }))),
      assert(owned.isAssignerOf(core)),
      assert(flushedOwned.assigns ==
        List({ core: core, assigner: ParachainServiceId, queue: assignQueueFor(three, 5) })),
      assert(flushedOwned.logs == List()),
      // Applied late at slot 10, it is still due again at 5 + 80.
      assert(flushedOwned.state.pendingAssignCores.get(core) == 85),
      init
    }
  }

  /// §5.1: due assigns beyond the always-accumulate allowance stay cached. The
  /// earliest due fire first, more fire after the work packages if gas is left,
  /// and the rest are carried over to the next block. The flush ends with a
  /// `checkpoint`, so running out of gas later, in a work package or in the late
  /// flush, keeps the assigns it fired and carries over those the late flush
  /// would have fired.
  run dueAssignsBeyondBudgetCarriedOverTest = {
    val one = List({ authBytes: 1 })
    val armed = {
      ...initialState,
      pendingAssigns: Map(
        3 -> { queue: one, assigner: None },
        4 -> { queue: one, assigner: None },
        5 -> { queue: one, assigner: None },
        6 -> { queue: one, assigner: None }
      ),
      pendingAssignCores: Map(3 -> 5, 4 -> 2, 5 -> 8, 6 -> 5)
    }
    pure def call(c: CoreIndex): AssignCall =
      { core: c, assigner: ParachainServiceId, queue: repeatList(one, AuthorizerQueueLength) }
    val first = accumulateBlockBudgeted(
      armed, 10, List(), Set(), FixedCodeLen, List(), Map(), { flush: 1, end: 2 }, Never
    )
    val second = accumulateBlockBudgeted(
      first.state, 11, List(), Set(), FixedCodeLen, List(), Map(), { flush: 1, end: 0 }, Never
    )
    val everything = accumulateBlockBudgeted(
      armed, 10, List(), Set(), FixedCodeLen, List(), Map(), FullAssignBudget, Never
    )
    val outOfGasLate = accumulateBlockBudgeted(
      armed, 10, List(), Set(), FixedCodeLen, List(), Map(), { flush: 1, end: 2 }, InLateAssigns
    )
    val outOfGasInPackage = accumulateBlockBudgeted(
      armed, 10, List(), Set(), FixedCodeLen,
      List({ result: refineWithMsgs(armed, CoretimeParaId, List()), authTrace: 0 }),
      Map(), { flush: 1, end: 2 }, InPackage(0)
    )
    all {
      // A budget covering every due assign still fires them in order.
      assert(everything.assigns == List(call(4), call(3), call(6), call(5))),
      // Core 4 is due first; 3 and 6 tie on slot 5, so the lower index goes.
      assert(first.assigns == List(call(4), call(3), call(6))),
      assert(first.state.pendingAssignCores == Map(5 -> 8)),
      assert(second.assigns == List(call(5))),
      assert(second.state.pendingAssignCores == Map()),
      assert(outOfGasLate.assigns == List(call(4))),
      assert(outOfGasLate.state.pendingAssignCores == Map(3 -> 5, 5 -> 8, 6 -> 5)),
      assert(outOfGasInPackage.assigns == List(call(4))),
      assert(outOfGasInPackage.state.pendingAssignCores == Map(3 -> 5, 5 -> 8, 6 -> 5)),
      assert(outOfGasInPackage.state.paraInfo(CoretimeParaId).headData == 0),
      init
    }
  }

  /// §5.1: only the first invocation of a timeslot is the always-accumulate one
  /// and flushes due assigns up front. A later invocation in the same timeslot
  /// does not flush up front a second time, even with allowance to spare, but
  /// still flushes at its end while gas lasts. The next timeslot's first
  /// invocation flushes again. Running out of gas keeps the recorded timeslot,
  /// since it is part of the checkpoint.
  run alwaysAccumulateOncePerTimeslotTest = {
    val one = List({ authBytes: 1 })
    // Two due cores: 3 (due at 5) goes before 4 (due at 6).
    val armed = {
      ...initialState,
      pendingAssigns: Map(
        3 -> { queue: one, assigner: None },
        4 -> { queue: one, assigner: None }
      ),
      pendingAssignCores: Map(3 -> 5, 4 -> 6)
    }
    pure def call(c: CoreIndex): AssignCall =
      { core: c, assigner: ParachainServiceId, queue: repeatList(one, AuthorizerQueueLength) }
    // The timeslot's first invocation, whose allowance covers one due assign.
    val first = accumulateBlockBudgeted(
      armed, 10, List(), Set(), FixedCodeLen, List(), Map(), { flush: 1, end: 0 }, Never
    )
    // A second invocation in the same timeslot, with allowance for every due one.
    val sameSlot = accumulateTransfers(first.state, 10, List(), FullAssignBudget)
    val sameSlotLate = accumulateTransfers(first.state, 10, List(), { flush: 0, end: 1 })
    val nextSlot = accumulateTransfers(sameSlot.state, 11, List(), FullAssignBudget)
    val outOfGas = accumulateBlockBudgeted(
      armed, 10, List(), Set(), FixedCodeLen,
      List({ result: refineWithMsgs(armed, CoretimeParaId, List()), authTrace: 0 }),
      Map(), FullAssignBudget, InPackage(0)
    )
    all {
      assert(armed.lastAccumulate == None),
      assert(first.assigns == List(call(3))),
      assert(first.state.lastAccumulate == Some(10)),
      assert(first.state.pendingAssignCores == Map(4 -> 6)),
      // The same timeslot flushes up front only once.
      assert(sameSlot.assigns == List()),
      assert(sameSlot.state.pendingAssignCores == Map(4 -> 6)),
      assert(sameSlot.state.lastAccumulate == Some(10)),
      // Leftover gas at the end still applies it.
      assert(sameSlotLate.assigns == List(call(4))),
      assert(nextSlot.assigns == List(call(4))),
      assert(nextSlot.state.lastAccumulate == Some(11)),
      assert(outOfGas.assigns == List(call(3), call(4))),
      assert(outOfGas.state.lastAccumulate == Some(10)),
      init
    }
  }
}
