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
  // A `set_validator_keys` chunk exceeded 30 keys, or
  // `set_validator_keys` was called more than once in a single
  // Refine invocation.
  | SetValidatorKeysTooManyKeys
  // The PVF emitted more than `MaxUpwardMessages` (1024) upward messages
  // in a single Refine invocation (§4.3).
  | TooManyUpwardMessages
  // The PVF invoked a host function restricted to another parachain (Asset Hub
  // or the Coretime chain), or named a `para_id` it may not act for (§4.3).
  | RestrictedHostFunction
  // The authorizer config's `authorized_paras` prefix has a different
  // length than the work package's items (§4.1 step 1).
  | AuthConfigMismatch
  // The work package has 0 items or more than 1 item; only single-item
  // packages are currently supported (§3.2).
  | InvalidItemCount
  // The opaque `AuthorizerConfig` blob failed to decode (§4.1 step 1).
  | MalformedAuthorizerConfig
  // The encoded digest plus the work-report's authorizer trace exceeds
  // the Gray Paper's 48 KiB combined-result-blob budget (§4.1 step 7).
  | RefineOutputTooLarge
  // The PVF exited without calling `set_parent_head_hash` and/or `set_head`
  // exactly once — both head declarations are mandatory (§4.2).
  | MissingHeadDeclaration

  // --- Accumulate-side log payloads ---------------------------------------

  // Why a state-balance reservation failed. `FromSolicit` carries the
  // preimage hash and length. Distinct constructor name because `Solicit` is
  // already an `UpwardMessage` label.
  type InsufficientBalanceReason =
  | FromSolicit({ hash: Hash, len: int })
  | FromSetKV({ keyHash: Hash })

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
  // The transfer would leave the service below its threshold balance.
  | InsufficientServiceBalance

  type AccumulateLog =
  // Available state balance insufficient — see the reason for the cause.
  | InsufficientStateBalance(InsufficientBalanceReason)
  // parachain_set_state_balance rejected because attempted < current_used.
  | StateBalanceUpdateRejected({
      attempted: Balance,
      currentTotal: Balance,
      currentUsed: Balance
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
  // `TransferOut`, echoed back so the parachain can match the failure to its
  // own record. See §5.1 step 7.
  | TransferFailed({ id: int, error: TransferError })
  // A `forget` removed the last referencer without expunging the preimage;
  // the caller must `forget(hash, len)` again at `due` to free it. See §6.1.
  | ForgetAgainAt({ hash: Hash, len: int, due: Timeslot })
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
    // request_code_upgrade(hash, len) — start a PVF code upgrade (§5.2).
    | RequestCodeUpgrade({ hash: ValidationCodeHash, len: int })
  // solicit(hash, len)
  | Solicit({ hash: Hash, len: int })
  // forget(para_id, hash, len). `paraId` names whose reference is released; a
  // para may only name itself, while the Coretime chain may name any para, which
  // is how a para that can no longer produce blocks gets drained (§6.4).
  | Forget({ paraId: ParaId, hash: Hash, len: int })
  // kv_set(key, value) — write to key_value_storage.
  | SetKV({ key: List[int], value: List[int] })
  // kv_remove(para_id, key) — remove from key_value_storage. Same `paraId`
  // delegation rule as `Forget`.
  | RemoveKV({ paraId: ParaId, key: List[int] })
  // transfer_out(source, dest, amount, id, sourceSupervisorBalance,
  // destSupervisorBalance, deferred) — Asset Hub only.
  // `deferred` is `None` for a plain move and `Some((memo, gas))` for a deferred
  // transfer; the two are inseparable because JAM ignores the gas limit when no
  // memo is supplied. `source = None` means this service, matching JAM's self
  // sentinel. The two selectors choose the balance on each side — which of
  // source's is debited and which of dest's receives the funds; true means the
  // supervisor balance (§5.1 outgoing transfers).
  | TransferOut({
      source: Option[ServiceId], dest: ServiceId, amount: Balance, id: int,
      sourceSupervisorBalance: bool, destSupervisorBalance: bool,
      deferred: Option[(Memo, int)]
    })
  // assign_core(core, queue, new_assigner, jam_slot) — Coretime only. Mirrors
  // JAM's `assign`, which writes a core's authorizer queue AND its assigner
  // atomically, so one call covers both.
  //
  // - `jamSlot`: when to apply. The entry is cached and forwarded in the
  //   always-accumulate phase once the timeslot reaches `jamSlot`; if already
  //   due (`jamSlot <= now`) it is applied inline. The Coretime chain chooses
  //   `jamSlot` so the pool fills without overwriting the active authorizers.
  // - `queue`: an empty list cancels any cached entry for the core (a
  //   service-level cancel — no JAM call is made).
  // - `newAssigner`: `None` leaves this service as the core's assigner (the
  //   common queue-rotation case); `Some(s)` hands the core to `s`. JAM always
  //   stores SOME assigner, so `None` means "unchanged", never "no assigner" —
  //   the service passes its own id to `assign`. A handoff is one-way: once it
  //   fires the service is no longer `assigners[core]`, so a later `assign` for
  //   that core fails with HUH.
  | AssignCore({
      core: CoreIndex,
      queue: List[AuthorizerHash],
      newAssigner: Option[ServiceId],
      jamSlot: Timeslot
  })
  // set_validator_keys(chunk, is_last) — Asset Hub only.
  | SetValidatorKeys({ keys: List[ValidatorKey], isLast: bool })
  // consume_transfers_up_to(index) — Asset Hub only.
  | ConsumeTransfersUpTo(int)
  // parachain_set_head(para_id, new_head) — Coretime only.
  | ParachainSetHead({ paraId: ParaId, newHead: HeadData })
  // parachain_set_validation_code(para_id, new_hash, new_len) — Coretime only.
  | ParachainSetValidationCode({
      paraId: ParaId,
      newValidationCodeHash: ValidationCodeHash,
      newValidationCodeLen: int
  })
  // parachain_clean_up(para_id) — Coretime only.
  | ParachainCleanUp(ParaId)
  // parachain_set_state_balance(para_id, new_total) — Coretime only.
  | ParachainSetStateBalance({ paraId: ParaId, newTotal: Balance })
  // parachain_service_upgrade(code_hash, len, min_acc_gas, min_memo_gas) —
  // Asset Hub only (design §5.4).
  | UpgradeService({ codeHash: Hash, len: int, minAccGas: int, minMemoGas: int })

  // --- Refine output per work item ---------------------------------------

  /// A successful Refine output for one parachain candidate.
  type ParachainWorkDigestOk = {
    paraId: ParaId,
    validationCode: ValidationCodeRef,
    parentHeadHash: Hash,
    headData: HeadData,
    upwardMessages: List[UpwardMessage],
    lookupAnchor: Timeslot
  }

  /// Refine output per work item: a success digest or a Refine error.
  type ParachainWorkDigest =
  | Ok(ParachainWorkDigestOk)
  | Err({ paraId: ParaId, error: RefineLog })

  /// Gray paper `WorkExecResult`. `WorkOk` carries the service-produced
  /// `ParachainWorkDigest`; `WorkErr` is the JAM-substituted error case
  /// (gray paper `WorkExecResult::Error(WorkErrorCode)`) which Accumulate
  /// skips entirely — no `parachain_log` entry, no state change (§3.3).
  /// Constructor names are prefixed to avoid Quint's module-wide name
  /// collision with `ParachainWorkDigest::Ok` / `Err`.
  type WorkExecResult =
  | WorkOk(ParachainWorkDigest)
  | WorkErr

  /// Gray paper per-work-item record consumed by Accumulate. `result`
  /// carries the outcome (service digest or JAM-substituted error);
  /// `authTrace` is the work-report authorizer trace (one per report
  /// in JAM, but per-item here since the model fixes one work-item per
  /// work-report — design §3.2).
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
    | ConsumeTransfersUpTo(_) => true
    | UpgradeService(_) => true
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
  /// calling parachain — except from the Coretime chain, which may name any para
  /// (§6.4). `true` iff `m` names a para other than `para_id` without that right.
  pure def targetsForeignPara(m: UpwardMessage, para_id: ParaId): bool =
  para_id != CoretimeParaId and
  (match m {
    | Forget(p) => p.paraId != para_id
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
