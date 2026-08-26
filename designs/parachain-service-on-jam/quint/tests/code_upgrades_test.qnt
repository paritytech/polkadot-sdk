// Parachain code-upgrade lifecycle and pending-upgrade timeout reap (design 5.2).
module code_upgrades_test {
  import types.* from "../types"
  import messages.* from "../messages"
  import state.* from "../state"
  import state_balance.* from "../state_balance"
  import code_upgrades.* from "../code_upgrades"
  import accumulate.* from "../accumulate"
  import management.* from "../management"
  import state_vars.* from "../state_vars"
  import main.* from "../main"
  import support.* from "./support"

  /// 1. Coretime registers para 3 with balance headroom for the new code.
  /// 2. Coretime sets para 3's active code to `codeA`.
  /// 3. Para 3 requests an upgrade to `codeB` (preimage gets added to
  ///    registry via `addReferencer`; `pendingUpgrade` armed).
  /// 4. A few blocks pass (each advances `now`), well below the deadline;
  ///    the pending upgrade must persist across them.
  /// 5. Para 3 validates with `codeB` (dual-code window): activation
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
        activeCodeHashOr(svc.paraInfo(MkParaId(3)), NoCodeHash) == codeA,
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
      // codeA was validated against, so its preimage is available (Provided);
      // only then does activation's `forget` arm the two-step expunge.
      .then(provide(vchAsHash(codeA), FixedCodeLen))
      .then(applyHappyPath(MkParaId(3), codeB, List()))
      .then(expectThat(all {
        activeCodeHashOr(svc.paraInfo(MkParaId(3)), NoCodeHash) == codeB,
        svc.paraInfo(MkParaId(3)).pendingUpgrade == None,
        // Old codeA was the sole referencer of an available preimage, so
        // activation's first `forget` unrequests it (§6.1): the registry entry
        // survives and the code's status is now `Unrequested`.
        isUnrequested(svc.preimageStatus.get((vchAsHash(codeA), FixedCodeLen))),
        svc.preimageRegistry.keys().contains((vchAsHash(codeA), FixedCodeLen)),
        svc.preimageRegistry.get((vchAsHash(codeA), FixedCodeLen)).referencers
          == Set(MkParaId(3))
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
    // (h, 100) becomes available, so its forget takes the two-step path.
    val provided = markProvided(d2.state, h, 100)
    // First forget of (h, 100) for p1 (its sole referencer) at now = 0:
    // unrequests it, the entry survives awaiting expunge.
    val d3 = provided.removeReferencer(p1, h, 100, 0)
    // Second forget once strictly past the deadline: the entry is expunged.
    val d4 = d3.state.removeReferencer(p1, h, 100, ExpungePeriod + 1)
    all {
      // After soliciting (h,100) and (h,200): two distinct entries.
      assert(d2.state.preimageRegistry.keys().contains((h, 100))),
      assert(d2.state.preimageRegistry.keys().contains((h, 200))),
      // (h,100) has p1 as referencer; (h,200) has p2.
      assert(d2.state.preimageRegistry.get((h, 100)).referencers == Set(p1)),
      assert(d2.state.preimageRegistry.get((h, 200)).referencers == Set(p2)),
      // First forget: (h,100) still present (awaiting expunge), (h,200) untouched.
      assert(d3.state.preimageRegistry.keys().contains((h, 100))),
      assert(isUnrequested(d3.state.preimageStatus.get((h, 100)))),
      // Second forget once due: (h,100) entry is gone; (h,200) untouched.
      assert(not(d4.state.preimageRegistry.keys().contains((h, 100)))),
      assert(d4.state.preimageRegistry.keys().contains((h, 200))),
      assert(d4.state.preimageRegistry.get((h, 200)).referencers == Set(p2)),
      init
    }
  }

  /// A forced `parachain_set_validation_code` releases BOTH the old active
  /// (hA, 100) and the old pending (hB, 200) using their OWN stored lens,
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
    val s1 = parachainSetValidationCode(s0, para, hA, lenA, 0).state
    val s2 = requestCodeUpgrade(s1, 0, para, hB, lenB).state
    // Both codes become available, so their release takes the two-step path.
    val s2p = markProvided(markProvided(s2, vchAsHash(hA), lenA), vchAsHash(hB), lenB)
    // Now force-replace with hC: releases (hA, lenA) and (hB, lenB) via the
    // two-step forget, so both go `Unrequested` under their OWN len.
    val s3 = parachainSetValidationCode(s2p, para, hC, lenC, 0).state
    all {
      // Registry before force-replace: (hA, lenA) and (hB, lenB) present.
      assert(s2.preimageRegistry.keys().contains((vchAsHash(hA), lenA))),
      assert(s2.preimageRegistry.keys().contains((vchAsHash(hB), lenB))),
      // After force-replace: (hA, lenA) and (hB, lenB) awaiting expunge under
      // their OWN lens (proves no wrong cross-code len).
      assert(isUnrequested(s3.preimageStatus.get((vchAsHash(hA), lenA)))),
      assert(isUnrequested(s3.preimageStatus.get((vchAsHash(hB), lenB)))),
      // (hC, lenC) now has para as referencer.
      assert(s3.hasReferencer(vchAsHash(hC), lenC, para)),
      // Wrong-len entries were never created (proves no cross-contamination).
      assert(not(s3.preimageStatus.keys().contains((vchAsHash(hA), lenC)))),
      assert(not(s3.preimageStatus.keys().contains((vchAsHash(hB), lenC)))),
      init
    }
  }

  /// Forgetting BOTH an active and a pending code in one call emits a SEPARATE
  /// `ForgetAgainAt` for each, with each code's OWN deadline. Regression for the
  /// bug where only one log entry was kept, dropping the other code's retry
  /// deadline. Here the two codes are in different limbo states, so the two
  /// deadlines are genuinely distinct.
  run cleanUpEmitsBothForgetAgainAtTest = {
    val para = MkParaId(9)
    val hA: ValidationCodeHash = { vchBytes: 0xA9 }
    val hB: ValidationCodeHash = { vchBytes: 0xB9 }
    val lenA = 100
    val lenB = 200
    val bal = BaselineFootprint + preimageFootprint(lenA) + preimageFootprint(lenB)
    val s0 = parachainSetStateBalance(initialState, para, bal).state
    val s1 = parachainSetValidationCode(s0, para, hA, lenA, 0).state
    val s2 = requestCodeUpgrade(s1, 0, para, hB, lenB).state
    // Pending code (hB) is available; active code (hA) is already `Unrequested(5)`
    // (its window not yet elapsed), so their forget deadlines differ.
    val s3 = markProvided(s2, vchAsHash(hB), lenB)
    val s4 = { ...s3, preimageStatus: s3.preimageStatus.put((vchAsHash(hA), lenA), Unrequested(5)) }
    // Clean up at now = 10, inside hA's window (10 <= 5 + ExpungePeriod):
    //   active hA:  Unrequested(5) → no-op retry, due = 5 + ExpungePeriod
    //   pending hB: Provided       → Unrequested(10), due = 10 + ExpungePeriod
    val nowT = 10
    val c = parachainCleanUp(s4, para, nowT)
    all {
      // Two log entries, one ForgetAgainAt per code, with DISTINCT deadlines,
      // the pick-one bug would have kept only one of these.
      assert(c.logs.length() == 2),
      assert(c.logs.select(e =>
        e == ForgetAgainAt({ hash: vchAsHash(hA), len: lenA, due: 5 + ExpungePeriod })).length() == 1),
      assert(c.logs.select(e =>
        e == ForgetAgainAt({ hash: vchAsHash(hB), len: lenB, due: nowT + ExpungePeriod })).length() == 1),
      init
    }
  }

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
      assert(reaped.jamForget == None),
      init
    }
  }

  /// Reaping AT/AFTER the deadline clears the pending upgrade and releases the
  /// (unpinned) pending code's referencer. The preimage was never provided
  /// (`Unprovided`), so the single JAM `forget` DROPS it in one step, no
  /// limbo, no retained referencer (§6.1, JAM `ForgetImplication::Drop`).
  run reapAfterDeadlineTest = {
    val para = CoretimeParaId
    val s0 = stateWithPendingUpgrade
    // now = UpgradeTimeoutTimeslots == deadline; reap fires (deadline <= now).
    val reaped = reapTimedOutUpgrade(s0, UpgradeTimeoutTimeslots, para, FixedCodeLen)
    val cleared = reaped.state.paraInfo(para).pendingUpgrade == None
    all {
      // Pending upgrade is gone.
      assert(cleared),
      // The never-provided pending code is dropped outright: the entry and its
      // referencer are gone, no limbo status remains, JAM `forget` forwarded.
      assert(not(reaped.state.preimageRegistry.keys().contains((vchAsHash(pendingUpgradeHash), FixedCodeLen)))),
      assert(not(reaped.state.preimageStatus.keys().contains((vchAsHash(pendingUpgradeHash), FixedCodeLen)))),
      assert(reaped.jamForget == Some(vchAsHash(pendingUpgradeHash))),
      // The active code is untouched.
      assert(reaped.state.hasReferencer(
        vchAsHash(activeCodeHashOr(s0.paraInfo(para), NoCodeHash)), FixedCodeLen, para)),
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
      baseAcc, para, FixedCodeLen, 0, 0,
      Solicit({ target: Parachain(para), hash: vchAsHash(pendingUpgradeHash), len: FixedCodeLen })
    ).state
    val reaped = reapTimedOutUpgrade(pinned, UpgradeTimeoutTimeslots, para, FixedCodeLen)
    all {
      assert(reaped.state.paraInfo(para).pendingUpgrade == None),
      // Referencer survives because the para independently solicited it.
      assert(reaped.state.hasReferencer(vchAsHash(pendingUpgradeHash), FixedCodeLen, para)),
      assert(reaped.jamForget == None),
      init
    }
  }

  /// Activating an upgrade over an old code that was still `Provided` cannot
  /// free its slot in one step: §6.1's two-step forget only unrequests it and
  /// RETAINS the referencer until a second forget past `ForgetAgainAt`. 
  run activationOverProvidedCodeRetainsSlotTest = {
    val para = CoretimeParaId
    val oldRef: ValidationCodeRef = { hash: { vchBytes: 1 }, len: FixedCodeLen }
    val oldHash = vchAsHash(oldRef.hash)
    val newHash: ValidationCodeHash = { vchBytes: 101 }
    // The old (active) code has been supplied to JAM.
    val provided = markProvided(initialState, oldHash, FixedCodeLen)
    // Arm an upgrade to `newHash`, then activate it with a candidate using it.
    val requested = requestCodeUpgrade(provided, 0, para, newHash, FixedCodeLen)
    val activated = activateUpgradeIfMatch(requested.state, para, newHash, FixedCodeLen, 5)
    val pi = activated.state.paraInfo(para)
    all {
      // The upgrade did activate.
      assert(pi.validationCode == Some({ ref: { hash: newHash, len: FixedCodeLen }, pinned: false })),
      assert(pi.pendingUpgrade == None),
      // ...yet the old code's referencer legitimately survives, in limbo.
      assert(activated.state.hasReferencer(oldHash, FixedCodeLen, para)),
      assert(isUnrequested(activated.state.preimageStatus.get((oldHash, FixedCodeLen)))),
      assert(activated.retained),
      init
    }
  }
}
