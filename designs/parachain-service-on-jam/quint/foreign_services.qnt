// Supervised JAM services (design 6.5).
//
// These are JAM accounts rather than Parachain Service storage, so they sit
// beside `svc` instead of inside it. Only what the 6.5 operations turn on is
// modelled: existence, the supervisor link, the store, and the creation slot.
// Balances stay abstract, as this service's own always have, so the two
// affordability outcomes (`TargetCannotAfford`, `CannotAfford`) are the only
// ones left undecided.
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
    storage: Set[List[int]]
  }

  type ForeignServices = ServiceId -> ForeignService

  /// Outcome of one operation: the services afterwards, plus the entry to log.
  type ForeignOutcome = {
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

  /// `items` is zero exactly when both halves of the store are empty, which is
  /// what `eject` requires.
  pure def isEmptyService(f: ForeignService): bool =
    f.requests.keys() == Set() and f.storage == Set()

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
      { services: fs.put(s, { ...f, storage: f.storage.exclude(Set(key)) }), log: None }
    }
  }

  // --- solicit -------------------------------------------------------------

  /// A `Service`-targeted `solicit`. Whether the target can afford the new
  /// footprint is not modelled, so `TargetCannotAfford` never appears here.
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
    if (not(f.requests.keys().contains(key)))
      { services: fs.put(s, { ...f, requests: f.requests.put(key, Unprovided) }), log: None }
    else match f.requests.get(key) {
      // The rescue: an unrequested blob JAM still holds comes back.
      | Unrequested(since) =>
        { services: fs.put(s, { ...f, requests: f.requests.put(key, Rerequested(since)) }),
          log: None }
      | _ =>
        unchanged(fs, ServiceSolicitFailed({ service: s, error: AlreadySolicited }))
    }
  }

  // --- eject ---------------------------------------------------------------

  /// Destroying an empty supervised service. Its balances would come to this
  /// service, which is not modelled.
  pure def foreignEject(
    fs: ForeignServices, s: ServiceId, now: Timeslot
  ): ForeignOutcome =
  if (s == ParachainServiceId)
    unchanged(fs, ServiceEjectFailed({ service: s, error: TargetIsSelf }))
  else if (not(fs.keys().contains(s)))
    unchanged(fs, ServiceEjectFailed({ service: s, error: EjectUnknownService }))
  else if (effectiveSupervisor(fs, s) != ParachainServiceId)
    unchanged(fs, ServiceEjectFailed({ service: s, error: EjectNotSupervised }))
  else if (fs.get(s).created == now)
    unchanged(fs, ServiceEjectFailed({ service: s, error: CreatedThisSlot }))
  else if (not(isEmptyService(fs.get(s))))
    unchanged(fs, ServiceEjectFailed({ service: s, error: NotEmpty }))
  else { services: fs.mapRemove(s), log: None }

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
  /// available. Whether this service can fund it is not modelled, so
  /// `CannotAfford` never appears.
  pure def foreignCreate(
    fs: ForeignServices, codeHash: Hash, len: int, id: int,
    desiredId: Option[ServiceId], now: Timeslot
  ): ForeignOutcome = {
    val fresh = resolveNewId(fs, desiredId)
    if (createdOf(fs, fresh) != None)
      // Only reachable for a protected `desiredId`: the allocator never
      // returns a live index.
      unchanged(fs, ServiceCreation({ id: id, result: IdTaken }))
    else
      { services: fs.put(fresh, {
          supervisor: ParachainServiceId,
          supervisorCreated: ParachainServiceCreated,
          created: now,
          requests: Map((codeHash, len) -> Unprovided),
          storage: Set()
        }),
        log: Some(ServiceCreation({ id: id, result: Created(fresh) })) }
  }
}
