// Refine output types and the upward-message vocabulary the PVF can emit
// during a Refine invocation (design §3.3, §4.3).
//
// Each variant of `UpwardMessage` corresponds 1:1 to a side-effect host
// function in §4.3. Accumulate replays the list in order (§5.1 step 7).
module messages {
  import types.* from "./types"

  /// A validation code reference: its hash plus its byte length.
  type ValidationCodeRef = { hash: ValidationCodeHash, len: int }

  /// Does the `(hash, len)` pair of a `solicit`/`forget` name this validation
  /// code? BOTH components must match: the preimage registry is keyed by
  /// `(hash, len)`, so the same hash at a different length is a distinct
  /// preimage (§6.1).
  pure def refIs(ref: ValidationCodeRef, h: Hash, len: int): bool =
  h == vchAsHash(ref.hash) and len == ref.len

  // --- Refine-side log payloads -------------------------------------------

  /// Why Refine failed, as recorded in `parachain_log`.
  ///
  /// `Opaque` is the most important variant: it is the only one carrying a
  /// payload the parachain's own code chose, and so the only one conveying
  /// context the parachain can act on. The rest are valuable for debugging, but
  /// each is a fixed structural failure carrying no parachain-supplied detail.
  /// `entryRank` (state.qnt) ranks them accordingly.
  type RefineLog =
  // historical_lookup(validation_code_hash) returned None.
  | InvalidCodeHash
  // PVF aborted itself via report_error(data). Payload ≤ 1024 B.
  | Opaque(int)
  // set_validator_keys was called more than once in a single Refine invocation.
  | SetValidatorKeysRepeated
  // A SetValidatorKeys chunk carried more than `SetValidatorKeysMaxKeys` keys.
  | TooManyValidatorKeys
  // The PVF emitted more than `MaxUpwardMessages` upward messages
  // in a single Refine invocation (§4.3).
  | TooManyUpwardMessages
  // The parachain's 40 KiB upward-message budget was exceeded.
  | UpwardMessagesTooLarge
  // The PVF invoked a host function restricted to another parachain (Asset Hub
  // or the Coretime chain), or named a `para_id` it may not act for (§4.3).
  | RestrictedHostFunction
  // An `assign_core` queue outside 1..`AuthorizerQueueLength` hashes, and any
  // queue other than exactly `AuthorizerQueueLength` when handing the core to a
  // new assigner (§4.3).
  | InvalidAuthorizerQueue
  // The encoded digest plus the work-report's authorizer trace exceeds
  // the Gray Paper's 48 KiB combined-result-blob budget (§4.1 step 7).
  | RefineOutputTooLarge
  // The PVF exited without calling `set_parent_head_hash` and/or `set_head`
  // exactly once: both head declarations are mandatory (§4.2).
  | MissingHeadDeclaration
  // `set_head` was called with head data beyond the 4 KiB `HeadData` bound
  // (§4.3).
  | HeadDataTooLarge
  // `set_messages_streams_root` or `set_messages_requires_roots` was called
  // more than once in a single Refine invocation (§4.3, §8.1).
  | MessagesCallRepeated
  // A `set_messages_requires_roots` set exceeded `MaxRequiresSources`,
  // repeated a source `ParaId`, or named the candidate's own `para_id`
  // (§4.3, §8.1).
  | MessagesInvalidRequires

  // --- Accumulate-side log payloads ---------------------------------------

  // Why a state-balance reservation failed. `FromSolicit` carries the
  // preimage hash and length. Distinct constructor name because `Solicit` is
  // already an `UpwardMessage` label.
  type InsufficientBalanceReason =
  | FromSolicit({ hash: Hash, len: int })
  | FromSetKV({ keyHash: KvKeyHash })

  /// What a `forget` acts on: a parachain's share of this service's own store,
  /// or a supervised service's store.
  type Target =
  | Parachain(ParaId)
  | Service(ServiceId)

  /// Why a `forget` or `remove_service_storage` against a supervised service's
  /// store failed.
  type ServiceStoreError =
  | StoreUnknownService
  | StoreNotSupervised
  // `forget` naming a preimage the target never requested: JAM's transition
  // has no clause for an absent request, so it returns HUH.
  | StoreNotRequested

  /// Why a `Service`-targeted `solicit` failed.
  type ServiceSolicitError =
  | SolicitUnknownService
  | SolicitNotSupervised
  // Would leave the target below its threshold balance.
  | TargetCannotAfford
  // The target already has a live request not awaiting re-solicitation.
  | AlreadySolicited

  /// Why an `eject_service` failed.
  type ServiceEjectError =
  | EjectUnknownService
  | EjectNotSupervised
  // Still holds storage or preimage requests.
  | NotEmpty
  // Created in this timeslot.
  | CreatedThisSlot
  // The Parachain Service named itself.
  | TargetIsSelf

  /// Why a `set_service_supervisor` failed.
  type ServiceSupervisorError =
  | HandoffUnknownService
  // The proposed new supervisor does not exist.
  | HandoffUnknownNewSupervisor
  | HandoffNotSupervised

  /// How a `create_service` turned out. The only operation logging a success,
  /// since Asset Hub needs the assigned id announced to it.
  type ServiceCreationResult =
  | Created(ServiceId)
  | CannotAfford
  // A desiredId in the protected range that is already in use.
  | IdTaken

  /// Why a JAM `transfer` replaying a `TransferOut` failed.
  /// See §5.1 outgoing transfers.
  type TransferError =
  // source is not a known service.
  | UnknownSource
  // dest is not a known service.
  | UnknownDestination
  // The service is not source's effective supervisor; only its own regular
  // balance is exempt. Takes precedence over the next one.
  | SourceNotSupervised
  // A plain move to another service needs the service to be dest's effective
  // supervisor. Also covers an identity write (source == dest, selectors equal).
  | DestinationNotSupervised
  // The supplied gas is below dest's `min_memo_gas`.
  | GasBelowDestinationMinimum
  // The source service cannot cover the amount: the debited balance is too
  // small, or the transfer would leave it below its threshold balance.
  | InsufficientServiceBalance

  /// Why `parachain_set_state_balance` was rejected (§6.1).
  type StateBalanceRejection =
  // attempted < current_used.
  | BelowUsed({ currentTotal: Balance, currentUsed: Balance })
  // The para is deregistering (§6.4).
  | ParachainIsDeregistering

  type AccumulateLog =
  // Available state balance insufficient; see the reason for the cause.
  | InsufficientStateBalance(InsufficientBalanceReason)
  // parachain_set_state_balance(paraId, attempted) rejected; see the reason.
  | StateBalanceUpdateRejected({
      paraId: ParaId, attempted: Balance, reason: StateBalanceRejection
  })
  // JAM `designate` rejected the assembled validator-key set (its length is
  // not in `valcount`, i.e. not a multiple of 3 in [6, 1023]); the staging
  // buffer is cleared regardless. `len` is the rejected length. See §5.3.
  | DesignateRejected({ len: int })
  // A `set_validator_keys` chunk would grow `staged_validator_keys` beyond its
  // reserved capacity (`MaxStagedValidatorKeys`); the append is rejected and the
  // buffer is left unchanged. See §5.3.
  | StagedValidatorKeysOverflow
  // `parachain_service_upgrade(code_hash, ...)` was rejected because the
  // new code's preimage is not in the Parachain Service's preimage store
  // (§5.4).
  | ServiceUpgradePreimageMissing({ codeHash: Hash })
  // The JAM `transfer` call replaying a `TransferOut` upward message
  // returned an error. `id` is the caller-supplied identifier from the
  // `TransferOut`, echoed back so Asset Hub can match the failure to its
  // request. See §5.1 step 7.
  | TransferFailed({ id: int, error: TransferError })
  // A `forget` removed the last referencer without expunging the preimage;
  // the caller must `forget(hash, len)` again at `due` to free it. See §6.1.
  | ForgetAgainAt({ hash: Hash, len: int, due: Timeslot })
  // Failures of the supervised-service operations, one per operation.
  | ServiceStoreFailed({ service: ServiceId, error: ServiceStoreError })
  | ServiceSolicitFailed({ service: ServiceId, error: ServiceSolicitError })
  | ServiceEjectFailed({ service: ServiceId, error: ServiceEjectError })
  | ServiceSupervisorFailed({ service: ServiceId, error: ServiceSupervisorError })
  // Announces a CreateService outcome to Asset Hub. `id` is the caller-supplied
  // identifier from the CreateService, echoed back so Asset Hub can match the
  // outcome to its request.
  | ServiceCreation({ id: int, result: ServiceCreationResult })
  // `parachain_clean_up` rejected: the parachain still holds state beyond its
  // baseline and validation code(s) and must release the rest first. See §6.4.
  | TooMuchStateHeld

  /// A JAM `assign` host-function call recorded during a step. JAM's `assign`
  /// (gray paper `Omega_A`) writes the core's authorizer queue AND its assigner
  /// atomically.
  type AssignCall = {
    core: CoreIndex,
    assigner: ServiceId,
    queue: List[AuthorizerHash]
  }

  // --- Upward messages emitted by the PVF, replayed by Accumulate ---------

  type UpwardMessage =
    // request_code_upgrade(hash, len): start a PVF code upgrade (§5.2).
    | RequestCodeUpgrade({ hash: ValidationCodeHash, len: int })
  // solicit(hash, len)
  // solicit(target, hash, len): §6.1 for a Parachain target.
  | Solicit({ target: Target, hash: Hash, len: int })
  // eject_service(service).
  | EjectService({ service: ServiceId })
  // set_service_supervisor(service, newSupervisor).
  | SetServiceSupervisor({ service: ServiceId, newSupervisor: ServiceId })
  // `id` is a caller-supplied identifier, echoed back in the ServiceCreation
  // entry so Asset Hub can match the outcome to its request.
  | CreateService({
      codeHash: Hash, len: int, minItemGas: int, minMemoGas: int, id: int,
      // Index to create the service at, in JAM's protected range.
      desiredId: Option[ServiceId],
      sourceSupervisorBalance: bool, newSupervisorBalance: bool
    })
  // forget(para_id, hash, len). `paraId` names whose reference is released; a
  // para may only name itself, while the Coretime chain may name any para, which
  // is how a para that can no longer produce blocks gets drained (§6.4).
  // forget(target, hash, len): §6.1 for a Parachain target.
  | Forget({ target: Target, hash: Hash, len: int })
  // remove_service_storage(service, key).
  | RemoveServiceStorage({ service: ServiceId, key: List[int] })
  // kv_set(key, value): write to key_value_storage.
  | SetKV({ key: List[int], value: List[int] })
  // kv_remove(para_id, key): remove from key_value_storage. Same `paraId`
  // delegation rule as `Forget`.
  | RemoveKV({ paraId: ParaId, key: List[int] })
  // transfer_out(source, dest, amount, id, sourceSupervisorBalance,
  // destSupervisorBalance, deferred): Asset Hub only.
  // `deferred` is `None` for a plain move and `Some((memo, gas))` for a deferred
  // transfer; the two are inseparable because JAM ignores the gas limit when no
  // memo is supplied. `source = None` means this service, matching JAM's self
  // sentinel. The two selectors choose the balance on each side: which of
  // source's is debited and which of dest's receives the funds; true means the
  // supervisor balance (§5.1 outgoing transfers). `id` is a caller-supplied
  // identifier, echoed back in the TransferFailed entry so Asset Hub can match
  // a failure to its request.
  | TransferOut({
      source: Option[ServiceId], dest: ServiceId, amount: Balance, id: int,
      sourceSupervisorBalance: bool, destSupervisorBalance: bool,
      deferred: Option[(Memo, int)]
    })
  // assign_core(core, queue, new_assigner, jam_slot): Coretime only. Mirrors
  // JAM's `assign`, which writes a core's authorizer queue AND its assigner
  // atomically, so one call covers both.
  //
  // - `jamSlot`: when to apply. The entry is cached and forwarded in the
  //   always-accumulate phase once the timeslot reaches `jamSlot`; if already
  //   due (`jamSlot <= now`) it is applied inline. The Coretime chain chooses
  //   `jamSlot` so the pool fills without overwriting the active authorizers.
  // - `queue`: 1 to `AuthorizerQueueLength` hashes; Refine rejects any other
  //   length, empty included (§4.3).
  // - `newAssigner`: `None` leaves this service as the core's assigner (the
  //   common queue-rotation case); `Some(s)` hands the core to `s`. A handoff is
  //   one-way: once it fires the service is no longer `assigners[core]`, so it
  //   cannot re-present a short queue afterwards, so `Some(s)` requires
  //   an exactly-`AuthorizerQueueLength` queue (§4.3). JAM always stores SOME
  //   assigner, so `None` means "unchanged": the service passes its own id.
  | AssignCore({
      core: CoreIndex,
      queue: List[AuthorizerHash],
      newAssigner: Option[ServiceId],
      jamSlot: Timeslot
  })
  // set_validator_keys(chunk, is_last): Asset Hub only.
  | SetValidatorKeys({ keys: List[ValidatorKey], isLast: bool })
  // clean_up_buckets_up_to(bucket_id): Asset Hub only.
  | CleanUpBucketsUpTo(BucketId)
  // parachain_set_head(para_id, new_head): Coretime only.
  | ParachainSetHead({ paraId: ParaId, newHead: HeadData })
  // parachain_set_validation_code(para_id, new_hash, new_len): Coretime only.
  | ParachainSetValidationCode({
      paraId: ParaId,
      newValidationCodeHash: ValidationCodeHash,
      newValidationCodeLen: int
  })
  // parachain_clean_up(para_id): Coretime only.
  | ParachainCleanUp(ParaId)
  // parachain_set_state_balance(para_id, new_total): Coretime only.
  | ParachainSetStateBalance({ paraId: ParaId, newTotal: Balance })
  // parachain_service_upgrade(code_hash, len, min_acc_gas, min_memo_gas):
  // Asset Hub only (design §5.4).
  | UpgradeService({ codeHash: Hash, len: int, minAccGas: int, minMemoGas: int })

  // --- Refine output per work item ---------------------------------------

  /// A successful Refine output for one parachain candidate.
  type ParachainWorkDigestOk = {
    paraId: ParaId,
    validationCode: ValidationCodeRef,
    parentHeadHash: HeadHash,
    headData: HeadData,
    upwardMessages: List[UpwardMessage],
    lookupAnchor: Timeslot,
    // §8: the `StreamsRoot` this candidate publishes, `None` when it declared
    // none. Pushed into the parachain's settlement ring on enactment (§5.1
    // step 6), so a root enters the ring only for an enacted candidate.
    messagesStreamsRoot: Option[StreamsRoot],
    // §8: the senders this candidate consumed messages from, one entry per
    // sender with the root it consumed against. Checked against the senders'
    // settlement rings before the head write (§5.1 step 6).
    messagesRequiresRoots: List[(ParaId, StreamsRoot)]
  }

  /// Refine output per work item: a success digest or a Refine error.
  type ParachainWorkDigest =
  | Ok(ParachainWorkDigestOk)
  | Err({ paraId: ParaId, error: RefineLog })

  /// Gray paper `WorkExecResult`. `WorkOk` carries the service-produced
  /// `ParachainWorkDigest`; `WorkErr` is the JAM-substituted error case
  /// (gray paper `WorkExecResult::Error(WorkErrorCode)`) which Accumulate
  /// skips entirely: no `parachain_log` entry, no state change (§3.3).
  /// Constructor names are prefixed to avoid Quint's module-wide name
  /// collision with `ParachainWorkDigest::Ok` / `Err`.
  type WorkExecResult =
  | WorkOk(ParachainWorkDigest)
  | WorkErr

  /// Gray paper per-work-item record consumed by Accumulate. `result`
  /// carries the outcome (service digest or JAM-substituted error);
  /// `authTrace` is the work-report authorizer trace (one per report
  /// in JAM, but per-item here since the model fixes one work-item per
  /// work-report, design §3.2).
  type WorkResult = {
    result: WorkExecResult,
    authTrace: AuthTrace
  }

  /// The originating authorizer trace from the work-report, carried by Err
  /// results into the parachain_log (design §3.3).
  type AuthTrace = int

  /// Service-chosen cap on the auth-trace bytes stored in `parachain_log`.
  pure val StoredAuthTraceCap: int = 256

  /// Truncate an auth trace to `StoredAuthTraceCap` before the service stores
  /// it in a `RefineLogEntry` (design §3.3).
  pure def truncateAuthTrace(t: AuthTrace): AuthTrace =
  if (t > StoredAuthTraceCap) StoredAuthTraceCap else t

  // --- Per-parachain log entries (design §3.1) ---------------------------

  type LogEntry =
  | RefineLogEntry({ error: RefineLog, authTrace: AuthTrace })
  | AccumulateLogEntry(List[AccumulateLog])

  // --- Helpers ------------------------------------------------------------

  /// Whether the host function backing this upward message is restricted
  /// to Asset Hub (design §4.3).
  pure def isAssetHubOnly(m: UpwardMessage): bool =
  match m {
    | TransferOut(_) => true
    | SetValidatorKeys(_) => true
    | CleanUpBucketsUpTo(_) => true
    | UpgradeService(_) => true
    | RemoveServiceStorage(_) => true
    | EjectService(_) => true
    | CreateService(_) => true
    | SetServiceSupervisor(_) => true
    // Only a Service target; a Parachain target keeps the §6.1 rules.
    | Forget(p) => match p.target { | Service(_) => true | Parachain(_) => false }
    | Solicit(p) => match p.target { | Service(_) => true | Parachain(_) => false }
    | _ => false
  }

  /// Whether the host function backing this upward message is restricted
  /// to the Coretime chain (design §4.3, §6).
  pure def isCoretimeOnly(m: UpwardMessage): bool =
  match m {
    | AssignCore(_) => true
    | ParachainSetHead(_) => true
    | ParachainSetValidationCode(_) => true
    | ParachainCleanUp(_) => true
    | ParachainSetStateBalance(_) => true
    | _ => false
  }

  /// Per §4.3: a host function taking a `para_id` aborts unless it names the
  /// calling parachain, except from the Coretime chain, which may name any para
  /// (§6.4). `true` iff `m` names a para other than `para_id` without that right.
  pure def targetsForeignPara(m: UpwardMessage, para_id: ParaId): bool =
  para_id != CoretimeParaId and
  (match m {
    | Forget(p) => match p.target {
      | Parachain(target) => target != para_id
      | Service(_) => false
    }
    // Same rule as `Forget`: a para may only name itself, so it cannot charge
    // another's balance, while Coretime may name any para.
    | Solicit(p) => match p.target {
      | Parachain(target) => target != para_id
      | Service(_) => false
    }
    | RemoveKV(p) => p.paraId != para_id
    | _ => false
  })

  /// Per §4.3: host functions restricted to a specific parachain (Asset Hub
  /// or the Coretime chain) abort when invoked from any other parachain, as does
  /// naming a foreign `para_id` without the right to.
  /// Refine enforces this as a PVF-level panic; Accumulate independently
  /// verifies and rejects the work-package as defense-in-depth.
  pure def violatesParachainRestrictions(
    msgs: List[UpwardMessage],
    para_id: ParaId
  ): bool =
  msgs.foldl(false, (acc, m) =>
    acc or
    (isAssetHubOnly(m) and para_id != AssetHubParaId) or
    (isCoretimeOnly(m) and para_id != CoretimeParaId) or
    targetsForeignPara(m, para_id))
}
