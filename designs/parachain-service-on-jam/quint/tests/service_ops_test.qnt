// Supervisor-driven operations on a supervised service (design 6.5).
module service_ops_test {
  import types.* from "../types"
  import messages.* from "../messages"
  import state.* from "../state"
  import accumulate.* from "../accumulate"
  import state_vars.* from "../state_vars"
  import main.* from "../main"
  import support.* from "./support"

  pure val victim: ServiceId = MkServiceId(4)
  pure val someHash: Hash = { hashBytes: 9 }

  pure val forgetIt = Forget({ target: Service(victim), hash: someHash, len: 1024 })
  pure val solicitIt = Solicit({ target: Service(victim), hash: someHash, len: 1024 })
  pure val removeIt = RemoveServiceStorage({ service: victim, key: List(1, 2) })
  pure val ejectIt = EjectService({ service: victim })
  pure val createIt = CreateService({
    codeHash: someHash, len: FixedCodeLen, minItemGas: 0, minMemoGas: 0,
    id: 77, sourceSupervisorBalance: false, newSupervisorBalance: false
  })
  // Handing the service to itself, which is the "set it free" form.
  pure val freeIt = SetServiceSupervisor({ service: victim, newSupervisor: victim })
  pure val allOps = List(forgetIt, solicitIt, removeIt, ejectIt, createIt, freeIt)

  /// None of these is decided here: each outcome turns on the target's
  /// existence, on whom this service supervises, and for solicit/create on
  /// balances, none of which is modelled. What the model does claim is that
  /// none of them touches this service's own parachain state.
  run serviceOpsOutcomeAbstractedTest = {
    init.then(expectThat(allOps.foldl(true, (ok, m) => {
      val out = replay(svc, AssetHubParaId, 0, m)
      ok and out.state == svc and out.logs == List()
        and out.jamForget == Set() and out.jamSolicit == Set()
    })))
  }

  /// Every one of them is Asset Hub only, while a `Parachain` target keeps the
  /// 6.1 rules: a para may name itself from anywhere.
  run serviceOpsAreAssetHubOnlyTest = {
    val solicitSelf =
      Solicit({ target: Parachain(CoretimeParaId), hash: someHash, len: 1024 })
    init.then(expectThat(all {
      allOps.foldl(true, (ok, m) =>
        ok and violatesParachainRestrictions(List(m), CoretimeParaId)),
      allOps.foldl(true, (ok, m) =>
        ok and not(violatesParachainRestrictions(List(m), AssetHubParaId))),
      not(violatesParachainRestrictions(List(solicitSelf), CoretimeParaId)),
    }))
  }

  /// A `Service` target carries no para, so it must not be mistaken for a
  /// cross-para call; that check applies to `Parachain` targets only. A
  /// `Parachain` target that names someone else is rejected for `Solicit` just
  /// A `Service` target carries no para, so it must not be mistaken for a
  /// cross-para call; that check applies to `Parachain` targets only. A
  /// `Parachain` target naming someone else is rejected for `Solicit` just as
  /// for `Forget`, which is what stops a para charging another's balance.
  run serviceOpsAreNotForeignParaTest = {
    init.then(expectThat(all {
      not(targetsForeignPara(forgetIt, AssetHubParaId)),
      not(targetsForeignPara(solicitIt, AssetHubParaId)),
      targetsForeignPara(
        Forget({ target: Parachain(CoretimeParaId), hash: someHash, len: 1024 }),
        AssetHubParaId),
      targetsForeignPara(
        Solicit({ target: Parachain(CoretimeParaId), hash: someHash, len: 1024 }),
        AssetHubParaId),
    }))
  }
}
