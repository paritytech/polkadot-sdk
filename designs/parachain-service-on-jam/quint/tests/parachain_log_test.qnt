// Parachain log eviction, entry ranking and the 64 KiB byte cap (design 3.1).
module parachain_log_test {
  import types.* from "../types"
  import messages.* from "../messages"
  import state.* from "../state"
  import refine.* from "../refine"
  import state_vars.* from "../state_vars"
  import main.* from "../main"

  /// When over capacity, evict the oldest Refine entry first and preserve
  /// older Accumulate entries if any Refine entry remains.
  run evictionPrefersDroppingRefineTest = {
    // Opaque(1024) ≈ 1033 B per entry; 1 acc + 63 refine fits under the cap,
    // so pushing one more triggers byte-cap eviction of the oldest Refine entry.
    val full = range(0, 64).foldl(List(), (acc, i) =>
      if (i == 0) acc.append((i, AccumulateLogEntry(List(ServiceUpgradePreimageMissing({ codeHash: { hashBytes: 7 } })))))
      else acc.append((i, RefineLogEntry({ error: Opaque(1024), authTrace: 0 }))))
    val after = pushLogEntry(
      full,
      (64, RefineLogEntry({ error: Opaque(1024), authTrace: 0 }))
    )
    val accCount = after.foldl(0, (n, pair) => match pair._2 {
      | AccumulateLogEntry(_) => n + 1
      | RefineLogEntry(_) => n
    })
    all {
      assert(after.length() == 64),
      assert(match after.nth(0)._2 {
        | AccumulateLogEntry(_) => true
        | RefineLogEntry(_) => false
      }),
      assert(accCount == 1),
      init
    }
  }

  /// A 1-event AccumulateLogEntry with a 32-octet-payload event costs 39 B
  /// (4 timeslot + 1 LogEntry disc + 1 vec prefix + 33 event), not the worst case.
  run bytecapSmallEntryCostTest = {
    val entry = (0, AccumulateLogEntry(List(ServiceUpgradePreimageMissing({ codeHash: { hashBytes: 1 } }))))
    val entrySize = logEntrySize(entry)
    all {
      assert(entrySize < 100),
      assert(entrySize > 0),
      init
    }
  }

  /// Fill a log with large Refine entries until near the byte cap, push one
  /// more, and verify the cap holds and the Accumulate entry survived eviction.
  run bytecapEvictionTest = {
    val bigAcc = (1, AccumulateLogEntry(List(ServiceUpgradePreimageMissing({ codeHash: { hashBytes: 1 } }))))
    val withAcc = pushLogEntry(List(), bigAcc)
    // Each Refine entry with Opaque(1014) ≈ 1023 B; after 64 pushes the
    // log wraps once (oldest Refine evicted), leaving the Accumulate intact.
    val nearFull = range(0, 64).foldl(withAcc, (log, i) =>
      pushLogEntry(log, (i + 2, RefineLogEntry({ error: Opaque(1014), authTrace: 0 }))))
    val after = pushLogEntry(nearFull, (100, RefineLogEntry({ error: Opaque(1014), authTrace: 0 })))
    val accCount = after.foldl(0, (n, pair) => match pair._2 {
      | AccumulateLogEntry(_) => n + 1
      | RefineLogEntry(_) => n
    })
    all {
      assert(parachainLogSize(after) <= ParachainLogByteCap),
      assert(accCount >= 1),
      init
    }
  }

  /// Two distinct Refine errors appended at the SAME timeslot for the SAME
  /// para must both be retained as separate entries in the flat log.
  run multiSameTimeslotRefineErrorsTest = {
    val para = CoretimeParaId
    val s0 = initialState
    // Append two distinct refine errors at the same timeslot (now=0).
    val s1 = s0.appendRefineLog(para, 0, InvalidCodeHash, 0)
    val s2 = s1.appendRefineLog(para, 0, MissingHeadDeclaration, 0)
    val log = s2.paraLog(para)
    // Both errors must be present as separate entries at timeslot 0.
    val hasInvalidCodeHash = log.foldl(false, (acc, pair) =>
      acc or (pair._1 == 0 and match pair._2 {
        | RefineLogEntry(e) => e.error == InvalidCodeHash
        | AccumulateLogEntry(_) => false
      }))
    val hasMissingHead = log.foldl(false, (acc, pair) =>
      acc or (pair._1 == 0 and match pair._2 {
        | RefineLogEntry(e) => e.error == MissingHeadDeclaration
        | AccumulateLogEntry(_) => false
      }))
    all {
      assert(hasInvalidCodeHash),
      assert(hasMissingHead),
      assert(log.length() == 2),
      init
    }
  }

  /// Count entries of a given rank in a log.
  pure def countRank(log: ParachainLog, rank: int): int =
    log.foldl(0, (n, e) => if (entryRank(e) == rank) n + 1 else n)

  /// §5.1 rank order: 0 = non-`Opaque` refine error, 1 = `Opaque`, 2 = accumulate.
  run entryRankOrderTest = {
    all {
      assert(entryRank((0, RefineLogEntry({ error: InvalidCodeHash, authTrace: 0 }))) == 0),
      assert(entryRank((0, RefineLogEntry({ error: MissingHeadDeclaration, authTrace: 0 }))) == 0),
      assert(entryRank((0, RefineLogEntry({ error: Opaque(8), authTrace: 0 }))) == 1),
      assert(entryRank((0, AccumulateLogEntry(List(TooMuchStateHeld)))) == 2),
      init
    }
  }

  /// The property the ranking exists for: a parachain's own `Opaque` reports are
  /// never evicted to make room for the submitter-provoked errors anyone with
  /// coretime can generate. Filling a log with `Opaque`, then pushing rank-0
  /// errors, must leave every `Opaque` intact and drop the newcomers instead.
  run spamCannotEvictOpaqueTest = {
    val opaqueEntry = (1, RefineLogEntry({ error: Opaque(1014), authTrace: 0 }))
    // Pack the log with the parachain's own reports until it is near capacity.
    val packed = range(0, 64).foldl(List(), (log, _) => pushLogEntry(log, opaqueEntry))
    val opaquesBefore = countRank(packed, 1)
    // Now a buyer floods it with rank-0 refine errors.
    val spam = (2, RefineLogEntry({ error: InvalidCodeHash, authTrace: 256 }))
    val flooded = range(0, 40).foldl(packed, (log, _) => pushLogEntry(log, spam))
    all {
      // The log was genuinely full: further Opaques could only displace Opaques.
      assert(opaquesBefore > 0),
      assert(parachainLogSize(packed) <= ParachainLogByteCap),
      // Not one of the parachain's own reports was evicted by the flood.
      assert(countRank(flooded, 1) == opaquesBefore),
      assert(parachainLogSize(flooded) <= ParachainLogByteCap),
      init
    }
  }

  /// The converse: a higher-ranked newcomer DOES displace lower-ranked entries,
  /// so the ranking is not simply "reject everything once full".
  run higherRankEvictsLowerTest = {
    // ~1008 B each, so ~80 pushes drive the log past the 64 KiB cap.
    val spam = (1, RefineLogEntry({ error: InvalidCodeHash, authTrace: 1000 }))
    val packed = range(0, 80).foldl(List(), (log, _) => pushLogEntry(log, spam))
    val spamBefore = countRank(packed, 0)
    // A ~506 B accumulate entry (rank 2) arriving at the saturated log.
    val bigAcc = (2, AccumulateLogEntry(
      range(0, 500).foldl(List(), (l, _) => l.append(TooMuchStateHeld))))
    val withAcc = pushLogEntry(packed, bigAcc)
    all {
      assert(spamBefore > 0),
      // Saturated: another rank-0 push evicts one and adds one, so the count holds.
      assert(countRank(pushLogEntry(packed, spam), 0) == spamBefore),
      // The accumulate entry IS admitted, at the cost of rank-0 entries.
      assert(countRank(withAcc, 2) == 1),
      assert(countRank(withAcc, 0) < spamBefore),
      assert(parachainLogSize(withAcc) <= ParachainLogByteCap),
      init
    }
  }
}
