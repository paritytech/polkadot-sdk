// Supervisor-driven operations on a supervised service (design 6.5).
module service_ops_test {
  import types.* from "../types"
  import messages.* from "../messages"
  import state.* from "../state"
  import accumulate.* from "../accumulate"
  import foreign_services.* from "../foreign_services"
  import state_vars.* from "../state_vars"
  import main.* from "../main"
  import support.* from "./support"

  pure val victim: ServiceId = MkServiceId(MinPublicServiceIndex)
  pure val stranger: ServiceId = MkServiceId(MinPublicServiceIndex + 1)
  pure val someHash: Hash = { hashBytes: 9 }
  pure val someLen: int = 1024
  pure val someKey: List[int] = List(1, 2)

  /// A service supervised by us, with the given store and creation slot, and
  /// funds to spare.
  pure def supervised(
    reqs: (Hash, int) -> RequestStatus, store: List[int] -> List[int], created: Timeslot
  ): ForeignService = {
    supervisor: ParachainServiceId,
    supervisorCreated: ParachainServiceCreated,
    created: created,
    requests: reqs,
    storage: store,
    account: { balance: 1000000, supervisorBalance: 0, minMemoGas: 0 }
  }

  pure val emptyOurs: ForeignServices = Map(victim -> supervised(Map(), Map(), 0))

  pure val forgetIt = Forget({ target: Service(victim), hash: someHash, len: someLen })
  pure val solicitIt = Solicit({ target: Service(victim), hash: someHash, len: someLen })
  pure val removeIt = RemoveServiceStorage({ service: victim, key: someKey })
  pure val ejectIt = EjectService({ service: victim })
  pure def createWith(desired: Option[ServiceId]): UpwardMessage = CreateService({
    codeHash: someHash, len: FixedCodeLen, minItemGas: 0, minMemoGas: 0,
    id: 77, desiredId: desired,
    sourceSupervisorBalance: false, newSupervisorBalance: false
  })

  pure val createIt = createWith(None)
  pure val allOps = List(forgetIt, solicitIt, removeIt, ejectIt, createIt)

  pure def logsOf(
    fs: ForeignServices, m: UpwardMessage, at: Timeslot
  ): List[AccumulateLog] =
    replayForeign(fs, AssetHubParaId, at, m).logs

  pure def after(
    fs: ForeignServices, m: UpwardMessage, at: Timeslot
  ): ForeignServices =
    replayForeign(fs, AssetHubParaId, at, m).foreign

  /// `create_service` bootstraps the whole surface: JAM makes the creator the
  /// new service's supervisor, so every other operation becomes available on
  /// it. The assigned id comes back in the log.
  run createMakesUsSupervisorTest = {
    val out = replayForeign(Map(), AssetHubParaId, 5, createIt)
    init.then(expectThat(all {
      out.logs == List(ServiceCreation({ id: 77, result: Created(victim) })),
      supervisedByUs(out.foreign, victim),
      out.foreign.get(victim).created == 5,
      // Starts holding only the request for its own code.
      out.foreign.get(victim).requests.keys() == Set((someHash, FixedCodeLen)),
      out.foreign.get(victim).storage == Map(),
      // A second create does not collide with the first.
      after(out.foreign, createIt, 6).keys() == Set(victim, stranger),
    }))
  }

  /// A `desired_id` places the service at that index; an index already in use
  /// is refused and changes nothing.
  run createHonoursDesiredIdTest = {
    val wanted = MkServiceId(42)
    val atWanted = replayForeign(Map(), AssetHubParaId, 5, createWith(Some(wanted)))
    init.then(expectThat(all {
      atWanted.logs == List(ServiceCreation({ id: 77, result: Created(wanted) })),
      supervisedByUs(atWanted.foreign, wanted),
      logsOf(atWanted.foreign, createWith(Some(wanted)), 6)
        == List(ServiceCreation({ id: 77, result: IdTaken })),
      after(atWanted.foreign, createWith(Some(wanted)), 6) == atWanted.foreign,
      // Including our own index, which sits in the protected range.
      logsOf(Map(), createWith(Some(ParachainServiceId)), 6)
        == List(ServiceCreation({ id: 77, result: IdTaken })),
      after(Map(), createWith(Some(ParachainServiceId)), 6) == Map(),
    }))
  }

  /// Every operation refuses a service that does not exist.
  run opsRefuseUnknownServiceTest = {
    init.then(expectThat(all {
      logsOf(Map(), forgetIt, 0)
        == List(ServiceStoreFailed({ service: victim, error: StoreUnknownService })),
      logsOf(Map(), removeIt, 0)
        == List(ServiceStoreFailed({ service: victim, error: StoreUnknownService })),
      logsOf(Map(), solicitIt, 0)
        == List(ServiceSolicitFailed({ service: victim, error: SolicitUnknownService })),
      logsOf(Map(), ejectIt, 0)
        == List(ServiceEjectFailed({ service: victim, error: EjectUnknownService })),
    }))
  }

  /// And refuses one it does not supervise.
  run opsRefuseUnsupervisedTest = {
    val theirs: ForeignServices =
      Map(victim -> { ...supervised(Map(), Map(), 0), supervisor: stranger })
    init.then(expectThat(all {
      not(supervisedByUs(theirs, victim)),
      logsOf(theirs, forgetIt, 0)
        == List(ServiceStoreFailed({ service: victim, error: StoreNotSupervised })),
      logsOf(theirs, solicitIt, 0)
        == List(ServiceSolicitFailed({ service: victim, error: SolicitNotSupervised })),
      logsOf(theirs, ejectIt, 0)
        == List(ServiceEjectFailed({ service: victim, error: EjectNotSupervised })),
    }))
  }

  /// A supervisor whose recorded creation slot no longer matches is not the
  /// effective supervisor: the gray paper's guard against a reused id
  /// inheriting power.
  run staleSupervisorLinkFallsBackToSelfTest = {
    val stale: ForeignServices =
      Map(victim -> { ...supervised(Map(), Map(), 0),
                      supervisorCreated: ParachainServiceCreated + 1 })
    init.then(expectThat(all {
      effectiveSupervisor(stale, victim) == victim,
      logsOf(stale, forgetIt, 0)
        == List(ServiceStoreFailed({ service: victim, error: StoreNotSupervised })),
    }))
  }

  /// `eject_service` is the last step of a wind-down, not a way to perform one.
  run ejectRequiresEmptyAndAgedTest = {
    val withRequest: ForeignServices =
      Map(victim -> supervised(Map((someHash, someLen) -> Unprovided), Map(), 0))
    val withStorage: ForeignServices =
      Map(victim -> supervised(Map(), Map(someKey -> List()), 0))
    init.then(expectThat(all {
      logsOf(withRequest, ejectIt, 10)
        == List(ServiceEjectFailed({ service: victim, error: NotEmpty })),
      logsOf(withStorage, ejectIt, 10)
        == List(ServiceEjectFailed({ service: victim, error: NotEmpty })),
      // Empty, but created in this very timeslot.
      logsOf(emptyOurs, ejectIt, 0)
        == List(ServiceEjectFailed({ service: victim, error: CreatedThisSlot })),
      // Empty and older: gone.
      logsOf(emptyOurs, ejectIt, 10) == List(),
      after(emptyOurs, ejectIt, 10).keys() == Set(),
      // Naming ourselves is refused outright.
      logsOf(emptyOurs, EjectService({ service: ParachainServiceId }), 10)
        == List(ServiceEjectFailed({ service: ParachainServiceId, error: TargetIsSelf })),
    }))
  }

  /// Removing a storage key shrinks the store and is idempotent.
  run removeStorageIsIdempotentTest = {
    val withStorage: ForeignServices =
      Map(victim -> supervised(Map(), Map(someKey -> List(), List(3) -> List()), 0))
    init.then(expectThat(all {
      after(withStorage, removeIt, 0).get(victim).storage.keys() == Set(List(3)),
      logsOf(withStorage, removeIt, 0) == List(),
      // Absent key: still no log, still no change.
      after(emptyOurs, removeIt, 0).get(victim).storage == Map(),
      logsOf(emptyOurs, removeIt, 0) == List(),
    }))
  }

  /// The two-step expunge of 6.1 applies to a `Service` target unchanged.
  run forgetOnServiceIsTwoStepTest = {
    val provided: ForeignServices =
      Map(victim -> supervised(Map((someHash, someLen) -> Provided), Map(), 0))
    val once = after(provided, forgetIt, 10)
    init.then(expectThat(all {
      // First forget only unrequests, and says when to return.
      logsOf(provided, forgetIt, 10)
        == List(ForgetAgainAt({ hash: someHash, len: someLen, due: 10 + ExpungePeriod })),
      once.get(victim).requests.get((someHash, someLen)) == Unrequested(10),
      // Too early: nothing happens and the ORIGINAL deadline is reported.
      logsOf(once, forgetIt, 11)
        == List(ForgetAgainAt({ hash: someHash, len: someLen, due: 10 + ExpungePeriod })),
      after(once, forgetIt, 11) == once,
      // Past the deadline: the request is gone.
      logsOf(once, forgetIt, 11 + ExpungePeriod) == List(),
      after(once, forgetIt, 11 + ExpungePeriod).get(victim).requests.keys() == Set(),
      // A never-provided request needs only one step.
      logsOf(Map(victim -> supervised(Map((someHash, someLen) -> Unprovided), Map(), 0)),
        forgetIt, 10) == List(),
      // Naming a request the target never made is refused.
      logsOf(emptyOurs, forgetIt, 10)
        == List(ServiceStoreFailed({ service: victim, error: StoreNotRequested })),
    }))
  }

  /// `solicit` either creates the request, rescues an unrequested one, or is
  /// refused because a live request already exists.
  run solicitOnServiceRescuesOrRefusesTest = {
    val unrequested: ForeignServices =
      Map(victim -> supervised(Map((someHash, someLen) -> Unrequested(3)), Map(), 0))
    val provided: ForeignServices =
      Map(victim -> supervised(Map((someHash, someLen) -> Provided), Map(), 0))
    init.then(expectThat(all {
      // Fresh request.
      logsOf(emptyOurs, solicitIt, 10) == List(),
      after(emptyOurs, solicitIt, 10).get(victim).requests.get((someHash, someLen))
        == Unprovided,
      // Rescue keeps the ORIGINAL unrequest slot, which still gates the forget.
      logsOf(unrequested, solicitIt, 10) == List(),
      after(unrequested, solicitIt, 10).get(victim).requests.get((someHash, someLen))
        == Rerequested(3),
      // Already live.
      logsOf(provided, solicitIt, 10)
        == List(ServiceSolicitFailed({ service: victim, error: AlreadySolicited })),
    }))
  }

  /// Handing supervision away is one-way: we immediately lose every operation
  /// on that service, `eject_service` included.
  run handoffLosesAccessTest = {
    val freeIt = SetServiceSupervisor({ service: victim, newSupervisor: victim })
    val freed = after(emptyOurs, freeIt, 10)
    init.then(expectThat(all {
      logsOf(emptyOurs, freeIt, 10) == List(),
      effectiveSupervisor(freed, victim) == victim,
      not(supervisedByUs(freed, victim)),
      logsOf(freed, ejectIt, 10)
        == List(ServiceEjectFailed({ service: victim, error: EjectNotSupervised })),
      logsOf(freed, freeIt, 10)
        == List(ServiceSupervisorFailed({ service: victim, error: HandoffNotSupervised })),
      // A new supervisor that does not exist is refused.
      logsOf(emptyOurs,
        SetServiceSupervisor({ service: victim, newSupervisor: stranger }), 10)
        == List(ServiceSupervisorFailed({
             service: victim, error: HandoffUnknownNewSupervisor })),
    }))
  }

  /// The replay of `m` against both this service's state and `fs`.
  pure def replayWith(
    s: ParachainServiceState, fs: ForeignServices, m: UpwardMessage, at: Timeslot
  ): AccumulateOutcome =
    applyUpwardMessage(accOverWith(s, fs), AssetHubParaId, FixedCodeLen, at, at, m)

  /// JAM funds a new service with exactly its threshold balance, from whichever
  /// of this service's balances the call names, and into whichever of the new
  /// service's it names.
  run createIsFundedByUsTest = {
    val out = replayForeign(Map(), AssetHubParaId, 5, createIt)
    val cost = foreignThreshold(out.foreign.get(victim))
    val intoSupervisor = replayForeign(Map(), AssetHubParaId, 5, CreateService({
      codeHash: someHash, len: FixedCodeLen, minItemGas: 0, minMemoGas: 0, id: 77,
      desiredId: None, sourceSupervisorBalance: false, newSupervisorBalance: true
    }))
    val fromSupervisor = replayForeign(Map(), AssetHubParaId, 5, CreateService({
      codeHash: someHash, len: FixedCodeLen, minItemGas: 0, minMemoGas: 0, id: 77,
      desiredId: None, sourceSupervisorBalance: true, newSupervisorBalance: false
    }))
    init.then(expectThat(all {
      // Its code request is its whole footprint.
      cost == BaseDeposit + 2 * ItemDeposit + PreimageRequestOverhead + FixedCodeLen,
      out.foreign.get(victim).account.balance == cost,
      out.state.jamAccount.balance == initialState.jamAccount.balance - cost,
      intoSupervisor.foreign.get(victim).account ==
        { balance: 0, supervisorBalance: cost, minMemoGas: 0 },
      // This service's supervisor balance starts out empty.
      fromSupervisor.logs == List(ServiceCreation({ id: 77, result: CannotAfford })),
      fromSupervisor.foreign == Map(),
    }))
  }

  /// Creating is refused when it would take this service below its threshold,
  /// and that is checked before whether a desired index is free.
  run createRefusedWhenWeCannotAffordTest = {
    val poor = { ...initialState, jamAccount:
      { ...initialState.jamAccount, balance: serviceThreshold(initialState) } }
    val wanted = MkServiceId(42)
    val taken: ForeignServices = Map(wanted -> supervised(Map(), Map(), 0))
    init.then(expectThat(all {
      replayWith(poor, Map(), createIt, 5).logs
        == List(ServiceCreation({ id: 77, result: CannotAfford })),
      replayWith(poor, Map(), createIt, 5).state == poor,
      replayWith(poor, taken, createWith(Some(wanted)), 5).logs
        == List(ServiceCreation({ id: 77, result: CannotAfford })),
    }))
  }

  /// Our empty-store service, holding the given balances.
  pure def funded(bal: Balance, sup: Balance): ForeignServices =
    Map(victim -> { ...supervised(Map(), Map(), 0),
      account: { balance: bal, supervisorBalance: sup, minMemoGas: 0 } })

  /// A request the target could not keep paying for is refused, and both of
  /// its balances count towards paying.
  run solicitRefusedWhenTargetCannotAffordTest = {
    val needed = BaseDeposit + 2 * ItemDeposit + PreimageRequestOverhead + someLen
    init.then(expectThat(all {
      logsOf(funded(needed - 1, 0), solicitIt, 10)
        == List(ServiceSolicitFailed({ service: victim, error: TargetCannotAfford })),
      after(funded(needed - 1, 0), solicitIt, 10) == funded(needed - 1, 0),
      logsOf(funded(needed, 0), solicitIt, 10) == List(),
      logsOf(funded(0, needed), solicitIt, 10) == List(),
    }))
  }

  /// Ejecting a service moves both its balances into this service's regular
  /// balance.
  run ejectCreditsBothBalancesTest = {
    val out = replayForeign(funded(300, 200), AssetHubParaId, 10, ejectIt)
    init.then(expectThat(all {
      out.foreign == Map(),
      out.state.jamAccount.balance == initialState.jamAccount.balance + 500,
    }))
  }

  /// Every one of them is Asset Hub only, while a `Parachain` target keeps the
  /// 6.1 rules: a para may name itself from anywhere.
  run serviceOpsAreAssetHubOnlyTest = {
    val solicitSelf =
      Solicit({ target: Parachain(CoretimeParaId), hash: someHash, len: someLen })
    init.then(expectThat(all {
      allOps.foldl(true, (ok, m) =>
        ok and violatesParachainRestrictions(List(m), CoretimeParaId)),
      allOps.foldl(true, (ok, m) =>
        ok and not(violatesParachainRestrictions(List(m), AssetHubParaId))),
      not(violatesParachainRestrictions(List(solicitSelf), CoretimeParaId)),
    }))
  }

  /// A `Service` target carries no para, so it must not be mistaken for a
  /// cross-para call; that check applies to `Parachain` targets only.
  run serviceOpsAreNotForeignParaTest = {
    init.then(expectThat(all {
      not(targetsForeignPara(forgetIt, AssetHubParaId)),
      not(targetsForeignPara(solicitIt, AssetHubParaId)),
      targetsForeignPara(
        Forget({ target: Parachain(CoretimeParaId), hash: someHash, len: someLen }),
        AssetHubParaId),
      targetsForeignPara(
        Solicit({ target: Parachain(CoretimeParaId), hash: someHash, len: someLen }),
        AssetHubParaId),
    }))
  }
}
