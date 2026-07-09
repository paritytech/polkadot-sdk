// Quint `run` tests. Driven via `quint test --backend=typescript tests.qnt`.
//
// Tests live here so they can be exercised independently of the main
// model; they import the same state, types, and actions as `main.qnt`.
//
// Where `main.qnt` exposes `stepRefineAccumulate` as a heavily
// nondeterministic top-level step (random para, candidate, messages,
// pvfPanic, etc.), the tests below need DETERMINISTIC scenarios — input
// → output relationships the Rust harness can replay. The shared helper
// `applyHappyPath` constructs one well-formed work package against the
// current state and threads it through `applyAlwaysAccumulate` + `refine`
// + `accumulateOnePackage` without any nondet.
module tests {
  import types.* from "./types"
  import messages.* from "./messages"
  import state.* from "./state"
  import state_balance.* from "./state_balance"
  import code_upgrades.* from "./code_upgrades"
  import refine.* from "./refine"
  import accumulate.* from "./accumulate"
  import management.* from "./management"
  import state_vars.* from "./state_vars"
  import main.* from "./main"

  // --- Deterministic helpers ---------------------------------------------

  /// Apply one happy-path work package: always-accumulate flushes first,
  /// then refine succeeds and accumulate runs. `codeHash`
  /// is the hash the collator says it validated against (active *or*
  /// pending — both are accepted by `accumulateOkPrefix` step 3).
  action applyHappyPath(
    paraId: ParaId, codeHash: ValidationCodeHash, msgs: List[UpwardMessage]
  ): bool = {
    // A block advances the clock to its own timeslot.
    val blockNow = now + 1
    // Refine ran at guarantee time, against the pre-block state — it must not
    // see this block's always-accumulate effects.
    val pi = svc.paraInfo(paraId)
    val pkg: WorkPackage = {
      authorizedParas: List(paraId),
      items: List({ validationCode: { hash: codeHash, len: FixedCodeLen }, pov: 0 })
    }
    val newHead = pi.headData + 1
    val wr: WorkResult = {
      result: WorkOk(refine(
        svc, pkg, false, false,
        Some(headHash(pi.headData)), Some(newHead), msgs
      )[0]),
      authTrace: 0
    }
    // The block then runs always-accumulate, then applies the package.
    val afterAlwaysAccumulate = applyAlwaysAccumulate(svc, blockNow)
    // lookup-anchor 0 = prune nothing; these deterministic tests don't
    // exercise §5.1 log pruning (covered by the `parachain_log_above_lookup`
    // invariant under `quint run`).
    val out = accumulateOnePackage(
      afterAlwaysAccumulate.state, wr, solicitedSet, None, FixedCodeLen, blockNow, 0
    )
    all {
      svc' = out.state,
      now' = blockNow,
      jamStagingSet' = match out.jamDesignate {
        | Some(v) => v
        | None => jamStagingSet
      },
      solicitedSet' = out.solicited,
      prevSvc' = svc,
      lastStepWorkResults' = List(wr),
      lastStepBouncedTransfers' = List(),
      lastStepAssigns' = afterAlwaysAccumulate.assigns.concat(out.jamAssigns),
      logPrunedBelow' = logPrunedBelow
    }
  }

  /// Two happy-path work packages applied in one step (same block). The
  /// second package refines against the post-wp1 state, so a same-para
  /// pair chains heads correctly.
  action applyTwoHappyPaths(
    p1: ParaId, c1: ValidationCodeHash, m1: List[UpwardMessage],
    p2: ParaId, c2: ValidationCodeHash, m2: List[UpwardMessage]
  ): bool = {
    // Both packages accumulate in one block at its own timeslot.
    val blockNow = now + 1
    // Refine wr1 at guarantee time, against the pre-block state.
    val pi1 = svc.paraInfo(p1)
    val pkg1: WorkPackage = {
      authorizedParas: List(p1),
      items: List({ validationCode: { hash: c1, len: FixedCodeLen }, pov: 0 })
    }
    val nh1 = pi1.headData + 1
    val wr1: WorkResult = {
      result: WorkOk(refine(
        svc, pkg1, false, false,
        Some(headHash(pi1.headData)), Some(nh1), m1
      )[0]),
      authTrace: 0
    }
    // Block: always-accumulate first, then apply wr1.
    val afterAlwaysAccumulate = applyAlwaysAccumulate(svc, blockNow)
    val out1 = accumulateOnePackage(
      afterAlwaysAccumulate.state, wr1, solicitedSet, None, FixedCodeLen, blockNow, 0
    )
    val pi2 = out1.state.paraInfo(p2)
    val pkg2: WorkPackage = {
      authorizedParas: List(p2),
      items: List({ validationCode: { hash: c2, len: FixedCodeLen }, pov: 0 })
    }
    val nh2 = pi2.headData + 1
    val wr2: WorkResult = {
      result: WorkOk(refine(
        out1.state, pkg2, false, false,
        Some(headHash(pi2.headData)), Some(nh2), m2
      )[0]),
      authTrace: 0
    }
    val out2Package = accumulateOnePackage(
      out1.state, wr2, out1.solicited, out1.jamDesignate,
      FixedCodeLen, blockNow, 0
    )
    val out2 = {
      ...out2Package,
      jamAssigns: out1.jamAssigns.concat(out2Package.jamAssigns)
    }
    all {
      svc' = out2.state,
      now' = blockNow,
      jamStagingSet' = match out2.jamDesignate {
        | Some(v) => v
        | None => jamStagingSet
      },
      solicitedSet' = out2.solicited,
      prevSvc' = svc,
      lastStepWorkResults' = List(wr1, wr2),
      lastStepBouncedTransfers' = List(),
      lastStepAssigns' = afterAlwaysAccumulate.assigns.concat(out2.jamAssigns),
      logPrunedBelow' = logPrunedBelow
    }
  }

  /// Generic guard-only assertion wrapper: holds all primes at their
  /// current values and requires `pred` to be true.
  action expectThat(pred: bool): bool = all {
    pred,
    svc' = svc,
    now' = now,
    jamStagingSet' = jamStagingSet,
    solicitedSet' = solicitedSet,
    prevSvc' = prevSvc,
    lastStepWorkResults' = lastStepWorkResults,
    lastStepBouncedTransfers' = lastStepBouncedTransfers,
    lastStepAssigns' = lastStepAssigns,
    logPrunedBelow' = logPrunedBelow
  }

  // --- Bounce-on-full -----------------------------------------------------

  /// Pre-built `incoming_transfers` at capacity for `bounceOnFullTest`.
  pure val fullIncomingTransfersFixture: List[{
    from: ServiceId, amount: Balance, memo: Memo
  }] =
    range(0, IncomingTransfersMaxLen).foldl(List(), (acc, i) =>
      acc.append({ from: MkServiceId(0), amount: i, memo: 0 }))

  /// Sets `svc.incomingTransfers` to the full fixture without going through
  /// `stepIncomingTransfer` (which would take 1 000 steps).
  action fillIncomingTransfers = all {
    svc' = { ...svc, incomingTransfers: fullIncomingTransfersFixture },
    now' = now,
    jamStagingSet' = jamStagingSet,
    solicitedSet' = solicitedSet,
    prevSvc' = svc,
    lastStepWorkResults' = lastStepWorkResults,
    lastStepBouncedTransfers' = lastStepBouncedTransfers,
    lastStepAssigns' = lastStepAssigns,
    logPrunedBelow' = logPrunedBelow
  }

  /// With `incoming_transfers` at capacity, the next ingress must bounce:
  /// the queue length must NOT grow, and exactly one entry must appear
  /// in `lastStepBouncedTransfers`.
  run bounceOnFullTest =
    init
      .then(fillIncomingTransfers)
      .then(stepIncomingTransfer)
      .then(expectThat(all {
        svc.incomingTransfers.length() == IncomingTransfersMaxLen,
        lastStepBouncedTransfers.length() == 1
      }))

  // --- Multi-package accumulate ------------------------------------------

  /// Two work packages in one block — one for each privileged chain.
  /// Both heads advance from 0 to 1, and `lastStepWorkResults` records
  /// two results in order.
  run multipleWorkPackagesAccumulateTest = {
    val coretimeCode: ValidationCodeHash = { vchBytes: 1 }
    val assetHubCode: ValidationCodeHash = { vchBytes: 2 }
    init
      .then(applyTwoHappyPaths(
        CoretimeParaId, coretimeCode, List(),
        AssetHubParaId, assetHubCode, List()
      ))
      .then(expectThat(all {
        lastStepWorkResults.length() == 2,
        svc.paraInfo(CoretimeParaId).headData == 1,
        svc.paraInfo(AssetHubParaId).headData == 1
      }))
  }

  /// Refine a candidate for `paraId` declaring a STALE parent head — the sentinel
  /// head the para never has (`StaleParentHead`) — as both parent and new head,
  /// carrying `msgs` as its upward messages. Accumulate's parent-head check (§5.1
  /// step 3) must reject it, so neither its head write nor its replayed messages
  /// (§5.1 step 7) take effect. The new head is deliberately NOT `current + 1`:
  /// if the check were broken and the candidate applied, the head would jump to
  /// an obviously-invalid sentinel value, so the "head unchanged" assertion can
  /// never pass by coincidence.
  action applyStaleParentCandidate(
    paraId: ParaId, codeHash: ValidationCodeHash, msgs: List[UpwardMessage]
  ): bool = {
    val blockNow = now + 1
    val pkg: WorkPackage = {
      authorizedParas: List(paraId),
      items: List({ validationCode: { hash: codeHash, len: FixedCodeLen }, pov: 0 })
    }
    val wr: WorkResult = {
      result: WorkOk(refine(
        svc, pkg, false, false,
        Some(headHash(StaleParentHead)), Some(StaleParentHead), msgs
      )[0]),
      authTrace: 0
    }
    val afterAlwaysAccumulate = applyAlwaysAccumulate(svc, blockNow)
    val out = accumulateOnePackage(
      afterAlwaysAccumulate.state, wr, solicitedSet, None, FixedCodeLen, blockNow, 0
    )
    all {
      svc' = out.state,
      now' = blockNow,
      jamStagingSet' = match out.jamDesignate {
        | Some(v) => v
        | None => jamStagingSet
      },
      solicitedSet' = out.solicited,
      prevSvc' = svc,
      lastStepWorkResults' = List(wr),
      lastStepBouncedTransfers' = List(),
      lastStepAssigns' = afterAlwaysAccumulate.assigns.concat(out.jamAssigns),
      logPrunedBelow' = logPrunedBelow
    }
  }

  /// Parent-head check (§5.1 step 3): a candidate declaring a stale parent head
  /// (one the para no longer has) is rejected, leaving the head unchanged — this
  /// is what stops a candidate built on a superseded head from overwriting a
  /// forced `parachain_set_head`. A candidate on the CURRENT head advances it
  /// normally (first step); the stale one does not (second step).
  run staleParentCandidateRejectedTest = {
    val assetHubCode: ValidationCodeHash = { vchBytes: 2 }
    init
      .then(applyHappyPath(AssetHubParaId, assetHubCode, List()))
      .then(expectThat(svc.paraInfo(AssetHubParaId).headData == 1))
      .then(applyStaleParentCandidate(AssetHubParaId, assetHubCode, List()))
      .then(expectThat(svc.paraInfo(AssetHubParaId).headData == 1))
  }

  /// §5.1 step 7: a rejected candidate's upward messages are never replayed, so a
  /// forced `parachain_set_head` from a STALE candidate is ignored. A valid
  /// Coretime candidate first force-sets Asset Hub's head to 42 (set_head applies
  /// on an accepted candidate). Then a stale Coretime candidate (rejected at the
  /// parent-head check) carries `parachain_set_head(AssetHub, 7)`; because it is
  /// rejected, that message is dropped and Asset Hub keeps 42.
  run staleParentSetHeadIgnoredTest = {
    val coretimeCode: ValidationCodeHash = { vchBytes: 1 }
    init
      .then(applyHappyPath(
        CoretimeParaId, coretimeCode,
        List(ParachainSetHead({ paraId: AssetHubParaId, newHead: 42 }))))
      .then(expectThat(all {
        svc.paraInfo(AssetHubParaId).headData == 42,
        svc.paraInfo(CoretimeParaId).headData == 1
      }))
      .then(applyStaleParentCandidate(
        CoretimeParaId, coretimeCode,
        List(ParachainSetHead({ paraId: AssetHubParaId, newHead: 7 }))))
      .then(expectThat(all {
        // The stale candidate's set_head(AssetHub, 7) was dropped: AH keeps 42.
        svc.paraInfo(AssetHubParaId).headData == 42,
        // The stale candidate itself was rejected: Coretime keeps its head.
        svc.paraInfo(CoretimeParaId).headData == 1
      }))
  }

  /// §6.2: `parachain_set_head` is an upsert only for a REGISTERED para — it is a
  /// no-op on a `ParaId` with no `ParaInfo` (Coretime must call
  /// `parachain_set_state_balance` first). A valid Coretime candidate is accepted
  /// and carries `parachain_set_head(MkParaId(9), 42)` for an unregistered para;
  /// the message is replayed but must NOT create a `ParaInfo` entry.
  run setHeadNonExistentParaNoOpTest = {
    val coretimeCode: ValidationCodeHash = { vchBytes: 1 }
    init
      .then(applyHappyPath(
        CoretimeParaId, coretimeCode,
        List(ParachainSetHead({ paraId: MkParaId(9), newHead: 42 }))))
      .then(expectThat(all {
        // The Coretime candidate was accepted (head advanced), so its set_head
        // message WAS replayed...
        svc.paraInfo(CoretimeParaId).headData == 1,
        // ...but set_head on an unregistered para is a no-op: no ParaInfo entry.
        not(svc.parachains.keys().contains(MkParaId(9)))
      }))
  }

  // --- Parachain log eviction --------------------------------------------

  /// When over capacity, evict the oldest Refine entry first and preserve
  /// older Accumulate entries if any Refine entry remains.
  run evictionPrefersDroppingRefineTest = {
    // Opaque(1024) ≈ 1033 B per entry; 1 acc + 63 refine fits under the cap,
    // so pushing one more triggers byte-cap eviction of the oldest Refine entry.
    val full = range(0, 64).foldl(List(), (acc, i) =>
      if (i == 0) acc.append((i, AccumulateLogEntry(List(InvalidCodeHashAcc({ vchBytes: 7 })))))
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

  /// A 1-event AccumulateLogEntry with an InvalidCodeHashAcc costs 39 B
  /// (4 timeslot + 1 LogEntry disc + 1 vec prefix + 33 event), not the worst case.
  run bytecapSmallEntryCostTest = {
    val entry = (0, AccumulateLogEntry(List(InvalidCodeHashAcc({ vchBytes: 1 }))))
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
    val bigAcc = (1, AccumulateLogEntry(List(InvalidCodeHashAcc({ vchBytes: 1 }))))
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

  // --- Code upgrade lifecycle --------------------------------------------

  /// 1. Coretime registers para 3 with balance headroom for the new code.
  /// 2. Coretime sets para 3's active code to `codeA`.
  /// 3. Para 3 requests an upgrade to `codeB` (preimage gets added to
  ///    registry via `addReferencer`; `pendingUpgrade` armed).
  /// 4. A few blocks pass (each advances `now`), well below the deadline;
  ///    the pending upgrade must persist across them.
  /// 5. Para 3 validates with `codeB` (dual-code window) — activation
  ///    flips `validationCode.ref.hash` to `codeB` and clears
  ///    `pendingUpgrade`. The old `codeA` referencer is released.
  run codeUpgradeLifecycleTest = {
    val codeA: ValidationCodeHash = { vchBytes: 0x11 }
    val codeB: ValidationCodeHash = { vchBytes: 0x22 }
    val balance = BaselineFootprint + preimageFootprint(FixedCodeLen) * 4
    init
      .then(applyHappyPath(CoretimeParaId, { vchBytes: 1 }, List(
        ParachainSetStateBalance({ paraId: MkParaId(3), newTotal: balance }),
        ParachainSetValidationCode({
          paraId: MkParaId(3),
          newValidationCodeHash: codeA,
          newValidationCodeLen: FixedCodeLen
        })
      )))
      .then(expectThat(all {
        svc.parachains.keys().contains(MkParaId(3)),
        svc.paraInfo(MkParaId(3)).validationCode.ref.hash == codeA,
        svc.paraInfo(MkParaId(3)).pendingUpgrade == None
      }))
      .then(applyHappyPath(MkParaId(3), codeA, List(
        RequestCodeUpgrade({ hash: codeB, len: FixedCodeLen })
      )))
      .then(expectThat(
        match svc.paraInfo(MkParaId(3)).pendingUpgrade {
        | None => false
        | Some(pair) => pair._1.ref.hash == codeB
        }
      ))
      // A few blocks pass on the still-active codeA (each advances `now`),
      // well below the deadline; the pending upgrade must persist.
      .then(applyHappyPath(MkParaId(3), codeA, List()))
      .then(applyHappyPath(MkParaId(3), codeA, List()))
      .then(expectThat(match svc.paraInfo(MkParaId(3)).pendingUpgrade {
        | None => false
        | Some(pair) => pair._1.ref.hash == codeB
      }))
      .then(applyHappyPath(MkParaId(3), codeB, List()))
      .then(expectThat(all {
        svc.paraInfo(MkParaId(3)).validationCode.ref.hash == codeB,
        svc.paraInfo(MkParaId(3)).pendingUpgrade == None,
        // Old codeA referencer released (no other para held it).
        not(svc.preimageRegistry.keys().contains((vchAsHash(codeA), FixedCodeLen)))
      }))
  }

  /// (h, 100) and (h, 200) are distinct registry entries:
  /// soliciting one does not affect the other, and forgetting one
  /// leaves the other intact.
  run differentLensAreDistinctEntriesTest = {
    val h: Hash = { hashBytes: 42 }
    val p1 = CoretimeParaId
    val p2 = AssetHubParaId
    // Seed state: p1 registered with enough headroom.
    val s0 = initialState
    // Solicit (h, 100) for p1.
    val d1 = s0.addReferencer(p1, h, 100)
    // Solicit (h, 200) for p2.
    val d2 = d1.state.addReferencer(p2, h, 200)
    // Forget (h, 100) for p1.
    val d3 = d2.state.removeReferencer(p1, h, 100)
    all {
      // After soliciting (h,100) and (h,200): two distinct entries.
      assert(d2.state.preimageRegistry.keys().contains((h, 100))),
      assert(d2.state.preimageRegistry.keys().contains((h, 200))),
      // (h,100) has p1 as referencer; (h,200) has p2.
      assert(d2.state.preimageRegistry.get((h, 100)).referencers == Set(p1)),
      assert(d2.state.preimageRegistry.get((h, 200)).referencers == Set(p2)),
      // After forgetting (h,100): (h,100) entry is gone; (h,200) untouched.
      assert(not(d3.state.preimageRegistry.keys().contains((h, 100)))),
      assert(d3.state.preimageRegistry.keys().contains((h, 200))),
      assert(d3.state.preimageRegistry.get((h, 200)).referencers == Set(p2)),
      init
    }
  }

  /// A forced `parachain_set_validation_code` releases BOTH the old active
  /// (hA, 100) and the old pending (hB, 200) using their OWN stored lens —
  /// not a wrong cross-code len.
  run codeUpgradeReleasesCorrectLenTest = {
    val para = CoretimeParaId
    val hA: ValidationCodeHash = { vchBytes: 0xA1 }
    val hB: ValidationCodeHash = { vchBytes: 0xB2 }
    val hC: ValidationCodeHash = { vchBytes: 0xC3 }
    val lenA = 100
    val lenB = 200
    val lenC = 300
    // Build a state where para has active (hA, lenA) and pending (hB, lenB).
    // Use parachainSetValidationCode to establish hA, then requestCodeUpgrade for hB.
    val s0 = initialState
    val s1 = parachainSetValidationCode(s0, para, hA, lenA).state
    val s2 = requestCodeUpgrade(s1, 0, para, hB, lenB).state
    // Now force-replace with hC: must release (hA, lenA) and (hB, lenB) correctly.
    val s3 = parachainSetValidationCode(s2, para, hC, lenC).state
    all {
      // Registry before force-replace: (hA, lenA) and (hB, lenB) present.
      assert(s2.preimageRegistry.keys().contains((vchAsHash(hA), lenA))),
      assert(s2.preimageRegistry.keys().contains((vchAsHash(hB), lenB))),
      // After force-replace: (hA, lenA) and (hB, lenB) released.
      assert(not(s3.hasReferencer(vchAsHash(hA), lenA, para))),
      assert(not(s3.hasReferencer(vchAsHash(hB), lenB, para))),
      // (hC, lenC) now has para as referencer.
      assert(s3.hasReferencer(vchAsHash(hC), lenC, para)),
      // Wrong-len lookups are NOT a referencer (proves no cross-contamination).
      assert(not(s3.hasReferencer(vchAsHash(hA), lenC, para))),
      assert(not(s3.hasReferencer(vchAsHash(hB), lenC, para))),
      init
    }
  }

  // --- Coretime para register / operate / cleanup ------------------------

  /// Full Coretime lifecycle for a new para:
  /// register → set head → clean up. After cleanup the para's `ParaInfo`
  /// is gone and the preimage registry no longer references it.
  run paraRegisterOperateCleanupTest = {
    val coretimeCode: ValidationCodeHash = { vchBytes: 1 }
    init
      .then(applyHappyPath(CoretimeParaId, coretimeCode, List(
        ParachainSetStateBalance({ paraId: MkParaId(4), newTotal: BaselineFootprint * 2 })
      )))
      .then(expectThat(all {
        svc.parachains.keys().contains(MkParaId(4)),
        svc.paraInfo(MkParaId(4)).headData == 0,
        svc.paraInfo(MkParaId(4)).usedStateBalance == BaselineFootprint
      }))
      .then(applyHappyPath(CoretimeParaId, coretimeCode, List(
        ParachainSetHead({ paraId: MkParaId(4), newHead: 42 })
      )))
      .then(expectThat(svc.paraInfo(MkParaId(4)).headData == 42))
      .then(applyHappyPath(CoretimeParaId, coretimeCode, List(
        ParachainCleanUp(MkParaId(4))
      )))
      .then(expectThat(all {
        not(svc.parachains.keys().contains(MkParaId(4))),
        svc.preimageRegistry.keys().forall(key =>
          not(svc.preimageRegistry.get(key).referencers.contains(MkParaId(4))))
      }))
  }

  // --- Pending-upgrade timeout reap (§5.2 Phase 5b) ----------------------
  //
  // The timeout is hard to hit in random simulation: `UpgradeTimeoutTimeslots
  // = 14400` while a block advances `now` by only a small gap, so `quint run`
  // essentially never advances past a deadline. These deterministic `run` tests
  // drive the lazy-reap path directly so it is actually exercised.

  /// A registered para with an armed pending upgrade. Built off the initial
  /// state by requesting an upgrade to `pendingUpgradeHash` at timeslot 0, so
  /// the deadline is `UpgradeTimeoutTimeslots`.
  pure val pendingUpgradeHash: ValidationCodeHash = { vchBytes: 777 }

  pure val stateWithPendingUpgrade: ParachainServiceState = {
    // Coretime chain is registered in `initialState`; arm an upgrade for it
    // via the Accumulate-side `requestCodeUpgrade` primitive.
    requestCodeUpgrade(
      initialState, 0, CoretimeParaId, pendingUpgradeHash, FixedCodeLen
    ).state
  }

  /// Reaping BEFORE the deadline is a no-op: the pending upgrade survives.
  run reapBeforeDeadlineTest = {
    val para = CoretimeParaId
    val s0 = stateWithPendingUpgrade
    // Sanity: the upgrade is armed and its code is solicited.
    val armed =
      match s0.paraInfo(para).pendingUpgrade {
      | None => false
      | Some(pair) => pair._1.ref.hash == pendingUpgradeHash
      }
    // now = 1 < deadline (UpgradeTimeoutTimeslots).
    val reaped = reapTimedOutUpgrade(s0, 1, para, FixedCodeLen)
    val stillArmed =
      match reaped.state.paraInfo(para).pendingUpgrade {
      | None => false
      | Some(pair) => pair._1.ref.hash == pendingUpgradeHash
      }
    all {
      assert(armed),
      assert(s0.hasReferencer(vchAsHash(pendingUpgradeHash), FixedCodeLen, para)),
      assert(stillArmed),
      assert(reaped.state.hasReferencer(vchAsHash(pendingUpgradeHash), FixedCodeLen, para)),
      assert(not(reaped.jamForget)),
      init
    }
  }

  /// Reaping AT/AFTER the deadline clears the pending upgrade and releases
  /// the (unpinned) pending code's referencer, forwarding JAM `forget`.
  run reapAfterDeadlineTest = {
    val para = CoretimeParaId
    val s0 = stateWithPendingUpgrade
    // now = UpgradeTimeoutTimeslots == deadline; reap fires (deadline <= now).
    val reaped = reapTimedOutUpgrade(s0, UpgradeTimeoutTimeslots, para, FixedCodeLen)
    val cleared = reaped.state.paraInfo(para).pendingUpgrade == None
    all {
      // Pending upgrade is gone.
      assert(cleared),
      // The pending code was unpinned, so its referencer is released and
      // JAM `forget` is forwarded (it was the only referencer).
      assert(not(reaped.state.hasReferencer(vchAsHash(pendingUpgradeHash), FixedCodeLen, para))),
      assert(reaped.jamForget),
      // The active code is untouched.
      assert(reaped.state.hasReferencer(
        vchAsHash(s0.paraInfo(para).validationCode.ref.hash), FixedCodeLen, para)),
      init
    }
  }

  /// A pinned pending code survives the reap as an ordinary solicited
  /// preimage: `pendingUpgrade` is cleared but the referencer stays and no
  /// JAM `forget` is forwarded (§5.2 `pinned`).
  run reapPinnedPendingTest = {
    val para = CoretimeParaId
    // Pin the pending code by replaying a `Solicit` of it during Accumulate.
    val baseAcc: AccumulateOutcome = {
      state: stateWithPendingUpgrade,
      jamDesignate: None,
      jamSolicit: Set(),
      jamForget: Set(),
      jamAssigns: List(),
      logs: List()
    }
    val pinned = applyUpwardMessage(
      baseAcc, para, FixedCodeLen, 0,
      Solicit({ hash: vchAsHash(pendingUpgradeHash), len: FixedCodeLen })
    ).state
    val reaped = reapTimedOutUpgrade(pinned, UpgradeTimeoutTimeslots, para, FixedCodeLen)
    all {
      assert(reaped.state.paraInfo(para).pendingUpgrade == None),
      // Referencer survives because the para independently solicited it.
      assert(reaped.state.hasReferencer(vchAsHash(pendingUpgradeHash), FixedCodeLen, para)),
      assert(not(reaped.jamForget)),
      init
    }
  }

  // --- Missing head declaration (§4.2) -----------------------------------

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

  // --- Authorizer queue flush threshold ---------------------------------

  /// A pending queue for a core assigned at timeslot 0 must NOT be flushed
  /// at timeslot 1 — only 1 slot has elapsed, far below AuthorizerQueueLength (80).
  run authorizerQueueNotFlushedBeforeExhaustionTest = {
    val anyHash = { authBytes: 1 }
    val s0 = { ...initialState,
      pendingAuthorizerQueues: Map(0 -> List(anyHash)),
      pendingAuthorizerCores: Map(0 -> 80),
      lastAuthorizerAssignment: Map(0 -> 0)
    }
    val out = applyAlwaysAccumulate(s0, 1)
    all {
      assert(out.state.pendingAuthorizerQueues.keys().contains(0)),
      assert(out.state.pendingAuthorizerQueues.get(0) == List(anyHash)),
      assert(out.assigns == List()),
      init
    }
  }

  /// A non-immediate `set_authorizer_queue` marks the core dirty; once the
  /// queue is exhausted, always-accumulate flushes it and clears the index.
  run dirtyCoreTrackedOnSetAndClearedOnFlushTest = {
    val core = 5
    val anyHash: AuthorizerHash = { authBytes: 5 }
    val baseAcc: AccumulateOutcome = {
      state: { ...initialState, lastAuthorizerAssignment: Map(core -> 0) },
      jamDesignate: None,
      jamSolicit: Set(),
      jamForget: Set(),
      jamAssigns: List(),
      logs: List()
    }
    val out = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 0,
      SetAuthorizerQueue({
        core: core,
        queue: List(anyHash),
        immediate: false
      })
    )
    val flushed = applyAlwaysAccumulate(out.state, AuthorizerQueueLength)
    all {
      assert(out.state.pendingAuthorizerCores.keys().contains(core)),
      assert(not(flushed.state.pendingAuthorizerCores.keys().contains(core))),
      assert(not(flushed.state.pendingAuthorizerQueues.keys().contains(core))),
      assert(flushed.assigns == List(QueueSet({ core: core }))),
      init
    }
  }

  /// A cached queue stays scheduled in the future until it is due: an
  /// always-accumulate flush (which every block runs) before `apply_at` leaves
  /// the entry in place with `apply_at > now`; a flush at `apply_at` clears it.
  run pendingApplyAtStaysFutureUntilDueTest = {
    val core = 5
    val anyHash: AuthorizerHash = { authBytes: 5 }
    val baseAcc: AccumulateOutcome = {
      state: { ...initialState, lastAuthorizerAssignment: Map(core -> 0) },
      jamDesignate: None,
      jamSolicit: Set(),
      jamForget: Set(),
      jamAssigns: List(),
      logs: List()
    }
    // Warm core cached at now = 0 → apply_at = 0 + ((0 - 0) / 80 + 1) * 80 = 80.
    val cached = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 0,
      SetAuthorizerQueue({
        core: core, queue: List(anyHash), immediate: false
      })
    ).state
    val beforeDue = applyAlwaysAccumulate(cached, AuthorizerQueueLength - 1)
    val atDue = applyAlwaysAccumulate(cached, AuthorizerQueueLength)
    all {
      assert(cached.pendingAuthorizerCores.get(core) == AuthorizerQueueLength),
      // Not due yet: entry retained, and its deadline is still in the future.
      assert(beforeDue.state.pendingAuthorizerCores.keys().contains(core)),
      assert(beforeDue.state.pendingAuthorizerCores.get(core) > AuthorizerQueueLength - 1),
      // Due: flushed.
      assert(not(atDue.state.pendingAuthorizerCores.keys().contains(core))),
      init
    }
  }

  /// A non-immediate `set_authorizer_queue` for a never-assigned core applies
  /// inline because the cold core has no active authorizers.
  run coldCoreAppliedInlineTest = {
    val core = 7
    val nowSlot = 9
    val anyHash: AuthorizerHash = { authBytes: 9 }
    val baseAcc: AccumulateOutcome = {
      state: initialState,
      jamDesignate: None,
      jamSolicit: Set(),
      jamForget: Set(),
      jamAssigns: List(),
      logs: List()
    }
    val out = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, nowSlot,
      SetAuthorizerQueue({
        core: core,
        queue: List(anyHash),
        immediate: false
      })
    )
    all {
      assert(not(initialState.lastAuthorizerAssignment.keys().contains(core))),
      assert(out.jamAssigns == List(QueueSet({ core: core }))),
      assert(out.state.lastAuthorizerAssignment.get(core) == nowSlot),
      assert(not(out.state.pendingAuthorizerCores.keys().contains(core))),
      assert(not(out.state.pendingAuthorizerQueues.keys().contains(core))),
      init
    }
  }

  /// End-to-end cold -> warm -> cache with two real `set_authorizer_queue`
  /// calls (no hand-seeded warmth): the first, on a cold core, applies inline
  /// (assigns, does not cache); the second, on the now-warm core, caches into
  /// `pendingAuthorizerCores` with `apply_at` at the next 80-slot boundary.
  run secondSetAuthorizerQueueCachesOnWarmCoreTest = {
    val core = 5
    val hash1: AuthorizerHash = { authBytes: 1 }
    val hash2: AuthorizerHash = { authBytes: 2 }
    val baseAcc: AccumulateOutcome = {
      state: initialState,
      jamDesignate: None,
      jamSolicit: Set(),
      jamForget: Set(),
      jamAssigns: List(),
      logs: List()
    }
    // First call at now = 0: core is cold → applied inline, not cached.
    val after1 = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 0,
      SetAuthorizerQueue({ core: core, queue: List(hash1), immediate: false })
    )
    // Second call at now = 1: core is now warm → cached, apply_at = next
    // 80-slot boundary = 0 + ((1 - 0) / 80 + 1) * 80 = 80.
    val after2 = applyUpwardMessage(
      after1, CoretimeParaId, FixedCodeLen, 1,
      SetAuthorizerQueue({ core: core, queue: List(hash2), immediate: false })
    )
    all {
      // Cold to start.
      assert(not(initialState.lastAuthorizerAssignment.keys().contains(core))),
      // First (cold) call: assigned inline at now = 0, not cached.
      assert(after1.state.lastAuthorizerAssignment.get(core) == 0),
      assert(not(after1.state.pendingAuthorizerCores.keys().contains(core))),
      // Second (warm) call: cached with the next-boundary apply_at and the
      // new queue.
      assert(after2.state.pendingAuthorizerCores.keys().contains(core)),
      assert(after2.state.pendingAuthorizerCores.get(core) == AuthorizerQueueLength),
      assert(after2.state.pendingAuthorizerQueues.get(core) == List(hash2)),
      init
    }
  }

  /// `set_assigner` changes the core's assigner directly via JAM `assign`,
  /// independent of the queue: `Some(s)` sets it, `None` resets it.
  run setAssignerForwardsAssignerTest = {
    val core = 0
    val assigner = MkServiceId(7)
    val baseAcc: AccumulateOutcome = {
      state: initialState,
      jamDesignate: None,
      jamSolicit: Set(),
      jamForget: Set(),
      jamAssigns: List(),
      logs: List()
    }
    // Some(assigner): set the assigner.
    val setOut = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 0,
      SetAssigner({ core: core, newAssigner: Some(assigner) })
    )
    // None: reset the assigner.
    val resetOut = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 0,
      SetAssigner({ core: core, newAssigner: None })
    )
    all {
      assert(setOut.jamAssigns == List(AssignerSet({ core: core, newAssigner: Some(assigner) }))),
      assert(resetOut.jamAssigns == List(AssignerSet({ core: core, newAssigner: None }))),
      // no queue/pending state is touched by either.
      assert(setOut.state == initialState),
      assert(resetOut.state == initialState),
      init
    }
  }

  /// An empty `queue` cancels any cached queue for the core: it is dropped (not
  /// applied), with no `assign` and no update to `lastAuthorizerAssignment`.
  run emptyQueueCancelsCachedQueueTest = {
    val core = 5
    val staleHash: AuthorizerHash = { authBytes: 5 }
    // Warm core with a queue already cached for a future boundary.
    val cachedState = { ...initialState,
      pendingAuthorizerQueues: Map(core -> List(staleHash)),
      pendingAuthorizerCores: Map(core -> AuthorizerQueueLength),
      lastAuthorizerAssignment: Map(core -> 0)
    }
    val baseAcc: AccumulateOutcome = {
      state: cachedState,
      jamDesignate: None,
      jamSolicit: Set(),
      jamForget: Set(),
      jamAssigns: List(),
      logs: List()
    }
    val out = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 1,
      SetAuthorizerQueue({ core: core, queue: List(), immediate: false })
    )
    all {
      // cached queue dropped, not applied.
      assert(not(out.state.pendingAuthorizerQueues.keys().contains(core))),
      assert(not(out.state.pendingAuthorizerCores.keys().contains(core))),
      // no assign, and last-applied left untouched.
      assert(out.jamAssigns == List()),
      assert(out.state.lastAuthorizerAssignment.get(core) == 0),
      init
    }
  }

  /// An immediate `set_authorizer_queue` for a core that already has a cached
  /// pending queue drops that pending entry — the immediate update supersedes
  /// it (removed from both `pendingAuthorizerQueues` and `pendingAuthorizerCores`).
  run immediateDropsPendingQueueForSameCoreTest = {
    val core = 3
    val staleHash: AuthorizerHash = { authBytes: 1 }
    val immediateHash: AuthorizerHash = { authBytes: 2 }
    val cachedState = { ...initialState,
      pendingAuthorizerQueues: Map(core -> List(staleHash)),
      pendingAuthorizerCores: Map(core -> 80),
      lastAuthorizerAssignment: Map(core -> 0)
    }
    val baseAcc: AccumulateOutcome = {
      state: cachedState,
      jamDesignate: None,
      jamSolicit: Set(),
      jamForget: Set(),
      jamAssigns: List(),
      logs: List()
    }
    val out = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 1,
      SetAuthorizerQueue({
        core: core,
        queue: List(immediateHash),
        immediate: true
      })
    )
    all {
      // The stale cached queue is gone from both structures.
      assert(not(out.state.pendingAuthorizerQueues.keys().contains(core))),
      assert(not(out.state.pendingAuthorizerCores.keys().contains(core))),
      // The immediate assign was recorded and the assignment timestamp updated.
      assert(out.jamAssigns == List(QueueSet({ core: core }))),
      assert(out.state.lastAuthorizerAssignment.get(core) == 1),
      init
    }
  }

  /// A due cached queue flush and an immediate queue update for the same core
  /// in one step must leave the immediate assign last.
  run immediateNotClobberedByAlwaysAccumulateTest = {
    val core = 0
    val staleHash: AuthorizerHash = { authBytes: 1 }
    val immediateHash: AuthorizerHash = { authBytes: 2 }
    val t = AuthorizerQueueLength
    val dueState = { ...initialState,
      pendingAuthorizerQueues: Map(core -> List(staleHash)),
      pendingAuthorizerCores: Map(core -> t),
      lastAuthorizerAssignment: Map(core -> 0)
    }
    val afterAlwaysAccumulate = applyAlwaysAccumulate(dueState, t)
    val baseAcc: AccumulateOutcome = {
      state: afterAlwaysAccumulate.state,
      jamDesignate: None,
      jamSolicit: Set(),
      jamForget: Set(),
      jamAssigns: List(),
      logs: List()
    }
    val out = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, t,
      SetAuthorizerQueue({ core: core, queue: List(immediateHash), immediate: true })
    )
    val assigns = afterAlwaysAccumulate.assigns.concat(out.jamAssigns)
    all {
      assert(afterAlwaysAccumulate.assigns == List(QueueSet({ core: core }))),
      assert(assigns == List(
        QueueSet({ core: core }),
        QueueSet({ core: core })
      )),
      assert(not(out.state.pendingAuthorizerQueues.keys().contains(core))),
      assert(not(out.state.pendingAuthorizerCores.keys().contains(core))),
      assert(out.state.lastAuthorizerAssignment.get(core) == t),
      init
    }
  }

  /// An otherwise-valid candidate whose PVF skipped `set_parent_head_hash`
  /// and/or `set_head` is rejected by Refine with `MissingHeadDeclaration`.
  /// Coretime's active code (`{ vchBytes: 1 }`) is solicited in
  /// `initialState`, so the candidate passes `codeAvailable` and reaches the
  /// head-declaration check rather than failing earlier with `InvalidCodeHash`.
  run refineMissingHeadDeclarationTest = {
    val pkg: WorkPackage = {
      authorizedParas: List(CoretimeParaId),
      items: List({ validationCode: { hash: { vchBytes: 1 }, len: FixedCodeLen }, pov: 0 })
    }
    val parentOnly =
      refine(initialState, pkg, false, false, Some(headHash(0)), None, List())[0]
    val headOnly =
      refine(initialState, pkg, false, false, None, Some(1), List())[0]
    val neither =
      refine(initialState, pkg, false, false, None, None, List())[0]
    val both =
      refine(initialState, pkg, false, false, Some(headHash(0)), Some(1), List())[0]
    all {
      // Missing new head, missing parent head, or both → rejected.
      assert(parentOnly == Err({ paraId: CoretimeParaId, error: MissingHeadDeclaration })),
      assert(headOnly == Err({ paraId: CoretimeParaId, error: MissingHeadDeclaration })),
      assert(neither == Err({ paraId: CoretimeParaId, error: MissingHeadDeclaration })),
      // Both declared → not a MissingHeadDeclaration error (succeeds here).
      assert(both == Ok({
        paraId: CoretimeParaId,
        validationCode: { hash: { vchBytes: 1 }, len: FixedCodeLen },
        parentHeadHash: headHash(0),
        headData: 1,
        upwardMessages: List()
      })),
      init
    }
  }
}
