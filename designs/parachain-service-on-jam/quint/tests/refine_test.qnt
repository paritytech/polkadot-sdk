// Refine-side enforcement: PVF outcomes and refine error paths (design 4).
module refine_test {
  import types.* from "../types"
  import messages.* from "../messages"
  import state.* from "../state"
  import state_balance.* from "../state_balance"
  import refine.* from "../refine"
  import accumulate.* from "../accumulate"
  import main.* from "../main"
  import support.* from "./support"

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
  /// result-blob budget: the count cap, not the byte budget, must be the reason.
  run tooManyUpwardMessagesTest = {
    val pkg: WorkPackage = {
      authorizedParas: List(CoretimeParaId),
      items: List({ validationCode: { hash: { vchBytes: 1 }, len: FixedCodeLen }, pov: 0 })
    }
    val msg = Solicit({ target: Parachain(CoretimeParaId), hash: { hashBytes: 7 }, len: 1024 })
    val over = range(0, MaxUpwardMessages + 1).foldl(List(), (acc, _) => acc.append(msg))
    val atCap = range(0, MaxUpwardMessages).foldl(List(), (acc, _) => acc.append(msg))
    val overOut = refine(initialState, pkg, false, PvfOk, Some(headHash(0)), Some(1), over, 0)[0]
    val atCapOut = refine(initialState, pkg, false, PvfOk, Some(headHash(0)), Some(1), atCap, 0)[0]
    all {
      assert(over.length() == 1025),
      assert(overOut == WorkOk(Err({ paraId: CoretimeParaId, error: TooManyUpwardMessages }))),
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
      jamForget: Set(), jamAssigns: List(), logs: List(), foreign: Map()
    }
    val pi0 = initialState.paraInfo(para)
    // 1. Solicit at the code's OWN len: matches the active code, so it only
    //    sets `pinned`: no new entry, no charge.
    val pinned = applyUpwardMessage(baseAcc, para, FixedCodeLen, 0, 0,
      Solicit({ target: Parachain(para), hash: h, len: FixedCodeLen }))
    val piP = pinned.state.paraInfo(para)
    // 2. Solicit the SAME hash at a different len: a genuinely distinct entry,
    //    charged, and it must NOT disturb the active code's `pinned` bit.
    val sol = applyUpwardMessage({ ...baseAcc, state: pinned.state }, para, FixedCodeLen, 0, 0,
      Solicit({ target: Parachain(para), hash: h, len: otherLen }))
    val pi1 = sol.state.paraInfo(para)
    // 3. Forget that distinct entry: released (never provided -> one-step drop),
    //    and the active code stays pinned.
    val fgt = applyUpwardMessage({ ...baseAcc, state: sol.state }, para, FixedCodeLen, 0, 0,
      Forget({ target: Parachain(para), hash: h, len: otherLen }))
    val pi2 = fgt.state.paraInfo(para)
    // 4. Forget at the code's OWN len: matches the active code, so it unpins it.
    val unpinned = applyUpwardMessage({ ...baseAcc, state: fgt.state }, para, FixedCodeLen, 0, 0,
      Forget({ target: Parachain(para), hash: h, len: FixedCodeLen }))
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

  /// §4.2: a PVF has exactly two ways to fail, and they differ in what is
  /// recorded. `report_error(data)` aborts it and is logged as
  /// `RefineLog::Opaque(data)`; any other abnormal exit is not caught, so the
  /// whole `refine` fails and yields a work error with NO digest, which
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
      initialState, { result: panicked, authTrace: 7 }, Set(), None, FixedCodeLen, 5, Map())
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

  /// §4.3: Refine checks the `SetValidatorKeys` chunk length explicitly and
  /// rejects a chunk over `SetValidatorKeysMaxKeys` with `TooManyValidatorKeys`.
  /// Exactly 30 keys refines fine. The cap and the once-per-Refine rule are
  /// separate rules with separate errors.
  run setValidatorKeysChunkCapTest = {
    val capKeys = range(0, SetValidatorKeysMaxKeys).foldl(List(), (acc, _) => acc.append(0))
    val overKeys = range(0, SetValidatorKeysMaxKeys + 1).foldl(List(), (acc, _) => acc.append(0))
    val atCap = SetValidatorKeys({ keys: capKeys, isLast: true })
    val overCap = SetValidatorKeys({ keys: overKeys, isLast: true })
    all {
      assert(capKeys.length() == 30),
      assert(overKeys.length() == 31),
      // 31 keys: rejected by the explicit length check.
      assert(refineWithMsgs(initialState, AssetHubParaId, List(overCap))
        == WorkOk(refineErr(AssetHubParaId, TooManyValidatorKeys))),
      // 30 keys: at the cap, so not rejected by any Refine check.
      assert(isOkDigest(refineWithMsgs(initialState, AssetHubParaId, List(atCap)))),
      // The cap is per chunk, not per invocation: two at-cap chunks trip the
      // separate repeat rule instead.
      assert(refineWithMsgs(initialState, AssetHubParaId, List(atCap, atCap))
        == WorkOk(refineErr(AssetHubParaId, SetValidatorKeysRepeated))),
      init
    }
  }

  /// §4.3: naming a `para_id` the caller may not touch makes REFINE fail with
  /// `RestrictedHostFunction`, so the message never reaches Accumulate at all.
  /// Naming itself, or being the Coretime chain, refines fine.
  run refineRejectsForeignParaIdTest = {
    val h: Hash = { hashBytes: 7777 }
    all {
      // Asset Hub reaching for Coretime's state: Refine rejects, for both calls.
      assert(refineWithMsgs(initialState, AssetHubParaId,
        List(Forget({ target: Parachain(CoretimeParaId), hash: h, len: 1024 })))
        == WorkOk(refineErr(AssetHubParaId, RestrictedHostFunction))),
      assert(refineWithMsgs(initialState, AssetHubParaId,
        List(RemoveKV({ paraId: CoretimeParaId, key: List(0) })))
        == WorkOk(refineErr(AssetHubParaId, RestrictedHostFunction))),
      // Asset Hub releasing its own state: refines fine.
      assert(isOkDigest(refineWithMsgs(initialState, AssetHubParaId,
        List(Forget({ target: Parachain(AssetHubParaId), hash: h, len: 1024 }))))),
      assert(isOkDigest(refineWithMsgs(initialState, AssetHubParaId,
        List(RemoveKV({ paraId: AssetHubParaId, key: List(0) }))))),
      // Coretime may name anyone.
      assert(isOkDigest(refineWithMsgs(initialState, CoretimeParaId,
        List(Forget({ target: Parachain(AssetHubParaId), hash: h, len: 1024 }))))),
      init
    }
  }
}
