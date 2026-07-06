// Refine output types and the upward-message vocabulary the PVF can emit
// during a Refine invocation (design §3.3, §4.3).
//
// Each variant of `UpwardMessage` corresponds 1:1 to a side-effect host
// function in §4.3. Accumulate replays the list in order (§5.1 step 7).
module messages {
  import types.* from "./types"

  /// A validation code reference: its hash plus its byte length.
  type ValidationCodeRef = { hash: ValidationCodeHash, len: int }

  // --- Refine-side log payloads -------------------------------------------

  type RefineLog =
  // historical_lookup(validation_code_hash) returned None.
  | InvalidCodeHash
  // PVF called report_error(data) before failing. Payload ≤ 1024 B.
  | Opaque(int)
  // A `set_validator_keys` chunk exceeded 30 keys, or
  // `set_validator_keys` was called more than once in a single
  // Refine invocation.
  | SetValidatorKeysTooManyKeys
  // The PVF invoked a host function restricted to another parachain
  // (Asset Hub or the Coretime chain).
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
  | WorkDigestTooLarge
  // The PVF exited without calling `set_parent_head_hash` and/or `set_head`
  // exactly once — both head declarations are mandatory (§4.2).
  | MissingHeadDeclaration

  // --- Accumulate-side log payloads ---------------------------------------

  // Why a state-balance reservation failed. `FromSolicit` carries the
  // preimage hash and length; `FromValidatorKeys` covers a `set_validator_keys`
  // chunk append (which has no single hash). Distinct constructor names
  // because `Solicit`/`SetValidatorKeys` are already `UpwardMessage` labels.
  type InsufficientBalanceReason =
  | FromSolicit({ hash: Hash, len: int })
  | FromValidatorKeys

  type AccumulateLog =
  // Work-result's validation_code_hash matches neither active nor pending.
  | InvalidCodeHashAcc(ValidationCodeHash)
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
  // `parachain_service_upgrade(code_hash, ...)` was rejected because the
  // new code's preimage is not in the Parachain Service's preimage store
  // (§5.4).
  | ServiceUpgradePreimageMissing({ codeHash: Hash })

  /// A JAM `assign(core, newAssigner)` host-function call recorded during a
  /// step. `newAssigner` mirrors `set_authorizer_queue`'s argument; `None`
  /// on the exhaustion-flush path (a cached queue update carried no assigner).
  type AssignCall = { core: CoreIndex, newAssigner: Option[ServiceId] }

  // --- Upward messages emitted by the PVF, replayed by Accumulate ---------

  type UpwardMessage =
    // request_code_upgrade(hash, len) — start a PVF code upgrade (§5.2).
    | RequestCodeUpgrade({ hash: ValidationCodeHash, len: int })
  // solicit(hash, len)
  | Solicit({ hash: Hash, len: int })
  // forget(hash, len)
  | Forget({ hash: Hash, len: int })
  // transfer_out(dest, amount, memo) — Asset Hub only.
  | TransferOut({ dest: ServiceId, amount: Balance, memo: Memo })
  // set_authorizer_queue(core, queue, immediate, new_assigner) — Coretime only.
  | SetAuthorizerQueue({
      core: CoreIndex,
      queue: List[AuthorizerHash],
      immediate: bool,
      newAssigner: Option[ServiceId]
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
  // parachain_service_upgrade(code_hash, len, min_item_gas, min_memo_gas) —
  // Asset Hub only (design §5.4).
  | UpgradeService({ codeHash: Hash, len: int, minItemGas: int, minMemoGas: int })

  // --- Refine output per work item ---------------------------------------

  type ParachainWorkDigestOk = {
    paraId: ParaId,
    validationCode: ValidationCodeRef,
    parentHeadHash: Hash,
    headData: HeadData,
    upwardMessages: List[UpwardMessage]
  }

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
    | SetAuthorizerQueue(_) => true
    | ParachainSetHead(_) => true
    | ParachainSetValidationCode(_) => true
    | ParachainCleanUp(_) => true
    | ParachainSetStateBalance(_) => true
    | _ => false
  }

  /// Per §4.3: host functions restricted to a specific parachain (Asset Hub
  /// or the Coretime chain) abort when invoked from any other parachain.
  /// Refine enforces this as a PVF-level panic; Accumulate independently
  /// verifies and rejects the work-package as defense-in-depth.
  pure def violatesParachainRestrictions(
    msgs: List[UpwardMessage],
    para_id: ParaId
  ): bool =
  msgs.foldl(false, (acc, m) =>
    acc or
    (isAssetHubOnly(m) and para_id != AssetHubParaId) or
    (isCoretimeOnly(m) and para_id != CoretimeParaId))
}
