// Chunked `designate` flow (design §5.3).
//
// `designate` is atomic and requires a complete validator-key vector of
// length ∈ ValCount. A full set is up to 1023 × 336 B ≈ 336 KiB, which
// cannot fit in a single 48 KiB work-report. The PVF therefore emits
// `SetValidatorKeys { keys, isLast }` upward messages across multiple
// Asset Hub blocks; Accumulate appends each chunk and finalizes on
// `isLast`.
//
// Refine-side rules (enforced in `refine.qnt`):
// - `set_validator_keys` may be called at most once per Refine invocation.
// - Each `keys` chunk contains at most 30 keys.
//
// Accumulate-side rules (this module):
// - A partial (non-last) append grows `usedStateBalance` for Asset Hub by
// `336 * len(keys)`, since the chunk persists in the buffer until
// finalization. If it would exceed `totalStateBalance`, log
// `InsufficientStateBalance` and leave the buffer unchanged.
// - On `isLast == true`, assemble the full set, validate `len ∈ ValCount`,
// call `designate`, and clear the buffer (either way). The final chunk
// goes straight to `designate` and never persists, so it needs no headroom
// check or balance charge; clearing releases the prior buffer's footprint.
module validator_keys {
  import types.* from "./types"
  import messages.* from "./messages"
  import state.* from "./state"
  import state_balance.* from "./state_balance"

  type ChunkOutcome = {
    state: ParachainServiceState,
    // Whether to invoke JAM `designate` with the assembled buffer.
    jamDesignate: bool,
    // The vector to pass to `designate` (only meaningful if jamDesignate).
    designateVec: List[ValidatorKey],
    // Optional accumulate-log entry.
    log: Option[AccumulateLog]
  }

  /// Replay of `SetValidatorKeys { keys, isLast }` during Accumulate.
  /// Asset Hub (`AssetHubParaId`) is the only caller permitted by §4.3.
  pure def applySetValidatorKeys(
    s: ParachainServiceState,
    chunk: List[ValidatorKey],
    isLast: bool
  ): ChunkOutcome = {
    if (isLast) {
      // Final chunk: assemble the full set in memory, hand it to `designate`,
      // and clear the staging buffer in the same step.
      val assembled = s.stagedValidatorKeys.concat(chunk)
      val sCleared = { ...s, stagedValidatorKeys: List() }
      val ok = ValCount.contains(assembled.length())
      {
        state: sCleared,
        jamDesignate: ok,
        designateVec: if (ok) assembled else List(),
        log:
        if (ok) None
        else Some(DesignateRejected({ len: assembled.length() }))
      }
    } else {
      // Partial append: the chunk stays in the buffer until finalization.
      if (s.stagedValidatorKeys.length() + chunk.length() > MaxStagedValidatorKeys) {
        {
          state: s,
          jamDesignate: false,
          designateVec: List(),
          log: Some(StagedValidatorKeysOverflow)
        }
      } else {
        {
          state: { ...s, stagedValidatorKeys: s.stagedValidatorKeys.concat(chunk) },
          jamDesignate: false,
          designateVec: List(),
          log: None
        }
      }
    }
  }
}
