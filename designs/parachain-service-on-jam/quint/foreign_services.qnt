// JAM services other than this one (design 6.5, and `TransferOut` in 5.1).
//
// These are JAM accounts rather than Parachain Service storage, so they sit
// beside `svc` instead of inside it. Only what those operations turn on is
// modelled: existence, the supervisor link, the store, the creation slot and
// the account. This service's own account is `svc.jamAccount`.
module foreign_services {
  import types.* from "./types"
  import messages.* from "./messages"
  import state.* from "./state"

  type ForeignService = {
    // Both fields are needed to resolve the effective supervisor.
    supervisor: ServiceId,
    supervisorCreated: Timeslot,
    created: Timeslot,
    // Preimage requests, sharing the `RequestStatus` machine of 6.1.
    requests: (Hash, int) -> RequestStatus,
    // Storage key -> value.
    storage: List[int] -> List[int],
    account: JamAccount
  }

  type ForeignServices = ServiceId -> ForeignService

  /// Outcome of one operation: the services afterwards, plus the entry to log.
  type ForeignOutcome = {
    services: ForeignServices,
    log: Option[AccumulateLog]
  }

  /// Outcome of an operation that also moves this service's own balance.
  type FundedOutcome = {
    state: ParachainServiceState,
    services: ForeignServices,
    log: Option[AccumulateLog]
  }

  pure def unchanged(fs: ForeignServices, l: AccumulateLog): ForeignOutcome =
    { services: fs, log: Some(l) }

  /// Stands in for the Parachain Service's own `created` slot. Resolving the
  /// effective supervisor of a service we created means comparing our `created`
  /// against the `supervisorCreated` that service recorded, and the Parachain
  /// Service is not itself in the map. The design never says when the service
  /// was created and the value is never observed, so any fixed anchor does: all
  /// that matters is that `createdOf` and `foreignCreate` agree on it. A
  /// modelling artifact, which is why it is here rather than with the protocol
  /// constants in `types.qnt`.
  pure val ParachainServiceCreated: Timeslot = 0

  /// Creation slot of any service the model knows, including this one. 
  pure def createdOf(fs: ForeignServices, s: ServiceId): Option[Timeslot] =
  if (s == ParachainServiceId) Some(ParachainServiceCreated)
  else if (fs.keys().contains(s)) Some(fs.get(s).created)
  else None

  /// The designated supervisor if it exists and its creation slot still
  /// matches, else the service itself. Mirrors the gray paper's fallback, which
  /// is what stops an ejected supervisor's reused id from inheriting power.
  pure def effectiveSupervisor(fs: ForeignServices, s: ServiceId): ServiceId = {
    val f = fs.get(s)
    match createdOf(fs, f.supervisor) {
      | Some(c) => if (c == f.supervisorCreated) f.supervisor else s
      | None => s
    }
  }

  pure def supervisedByUs(fs: ForeignServices, s: ServiceId): bool =
    fs.keys().contains(s) and effectiveSupervisor(fs, s) == ParachainServiceId

  /// Whether this service may move `s`'s funds: it is `s`, or `s`'s effective
  /// supervisor. The design names no supervisor for this service, so it is its
  /// own.
  pure def controlsService(fs: ForeignServices, s: ServiceId): bool =
    s == ParachainServiceId or supervisedByUs(fs, s)

  /// `items` is zero exactly when both halves of the store are empty, which is
  /// what `eject` requires.
  pure def isEmptyService(f: ForeignService): bool =
    f.requests.keys() == Set() and f.storage.keys() == Set()

  /// JAM's threshold balance for `f` (gray paper, *Account Footprint and
  /// Threshold Balance*). No modelled service has gratis storage.
  pure def foreignThreshold(f: ForeignService): Balance = {
    val items = 2 * f.requests.keys().size() + f.storage.keys().size()
    val requestOctets =
      f.requests.keys().fold(0, (acc, k) => acc + PreimageRequestOverhead + k._2)
    val storageOctets = f.storage.keys().fold(0, (acc, k) =>
      acc + StorageEntryOverhead + k.length() + f.storage.get(k).length())
    BaseDeposit + ItemDeposit * items + requestOctets + storageOctets
  }

  /// The account and threshold balance of `id`, which must be this service or
  /// one in `fs`.
  pure def accountOf(s: ParachainServiceState, fs: ForeignServices, id: ServiceId): JamAccount =
    if (id == ParachainServiceId) s.jamAccount else fs.get(id).account

  pure def thresholdOf(s: ParachainServiceState, fs: ForeignServices, id: ServiceId): Balance =
    if (id == ParachainServiceId) serviceThreshold(s) else foreignThreshold(fs.get(id))

  /// Replace `id`'s account.
  pure def withAccount(
    s: ParachainServiceState, fs: ForeignServices, id: ServiceId, a: JamAccount
  ): (ParachainServiceState, ForeignServices) =
    if (id == ParachainServiceId) ({ ...s, jamAccount: a }, fs)
    else (s, fs.put(id, { ...fs.get(id), account: a }))

  // --- forget / remove_service_storage ------------------------------------

  /// Shared guard for the two store operations: it must exist and we must
  /// supervise it.
  pure def storeGuard(fs: ForeignServices, s: ServiceId): Option[ServiceStoreError] =
  if (not(fs.keys().contains(s))) Some(StoreUnknownService)
  else if (effectiveSupervisor(fs, s) != ParachainServiceId) Some(StoreNotSupervised)
  else None

  /// A `Service`-targeted `forget`. The two-step expunge of 6.1 applies here
  /// unchanged, so an available preimage is only unrequested and must be
  /// forgotten again after the expunge period.
  pure def foreignForget(
    fs: ForeignServices, s: ServiceId, h: Hash, len: int, now: Timeslot
  ): ForeignOutcome =
  match storeGuard(fs, s) {
    | Some(e) => unchanged(fs, ServiceStoreFailed({ service: s, error: e }))
    | None => {
      val f = fs.get(s)
      val key = (h, len)
      if (not(f.requests.keys().contains(key)))
        // Absent: the gray paper's `forget` has no case for it and refuses.
        unchanged(fs, ServiceStoreFailed({ service: s, error: StoreNotRequested }))
      else {
        val dropped = { services: fs.put(s, { ...f, requests: f.requests.mapRemove(key) }),
                        log: None }
        match f.requests.get(key) {
          // Never provided, so one step frees it.
          | Unprovided => dropped
          // Available: unrequest only, and say when to come back.
          | Provided =>
            { services: fs.put(s, { ...f, requests: f.requests.put(key, Unrequested(now)) }),
              log: Some(ForgetAgainAt({ hash: h, len: len, due: (now + ExpungePeriod) })) }
          | Unrequested(since) =>
            if (now > (since + ExpungePeriod)) dropped
            else { services: fs,
                   log: Some(ForgetAgainAt({ hash: h, len: len, due: (since + ExpungePeriod) })) }
          // Rescued: gated on the ORIGINAL unrequest slot, and never expunges
          // here; past the period it unrequests again on a fresh clock.
          | Rerequested(since) =>
            if (now > (since + ExpungePeriod))
              { services: fs.put(s, { ...f, requests: f.requests.put(key, Unrequested(now)) }),
                log: Some(ForgetAgainAt({ hash: h, len: len, due: (now + ExpungePeriod) })) }
            else { services: fs,
                   log: Some(ForgetAgainAt({ hash: h, len: len, due: (since + ExpungePeriod) })) }
        }
      }
    }
  }

  /// Deleting one storage key. Idempotent, and it only ever shrinks the
  /// footprint, so it has no affordability failure.
  pure def foreignRemoveStorage(
    fs: ForeignServices, s: ServiceId, key: List[int]
  ): ForeignOutcome =
  match storeGuard(fs, s) {
    | Some(e) => unchanged(fs, ServiceStoreFailed({ service: s, error: e }))
    | None => {
      val f = fs.get(s)
      { services: fs.put(s, { ...f, storage: f.storage.mapRemove(key) }), log: None }
    }
  }

  // --- solicit -------------------------------------------------------------

  /// A `Service`-targeted `solicit`.
  pure def foreignSolicit(
    fs: ForeignServices, s: ServiceId, h: Hash, len: int, now: Timeslot
  ): ForeignOutcome =
  if (not(fs.keys().contains(s)))
    unchanged(fs, ServiceSolicitFailed({ service: s, error: SolicitUnknownService }))
  else if (effectiveSupervisor(fs, s) != ParachainServiceId)
    unchanged(fs, ServiceSolicitFailed({ service: s, error: SolicitNotSupervised }))
  else {
    val f = fs.get(s)
    val key = (h, len)
    // The target once the request is made, `None` while a live one exists.
    val requested: Option[ForeignService] =
      if (not(f.requests.keys().contains(key)))
        Some({ ...f, requests: f.requests.put(key, Unprovided) })
      else match f.requests.get(key) {
        // The rescue: an unrequested blob JAM still holds comes back.
        | Unrequested(since) => Some({ ...f, requests: f.requests.put(key, Rerequested(since)) })
        | _ => None
      }
    match requested {
      | None => unchanged(fs, ServiceSolicitFailed({ service: s, error: AlreadySolicited }))
      | Some(g) =>
        if (isOverdrawn(g.account, foreignThreshold(g)))
          unchanged(fs, ServiceSolicitFailed({ service: s, error: TargetCannotAfford }))
        else { services: fs.put(s, g), log: None }
    }
  }

  // --- eject ---------------------------------------------------------------

  /// Destroying an empty supervised service. Both its balances come to this
  /// service's regular balance.
  pure def foreignEject(
    s: ParachainServiceState, fs: ForeignServices, target: ServiceId, now: Timeslot
  ): FundedOutcome = {
    val error =
      if (target == ParachainServiceId) Some(TargetIsSelf)
      else if (not(fs.keys().contains(target))) Some(EjectUnknownService)
      else if (effectiveSupervisor(fs, target) != ParachainServiceId) Some(EjectNotSupervised)
      else if (fs.get(target).created == now) Some(CreatedThisSlot)
      else if (not(isEmptyService(fs.get(target)))) Some(NotEmpty)
      else None
    match error {
      | Some(e) =>
        { state: s, services: fs, log: Some(ServiceEjectFailed({ service: target, error: e })) }
      | None => {
        val a = fs.get(target).account
        { state: { ...s, jamAccount: s.jamAccount.credit(a.balance + a.supervisorBalance, false) },
          services: fs.mapRemove(target),
          log: None }
      }
    }
  }

  // --- set_service_supervisor ---------------------------------------------

  /// Handing a service to another supervisor, or to itself to set it free.
  pure def foreignSetSupervisor(
    fs: ForeignServices, s: ServiceId, u: ServiceId
  ): ForeignOutcome =
  if (not(fs.keys().contains(s)))
    unchanged(fs, ServiceSupervisorFailed({ service: s, error: HandoffUnknownService }))
  else if (createdOf(fs, u) == None)
    unchanged(fs, ServiceSupervisorFailed({
      service: s, error: HandoffUnknownNewSupervisor
    }))
  else if (effectiveSupervisor(fs, s) != ParachainServiceId)
    unchanged(fs, ServiceSupervisorFailed({ service: s, error: HandoffNotSupervised }))
  else {
    val f = fs.get(s)
    val newCreated = match createdOf(fs, u) { | Some(c) => c | None => 0 }
    { services: fs.put(s, { ...f, supervisor: u, supervisorCreated: newCreated }),
      log: None }
  }

  // --- create_service ------------------------------------------------------

  /// The id JAM would assign: the lowest public index this model has not used.
  /// The gray paper's allocator wanders, but only freshness is observable.
  pure def nextServiceId(fs: ForeignServices): ServiceId =
  MkServiceId(
    MinPublicServiceIndex.to(MinPublicServiceIndex + 64)
      .filter(i => not(fs.keys().contains(MkServiceId(i))))
      .fold(MinPublicServiceIndex + 64, (acc, i) => if (i < acc) i else acc))

  /// The index a create lands at.
  pure def resolveNewId(fs: ForeignServices, desiredId: Option[ServiceId]): ServiceId =
  match desiredId {
    | None => nextServiceId(fs)
    | Some(want) => want
  }

  /// Creating a service supervised by this one. JAM sets the new service's
  /// supervisor to its creator, which is what makes every other operation here
  /// available, and funds it with exactly its threshold balance, taken from
  /// this service.
  pure def foreignCreate(
    s: ParachainServiceState, fs: ForeignServices, c: CreateServiceArgs, now: Timeslot
  ): FundedOutcome = {
    val fresh = resolveNewId(fs, c.desiredId)
    val created: ForeignService = {
      supervisor: ParachainServiceId,
      supervisorCreated: ParachainServiceCreated,
      created: now,
      requests: Map((c.codeHash, c.len) -> Unprovided),
      storage: Map(),
      account: { balance: 0, supervisorBalance: 0, minMemoGas: c.minMemoGas }
    }
    val cost = foreignThreshold(created)
    val funder = s.jamAccount.debit(cost, c.sourceSupervisorBalance)
    if (isOverdrawn(funder, serviceThreshold(s)))
      { state: s, services: fs, log: Some(ServiceCreation({ id: c.id, result: CannotAfford })) }
    else if (createdOf(fs, fresh) != None)
      // Only reachable for a protected `desiredId`: the allocator never
      // returns a live index.
      { state: s, services: fs, log: Some(ServiceCreation({ id: c.id, result: IdTaken })) }
    else
      { state: { ...s, jamAccount: funder },
        services: fs.put(fresh, {
          ...created, account: created.account.credit(cost, c.newSupervisorBalance)
        }),
        log: Some(ServiceCreation({ id: c.id, result: Created(fresh) })) }
  }

  // --- transfer (5.1) ------------------------------------------------------

  pure def transferFailed(
    s: ParachainServiceState, fs: ForeignServices, t: TransferOutArgs, e: TransferError
  ): FundedOutcome =
    { state: s, services: fs, log: Some(TransferFailed({ id: t.id, error: e })) }

  /// Replay of a `TransferOut`: JAM's `transfer`, called by this service.
  pure def transferOut(
    s: ParachainServiceState, fs: ForeignServices, t: TransferOutArgs
  ): FundedOutcome = {
    val source = t.source.optionGetOr(ParachainServiceId)
    val plainMove = t.deferred == None
    val ontoItself = source == t.dest and t.sourceSupervisorBalance == t.destSupervisorBalance
    if (createdOf(fs, source) == None) transferFailed(s, fs, t, UnknownSource)
    else if (createdOf(fs, t.dest) == None) transferFailed(s, fs, t, UnknownDestination)
    else if (not(controlsService(fs, source))) transferFailed(s, fs, t, SourceNotSupervised)
    // Only a plain move writes `dest` directly.
    else if (plainMove and (ontoItself or not(controlsService(fs, t.dest))))
      transferFailed(s, fs, t, DestinationNotSupervised)
    else if (match t.deferred {
               | Some(d) => d._2 < accountOf(s, fs, t.dest).minMemoGas
               | None => false
             })
      transferFailed(s, fs, t, GasBelowDestinationMinimum)
    else {
      val afterDebit = withAccount(s, fs, source,
        accountOf(s, fs, source).debit(t.amount, t.sourceSupervisorBalance))
      val afterCredit = withAccount(afterDebit._1, afterDebit._2, t.dest,
        accountOf(afterDebit._1, afterDebit._2, t.dest).credit(t.amount, t.destSupervisorBalance))
      // JAM checks `source` after a plain move's credit but before a deferred
      // transfer's, which only matters when `source` is `dest`.
      val checked = if (plainMove) afterCredit else afterDebit
      if (isOverdrawn(accountOf(checked._1, checked._2, source), thresholdOf(s, fs, source)))
        transferFailed(s, fs, t, InsufficientServiceBalance)
      // JAM credits a deferred transfer when `dest` accumulates later in the
      // block, which the model folds into this step. One to this service is
      // therefore not recorded in `incomingTransfers`.
      else { state: afterCredit._1, services: afterCredit._2, log: None }
    }
  }
}
