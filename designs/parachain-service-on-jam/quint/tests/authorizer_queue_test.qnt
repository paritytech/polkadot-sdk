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
      state: initialState, jamDesignate: List(), jamSolicit: Set(),
      jamForget: Set(), jamAssigns: List(), logs: List(), foreign: Map()
    }
    // X = 10 tiles exactly: 8 copies fill 80.
    val ten = range(0, 10).foldl(List(), (acc, i) => acc.append({ authBytes: i }))
    val tiled = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 0, 0,
      AssignCore({ core: core, queue: ten, newAssigner: None, jamSlot: 0 }))
    // A full 80-hash queue is installed as-is.
    val h80: AuthorizerHash = { authBytes: 1 }
    val eighty = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 0, 0,
      AssignCore({ core: core, queue: fullQ(h80), newAssigner: None, jamSlot: 0 }))
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
  run tilingQueueFiresOnceAtJamSlotTest = {
    val core = 4
    val ten = range(0, 10).foldl(List(), (acc, i) => acc.append({ authBytes: i }))
    val baseAcc: AccumulateOutcome = {
      state: initialState, jamDesignate: List(), jamSolicit: Set(),
      jamForget: Set(), jamAssigns: List(), logs: List(), foreign: Map()
    }
    val cached = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 0, 0,
      AssignCore({ core: core, queue: ten, newAssigner: None, jamSlot: 50 })).state
    val beforeDue = applyAlwaysAccumulate(cached, 49)
    val atDue = applyAlwaysAccumulate(cached, 50)
    all {
      assert(cached.pendingAssignCores.get(core) == 50),
      assert(beforeDue.assigns == List()),
      assert(beforeDue.state.pendingAssignCores.get(core) == 50),
      assert(atDue.assigns ==
        List({ core: core, assigner: ParachainServiceId, queue: assignQueueFor(ten) })),
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
    val seeded = { ...initialState,
      pendingAssigns: Map(core -> { queue: List(storedHash), assigner: None }),
      pendingAssignCores: Map(core -> 100) }
    val baseAcc: AccumulateOutcome = {
      state: seeded, jamDesignate: List(), jamSolicit: Set(),
      jamForget: Set(), jamAssigns: List(), logs: List(), foreign: Map()
    }
    val empty = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 1, 1,
      AssignCore({ core: core, queue: List(), newAssigner: None, jamSlot: 0 }))
    val tooLong = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 1, 1,
      AssignCore({ core: core, queue: fullQ({ authBytes: 1 }).append({ authBytes: 2 }),
                  newAssigner: None, jamSlot: 0 }))
    // A handoff must carry exactly 80 hashes, so a short one is malformed.
    val shortHandoff = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 1, 1,
      AssignCore({ core: core, queue: List(storedHash),
                  newAssigner: Some(MkServiceId(7)), jamSlot: 0 }))
    // Due later, so nothing would call `assign` yet: only caching it is at stake.
    val unknownCore = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 1, 1,
      AssignCore({ core: CoreCount, queue: List(storedHash), newAssigner: None, jamSlot: 100 }))
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
  /// `jam_slot`, then kept and re-presented every 80 blocks with the rotation
  /// advanced (§7.1).
  run shortQueueStoredAndScheduledTest = {
    val core = 5
    val q = List({ authBytes: 0 }, { authBytes: 1 }, { authBytes: 2 })
    val baseAcc: AccumulateOutcome = {
      state: initialState, jamDesignate: List(), jamSolicit: Set(),
      jamForget: Set(), jamAssigns: List(), logs: List(), foreign: Map()
    }
    // Cached at now = 0 for a future jam_slot of 100.
    val stored = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 0, 0,
      AssignCore({ core: core, queue: q, newAssigner: None, jamSlot: 100 }))
    // 80 slots from the top of a 3-set: 26 full passes (78) then the first 2.
    val expected = assignQueueFor(q)
    val beforeDue = applyAlwaysAccumulate(stored.state, 99)
    val first = applyAlwaysAccumulate(stored.state, 100)
    val notDueYet = applyAlwaysAccumulate(first.state, 179)
    val second = applyAlwaysAccumulate(first.state, 180)
    all {
      // Cached at its jam_slot, nothing forwarded yet.
      assert(stored.jamAssigns == List()),
      assert(stored.state.pendingAssigns.get(core).queue == q),
      assert(stored.state.pendingAssignCores.get(core) == 100),
      assert(beforeDue.assigns == List()),
      // At jam_slot: filled queue emitted, rotation advanced, due again in 80.
      assert(first.assigns ==
        List({ core: core, assigner: ParachainServiceId, queue: expected })),
      // The stored queue is rotated on by the 80 slots just consumed.
      assert(first.state.pendingAssigns.get(core).queue == advanceAssignQueue(q)),
      assert(first.state.pendingAssignCores.get(core) == 180),
      assert(notDueYet.assigns == List()),
      assert(notDueYet.state.pendingAssigns.get(core).queue == advanceAssignQueue(q)),
      // The next cycle resumes from the rotated queue.
      assert(second.assigns ==
        List({ core: core, assigner: ParachainServiceId,
               queue: assignQueueFor(advanceAssignQueue(q)) })),
      assert(second.state.pendingAssigns.get(core).queue ==
        advanceAssignQueue(advanceAssignQueue(q))),
      assert(second.state.pendingAssignCores.get(core) == 260),
      init
    }
  }

  /// The X = 11 example: 80 slots do not land on a set boundary, so each cycle
  /// resumes where the last stopped. Cycle 1 is 7 full passes (77) plus the first
  /// 3; cycle 2 starts at the 4th authorizer, runs to the 11th, then repeats.
  run assignQueueCyclesContinuouslyTest = {
    val q = range(0, 11).foldl(List(), (acc, i) => acc.append({ authBytes: i }))
    val q1 = advanceAssignQueue(q)
    val q2 = advanceAssignQueue(q1)
    val cycle1 = assignQueueFor(q)
    val cycle2 = assignQueueFor(q1)
    val cycle3 = assignQueueFor(q2)
    all {
      assert(cycle1.length() == AuthorizerQueueLength),
      assert(cycle2.length() == AuthorizerQueueLength),
      // Cycle 1: 7 full passes then the first 3.
      assert(cycle1.slice(0, 77) == repeatList(q, 7)),
      assert(cycle1.slice(77, 80) == takeFirst(3, q)),
      // Cycle 2 picks up at the 4th authorizer and runs to the 11th...
      assert(cycle2.slice(0, 8) == q.slice(3, 11)),
      // ...then repeats the set from the top.
      assert(cycle2.slice(8, 19) == q),
      // Rotating on by 80 is rotating on by 80 % 11 = 3.
      assert(q1 == rotateList(q, 3)),
      // Three cycles laid end to end are the endless sequence, unbroken.
      assert(cycle1.concat(cycle2).concat(cycle3) ==
        takeFirst(240, repeatList(q, 23))),
      init
    }
  }

  /// A second short assignment for a core that already has one stored replaces
  /// it wholesale (queue, due time and rotation reset; last write wins).
  run newAssignmentReplacesPendingTest = {
    val core = 5
    val q1 = List({ authBytes: 0 }, { authBytes: 1 }, { authBytes: 2 })
    val q2 = range(0, 11).foldl(List(), (acc, i) => acc.append({ authBytes: 10 + i }))
    val baseAcc: AccumulateOutcome = {
      state: initialState, jamDesignate: List(), jamSolicit: Set(),
      jamForget: Set(), jamAssigns: List(), logs: List(), foreign: Map()
    }
    val after1 = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 0, 0,
      AssignCore({ core: core, queue: q1, newAssigner: None, jamSlot: 100 }))
    val after2 = applyUpwardMessage(
      after1, CoretimeParaId, FixedCodeLen, 5, 5,
      AssignCore({ core: core, queue: q2, newAssigner: None, jamSlot: 150 }))
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
      state: initialState, jamDesignate: List(), jamSolicit: Set(),
      jamForget: Set(), jamAssigns: List(), logs: List(), foreign: Map()
    }
    // Handoff with the required full queue, due inline.
    val out = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 0, 0,
      AssignCore({ core: core, queue: h80, newAssigner: Some(assigner), jamSlot: 0 }))
    // A later assign for the SAME core, which `assigner` now controls. JAM
    // rejects it: nothing takes effect or is scheduled, and it is logged.
    val after = applyUpwardMessage(
      { ...out, jamAssigns: List(), logs: List() }, CoretimeParaId, FixedCodeLen, 0, 0,
      AssignCore({ core: core, queue: three, newAssigner: None, jamSlot: 0 }))
    // Keep-assigner short queue is stored and rotated as usual, from a state
    // where the core was never handed away.
    val stored = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 0, 0,
      AssignCore({ core: core, queue: three, newAssigner: None, jamSlot: 0 }))
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
      // A short keep-assigner queue still rotates, `None` resolving to this service.
      assert(stored.jamAssigns ==
        List({ core: core, assigner: ParachainServiceId, queue: assignQueueFor(three) })),
      assert(stored.state.pendingAssigns.get(core).assigner == None),
      assert(stored.state.pendingAssigns.get(core).queue == advanceAssignQueue(three)),
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
        == WorkOk(refineErr(ct, InvalidAuthorizerQueue))),
      assert(refineWithMsgs(initialState, ct, List(msg(List(), None)))
        == WorkOk(refineErr(ct, InvalidAuthorizerQueue))),
      // At the maximum and at one, both refine fine.
      assert(isOkDigest(refineWithMsgs(initialState, ct, List(msg(fullQ(h), None))))),
      assert(isOkDigest(refineWithMsgs(initialState, ct, List(msg(List(h), None))))),
      // A hand-off demands an exact 80-hash queue: short is rejected...
      assert(refineWithMsgs(initialState, ct, List(msg(List(h), Some(MkServiceId(7)))))
        == WorkOk(refineErr(ct, InvalidAuthorizerQueue))),
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
        == WorkOk(refineErr(ct, InvalidCoreIndex))),
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
    val handedAway = { ...initialState,
      jamCoreAssigners: initialState.jamCoreAssigners.put(core, other) }
    val baseAcc: AccumulateOutcome = {
      state: handedAway, jamDesignate: List(), jamSolicit: Set(),
      jamForget: Set(), jamAssigns: List(), logs: List(), foreign: Map()
    }
    val scheduled = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 1, 1,
      AssignCore({ core: core, queue: three, newAssigner: None, jamSlot: 10 }))
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

  /// A rotation left armed on a core that has since been handed away is
  /// rejected by JAM when it falls due: it takes no effect, the entry is
  /// dropped, and the rejection is logged (§7.1).
  run dueFlushDropsHandedAwayCoreTest = {
    val core = 3
    val other = MkServiceId(7)
    val three = List({ authBytes: 1 }, { authBytes: 2 }, { authBytes: 3 })
    // A rotation armed for `core`, due now, but `other` owns the core.
    val armed = {
      ...initialState,
      pendingAssigns: initialState.pendingAssigns.put(
        core, { queue: three, assigner: None }),
      pendingAssignCores: initialState.pendingAssignCores.put(core, 5),
      jamCoreAssigners: initialState.jamCoreAssigners.put(core, other)
    }
    val flushed = applyAlwaysAccumulate(armed, 10)
    // Same rotation on a core still owned by the service does fire.
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
        List({ core: core, assigner: ParachainServiceId, queue: assignQueueFor(three) })),
      assert(flushedOwned.logs == List()),
      init
    }
  }
}
