// Multi-package accumulate replay, rejected candidates, package-level
// failures, and service self-upgrade (design 5.1, 5.4).
module accumulate_test {
  import types.* from "../types"
  import messages.* from "../messages"
  import state.* from "../state"
  import state_balance.* from "../state_balance"
  import refine.* from "../refine"
  import accumulate.* from "../accumulate"
  import state_vars.* from "../state_vars"
  import main.* from "../main"
  import support.* from "./support"

  /// Two work packages in one block: one for each privileged chain.
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

  /// §5.1: rejection is what confines the privileged host functions of §4.3. A
  /// candidate rejected at the parent-head check must never reach the replay step,
  /// so a stale-parent candidate carrying `UpgradeService` cannot swap the service's
  /// own code. The control at the end replays the identical message from an accepted
  /// candidate and shows it does take effect, so the block is rejection and not some
  /// unrelated precondition.
  run rejectedCandidateCannotUsePrivilegedCallsTest = {
    val ah = AssetHubParaId
    val ahCode: ValidationCodeHash = { vchBytes: 2 }
    val newCode: Hash = { hashBytes: 4242 }
    val upgrade =
      UpgradeService({ codeHash: newCode, len: FixedCodeLen, minAccGas: 0, minMemoGas: 0 })
    init
      // Make the new service code available, so the upgrade's own preconditions hold.
      .then(applyHappyPath(ah, ahCode, List(Solicit({ hash: newCode, len: FixedCodeLen }))))
      .then(provide(newCode, FixedCodeLen))
      .then(expectThat(all {
        svc.preimageAvailable(newCode, FixedCodeLen),
        svc.serviceCodeHash == initialState.serviceCodeHash
      }))
      // A REJECTED candidate carrying the upgrade: nothing may happen.
      .then(applyStaleParentCandidate(ah, ahCode, List(upgrade)))
      .then(expectThat(svc.serviceCodeHash == initialState.serviceCodeHash))
      // Control: the same message from an accepted candidate does upgrade.
      .then(applyHappyPath(ah, ahCode, List(upgrade)))
      .then(expectThat(svc.serviceCodeHash == newCode))
  }

  /// §5.1: a rejected candidate changes NOTHING. Pruning in particular must not
  /// happen: the lookup-anchor is chosen by whoever submitted the package, so a
  /// rejected candidate that pruned would let anyone holding coretime erase a
  /// parachain's log at any rank, bypassing the eviction ranking. The log here is
  /// seeded with an old `Opaque` (rank 1) that a prune to `now` would destroy.
  run rejectedCandidateChangesNothingTest = {
    val para = CoretimeParaId
    val code: ValidationCodeHash = { vchBytes: 1 }
    // An old, high-value entry the parachain itself reported.
    val seeded = initialState.appendRefineLog(para, 1, Opaque(64), 0)
    val ok: ParachainWorkDigestOk = {
      paraId: para,
      validationCode: { hash: code, len: FixedCodeLen },
      // Stale parent: rejected at the parent-head check.
      parentHeadHash: headHash(StaleParentHead),
      headData: StaleParentHead,
      upwardMessages: List(),
      // A lookup-anchor far above the seeded entry's timeslot: were this candidate
      // to prune, the entry would be gone.
      lookupAnchor: 500
    }
    val out = accumulateOnePackage(
      seeded, { result: WorkOk(Ok(ok)), authTrace: 9 }, Set(), None, FixedCodeLen, 500)
    all {
      assert(seeded.paraLog(para).length() == 1),
      // Nothing at all changed, and the old entry survived.
      assert(out.state == seeded),
      assert(out.state.paraLog(para).length() == 1),
      assert(out.logPruned == None),
      init
    }
  }

  /// §4.1 step 1 / §4.2: every package-level failure panics, because none has an
  /// authoritative `para_id` to attribute a `RefineLog` to. A work error carries
  /// no digest, so nothing is logged for any of them.
  run packageLevelFailuresPanicTest = {
    val wi = { validationCode: { hash: { vchBytes: 1 }, len: FixedCodeLen }, pov: 0 }
    val good: WorkPackage = { authorizedParas: List(CoretimeParaId), items: List(wi) }
    // Two items: not single-item (§3.2).
    val twoItems: WorkPackage = {
      authorizedParas: List(CoretimeParaId, CoretimeParaId), items: List(wi, wi) }
    // Config length disagrees with the item count.
    val mismatch: WorkPackage = {
      authorizedParas: List(CoretimeParaId, CoretimeParaId), items: List(wi) }
    // No para named at all: the old code fabricated `MkParaId(0)` here.
    val noParas: WorkPackage = { authorizedParas: List(), items: List(wi) }
    pure def go(pkg: WorkPackage, noPrefix: bool): WorkExecResult =
      refine(initialState, pkg, noPrefix, PvfOk, Some(headHash(0)), Some(1), List(), 0)[0]
    all {
      // Config carries no `Vec<ParaId>` prefix, so no para can be sourced.
      assert(go(good, true) == WorkErr),
      assert(go(twoItems, false) == WorkErr),
      assert(go(mismatch, false) == WorkErr),
      assert(go(noParas, false) == WorkErr),
      // The same package without those faults still refines to a digest.
      assert(isOkDigest(go(good, false))),
      init
    }
  }
}
