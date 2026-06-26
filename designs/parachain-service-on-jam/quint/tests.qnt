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
// current state and threads it through `refine` + `accumulateOnePackage`
// + `applyAlwaysAccumulate` without any nondet.
module tests {
  import types.* from "./types"
  import messages.* from "./messages"
  import state.* from "./state"
  import state_balance.* from "./state_balance"
  import code_upgrades.* from "./code_upgrades"
  import refine.* from "./refine"
  import accumulate.* from "./accumulate"
  import state_vars.* from "./state_vars"
  import main.* from "./main"

  // --- Deterministic helpers ---------------------------------------------

  /// Apply one happy-path work package: refine succeeds, accumulate runs,
  /// always-accumulate flushes any pending authorizer queues. `codeHash`
  /// is the hash the collator says it validated against (active *or*
  /// pending — both are accepted by `accumulateOkPrefix` step 3).
  action applyHappyPath(
    paraId: ParaId, codeHash: ValidationCodeHash, msgs: List[UpwardMessage]
  ): bool = {
    val pi = svc.paraInfo(paraId)
    val pkg: WorkPackage = {
      authorizedParas: List(paraId),
      items: List({ validationCodeHash: codeHash, pov: 0 })
    }
    val newHead = pi.headData + 1
    val wr: WorkResult = {
      result: WorkOk(refine(svc, pkg, false, false, headHash(pi.headData), newHead, msgs)[0]),
      authTrace: 0
    }
    val out = accumulateOnePackage(
      svc, wr, solicitedSet, None, FixedCodeLen, now
    )
    val sFinal = applyAlwaysAccumulate(out.state, now)
    all {
      svc' = sFinal,
      now' = now,
      jamStagingSet' = match out.jamDesignate {
        | Some(v) => v
        | None => jamStagingSet
      },
      solicitedSet' = out.solicited,
      prevSvc' = svc,
      lastStepWorkResults' = List(wr),
      lastStepBouncedTransfers' = List()
    }
  }

  /// Two happy-path work packages applied in one step (same block). The
  /// second package refines against the post-wp1 state, so a same-para
  /// pair chains heads correctly.
  action applyTwoHappyPaths(
    p1: ParaId, c1: ValidationCodeHash, m1: List[UpwardMessage],
    p2: ParaId, c2: ValidationCodeHash, m2: List[UpwardMessage]
  ): bool = {
    val pi1 = svc.paraInfo(p1)
    val pkg1: WorkPackage = {
      authorizedParas: List(p1),
      items: List({ validationCodeHash: c1, pov: 0 })
    }
    val nh1 = pi1.headData + 1
    val wr1: WorkResult = {
      result: WorkOk(refine(svc, pkg1, false, false, headHash(pi1.headData), nh1, m1)[0]),
      authTrace: 0
    }
    val out1 = accumulateOnePackage(
      svc, wr1, solicitedSet, None, FixedCodeLen, now
    )
    val pi2 = out1.state.paraInfo(p2)
    val pkg2: WorkPackage = {
      authorizedParas: List(p2),
      items: List({ validationCodeHash: c2, pov: 0 })
    }
    val nh2 = pi2.headData + 1
    val wr2: WorkResult = {
      result: WorkOk(refine(out1.state, pkg2, false, false, headHash(pi2.headData), nh2, m2)[0]),
      authTrace: 0
    }
    val out2 = accumulateOnePackage(
      out1.state, wr2, out1.solicited, out1.jamDesignate,
      FixedCodeLen, now
    )
    val sFinal = applyAlwaysAccumulate(out2.state, now)
    all {
      svc' = sFinal,
      now' = now,
      jamStagingSet' = match out2.jamDesignate {
        | Some(v) => v
        | None => jamStagingSet
      },
      solicitedSet' = out2.solicited,
      prevSvc' = svc,
      lastStepWorkResults' = List(wr1, wr2),
      lastStepBouncedTransfers' = List()
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
    lastStepBouncedTransfers' = lastStepBouncedTransfers
  }

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
    solicitedSet' = solicitedSet,
    prevSvc' = svc,
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
      .then(expectThat(all {
        svc.incomingTransfers.length() == IncomingTransfersMaxLen,
        lastStepBouncedTransfers.length() == 1
      }))

  // --- Multi-package accumulate ------------------------------------------

  /// Two work packages in one block — one for each privileged chain.
  /// Both heads advance from 0 to 1, and `lastStepWorkResults` records
  /// two results in order.
  run testMultipleWorkPackagesAccumulate = {
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

  // --- Code upgrade lifecycle --------------------------------------------

  /// 1. Coretime registers para 3 with balance headroom for the new code.
  /// 2. Coretime sets para 3's active code to `codeA`.
  /// 3. Para 3 requests an upgrade to `codeB` (preimage gets added to
  ///    registry via `addReferencer`; `pendingUpgrade` armed).
  /// 4. A few ticks pass, well below the upgrade deadline.
  /// 5. Para 3 validates with `codeB` (dual-code window) — activation
  ///    flips `validationCode.hash` to `codeB` and clears
  ///    `pendingUpgrade`. The old `codeA` referencer is released.
  run testCodeUpgradeLifecycle = {
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
        svc.paraInfo(MkParaId(3)).validationCode.hash == codeA,
        svc.paraInfo(MkParaId(3)).pendingUpgrade == None
      }))
      .then(applyHappyPath(MkParaId(3), codeA, List(
        RequestCodeUpgrade({ hash: codeB, len: FixedCodeLen })
      )))
      .then(expectThat(
        match svc.paraInfo(MkParaId(3)).pendingUpgrade {
          | None => false
          | Some(pair) => pair._1.hash == codeB
        }
      ))
      .then(stepTick)
      .then(stepTick)
      .then(stepTick)
      .then(applyHappyPath(MkParaId(3), codeB, List()))
      .then(expectThat(all {
        svc.paraInfo(MkParaId(3)).validationCode.hash == codeB,
        svc.paraInfo(MkParaId(3)).pendingUpgrade == None,
        // Old codeA referencer released (no other para held it).
        not(svc.preimageRegistry.keys().contains(vchAsHash(codeA)))
      }))
  }

  // --- Coretime para register / operate / cleanup ------------------------

  /// Full Coretime lifecycle for a new para:
  /// register → set head → clean up. After cleanup the para's `ParaInfo`
  /// is gone and the preimage registry no longer references it.
  run testParaRegisterOperateCleanup = {
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
        svc.preimageRegistry.keys().forall(h =>
          not(svc.preimageRegistry.get(h).referencers.contains(MkParaId(4))))
      }))
  }

  // --- Pending-upgrade timeout reap (§5.2 Phase 5b) ----------------------
  //
  // The timeout is hard to hit in random simulation: `UpgradeTimeoutTimeslots
  // = 14400` and `stepTick` advances `now` by 1, so `quint run` essentially
  // never advances past a deadline. These deterministic `run` tests drive the
  // lazy-reap path directly so it is actually exercised.

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
        | Some(pair) => pair._1.hash == pendingUpgradeHash
      }
    // now = 1 < deadline (UpgradeTimeoutTimeslots).
    val reaped = reapTimedOutUpgrade(s0, 1, para, FixedCodeLen)
    val stillArmed =
      match reaped.state.paraInfo(para).pendingUpgrade {
        | None => false
        | Some(pair) => pair._1.hash == pendingUpgradeHash
      }
    all {
      assert(armed),
      assert(s0.hasReferencer(vchAsHash(pendingUpgradeHash), para)),
      assert(stillArmed),
      assert(reaped.state.hasReferencer(vchAsHash(pendingUpgradeHash), para)),
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
      assert(not(reaped.state.hasReferencer(vchAsHash(pendingUpgradeHash), para))),
      assert(reaped.jamForget),
      // The active code is untouched.
      assert(reaped.state.hasReferencer(
        vchAsHash(s0.paraInfo(para).validationCode.hash), para)),
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
      jamDesignate: None, jamSolicit: Set(), jamForget: Set(), logs: List()
    }
    val pinned = applyUpwardMessage(
      baseAcc, para, FixedCodeLen, 0,
      Solicit({ hash: vchAsHash(pendingUpgradeHash), len: FixedCodeLen })
    ).state
    val reaped = reapTimedOutUpgrade(pinned, UpgradeTimeoutTimeslots, para, FixedCodeLen)
    all {
      assert(reaped.state.paraInfo(para).pendingUpgrade == None),
      // Referencer survives because the para independently solicited it.
      assert(reaped.state.hasReferencer(vchAsHash(pendingUpgradeHash), para)),
      assert(not(reaped.jamForget)),
      init
    }
  }
}
