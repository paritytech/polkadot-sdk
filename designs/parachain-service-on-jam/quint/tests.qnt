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
  import head_commitment.* from "./head_commitment"
  import state_vars.* from "./state_vars"
  import main.* from "./main"
  import invariants.* from "./invariants"

  // --- Deterministic helpers ---------------------------------------------

  /// Mark a solicited preimage as `Provided`.
  pure def markProvided(
    s: ParachainServiceState, h: Hash, len: int
  ): ParachainServiceState =
    { ...s, preimageStatus: s.preimageStatus.put((h, len), Provided) }

  /// True iff a `RequestStatus` is `Unrequested(_)`.
  pure def isUnrequested(st: RequestStatus): bool =
    match st { | Unrequested(_) => true | _ => false }

  /// Apply a sequence of happy-path work packages in one block via the shared
  /// `accumulateBlock` pipeline — the SAME function the model step
  /// (`stepRefineAccumulate`) uses — so these tests exercise the real logic
  /// rather than a parallel copy. Each package is refined (at guarantee time,
  /// against the pre-block state) and paired with lookup-anchor 0 (prune
  /// nothing). Each element is `(para, codeHash, msgs)`; `codeHash` is what the
  /// collator says it validated against (active *or* pending — both accepted by
  /// `accumulateOkPrefix` step 3).
  action applyHappyPaths(
    pkgs: List[(ParaId, ValidationCodeHash, List[UpwardMessage])]
  ): bool = {
    // A block advances the clock to its own timeslot.
    val blockNow = now + 1
    // Refine each package at guarantee time against the pre-block state (it must
    // not see this block's always-accumulate effects).
    val packages: List[WorkResult] = pkgs.foldl(List(), (acc, p) => {
      val pi = svc.paraInfo(p._1)
      val pkg: WorkPackage = {
        authorizedParas: List(p._1),
        items: List({ validationCode: { hash: p._2, len: FixedCodeLen }, pov: 0 })
      }
      val wr: WorkResult = {
        result: refine(
          svc, pkg, false, PvfOk,
          Some(headHash(pi.headData)), Some(pi.headData + 1), p._3, 0
        )[0],
        authTrace: 0
      }
      acc.append(wr)
    })
    // Run the exact per-block Accumulate pipeline the model step runs.
    val blockOut = accumulateBlock(
      svc, blockNow, jamStagingSet, solicitedSet, FixedCodeLen, packages)
    all {
      svc' = blockOut.state,
      now' = blockNow,
      jamStagingSet' = blockOut.staging,
      solicitedSet' = blockOut.solicited,
      prevSvc' = svc,
      lastStepWorkResults' = blockOut.results,
      lastStepAssigns' = blockOut.assigns,
      lastHeadRoot' = blockOut.headRoot,
      logPrunedBelow' =
        blockOut.logPruned.foldl(logPrunedBelow, (m, pair) => m.put(pair._1, pair._2))
    }
  }

  /// Singular convenience wrapper: apply one happy-path work package.
  action applyHappyPath(
    paraId: ParaId, codeHash: ValidationCodeHash, msgs: List[UpwardMessage]
  ): bool = applyHappyPaths(List((paraId, codeHash, msgs)))

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
    lastStepAssigns' = lastStepAssigns,
    lastHeadRoot' = lastHeadRoot,
    logPrunedBelow' = logPrunedBelow
  }

  /// Force `svc` to a specific state so an invariant can be evaluated on it.
  action setSvc(s: ParachainServiceState): bool = all {
    svc' = s,
    now' = now,
    jamStagingSet' = jamStagingSet,
    solicitedSet' = solicitedSet,
    prevSvc' = prevSvc,
    lastStepWorkResults' = lastStepWorkResults,
    lastStepAssigns' = lastStepAssigns,
    lastHeadRoot' = lastHeadRoot,
    logPrunedBelow' = logPrunedBelow
  }

  /// Action wrapper for `markProvided`: flips a solicited preimage's `query`
  /// status to `Provided`.
  action provide(h: Hash, len: int): bool = all {
    svc' = markProvided(svc, h, len),
    now' = now,
    jamStagingSet' = jamStagingSet,
    solicitedSet' = solicitedSet,
    prevSvc' = prevSvc,
    lastStepWorkResults' = lastStepWorkResults,
    lastStepAssigns' = lastStepAssigns,
    lastHeadRoot' = lastHeadRoot,
    logPrunedBelow' = logPrunedBelow
  }

  // --- Best-effort transfer admission (§5.1) ------------------------------

  /// `incoming_transfers` at capacity, maximally fragmented: one transfer per
  /// slot in slots `0 .. MaxIncomingTransfers`, chained head to tail.
  pure val fullIncomingTransfersFixture: Timeslot -> {
    transfers: List[{ from: ServiceId, amount: Balance, memo: Memo }],
    nextSlot: Option[Timeslot]
  } =
    range(0, MaxIncomingTransfers).foldl(Map(), (acc, i) =>
      acc.put(i, {
        transfers: List({ from: MkServiceId(0), amount: 1000 + i, memo: 0 }),
        nextSlot: if (i == MaxIncomingTransfers - 1) None else Some(i + 1)
      }))

  pure val fullTransferQueue: ParachainServiceState = {
    ...initialState,
    incomingTransfers: fullIncomingTransfersFixture,
    incomingTransferChain:
      Some({ firstSlot: 0, lastSlot: MaxIncomingTransfers - 1 })
  }

  /// `MaxIncomingTransfers` is a reservation, not a cap (§5.1). Below it a
  /// transfer is recorded whatever it carries. At it the slot is unreserved, so
  /// only a transfer covering its own entry cost is recorded, and the queue then
  /// grows past the bound.
  run reservedPortionThenSelfFundedTest = {
    val full = fullTransferQueue
    val at = MaxIncomingTransfers   // a fresh slot past the fixture's tail
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
      // Empty queue: the new bucket is both endpoints.
      assert(below.incomingTransferChain == Some({ firstSlot: 7, lastSlot: 7 })),
      assert(queuedTransferCount(below) == 1),
      assert(short == full),
      // Past the bound: a new bucket, chained from the old tail.
      assert(queuedTransferCount(paid) == MaxIncomingTransfers + 1),
      assert(paid.incomingTransferChain == Some({ firstSlot: 0, lastSlot: at })),
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
      assert(b.incomingTransferChain == Some({ firstSlot: 5, lastSlot: 5 })),
      assert(queuedTransferCount(b) == 2),
      init
    }
  }

  /// `consume_transfers_up_to(slot)` drops whole buckets up to and including
  /// `slot`, re-heads the chain, and clears it once nothing survives.
  run consumeDrainsBucketsUpToSlotTest = {
    val a = applyIncomingTransfer(initialState, 1,
      { from: MkServiceId(0), amount: 500, memo: 0 })
    val b = applyIncomingTransfer(a, 4, { from: MkServiceId(0), amount: 500, memo: 0 })
    val c = applyIncomingTransfer(b, 9, { from: MkServiceId(0), amount: 500, memo: 0 })
    val partial = dropBucketsUpTo(c, 4)     // drops slots 1 and 4
    val everything = dropBucketsUpTo(c, 9)  // drains the chain
    all {
      assert(c.incomingTransferChain == Some({ firstSlot: 1, lastSlot: 9 })),
      assert(partial.incomingTransfers.keys() == Set(9)),
      assert(partial.incomingTransferChain == Some({ firstSlot: 9, lastSlot: 9 })),
      assert(everything.incomingTransfers.keys() == Set()),
      assert(everything.incomingTransferChain == None),
      init
    }
  }

  // --- Multi-package accumulate ------------------------------------------

  /// Two work packages in one block — one for each privileged chain.
  /// Both heads advance from 0 to 1, and `lastStepWorkResults` records
  /// two results in order.
  run multipleWorkPackagesAccumulateTest = {
    val coretimeCode: ValidationCodeHash = { vchBytes: 1 }
    val assetHubCode: ValidationCodeHash = { vchBytes: 2 }
    init
      .then(applyHappyPaths(List(
        (CoretimeParaId, coretimeCode, List()),
        (AssetHubParaId, assetHubCode, List())
      )))
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
      result: refine(
        svc, pkg, false, PvfOk,
        Some(headHash(StaleParentHead)), Some(StaleParentHead), msgs, 0
      )[0],
      authTrace: 0
    }
    val afterAlwaysAccumulate = applyAlwaysAccumulate(svc, blockNow)
    val out = accumulateOnePackage(
      afterAlwaysAccumulate.state, wr, solicitedSet, None, FixedCodeLen, blockNow
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
      lastStepAssigns' = afterAlwaysAccumulate.assigns.concat(out.jamAssigns),
      lastHeadRoot' = headCommitment(svc, out.state),
      logPrunedBelow' = logPrunedBelow
    }
  }

  // --- Parachain head commitment (§5.5) ----------------------------------

  /// Leaf and node hashes occupy disjoint images, so no leaf hash can ever
  /// equal a node hash — what §5.5 gets from covering the SCALE discriminant.
  run headCommitmentLeafNodeDisjointTest = {
    val leaves = Set(MkParaId(1), MkParaId(2), MkParaId(7)).map(p =>
      merkleHash(Leaf({ paraId: p, headHash: headHash(paraIdInt(p) * 3) })))
    val nodes = tuples(leaves, leaves).map(pair =>
      merkleHash(Node({ left: pair._1, right: pair._2 })))
    init.then(expectThat(leaves.intersect(nodes) == Set()))
  }

  /// Tree shape (§5.5): no leaves yields no hash at all, and a single leaf is
  /// its own root rather than being hashed again.
  run headCommitmentRootShapeTest = {
    val leaf = merkleHash(Leaf({
      paraId: AssetHubParaId, headHash: headHash(9)
    }))
    init.then(expectThat(all {
      merkleRoot(List()) == None,
      merkleRoot(List(leaf)) == Some(leaf),
      // Two leaves combine into one node, so the root is neither leaf.
      merkleRoot(List(leaf, leaf)) == Some(merkleHash(Node({
        left: leaf, right: leaf
      }))),
    }))
  }

  /// Leaves are ordered by ascending `para_id` regardless of how the changed
  /// set was assembled, so every verifier builds the same tree (§5.5).
  run headCommitmentOrdersByParaIdTest = {
    init.then(expectThat(
      sortedParaIds(Set(MkParaId(7), MkParaId(1), MkParaId(4)))
        == List(MkParaId(1), MkParaId(4), MkParaId(7))))
  }

  /// An odd leaf count promotes the trailing element to the next level
  /// unchanged rather than duplicating it — see the README note, since §5.5
  /// leaves this open.
  run headCommitmentOddLevelPromotesTest = {
    val a = headHash(1)
    val b = headHash(2)
    val c = headHash(3)
    init.then(expectThat(all {
      pairUp(List(a, b, c)) == List(merkleHash(Node({ left: a, right: b })), c),
      merkleRoot(List(a, b, c)) == Some(merkleHash(Node({
        left: merkleHash(Node({ left: a, right: b })), right: c
      }))),
    }))
  }

  /// End to end (§5.5): a block that advances one para's head commits to
  /// exactly that leaf, and a block that touches no head returns no hash. The
  /// head carried is the value the para ended the block with.
  run headCommitmentTracksChangedHeadsTest = {
    val assetHubCode: ValidationCodeHash = { vchBytes: 2 }
    init
      // A block advancing only Asset Hub's head commits to its single leaf.
      .then(applyHappyPath(AssetHubParaId, assetHubCode, List()))
      .then(expectThat(
        lastHeadRoot == Some(merkleHash(Leaf({
          paraId: AssetHubParaId,
          headHash: headHash(svc.paraInfo(AssetHubParaId).headData)
        })))))
      // A step that moves no head contributes no entry for that block.
      .then(provisionPreimage)
      .then(expectThat(lastHeadRoot == None))
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
      // Two log entries, one ForgetAgainAt per code, with DISTINCT deadlines —
      // the pick-one bug would have kept only one of these.
      assert(c.logs.length() == 2),
      assert(c.logs.select(e =>
        e == ForgetAgainAt({ hash: vchAsHash(hA), len: lenA, due: 5 + ExpungePeriod })).length() == 1),
      assert(c.logs.select(e =>
        e == ForgetAgainAt({ hash: vchAsHash(hB), len: lenB, due: nowT + ExpungePeriod })).length() == 1),
      init
    }
  }

  // --- Insufficient state balance ----------------------------------------

  /// A parachain at its state-balance cap has its next `Solicit` rejected:
  /// - Coretime registers para 6 with `totalStateBalance = BaselineFootprint
  ///   + preimageFootprint(FixedCodeLen)` — headroom for exactly the
  ///   validation-code preimage and nothing else.
  /// - Coretime sets para 6's active code, consuming that headroom.
  /// - Para 6 emits a `Solicit` upward message for a distinct preimage —
  ///   `addReferencer` finds no headroom, appends
  ///   `InsufficientStateBalance(FromSolicit(...))` to the parachain log, and
  ///   leaves the registry and `usedStateBalance` unchanged.
  run solicitRejectedAtBalanceCapTest = {
    val para = MkParaId(6)
    val paraCode: ValidationCodeHash = { vchBytes: 0x66 }
    val soliciteeHash: Hash = { hashBytes: 0xDD }
    val soliciteeLen = 500
    val cap = BaselineFootprint + preimageFootprint(FixedCodeLen)
    init
      .then(applyHappyPath(CoretimeParaId, { vchBytes: 1 }, List(
        ParachainSetStateBalance({ paraId: para, newTotal: cap }),
        ParachainSetValidationCode({
          paraId: para,
          newValidationCodeHash: paraCode,
          newValidationCodeLen: FixedCodeLen
        })
      )))
      // After registration + set validation code: usedStateBalance is exactly at the cap.
      .then(expectThat(all {
        svc.paraInfo(para).usedStateBalance == cap,
        svc.paraInfo(para).totalStateBalance == cap
      }))
      // Para tries to solicit a new preimage — must be rejected.
      .then(applyHappyPath(para, paraCode, List(
        Solicit({ hash: soliciteeHash, len: soliciteeLen })
      )))
      .then(expectThat(all {
        // No referencer added for the rejected preimage.
        not(svc.preimageRegistry.keys().contains((soliciteeHash, soliciteeLen))),
        // usedStateBalance stayed at the cap.
        svc.paraInfo(para).usedStateBalance == cap,
        // Accumulate log carries InsufficientStateBalance(FromSolicit(...)) for this block.
        svc.paraLog(para).foldl(false, (found, pair) =>
          found or (pair._1 == now and match pair._2 {
            | AccumulateLogEntry(evs) => evs.foldl(false, (a, e) => match e {
              | InsufficientStateBalance(reason) => match reason {
                | FromSolicit(p) => a or (p.hash == soliciteeHash and p.len == soliciteeLen)
                | FromSetKV(_) => a
              }
              | _ => a
            })
            | RefineLogEntry(_) => false
          }))
      }))
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
      // depends on self-supervision the model abstracts away — no failure here.
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

  /// Clean-up of a para holding an available validation code is a two-step
  /// forget: the first `parachain_clean_up` unrequests the code (does not
  /// expunge), so it sets `isDeregistering` and keeps the entry; a retry once
  /// expungeable (`now > due`) fully removes the para (§6.4).
  run cleanUpTwoStepForgetTest = {
    val para = MkParaId(5)
    val code: ValidationCodeHash = { vchBytes: 0x55 }
    val bal = BaselineFootprint + preimageFootprint(FixedCodeLen)
    val s0 = parachainSetStateBalance(initialState, para, bal).state
    val s1raw = parachainSetValidationCode(s0, para, code, FixedCodeLen, 0).state
    // The code is available, so cleanup's forget takes the two-step path.
    val s1 = markProvided(s1raw, vchAsHash(code), FixedCodeLen)
    // First clean-up at now = 0: code is the sole referencer, first forget only
    // unrequests it, so the para is left deregistering.
    val c1 = parachainCleanUp(s1, para, 0)
    // Retry once strictly past the deadline: second forget expunges + removes.
    val c2 = parachainCleanUp(c1.state, para, ExpungePeriod + 1)
    all {
      // First clean-up: para retained, marked deregistering, code unrequested,
      // first JAM forget forwarded, ForgetAgainAt logged.
      assert(c1.state.isLive(para)),
      assert(c1.state.paraInfo(para).isDeregistering),
      assert(isUnrequested(c1.state.preimageStatus.get((vchAsHash(code), FixedCodeLen)))),
      assert(c1.jamForget == Set(vchAsHash(code))),
      assert(c1.logs != List()),
      // Retry: para fully removed, code expunged.
      assert(not(c2.state.isLive(para))),
      assert(not(c2.state.preimageStatus.keys().contains((vchAsHash(code), FixedCodeLen)))),
      assert(c2.jamForget == Set(vchAsHash(code))),
      init
    }
  }

  /// Clean-up is rejected with `TooMuchStateHeld` while the para still holds
  /// state beyond its baseline and validation code — here an extra solicited
  /// preimage (§6.4).
  run cleanUpTooMuchStateHeldTest = {
    val para = MkParaId(6)
    val code: ValidationCodeHash = { vchBytes: 0x66 }
    val extra: Hash = { hashBytes: 0x99 }
    val bal = BaselineFootprint + preimageFootprint(FixedCodeLen) + preimageFootprint(512)
    val s0 = parachainSetStateBalance(initialState, para, bal).state
    val s1 = parachainSetValidationCode(s0, para, code, FixedCodeLen, 0).state
    // Solicit an extra preimage — now the para holds more than baseline + code.
    val s2 = s1.addReferencer(para, extra, 512).state
    val c = parachainCleanUp(s2, para, 0)
    all {
      // Rejected: state unchanged, TooMuchStateHeld logged, para still live.
      assert(c.state == s2),
      assert(c.logs == List(TooMuchStateHeld)),
      assert(c.state.isLive(para)),
      assert(not(c.state.paraInfo(para).isDeregistering)),
      init
    }
  }

  /// A never-provided preimage (`Unprovided`) is dropped by a SINGLE `forget`:
  /// no limbo, no retained referencer, refund now (JAM `ForgetImplication::Drop`).
  run forgetUnprovidedDropsInOneStepTest = {
    val para = CoretimeParaId
    val h: Hash = { hashBytes: 0x7A }
    // Solicit only — the preimage is never provided, so status is `Unprovided`.
    val d1 = initialState.addReferencer(para, h, 512)
    val d2 = d1.state.removeReferencer(para, h, 512, 0)
    all {
      assert(d1.state.preimageStatus.get((h, 512)) == Unprovided),
      // Single forget drops it: entry gone, status gone, JAM forget forwarded,
      // NOT retained (footprint refunded), and no ForgetAgainAt.
      assert(not(d2.state.preimageRegistry.keys().contains((h, 512)))),
      assert(not(d2.state.preimageStatus.keys().contains((h, 512)))),
      assert(d2.jamForget == Some(h)),
      assert(not(d2.retained)),
      assert(d2.log == None),
      init
    }
  }

  /// The expunge boundary is strict (JAM `now > unrequested_since +
  /// min_turnaround`): a second forget at `now == due` is a no-op retry, at
  /// `now == due + 1` it expunges.
  run forgetExpungeBoundaryTest = {
    val para = CoretimeParaId
    val h: Hash = { hashBytes: 0x7B }
    // Solicit, provide, then first forget at now = 0 -> Unrequested(0),
    // due = 0 + ExpungePeriod.
    val d1 = initialState.addReferencer(para, h, 512)
    val provided = markProvided(d1.state, h, 512)
    val d2 = provided.removeReferencer(para, h, 512, 0)
    // At now == due: not yet expungeable (no-op retry, entry survives).
    val atDue = d2.state.removeReferencer(para, h, 512, ExpungePeriod)
    // At now == due + 1: expunges.
    val pastDue = d2.state.removeReferencer(para, h, 512, ExpungePeriod + 1)
    all {
      assert(d2.state.preimageStatus.get((h, 512)) == Unrequested(0)),
      // now == due -> no-op, still present and retained, ForgetAgainAt re-emitted.
      assert(atDue.state.preimageRegistry.keys().contains((h, 512))),
      assert(atDue.retained),
      assert(atDue.jamForget == None),
      assert(atDue.log == Some(ForgetAgainAt({ hash: h, len: 512, due: ExpungePeriod }))),
      // now == due + 1 -> expunged.
      assert(not(pastDue.state.preimageRegistry.keys().contains((h, 512)))),
      assert(not(pastDue.state.preimageStatus.keys().contains((h, 512)))),
      assert(pastDue.jamForget == Some(h)),
      assert(not(pastDue.retained)),
      init
    }
  }

  /// Rescue: a `solicit` on an `Unrequested` preimage rescues the still-held
  /// blob back to available (JAM `Unrequested → Rerequested`), keeping the
  /// ORIGINAL unrequest slot for the next forget's gate. A DIFFERENT para (p2)
  /// soliciting p1's limbo entry triggers it — and frees the retained ghost p1:
  /// p1 had already forgotten, so its retention was only the two-step stand-in.
  /// Once p2 is the live referencer covering the single JAM entry, keeping p1
  /// charged would double-count it, so p1 is refunded and dropped (§6.1).
  run forgetRescueRerequestedTest = {
    val h: Hash = { hashBytes: 0x8A }
    val p1 = CoretimeParaId
    val p2 = AssetHubParaId
    // p1 solicits, the blob is provided, p1 forgets (last ref) at now = 5.
    val d1 = initialState.addReferencer(p1, h, 512)
    val provided = markProvided(d1.state, h, 512)
    val d2 = provided.removeReferencer(p1, h, 512, 5)
    // p2 solicits the same (h, 512) while it is Unrequested(5) — rescue.
    val d3 = d2.state.addReferencer(p2, h, 512)
    all {
      assert(d2.state.preimageStatus.get((h, 512)) == Unrequested(5)),
      // p1 still charged the footprint while retained as the ghost.
      assert(d2.state.paraInfo(p1).usedStateBalance == d1.state.paraInfo(p1).usedStateBalance),
      // Rescue fires a real JAM solicit and moves to Rerequested keeping y = 5.
      assert(d3.jamSolicit == Some(h)),
      assert(d3.state.preimageStatus.get((h, 512)) == Rerequested(5)),
      // Ghost p1 is freed+refunded; p2 is the sole live referencer.
      assert(d3.state.preimageRegistry.get((h, 512)).referencers == Set(p2)),
      assert(d3.state.paraInfo(p1).usedStateBalance
        == d2.state.paraInfo(p1).usedStateBalance - preimageFootprint(512)),
      init
    }
  }

  /// NotYetUnrequest (Oracle Q6): a rescued preimage forgotten again WITHIN the
  /// original turnaround is a no-op retry — the gate keys off the original
  /// unrequest slot, NOT `now`. Proves collapsing `Rerequested` into `Provided`
  /// would be unfaithful (it would wrongly succeed with a shifted deadline).
  run forgetRerequestedNotYetTest = {
    val h: Hash = { hashBytes: 0x8B }
    val p1 = CoretimeParaId
    val p2 = AssetHubParaId
    // p1 solicit + provide + forget at now = 10 -> Unrequested(10).
    val d1 = initialState.addReferencer(p1, h, 512)
    val provided = markProvided(d1.state, h, 512)
    val d2 = provided.removeReferencer(p1, h, 512, 10)
    // p2 rescues at now = 20 -> Rerequested(10), freeing ghost p1: p2 is now
    // the sole referencer.
    val d3 = d2.state.addReferencer(p2, h, 512)
    // A forget of p1 is now a no-op (p1 was freed on rescue); p2 (last live
    // referencer) forgets at now = 25, still within the ORIGINAL turnaround
    // (25 <= 10 + ExpungePeriod), so it hits the NotYetUnrequest gate.
    val d4 = d3.state.removeReferencer(p1, h, 512, 25)
    val d5 = d4.state.removeReferencer(p2, h, 512, 25)
    all {
      assert(d3.state.preimageStatus.get((h, 512)) == Rerequested(10)),
      assert(d3.state.preimageRegistry.get((h, 512)).referencers == Set(p2)),
      // Last forget is NotYetUnrequest: no-op, entry survives, retained, no JAM
      // forget, and the retry deadline is the ORIGINAL 10 + ExpungePeriod.
      assert(d5.state.preimageRegistry.keys().contains((h, 512))),
      assert(d5.retained),
      assert(d5.jamForget == None),
      assert(d5.log == Some(ForgetAgainAt({ hash: h, len: 512, due: 10 + ExpungePeriod }))),
      // Status is unchanged (still Rerequested(10)).
      assert(d5.state.preimageStatus.get((h, 512)) == Rerequested(10)),
      init
    }
  }

  /// Retained self-rescue: the SAME para that forgot its own code (leaving it
  /// `Unrequested` with itself retained) re-solicits it — the idempotent
  /// early-return must NOT short-circuit; JAM `solicit` still fires to rescue.
  run forgetRetainedSelfRescueTest = {
    val h: Hash = { hashBytes: 0x8C }
    val para = CoretimeParaId
    val d1 = initialState.addReferencer(para, h, 512)
    val provided = markProvided(d1.state, h, 512)
    val d2 = provided.removeReferencer(para, h, 512, 7)
    // Same para re-solicits its own code sitting in limbo.
    val d3 = d2.state.addReferencer(para, h, 512)
    all {
      assert(d2.state.preimageStatus.get((h, 512)) == Unrequested(7)),
      assert(d2.state.preimageRegistry.get((h, 512)).referencers == Set(para)),
      // Idempotent branch still fires JAM solicit and rescues to Rerequested(7).
      assert(d3.jamSolicit == Some(h)),
      assert(d3.state.preimageStatus.get((h, 512)) == Rerequested(7)),
      assert(d3.state.preimageRegistry.get((h, 512)).referencers == Set(para)),
      init
    }
  }

  /// Ghost accumulation regression: repeated forget → rescue cycles by
  /// alternating paras must NOT pile up ghosts. Each rescue frees the prior
  /// ghost, so the referencer set stays size 1 and the footprint is charged
  /// exactly once — never `N × footprint` across N cycles (§6.1).
  run forgetRescueNoGhostAccumulationTest = {
    val h: Hash = { hashBytes: 0x8D }
    val p1 = CoretimeParaId
    val p2 = AssetHubParaId
    // p1 solicit + provide + forget at now=5 -> Unrequested(5), p1 ghost.
    val d1 = initialState.addReferencer(p1, h, 512)
    val prov = markProvided(d1.state, h, 512)
    val d2 = prov.removeReferencer(p1, h, 512, 5)
    // p2 rescues (frees p1) -> Rerequested(5), referencers = {p2}.
    val d3 = d2.state.addReferencer(p2, h, 512)
    // p2 forgets past the ORIGINAL deadline (now > 5 + ExpungePeriod): the
    // Rerequested forget succeeds -> Unrequested(now), p2 retained (ghost).
    val t2 = 5 + ExpungePeriod + 1
    val d4 = d3.state.removeReferencer(p2, h, 512, t2)
    // p1 rescues again (frees p2) -> Rerequested, referencers = {p1}.
    val d5 = d4.state.addReferencer(p1, h, 512)
    all {
      // After every cycle the set is a singleton — no ghost pile-up.
      assert(d3.state.preimageRegistry.get((h, 512)).referencers == Set(p2)),
      assert(d4.state.preimageStatus.get((h, 512)) == Unrequested(t2)),
      assert(d4.state.preimageRegistry.get((h, 512)).referencers == Set(p2)),
      assert(d5.state.preimageRegistry.get((h, 512)).referencers == Set(p1)),
      // Only the current sole referencer is charged the footprint; the other
      // para is back at baseline (freed on the rescue that displaced it).
      assert(d5.state.paraInfo(p2).usedStateBalance == d2.state.paraInfo(p2).usedStateBalance),
      init
    }
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
      assert(reaped.jamForget == None),
      init
    }
  }

  /// Reaping AT/AFTER the deadline clears the pending upgrade and releases the
  /// (unpinned) pending code's referencer. The preimage was never provided
  /// (`Unprovided`), so the single JAM `forget` DROPS it in one step — no
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
      baseAcc, para, FixedCodeLen, 0,
      Solicit({ hash: vchAsHash(pendingUpgradeHash), len: FixedCodeLen })
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

  // --- Authorizer queue scheduling --------------------------------------

  /// A `set_authorizer_queue` caches the queue for its `jam_slot`;
  /// always-accumulate flushes it (emitting the `assign` and dropping the cached
  /// queue) once `now >= jam_slot`, and not before.
  run setAuthorizerQueueCachedUntilJamSlotTest = {
    val core = 5
    val anyHash: AuthorizerHash = { authBytes: 5 }
    val jamSlot = 100
    val baseAcc: AccumulateOutcome = {
      state: initialState,
      jamDesignate: None,
      jamSolicit: Set(),
      jamForget: Set(),
      jamAssigns: List(),
      logs: List()
    }
    val cached = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 0,
      AssignCore({ core: core, queue: List(anyHash), newAssigner: None, jamSlot: jamSlot })
    ).state
    val beforeDue = applyAlwaysAccumulate(cached, jamSlot - 1)
    val atDue = applyAlwaysAccumulate(cached, jamSlot)
    all {
      // Cached at jam_slot; the set itself emits no assign.
      assert(cached.pendingAssignCores.get(core) == jamSlot),
      assert(cached.pendingAssigns.get(core).queue == List(anyHash)),
      // Before jam_slot: retained, not flushed.
      assert(beforeDue.state.pendingAssigns.keys().contains(core)),
      assert(beforeDue.assigns == List()),
      // At jam_slot: flushed — assign emitted and the cached queue dropped.
      assert(not(atDue.state.pendingAssigns.keys().contains(core))),
      assert(not(atDue.state.pendingAssignCores.keys().contains(core))),
      assert(atDue.assigns ==
        List({ core: core, assigner: ParachainServiceId, queue: List(anyHash) })),
      init
    }
  }

  /// A second `set_authorizer_queue` for a core that already has a queue cached
  /// replaces the pending queue and its `jam_slot` (last write wins).
  run secondSetAuthorizerQueueReplacesPendingTest = {
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
    // First call caches hash1 for jam_slot 100.
    val after1 = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 0,
      AssignCore({ core: core, queue: List(hash1), newAssigner: None, jamSlot: 100 })
    )
    // Second call replaces the pending queue and its jam_slot.
    val after2 = applyUpwardMessage(
      after1, CoretimeParaId, FixedCodeLen, 1,
      AssignCore({ core: core, queue: List(hash2), newAssigner: None, jamSlot: 150 })
    )
    all {
      assert(after1.state.pendingAssigns.get(core).queue == List(hash1)),
      assert(after1.state.pendingAssignCores.get(core) == 100),
      assert(after2.state.pendingAssigns.get(core).queue == List(hash2)),
      assert(after2.state.pendingAssignCores.get(core) == 150),
      init
    }
  }

  // --- KV store accounting (§6.1) ----------------------------------------

  /// SetKV charges `kvEntryFootprint(kLen, vLen)` to `usedStateBalance`.
  run kvSetChargesTest = {
    val s = initialState
    val para = CoretimeParaId
    val key = List(1, 2, 3)
    val value = List(10, 20)
    val before = s.paraInfo(para).usedStateBalance
    val d = applySetKV(s, para, key, value)
    val expected = kvEntryFootprint(key.length(), value.length())
    all {
      assert(d.log == None),
      assert(d.state.paraInfo(para).usedStateBalance == before + expected),
      init
    }
  }

  /// RemoveKV refunds the full `kvEntryFootprint`, returning `usedStateBalance`
  /// to its pre-insert level.
  run kvRemoveRefundsTest = {
    val s = initialState
    val para = CoretimeParaId
    val key = List(1, 2, 3)
    val value = List(10, 20)
    val before = s.paraInfo(para).usedStateBalance
    val d1 = applySetKV(s, para, key, value)
    val d2 = applyRemoveKV(d1.state, para, key)
    all {
      assert(d1.log == None),
      assert(d2.log == None),
      assert(d2.state.paraInfo(para).usedStateBalance == before),
      init
    }
  }

  /// Overwriting an existing key charges only the value-size delta.
  run kvOverwriteDeltaTest = {
    val s = initialState
    val para = CoretimeParaId
    val key = List(5)
    val value1 = List(1, 2)
    val value2 = List(1, 2, 3, 4)
    val d1 = applySetKV(s, para, key, value1)
    val afterInsert = d1.state.paraInfo(para).usedStateBalance
    val d2 = applySetKV(d1.state, para, key, value2)
    // Growing overwrite: delta = (compactLen(4)+4) - (compactLen(2)+2) = 5 - 3 = 2
    val growDelta = (1 + value2.length()) - (1 + value1.length())
    all {
      assert(d1.log == None),
      assert(d2.log == None),
      assert(d2.state.paraInfo(para).usedStateBalance == afterInsert + growDelta),
      init
    }
  }

  /// A SetKV whose value would exceed the para's headroom is rejected with a log entry.
  run kvInsufficientBalanceTest = {
    val s = initialState
    val para = CoretimeParaId
    val key = List(7)
    // Build a value large enough to exceed the para's remaining headroom.
    // InitialParaBalance = BaselineFootprint + 2*preimageFootprint(FixedCodeLen) + BaselineFootprint
    // After registration, usedStateBalance = BaselineFootprint + 2*preimageFootprint(FixedCodeLen).
    // Remaining headroom ≈ BaselineFootprint. A value of length BaselineFootprint+1 bytes
    // will exceed it (kvEntryFootprint adds 39+ bytes of overhead on top).
    // We approximate with a list of length 200000 (well above any headroom).
    val v1 = List(1, 2, 3, 4, 5, 6, 7, 8, 9, 10)
    val v2 = v1.concat(v1).concat(v1).concat(v1).concat(v1)   // 50
    val v3 = v2.concat(v2).concat(v2).concat(v2).concat(v2)   // 250
    val v4 = v3.concat(v3).concat(v3).concat(v3).concat(v3)   // 1250
    val v5 = v4.concat(v4).concat(v4).concat(v4).concat(v4)   // 6250
    val v6 = v5.concat(v5).concat(v5).concat(v5).concat(v5)   // 31250
    val hugeValue = v6.concat(v6).concat(v6).concat(v6).concat(v6) // 156250
    val d = applySetKV(s, para, key, hugeValue)
    all {
      assert(d.state == s),
      assert(d.log != None),
      init
    }
  }

  /// `assign_core` with `Some(s)` hands the core off via JAM `assign`, which
  /// writes the queue and the assigner atomically. A due handoff applies inline,
  /// superseding (and dropping) any entry cached for that core. `None` instead
  /// keeps this service as the assigner.
  run setAssignerForwardsAssignerTest = {
    val core = 0
    val assigner = MkServiceId(7)
    val authHash: AuthorizerHash = { authBytes: 5 }
    // Seed a cached entry for the core, so the handoff must supersede it.
    val seeded = {
      ...initialState,
      pendingAssigns:
        initialState.pendingAssigns.put(core, { queue: List(authHash), assigner: None }),
      pendingAssignCores: initialState.pendingAssignCores.put(core, 100)
    }
    val baseAcc: AccumulateOutcome = {
      state: seeded,
      jamDesignate: None,
      jamSolicit: Set(),
      jamForget: Set(),
      jamAssigns: List(),
      logs: List()
    }
    // Due (`jamSlot <= now`), so it applies inline.
    val out = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 0,
      AssignCore({ core: core, queue: List(authHash),
                   newAssigner: Some(assigner), jamSlot: 0 })
    )
    // The same call with `None` keeps this service as the assigner.
    val kept = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 0,
      AssignCore({ core: core, queue: List(authHash),
                   newAssigner: None, jamSlot: 0 })
    )
    all {
      // One `assign` carrying BOTH the new assigner and the final queue.
      assert(out.jamAssigns ==
        List({ core: core, assigner: assigner, queue: List(authHash) })),
      // `None` resolves to this service — JAM always gets a concrete assigner.
      assert(kept.jamAssigns ==
        List({ core: core, assigner: ParachainServiceId, queue: List(authHash) })),
      // The cached entry for the core is dropped once applied.
      assert(not(out.state.pendingAssigns.keys().contains(core))),
      init
    }
  }

  /// An empty `queue` cancels any cached queue for the core: it is dropped (not
  /// applied), with no `assign`.
  run emptyQueueCancelsCachedQueueTest = {
    val core = 5
    val staleHash: AuthorizerHash = { authBytes: 5 }
    // Core with a queue already cached for a future jam_slot.
    val cachedState = { ...initialState,
      pendingAssigns: Map(core -> { queue: List(staleHash), assigner: None }),
      pendingAssignCores: Map(core -> 100)
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
      AssignCore({ core: core, queue: List(), newAssigner: None, jamSlot: 0 })
    )
    all {
      // cached queue dropped, not applied.
      assert(not(out.state.pendingAssigns.keys().contains(core))),
      assert(not(out.state.pendingAssignCores.keys().contains(core))),
      // no assign.
      assert(out.jamAssigns == List()),
      init
    }
  }

  /// A `set_authorizer_queue` whose `jam_slot` is already due (`jam_slot <= now`)
  /// is applied inline immediately: it emits the `assign` this block and caches
  /// nothing, superseding any queue currently cached for the core. This case
  /// exercises the strictly-past sub-case (`jam_slot < now`).
  run setAuthorizerQueuePastJamSlotAppliedInlineTest = {
    val core = 5
    val staleHash: AuthorizerHash = { authBytes: 1 }
    val newHash: AuthorizerHash = { authBytes: 2 }
    // Core already has a queue cached for a future jam_slot.
    val cachedState = { ...initialState,
      pendingAssigns: Map(core -> { queue: List(staleHash), assigner: None }),
      pendingAssignCores: Map(core -> 100)
    }
    val baseAcc: AccumulateOutcome = {
      state: cachedState,
      jamDesignate: None,
      jamSolicit: Set(),
      jamForget: Set(),
      jamAssigns: List(),
      logs: List()
    }
    // At now = 50, a queue with jam_slot = 10 (< now) applies inline.
    val out = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 50,
      AssignCore({ core: core, queue: List(newHash), newAssigner: None, jamSlot: 10 })
    )
    all {
      // Applied inline: assign emitted now.
      assert(out.jamAssigns == List({ core: core, assigner: ParachainServiceId, queue: List(newHash) })),
      // Nothing cached: the prior cached queue is superseded and dropped.
      assert(not(out.state.pendingAssigns.keys().contains(core))),
      assert(not(out.state.pendingAssignCores.keys().contains(core))),
      init
    }
  }

  /// Boundary of the inline-apply rule: a `set_authorizer_queue` whose `jam_slot`
  /// equals `now` is due (`jam_slot <= now`, the same condition always-accumulate
  /// flushes on), so it is applied inline this block rather than cached for the
  /// next one. This is the case that keeps `pending_authorizer_apply_at_future`
  /// (`apply_at > now`) true: a due queue is never left in the cache.
  run setAuthorizerQueueJamSlotEqualsNowAppliedInlineTest = {
    val core = 5
    val newHash: AuthorizerHash = { authBytes: 2 }
    val baseAcc: AccumulateOutcome = {
      state: initialState,
      jamDesignate: None,
      jamSolicit: Set(),
      jamForget: Set(),
      jamAssigns: List(),
      logs: List()
    }
    // At now = 10, a queue with jam_slot = 10 (== now) is due and applies inline.
    val out = applyUpwardMessage(
      baseAcc, CoretimeParaId, FixedCodeLen, 10,
      AssignCore({ core: core, queue: List(newHash), newAssigner: None, jamSlot: 10 })
    )
    all {
      // Applied inline: assign emitted now.
      assert(out.jamAssigns == List({ core: core, assigner: ParachainServiceId, queue: List(newHash) })),
      // Nothing cached.
      assert(not(out.state.pendingAssigns.keys().contains(core))),
      assert(not(out.state.pendingAssignCores.keys().contains(core))),
      init
    }
  }

  /// An otherwise-valid candidate whose PVF skipped `set_parent_head_hash`
  /// and/or `set_head` is rejected by Refine with `MissingHeadDeclaration`.
  /// Coretime's active code (`{ vchBytes: 1 }`) is solicited in
  /// `initialState`, so the candidate passes `codeAvailable` and reaches the
  /// head-declaration check rather than failing the whole invocation earlier.
  run refineMissingHeadDeclarationTest = {
    val pkg: WorkPackage = {
      authorizedParas: List(CoretimeParaId),
      items: List({ validationCode: { hash: { vchBytes: 1 }, len: FixedCodeLen }, pov: 0 })
    }
    val parentOnly =
      refine(initialState, pkg, false, PvfOk, Some(headHash(0)), None, List(), 0)[0]
    val headOnly =
      refine(initialState, pkg, false, PvfOk, None, Some(1), List(), 0)[0]
    val neither =
      refine(initialState, pkg, false, PvfOk, None, None, List(), 0)[0]
    val both =
      refine(initialState, pkg, false, PvfOk, Some(headHash(0)), Some(1), List(), 0)[0]
    all {
      // Missing new head, missing parent head, or both → rejected.
      assert(parentOnly == WorkOk(Err({ paraId: CoretimeParaId, error: MissingHeadDeclaration }))),
      assert(headOnly == WorkOk(Err({ paraId: CoretimeParaId, error: MissingHeadDeclaration }))),
      assert(neither == WorkOk(Err({ paraId: CoretimeParaId, error: MissingHeadDeclaration }))),
      // Both declared → not a MissingHeadDeclaration error (succeeds here).
      assert(both == WorkOk(Ok({
        paraId: CoretimeParaId,
        validationCode: { hash: { vchBytes: 1 }, len: FixedCodeLen },
        parentHeadHash: headHash(0),
        headData: 1,
        upwardMessages: List(),
        lookupAnchor: 0
      }))),
      init
    }
  }

  /// A Refine invocation emitting more than `MaxUpwardMessages` (1024) upward
  /// messages is rejected with `TooManyUpwardMessages`. Exactly 1024 is accepted.
  /// Uses `Solicit` (unrestricted, 37 B each) so 1025 stay under the 48 KiB
  /// result-blob budget — the count cap, not the byte budget, must be the reason.
  run tooManyUpwardMessagesTest = {
    val pkg: WorkPackage = {
      authorizedParas: List(CoretimeParaId),
      items: List({ validationCode: { hash: { vchBytes: 1 }, len: FixedCodeLen }, pov: 0 })
    }
    val msg = Solicit({ hash: { hashBytes: 7 }, len: 1024 })
    val over = range(0, MaxUpwardMessages + 1).foldl(List(), (acc, _) => acc.append(msg))
    val atCap = range(0, MaxUpwardMessages).foldl(List(), (acc, _) => acc.append(msg))
    val overOut = refine(initialState, pkg, false, PvfOk, Some(headHash(0)), Some(1), over, 0)[0]
    val atCapOut = refine(initialState, pkg, false, PvfOk, Some(headHash(0)), Some(1), atCap, 0)[0]
    all {
      assert(over.length() == 1025),
      assert(overOut == WorkOk(Err({ paraId: CoretimeParaId, error: TooManyUpwardMessages }))),
      // Exactly at the cap is not a TooManyUpwardMessages error.
      assert(atCapOut != WorkOk(Err({ paraId: CoretimeParaId, error: TooManyUpwardMessages }))),
      init
    }
  }
  /// Regression: `solicit`/`forget` match a validation code on the FULL
  /// `(hash, len)` pair, not the hash alone. The registry is keyed by
  /// `(hash, len)`, so the active code's hash at a different length is a
  /// distinct preimage. Exercises both directions of the `pinned` bit so a
  /// hash-only match cannot pass: the correct len pins/unpins the active code,
  /// the wrong len must leave that bit alone.
  run solicitForgetMatchesHashAndLenTest = {
    val para = CoretimeParaId
    val h = vchAsHash({ vchBytes: 1 })   // coretime's ACTIVE code hash
    val otherLen = 999                    // != FixedCodeLen
    val baseAcc: AccumulateOutcome = {
      state: initialState, jamDesignate: None, jamSolicit: Set(),
      jamForget: Set(), jamAssigns: List(), logs: List()
    }
    val pi0 = initialState.paraInfo(para)
    // 1. Solicit at the code's OWN len: matches the active code, so it only
    //    sets `pinned` — no new entry, no charge.
    val pinned = applyUpwardMessage(baseAcc, para, FixedCodeLen, 0,
      Solicit({ hash: h, len: FixedCodeLen }))
    val piP = pinned.state.paraInfo(para)
    // 2. Solicit the SAME hash at a different len: a genuinely distinct entry,
    //    charged, and it must NOT disturb the active code's `pinned` bit.
    val sol = applyUpwardMessage({ ...baseAcc, state: pinned.state }, para, FixedCodeLen, 0,
      Solicit({ hash: h, len: otherLen }))
    val pi1 = sol.state.paraInfo(para)
    // 3. Forget that distinct entry: released (never provided -> one-step drop),
    //    and the active code stays pinned.
    val fgt = applyUpwardMessage({ ...baseAcc, state: sol.state }, para, FixedCodeLen, 0,
      Forget({ paraId: para, hash: h, len: otherLen }))
    val pi2 = fgt.state.paraInfo(para)
    // 4. Forget at the code's OWN len: matches the active code, so it unpins it.
    val unpinned = applyUpwardMessage({ ...baseAcc, state: fgt.state }, para, FixedCodeLen, 0,
      Forget({ paraId: para, hash: h, len: FixedCodeLen }))
    all {
      assert(not(activeCodePinned(pi0))),
      // (1) correct len pins the active code, creating no entry and no charge.
      assert(activeCodePinned(piP)),
      assert(piP.usedStateBalance == pi0.usedStateBalance),
      // (2) wrong len is a distinct, charged entry; the pin is untouched.
      assert(sol.state.preimageRegistry.keys().contains((h, otherLen))),
      assert(pi1.usedStateBalance == pi0.usedStateBalance + preimageFootprint(otherLen)),
      assert(activeCodePinned(pi1)),
      assert(sol.state.preimageRegistry.keys().contains((h, FixedCodeLen))),
      // (3) forgetting the wrong-len entry releases it and refunds; still pinned.
      assert(not(fgt.state.preimageRegistry.keys().contains((h, otherLen)))),
      assert(pi2.usedStateBalance == pi0.usedStateBalance),
      assert(activeCodePinned(pi2)),
      // (4) correct len unpins the active code.
      assert(not(activeCodePinned(unpinned.state.paraInfo(para)))),
      init
    }
  }

  // --- Coretime-delegated release of another para's state (§6.4) -----------

  /// A fresh `AccumulateOutcome` over `s`, for replaying one upward message.
  pure def accOver(s: ParachainServiceState): AccumulateOutcome = {
    state: s, jamDesignate: None, jamSolicit: Set(),
    jamForget: Set(), jamAssigns: List(), logs: List()
  }

  /// Replay one upward message emitted by `caller` against `s`.
  pure def replay(
    s: ParachainServiceState, caller: ParaId, now: Timeslot, m: UpwardMessage
  ): AccumulateOutcome =
    applyUpwardMessage(accOver(s), caller, FixedCodeLen, now, m)

  /// Coretime naming another para in `forget` releases that para's preimage and
  /// refunds its footprint. Never provided → expunged in one step.
  run coretimeForgetDrainsPreimageTest = {
    val target = AssetHubParaId
    val h: Hash = { hashBytes: 4242 }
    val len = 1024
    val before = initialState.paraInfo(target).usedStateBalance
    val held = initialState.addReferencer(target, h, len).state
    val forced = replay(held, CoretimeParaId, 0, Forget({ paraId: target, hash: h, len: len }))
    all {
      assert(held.paraInfo(target).usedStateBalance == before + preimageFootprint(len)),
      assert(forced.state.paraInfo(target).usedStateBalance == before),
      assert(not(forced.state.preimageRegistry.keys().contains((h, len)))),
      assert(forced.jamForget == Set(h)),
      init
    }
  }

  /// A delegated forget of a PROVIDED preimage only unrequests it: the footprint
  /// stays charged and a `ForgetAgainAt` deadline is logged. A second call strictly
  /// past that deadline expunges it and refunds.
  run coretimeForgetTwoStepTest = {
    val target = AssetHubParaId
    val h: Hash = { hashBytes: 4243 }
    val len = 2048
    val before = initialState.paraInfo(target).usedStateBalance
    val held = markProvided(initialState.addReferencer(target, h, len).state, h, len)
    val msg = Forget({ paraId: target, hash: h, len: len })
    val first = replay(held, CoretimeParaId, 5, msg)
    val due = 5 + ExpungePeriod
    val second = replay(first.state, CoretimeParaId, due + 1, msg)
    all {
      assert(first.state.paraInfo(target).usedStateBalance
        == before + preimageFootprint(len)),
      assert(isUnrequested(first.state.preimageStatus.get((h, len)))),
      assert(first.logs == List(ForgetAgainAt({ hash: h, len: len, due: due }))),
      assert(second.state.paraInfo(target).usedStateBalance == before),
      assert(not(second.state.preimageRegistry.keys().contains((h, len)))),
      init
    }
  }

  /// A delegated forget naming the target's ACTIVE validation code only clears its
  /// `pinned` bit — Coretime can never strip a running PVF, nor refund the code.
  run coretimeForgetActiveCodeOnlyUnpinsTest = {
    val target = AssetHubParaId
    val activeRef: ValidationCodeRef = { hash: { vchBytes: 2 }, len: FixedCodeLen }
    val h = vchAsHash(activeRef.hash)
    val pinnedState = {
      ...initialState,
      parachains: initialState.parachains.put(
        target,
        { ...initialState.paraInfo(target),
          validationCode: Some({ ref: activeRef, pinned: true }) })
    }
    val before = pinnedState.paraInfo(target).usedStateBalance
    val forced = replay(pinnedState, CoretimeParaId, 0,
      Forget({ paraId: target, hash: h, len: FixedCodeLen }))
    val pi = forced.state.paraInfo(target)
    all {
      assert(activeCodePinned(pinnedState.paraInfo(target))),
      assert(not(activeCodePinned(pi))),
      assert(pi.validationCode == Some({ ref: activeRef, pinned: false })),
      assert(pi.usedStateBalance == before),
      assert(forced.state.preimageRegistry.keys().contains((h, FixedCodeLen))),
      assert(forced.jamForget == Set()),
      init
    }
  }

  /// Coretime naming another para in `kv_remove` drops that para's entry and
  /// refunds its `kvEntryFootprint`.
  run coretimeKVRemoveRefundsTest = {
    val target = AssetHubParaId
    val key = List(9, 9)
    val value = List(1, 2, 3)
    val before = initialState.paraInfo(target).usedStateBalance
    val held = applySetKV(initialState, target, key, value).state
    val forced = replay(held, CoretimeParaId, 0, RemoveKV({ paraId: target, key: key }))
    all {
      assert(held.paraInfo(target).usedStateBalance
        == before + kvEntryFootprint(key.length(), value.length())),
      assert(forced.state.paraInfo(target).usedStateBalance == before),
      assert(not(forced.state.keyValueStorage.keys().contains((target, key)))),
      init
    }
  }

  /// Refine one otherwise well-formed candidate for `caller`, carrying `msgs`.
  pure def refineWithMsgs(
    s: ParachainServiceState, caller: ParaId, msgs: List[UpwardMessage]
  ): WorkExecResult = {
    val pi = s.paraInfo(caller)
    val pkg: WorkPackage = {
      authorizedParas: List(caller),
      items: List({
        validationCode: { hash: activeCodeHashOr(pi, NoCodeHash), len: FixedCodeLen },
        pov: 0
      })
    }
    refine(s, pkg, false, PvfOk,
      Some(headHash(pi.headData)), Some(pi.headData + 1), msgs, 0)[0]
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

  /// §5.4: a service upgrade requires the new code to be AVAILABLE for lookup,
  /// not merely solicited. Soliciting reserves the slot but nobody may ever have
  /// supplied the blob, and JAM's `upgrade` does not check, so accepting an
  /// `Unprovided` hash would leave the service with no code to run.
  run serviceUpgradeRequiresAvailableCodeTest = {
    val ah = AssetHubParaId
    val newCode: Hash = { hashBytes: 31337 }
    val len = FixedCodeLen
    val msg = UpgradeService({ codeHash: newCode, len: len, minAccGas: 0, minMemoGas: 0 })
    // Solicited only: the request exists but the blob was never supplied.
    val solicited = initialState.addReferencer(ah, newCode, len).state
    val onlySolicited = replay(solicited, ah, 0, msg)
    // Once provided, the same call goes through.
    val provided = markProvided(solicited, newCode, len)
    val whenProvided = replay(provided, ah, 0, msg)
    all {
      assert(solicited.hasReferencer(newCode, len, ah)),
      assert(not(solicited.preimageAvailable(newCode, len))),
      // Rejected while unprovided: code hash untouched, rejection logged.
      assert(onlySolicited.state.serviceCodeHash == initialState.serviceCodeHash),
      assert(onlySolicited.logs == List(ServiceUpgradePreimageMissing({ codeHash: newCode }))),
      // Accepted once available.
      assert(provided.preimageAvailable(newCode, len)),
      assert(whenProvided.state.serviceCodeHash == newCode),
      assert(whenProvided.logs == List()),
      init
    }
  }

  /// True iff a work result carries a successful digest.
  pure def isOkDigest(r: WorkExecResult): bool =
    match r { | WorkOk(d) => match d { | Ok(_) => true | Err(_) => false } | WorkErr => false }

  /// §4.2: a PVF has exactly two ways to fail, and they differ in what is
  /// recorded. `report_error(data)` aborts it and is logged as
  /// `RefineLog::Opaque(data)`; any other abnormal exit is not caught, so the
  /// whole `refine` fails and yields a work error with NO digest — which
  /// Accumulate then skips entirely, leaving the para's log untouched.
  run pvfFailureModesTest = {
    val para = CoretimeParaId
    val pkg: WorkPackage = {
      authorizedParas: List(para),
      items: List({ validationCode: { hash: { vchBytes: 1 }, len: FixedCodeLen }, pov: 0 })
    }
    val reported = refine(initialState, pkg, false, PvfReportError(42),
      Some(headHash(0)), Some(1), List(), 0)[0]
    val panicked = refine(initialState, pkg, false, PvfPanic,
      Some(headHash(0)), Some(1), List(), 0)[0]
    // Accumulating the panicking result must touch nothing at all.
    val accPanic = accumulateOnePackage(
      initialState, { result: panicked, authTrace: 7 }, Set(), None, FixedCodeLen, 5)
    all {
      // Reporting is the only way a PVF records a reason.
      assert(reported == WorkOk(Err({ paraId: para, error: Opaque(42) }))),
      // A bare panic produces no digest whatsoever.
      assert(panicked == WorkErr),
      // ...and therefore no log entry and no state change (§3.3).
      assert(accPanic.state == initialState),
      assert(accPanic.state.paraLog(para) == List()),
      assert(accPanic.logPruned == None),
      init
    }
  }

  /// §4.3: naming a `para_id` the caller may not touch makes REFINE fail with
  /// `RestrictedHostFunction`, so the message never reaches Accumulate at all.
  /// Naming itself, or being the Coretime chain, refines fine.
  run refineRejectsForeignParaIdTest = {
    val h: Hash = { hashBytes: 7777 }
    all {
      // Asset Hub reaching for Coretime's state — Refine rejects, for both calls.
      assert(refineWithMsgs(initialState, AssetHubParaId,
        List(Forget({ paraId: CoretimeParaId, hash: h, len: 1024 })))
        == WorkOk(refineErr(AssetHubParaId, RestrictedHostFunction))),
      assert(refineWithMsgs(initialState, AssetHubParaId,
        List(RemoveKV({ paraId: CoretimeParaId, key: List(0) })))
        == WorkOk(refineErr(AssetHubParaId, RestrictedHostFunction))),
      // Asset Hub releasing its own state — refines fine.
      assert(isOkDigest(refineWithMsgs(initialState, AssetHubParaId,
        List(Forget({ paraId: AssetHubParaId, hash: h, len: 1024 }))))),
      assert(isOkDigest(refineWithMsgs(initialState, AssetHubParaId,
        List(RemoveKV({ paraId: AssetHubParaId, key: List(0) }))))),
      // Coretime may name anyone.
      assert(isOkDigest(refineWithMsgs(initialState, CoretimeParaId,
        List(Forget({ paraId: AssetHubParaId, hash: h, len: 1024 }))))),
      init
    }
  }

  /// Both calls are idempotent no-ops for an unregistered target and for an entry
  /// the target does not hold.
  run delegatedReleaseNoOpTest = {
    val ghost = MkParaId(9)
    val h: Hash = { hashBytes: 5555 }
    all {
      assert(replay(initialState, CoretimeParaId, 0,
        Forget({ paraId: ghost, hash: h, len: 1024 })).state == initialState),
      assert(replay(initialState, CoretimeParaId, 0,
        RemoveKV({ paraId: ghost, key: List(0) })).state == initialState),
      assert(replay(initialState, CoretimeParaId, 0,
        Forget({ paraId: AssetHubParaId, hash: h, len: 1024 })).state == initialState),
      init
    }
  }

  /// The reason the `para_id` parameter exists (§6.4): a para holding extra state
  /// cannot be cleaned up, and a para that can no longer produce blocks cannot
  /// drain itself. Coretime drains it, after which `parachain_clean_up` succeeds.
  run delegatedReleaseUnblocksCleanUpTest = {
    val target = AssetHubParaId
    val h: Hash = { hashBytes: 6161 }
    val len = 1024
    val key = List(3)
    val withPreimage = initialState.addReferencer(target, h, len).state
    val held = applySetKV(withPreimage, target, key, List(7)).state
    val refused = parachainCleanUp(held, target, 0)
    val d1 = replay(held, CoretimeParaId, 0, Forget({ paraId: target, hash: h, len: len }))
    val d2 = replay(d1.state, CoretimeParaId, 0, RemoveKV({ paraId: target, key: key }))
    val cleaned = parachainCleanUp(d2.state, target, 0)
    all {
      // Blocked while the extra preimage + KV entry are held.
      assert(refused.logs == List(TooMuchStateHeld)),
      assert(refused.state == held),
      // Drained back to exactly baseline + active validation code.
      assert(d2.state.paraInfo(target).usedStateBalance
        == baselineFor(target) + preimageFootprint(FixedCodeLen)),
      // Clean-up now completes: the code was never provided, so it expunges in
      // one step and the para is dropped outright.
      assert(cleaned.logs == List()),
      assert(not(cleaned.state.isLive(target))),
      init
    }
  }

  /// The `pinned` bit of a para's active validation code, or `false` if it has none.
  pure def activeCodePinned(pi: ParaInfo): bool =
    match pi.validationCode { | Some(vc) => vc.pinned | None => false }

  /// The chain invariant itself must reject a bucket no predecessor points at.
  /// Buckets 1 and 2 both link to 3, so walking from `firstSlot` goes 1 -> 3 and
  /// never visits 2, leaving it charged for but unreachable by
  /// `consume_transfers_up_to`. Every per-bucket conjunct still holds (endpoints
  /// present, keys in range, only the tail terminating, successors later and
  /// occupied), so only the successor-coverage conjunct catches it.
  pure val oneTransfer: List[{ from: ServiceId, amount: Balance, memo: Memo }] =
    List({ from: MkServiceId(0), amount: 1, memo: 0 })

  pure val skippingChainState: ParachainServiceState = {
    ...initialState,
    incomingTransfers: Map(
      1 -> { transfers: oneTransfer, nextSlot: Some(3) },
      2 -> { transfers: oneTransfer, nextSlot: Some(3) },
      3 -> { transfers: oneTransfer, nextSlot: None }),
    incomingTransferChain: Some({ firstSlot: 1, lastSlot: 3 })
  }

  pure val linkedChainState: ParachainServiceState = {
    ...initialState,
    incomingTransfers: Map(
      1 -> { transfers: oneTransfer, nextSlot: Some(2) },
      2 -> { transfers: oneTransfer, nextSlot: Some(3) },
      3 -> { transfers: oneTransfer, nextSlot: None }),
    incomingTransferChain: Some({ firstSlot: 1, lastSlot: 3 })
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
