// Coretime-chain-only management: register, operate, clean up, and
// delegated release of another para's state (design 6.2 to 6.4).
module management_test {
  import types.* from "../types"
  import messages.* from "../messages"
  import state.* from "../state"
  import state_balance.* from "../state_balance"
  import management.* from "../management"
  import state_vars.* from "../state_vars"
  import main.* from "../main"
  import support.* from "./support"

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
  /// state beyond its baseline and validation code; here an extra solicited
  /// preimage (§6.4).
  run cleanUpTooMuchStateHeldTest = {
    val para = MkParaId(6)
    val code: ValidationCodeHash = { vchBytes: 0x66 }
    val extra: Hash = { hashBytes: 0x99 }
    val bal = BaselineFootprint + preimageFootprint(FixedCodeLen) + preimageFootprint(512)
    val s0 = parachainSetStateBalance(initialState, para, bal).state
    val s1 = parachainSetValidationCode(s0, para, code, FixedCodeLen, 0).state
    // Solicit an extra preimage: now the para holds more than baseline + code.
    val s2 = s1.addReferencer(para, extra, 512).state
    val c = parachainCleanUp(s2, para, 0)
    all {
      assert(c.state == s2),
      assert(c.logs == List(TooMuchStateHeld)),
      assert(c.state.isLive(para)),
      assert(not(c.state.paraInfo(para).isDeregistering)),
      init
    }
  }

  /// Coretime naming another para in `forget` releases that para's preimage and
  /// refunds its footprint. Never provided → expunged in one step.
  run coretimeForgetDrainsPreimageTest = {
    val target = AssetHubParaId
    val h: Hash = { hashBytes: 4242 }
    val len = 1024
    val before = initialState.paraInfo(target).usedStateBalance
    val held = initialState.addReferencer(target, h, len).state
    val forced = replay(held, CoretimeParaId, 0, Forget({ target: Parachain(target), hash: h, len: len }))
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
    val msg = Forget({ target: Parachain(target), hash: h, len: len })
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
  /// `pinned` bit: Coretime can never strip a running PVF, nor refund the code.
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
      Forget({ target: Parachain(target), hash: h, len: FixedCodeLen }))
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

  /// Both calls are idempotent no-ops for an unregistered target and for an entry
  /// the target does not hold.
  run delegatedReleaseNoOpTest = {
    val ghost = MkParaId(9)
    val h: Hash = { hashBytes: 5555 }
    all {
      assert(replay(initialState, CoretimeParaId, 0,
        Forget({ target: Parachain(ghost), hash: h, len: 1024 })).state == initialState),
      assert(replay(initialState, CoretimeParaId, 0,
        RemoveKV({ paraId: ghost, key: List(0) })).state == initialState),
      assert(replay(initialState, CoretimeParaId, 0,
        Forget({ target: Parachain(AssetHubParaId), hash: h, len: 1024 })).state == initialState),
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
    val d1 = replay(held, CoretimeParaId, 0, Forget({ target: Parachain(target), hash: h, len: len }))
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


  /// §6, §6.4: once clean-up has set `isDeregistering`, the para is treated as
  /// if it no longer existed. `parachain_set_state_balance` is rejected as
  /// `ParachainIsDeregistering`; the two other Coretime calls, a `Solicit` naming
  /// it and a `SetKV` for it no-op without a log. So a half-removed para cannot
  /// be revived or grow state; only `parachain_clean_up` still acts.
  run deregisteringParaIsFrozenTest = {
    val para = MkParaId(5)
    val code: ValidationCodeHash = { vchBytes: 0x55 }
    val other: ValidationCodeHash = { vchBytes: 0x56 }
    val extra: Hash = { hashBytes: 0x99 }
    val bal = BaselineFootprint + preimageFootprint(FixedCodeLen) * 2
    val s0 = parachainSetStateBalance(initialState, para, bal).state
    val s1 = markProvided(
      parachainSetValidationCode(s0, para, code, FixedCodeLen, 0).state,
      vchAsHash(code), FixedCodeLen)
    val frozen = parachainCleanUp(s1, para, 0).state
    val setBal = parachainSetStateBalance(frozen, para, bal * 2)
    val setHead = parachainSetHead(frozen, para, 42)
    val setCode = parachainSetValidationCode(frozen, para, other, FixedCodeLen, 0)
    val solicit = replay(frozen, CoretimeParaId, 0,
      Solicit({ target: Parachain(para), hash: extra, len: 1024 }))
    val setKv = replay(frozen, para, 0, SetKV({ key: List(0), value: List(0) }))
    val removeKv = replay(frozen, CoretimeParaId, 0, RemoveKV({ paraId: para, key: List(0) }))
    val retried = parachainCleanUp(frozen, para, ExpungePeriod + 1)
    all {
      assert(frozen.paraInfo(para).isDeregistering),
      assert(setBal.state == frozen),
      assert(setBal.logs == List(StateBalanceUpdateRejected({
        paraId: para, attempted: bal * 2, reason: ParachainIsDeregistering }))),
      assert(setHead.state == frozen and setHead.logs == List()),
      assert(setCode.state == frozen and setCode.logs == List()),
      assert(setCode.jamSolicit == Set() and setCode.jamForget == Set()),
      assert(solicit.state == frozen and solicit.logs == List()),
      assert(solicit.jamSolicit == Set()),
      assert(setKv.state == frozen and setKv.logs == List()),
      assert(removeKv.state == frozen and removeKv.logs == List()),
      // Clean-up itself still completes.
      assert(not(retried.state.isLive(para))),
      init
    }
  }
}
