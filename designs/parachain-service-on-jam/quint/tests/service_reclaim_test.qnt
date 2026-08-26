// Reclaiming a supervised service's footprint (design 6.5).
module service_reclaim_test {
  import types.* from "../types"
  import messages.* from "../messages"
  import state.* from "../state"
  import accumulate.* from "../accumulate"
  import state_vars.* from "../state_vars"
  import main.* from "../main"
  import support.* from "./support"

  pure val victim: ServiceId = MkServiceId(4)

  /// Neither reclaim is decided here: the outcome turns on the target's
  /// existence and on whom this service supervises, neither of which is
  /// modelled. Both leave this service's own state alone, which is all the
  /// model does claim.
  run serviceReclaimOutcomeAbstractedTest = {
    val forgetIt = Forget({ target: Service(victim), hash: { hashBytes: 9 }, len: 1024 })
    val removeIt = RemoveServiceStorage({ service: victim, key: List(1, 2) })
    init.then(expectThat(all {
      replay(svc, AssetHubParaId, 0, forgetIt).state == svc,
      replay(svc, AssetHubParaId, 0, removeIt).state == svc,
      replay(svc, AssetHubParaId, 0, forgetIt).logs == List(),
      replay(svc, AssetHubParaId, 0, removeIt).logs == List(),
      // Nor does either touch the preimage registry of this service's store.
      replay(svc, AssetHubParaId, 0, forgetIt).jamForget == Set(),
    }))
  }

  /// Both are Asset Hub only, while a `Parachain` target keeps the 6.1 rules:
  /// a para may name itself from anywhere.
  run serviceReclaimIsAssetHubOnlyTest = {
    val forgetService = Forget({ target: Service(victim), hash: { hashBytes: 9 }, len: 1024 })
    val removeIt = RemoveServiceStorage({ service: victim, key: List(1, 2) })
    val forgetSelf =
      Forget({ target: Parachain(CoretimeParaId), hash: { hashBytes: 9 }, len: 1024 })
    init.then(expectThat(all {
      violatesParachainRestrictions(List(forgetService), CoretimeParaId),
      violatesParachainRestrictions(List(removeIt), CoretimeParaId),
      not(violatesParachainRestrictions(List(forgetService), AssetHubParaId)),
      not(violatesParachainRestrictions(List(removeIt), AssetHubParaId)),
      not(violatesParachainRestrictions(List(forgetSelf), CoretimeParaId)),
    }))
  }

  /// A `Service` target carries no para, so it must not be mistaken for a
  /// cross-para `forget`; that check applies to `Parachain` targets only.
  run serviceReclaimIsNotForeignParaTest = {
    init.then(expectThat(all {
      not(targetsForeignPara(
        Forget({ target: Service(victim), hash: { hashBytes: 9 }, len: 1024 }),
        AssetHubParaId)),
      targetsForeignPara(
        Forget({ target: Parachain(CoretimeParaId), hash: { hashBytes: 9 }, len: 1024 }),
        AssetHubParaId),
    }))
  }
}
