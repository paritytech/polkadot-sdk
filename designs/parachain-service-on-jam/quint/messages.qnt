// Refine output types and the upward-message vocabulary the PVF can emit
// during a Refine invocation (design §3.3, §4.3).
//
// Each variant of `UpwardMessage` corresponds 1:1 to a side-effect host
// function in §4.3. Accumulate replays the list in order (§5.1 step 5).
module messages {
  import types.* from "./types"

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

  // --- Accumulate-side log payloads ---------------------------------------

  // Why a state-balance reservation failed. `FromSolicit` carries the
  // preimage hash; `FromValidatorKeys` covers a `set_validator_keys`
  // chunk append (which has no single hash). Distinct constructor names
  // because `Solicit`/`SetValidatorKeys` are already `UpwardMessage` labels.
  type InsufficientBalanceReason =
  | FromSolicit({ hash: Hash })
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
  // parachain_set_validation_code(para_id, new_hash) — Coretime only.
  | ParachainSetValidationCode({
      paraId: ParaId,
      newValidationCodeHash: ValidationCodeHash
  })
  // parachain_clean_up(para_id) — Coretime only.
  | ParachainCleanUp(ParaId)
  // parachain_set_state_balance(para_id, new_total) — Coretime only.
  | ParachainSetStateBalance({ paraId: ParaId, newTotal: Balance })

  // --- Refine output per work item ---------------------------------------

  type ParachainWorkResultOk = {
    paraId: ParaId,
    validationCodeHash: ValidationCodeHash,
    parentHeadHash: Hash,
    headData: HeadData,
    upwardMessages: List[UpwardMessage]
  }

  type ParachainWorkResult =
  | Ok(ParachainWorkResultOk)
  | Err({ paraId: ParaId, error: RefineLog })

  /// The originating authorizer trace from the work-report. Carried by Err
  /// results into the parachain_log (design §3.3).
  type AuthTrace = int

  /// What Accumulate receives from JAM for each accumulated work-report.
  /// The `authTrace` is on the report, not per-result; we tag every result
  /// with the report-level trace for ease of modeling.
  type AccumulatedItem = {
    result: ParachainWorkResult,
    authTrace: AuthTrace
  }

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
    para_id: ParaId,
    assetHub: ParaId,
    coretimeChain: ParaId
  ): bool =
  msgs.foldl(false, (acc, m) =>
    acc or
    (isAssetHubOnly(m) and para_id != assetHub) or
    (isCoretimeOnly(m) and para_id != coretimeChain))
}
