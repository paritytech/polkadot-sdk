// Refine-side enforcement: validation-code outcomes and refine error paths (design 4).
module refine_test {
  import types.* from "../types"
  import messages.* from "../messages"
  import state.* from "../state"
  import state_balance.* from "../state_balance"
  import refine.* from "../refine"
  import accumulate.* from "../accumulate"
  import main.* from "../main"
  import support.* from "./support"

  /// An otherwise-valid candidate whose validation code skipped `set_parent_head_hash`
  /// and/or `set_head`, or called one twice, is rejected by Refine with `InvalidHeadDeclaration`.
  /// Coretime's active code (`{ vchBytes: 1 }`) is solicited in
  /// `initialState`, so the candidate passes `codeAvailable` and reaches the
  /// head-declaration check rather than failing the whole invocation earlier.
  run refineInvalidHeadDeclarationTest = {
    val pkg: WorkPackage = {
      authorizedParas: List(CoretimeParaId),
      items: List({ validationCode: { hash: { vchBytes: 1 }, len: FixedCodeLen } })
    }
    val parentOnly =
      refine(initialState, pkg, false, List(SetParentHeadHash(headHash(0))), 0)[0]
    val headOnly =
      refine(initialState, pkg, false, List(SetHead(1)), 0)[0]
    val neither =
      refine(initialState, pkg, false, List(), 0)[0]
    val both =
      refine(initialState, pkg, false, wellFormedCalls(headHash(0), 1, List()), 0)[0]
    // Declaring a head twice is as invalid as not declaring it.
    val headTwice = refine(initialState, pkg, false,
      wellFormedCalls(headHash(0), 1, List()).append(SetHead(1)), 0)[0]
    val parentTwice = refine(initialState, pkg, false,
      wellFormedCalls(headHash(0), 1, List()).append(SetParentHeadHash(headHash(0))), 0)[0]
    all {
      assert(headTwice == WorkOk(Err({ paraId: CoretimeParaId, error: InvalidHeadDeclaration }))),
      assert(parentTwice == WorkOk(Err({ paraId: CoretimeParaId, error: InvalidHeadDeclaration }))),
      // Missing new head, missing parent head, or both → rejected.
      assert(parentOnly == WorkOk(Err({ paraId: CoretimeParaId, error: InvalidHeadDeclaration }))),
      assert(headOnly == WorkOk(Err({ paraId: CoretimeParaId, error: InvalidHeadDeclaration }))),
      assert(neither == WorkOk(Err({ paraId: CoretimeParaId, error: InvalidHeadDeclaration }))),
      // Both declared → not an InvalidHeadDeclaration error (succeeds here).
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
  /// Uses `Solicit` (unrestricted, 39 B each) so 1025 stay under the 40 KiB
  /// budget: the count cap, not the byte budget, must be the reason.
  run tooManyUpwardMessagesTest = {
    val pkg: WorkPackage = {
      authorizedParas: List(CoretimeParaId),
      items: List({ validationCode: { hash: { vchBytes: 1 }, len: FixedCodeLen } })
    }
    val msg = Solicit({ target: Parachain(CoretimeParaId), hash: { hashBytes: 7 }, len: 1 })
    val over = range(0, MaxUpwardMessages + 1).foldl(List(), (acc, _) => acc.append(msg))
    val atCap = range(0, MaxUpwardMessages).foldl(List(), (acc, _) => acc.append(msg))
    val overOut =
      refine(initialState, pkg, false, wellFormedCalls(headHash(0), 1, over), 0)[0]
    val atCapOut =
      refine(initialState, pkg, false, wellFormedCalls(headHash(0), 1, atCap), 0)[0]
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
  /// distinct, ordinary preimage: soliciting it charges and forgetting it
  /// refunds, while the same hash at the code's OWN len is validation code and refused.
  run solicitForgetMatchesHashAndLenTest = {
    val para = CoretimeParaId
    val h = { vchBytes: 1 }.asPreimage()   // coretime's ACTIVE code hash
    val otherLen = 999                    // != FixedCodeLen
    val baseAcc: AccumulateOutcome = {
      state: initialState, jamDesignate: List(), jamSolicit: Set(),
      jamForget: Set(), jamAssigns: List(), logs: List(), foreign: Map()
    }
    val pi0 = initialState.paraInfo(para)
    // 1. Solicit at the code's OWN len: already the active code, so this
    //    changes nothing: no new entry, no charge.
    val resolicit = applyUpwardMessage(baseAcc, para, FixedCodeLen, 0, 0,
      Solicit({ target: Parachain(para), hash: h, len: FixedCodeLen }))
    // 2. Solicit the SAME hash at a different len: a genuinely distinct entry,
    //    charged, and it must NOT disturb the active code.
    val sol = applyUpwardMessage({ ...baseAcc, state: resolicit.state }, para, FixedCodeLen, 0, 0,
      Solicit({ target: Parachain(para), hash: h, len: otherLen }))
    val pi1 = sol.state.paraInfo(para)
    // 3. Forget that distinct entry: released (never provided -> one-step drop),
    //    and the active code is untouched.
    val fgt = applyUpwardMessage({ ...baseAcc, state: sol.state }, para, FixedCodeLen, 0, 0,
      Forget({ target: Parachain(para), hash: h, len: otherLen }))
    val pi2 = fgt.state.paraInfo(para)
    // 4. Forget at the code's OWN len: it is the active code, so refused.
    val refused = applyUpwardMessage({ ...baseAcc, state: fgt.state }, para, FixedCodeLen, 0, 0,
      Forget({ target: Parachain(para), hash: h, len: FixedCodeLen }))
    all {
      assert(isValidationCodeOf(pi0, h, FixedCodeLen)),
      // (1) re-soliciting the active code is a no-op: no entry, no charge.
      assert(resolicit.state == initialState),
      // (2) wrong len is a distinct, charged entry; the active code is untouched.
      assert(sol.state.preimageRegistry.keys().contains((h, otherLen))),
      assert(pi1.usedStateBalance == pi0.usedStateBalance + preimageFootprint(otherLen)),
      assert(not(isValidationCodeOf(pi1, h, otherLen))),
      assert(sol.state.preimageRegistry.keys().contains((h, FixedCodeLen))),
      // (3) forgetting the wrong-len entry releases it and refunds.
      assert(not(fgt.state.preimageRegistry.keys().contains((h, otherLen)))),
      assert(pi2.usedStateBalance == pi0.usedStateBalance),
      assert(isValidationCodeOf(pi2, h, FixedCodeLen)),
      // (4) correct len is refused: nothing changes, and it is logged.
      assert(refused.state == fgt.state),
      assert(refused.logs == List(
        CanNotRemoveCode({ hash: h, len: FixedCodeLen }))),
      assert(refused.state.preimageRegistry.keys().contains((h, FixedCodeLen))),
      init
    }
  }

  /// §4.2: validation code has exactly two ways to fail, and they differ in what is
  /// recorded. `report_error(data)` aborts it and is logged as
  /// `RefineLog::Opaque(data)`; any other abnormal exit is not caught, so the
  /// whole `refine` fails and yields a work error with NO digest, which
  /// Accumulate then skips entirely, leaving the para's log untouched.
  run codeFailureModesTest = {
    val para = CoretimeParaId
    val pkg: WorkPackage = {
      authorizedParas: List(para),
      items: List({ validationCode: { hash: { vchBytes: 1 }, len: FixedCodeLen } })
    }
    val reported = refine(initialState, pkg, false,
      wellFormedCalls(headHash(0), 1, List()).append(ReportError(42)), 0)[0]
    val panicked = refine(initialState, pkg, false,
      wellFormedCalls(headHash(0), 1, List()).append(Panic), 0)[0]
    // Accumulating the panicking result must touch nothing at all.
    val accPanic = accumulateOnePackage(
      initialState, { result: panicked, authTrace: 7 }, Set(), List(), FixedCodeLen, 5, Map())
    all {
      // Reporting is the only way a validation code records a reason.
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

  /// §4.3: `set_head` with head data past the 4 KiB `HeadData` cap aborts Refine
  /// with `HeadDataTooLarge`. The abort happens inside `set_head`, so it wins
  /// over the wrapper's post-exit check for a missing `set_parent_head_hash`.
  /// Like every other structural refine failure it is nullary (1 B encoded) and
  /// the most disposable eviction rank (§5.1).
  run headDataTooLargeTest = {
    val pkg: WorkPackage = {
      authorizedParas: List(CoretimeParaId),
      items: List({ validationCode: { hash: { vchBytes: 1 }, len: FixedCodeLen } })
    }
    val withParent =
      refine(initialState, pkg, false,
        List(SetParentHeadHash(headHash(0)), SetHeadOversized), 0)[0]
    val withoutParent =
      refine(initialState, pkg, false, List(SetHeadOversized), 0)[0]
    all {
      assert(withParent == WorkOk(Err({ paraId: CoretimeParaId, error: HeadDataTooLarge }))),
      assert(withoutParent == WorkOk(Err({ paraId: CoretimeParaId, error: HeadDataTooLarge }))),
      assert(refineLogSize(HeadDataTooLarge) == 1),
      assert(entryRank((0, RefineLogEntry({ error: HeadDataTooLarge, authTrace: 0 }))) == 0),
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

  /// §4.3: the first host call that breaks a rule aborts Refine, and a
  /// `send_upward_message` breaking several rules fails with the first in the
  /// §4.3 order.
  run firstFailingCallWinsTest = {
    val ah = AssetHubParaId
    val pi = initialState.paraInfo(ah)
    val parent = SetParentHeadHash(headHash(pi.headData))
    val setHead = SetHead(pi.headData + 1)
    val emptyAssign = Send(AssignCore({ core: 0, queue: List(), newAssigner: None, jamSlot: 0 }))
    val solicit = Solicit({ target: Parachain(ah), hash: { hashBytes: 7 }, len: 1 })
    val tooMany =
      sends(range(0, MaxUpwardMessages + 1).foldl(List(), (acc, _) => acc.append(solicit)))
    val oneKey = SetValidatorKeys({ keys: List(0), isLast: false })
    val overKeys = SetValidatorKeys({
      keys: range(0, SetValidatorKeysMaxKeys + 1).foldl(List(), (acc, _) => acc.append(0)),
      isLast: true
    })
    all {
      // The para restriction comes before the variant's own queue rule.
      assert(refineCalls(initialState, ah, List(parent, setHead, emptyAssign))
        == WorkOk(refineErr(ah, RestrictedHostFunction))),
      // An oversized head before the 1025th message wins, and after it loses.
      assert(refineCalls(initialState, ah, List(parent, SetHeadOversized).concat(tooMany))
        == WorkOk(refineErr(ah, HeadDataTooLarge))),
      assert(refineCalls(initialState, ah, List(parent).concat(tooMany).append(SetHeadOversized))
        == WorkOk(refineErr(ah, TooManyUpwardMessages))),
      // The chunk size comes before the repetition.
      assert(refineCalls(initialState, ah, List(parent, setHead, Send(oneKey), Send(overKeys)))
        == WorkOk(refineErr(ah, TooManyValidatorKeys))),
      // `report_error` records its reason only if nothing aborted before it.
      assert(refineCalls(initialState, ah, List(parent, setHead, emptyAssign, ReportError(1)))
        == WorkOk(refineErr(ah, RestrictedHostFunction))),
      assert(refineCalls(initialState, ah, List(parent, setHead, ReportError(1), emptyAssign))
        == WorkOk(refineErr(ah, Opaque(1)))),
      init
    }
  }
}
