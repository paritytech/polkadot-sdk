// Refine: in-core, stateless PVF execution (design §4).
//
// We do NOT model PVF semantics — instead we model Refine as a non-
// deterministic producer of `ParachainWorkResult`s subject to the
// structural rules from §4.1–§4.3:
//
// - The `paraId` of each work-item is sourced from the authorizer config's
// `authorized_paras` prefix, not from the extrinsic.
// - The PVF must call `set_parent_head_hash` exactly once (modeled as a
// well-formedness check on `Ok` results).
// - Refine returns `Err(InvalidCodeHash)` iff `historical_lookup` for the
// candidate's `validation_code_hash` returns None.
// - `set_validator_keys` may appear at most once in the emitted
//   `upwardMessages` list, and any chunk must contain at most 30 keys.
//
// The `parent_head_hash` field is mandatory but is *not* validated against
// service state here — that check lives in Accumulate (§5.1, step 1).
module refine {
  import types.* from "./types"
  import messages.* from "./messages"
  import state.* from "./state"

  /// Per-work-package extrinsic — one parachain candidate per item (design §3.2).
  type ParachainCandidate = {
    validationCodeHash: ValidationCodeHash,
    // PoV is opaque; the model treats it as a nondeterministic input.
    pov: int
  }

  type WorkPackage = {
    // Authorizer config prefix binding work-item index → ParaId (design §3.2, §7.1).
    authorizedParas: List[ParaId],
    // One candidate per work item. Currently `length(items) == 1` per design §3.2.
    items: List[ParachainCandidate]
  }

  // --- Upward-message validity rules (Refine-side enforcement) ------------

  /// Per §4.3: `set_validator_keys` may appear at most once per Refine.
  pure def setValidatorKeysCount(msgs: List[UpwardMessage]): int =
  msgs.foldl(0, (acc, m) => match m {
      | SetValidatorKeys(_) => acc + 1
      | _ => acc
  })

  /// Per §4.3: each `set_validator_keys` chunk contains at most 30 keys.
  pure def setValidatorKeysChunkOk(msgs: List[UpwardMessage]): bool =
  msgs.foldl(true, (acc, m) => match m {
      | SetValidatorKeys(payload) =>
      acc and (payload.keys.length() <= SetValidatorKeysMaxKeys)
      | _ => acc
  })

  /// Combined Refine-side well-formedness for emitted upward messages.
  pure def wellFormedUpwardMessages(msgs: List[UpwardMessage]): bool =
  setValidatorKeysCount(msgs) <= 1 and
  setValidatorKeysChunkOk(msgs)

  // --- Abstract Refine ----------------------------------------------------
  //
  // `refineItem` models §4.1 abstractly: given a candidate and the service's
  // view of the active+pending validation code for that para, produce
  // either an `Ok` with a list of upward messages (any from §4.3 that the
  // PVF chose to emit, subject to the well-formedness rules above) or an
  // `Err(InvalidCodeHash)` when the lookup fails.
  //
  // For the model, we nondeterministically pick the upward messages in
  // the spec's top-level `step` action — see `main.qnt`.

  pure def refineErr(paraId: ParaId, e: RefineLog): ParachainWorkResult =
  Err({ paraId: paraId, error: e })

  pure def refineOk(
    paraId: ParaId,
    vch: ValidationCodeHash,
    parentHead: Hash,
    newHead: HeadData,
    msgs: List[UpwardMessage]
  ): ParachainWorkResult =
  Ok({
      paraId: paraId,
      validationCodeHash: vch,
      parentHeadHash: parentHead,
      headData: newHead,
      upwardMessages: msgs
  })

  /// The §4.1 step that determines lookup success. In the real protocol this
  /// is `historical_lookup(validation_code_hash) -> Option<Vec<u8>>`. In the
  /// model: a hash is "lookupable" iff some referencer for it exists in
  /// `preimage_registry`. (This is a model-side proxy; the lookup itself goes
  /// through JAM's preimage store at the lookup-anchor.)
  pure def codeAvailable(s: ParachainServiceState, h: ValidationCodeHash): bool = {
    val key = vchAsHash(h)
    s.preimageRegistry.keys().contains(key) and
    s.preimageRegistry.get(key).referencers != Set()
  }

  /// Per-item Refine semantics (§4.1).
  ///
  /// The PVF itself is modeled as an oracle whose visible effects are:
  ///   - `pvfPanic`: did the guest trap / report_error before completing?
  ///   - `newHead`:  the head data the PVF committed (via the head host call).
  ///   - `msgs`:     the upward-message side effects the PVF emitted during Refine.
  ///
  /// The Refine wrapper then applies the rules from §4.1 and §4.3:
  ///   1. `historical_lookup` for the candidate's validation code (`codeAvailable`).
  ///   2. Treat panic / report_error as an `Err(Opaque(...))` result.
  ///   3. Reject ill-formed upward-message bundles (`wellFormedUpwardMessages`).
  ///   4. Otherwise assemble an `Ok` result tagged with the para id from the
  ///      authorizer config and the canonical parent-head hash.
  pure def refineItem(
    s: ParachainServiceState,
    para_id: ParaId,
    candidate: ParachainCandidate,
    pvfPanic: bool,
    newHead: HeadData,
    msgs: List[UpwardMessage],
    assetHub: ParaId,
    coretimeChain: ParaId
  ): ParachainWorkResult = {
    if (not(codeAvailable(s, candidate.validationCodeHash)))
    refineErr(para_id, InvalidCodeHash)
    else if (pvfPanic)
    refineErr(para_id, Opaque(0))
    else if (not(wellFormedUpwardMessages(msgs)))
    refineErr(para_id, SetValidatorKeysTooManyKeys)
    else if (violatesParachainRestrictions(msgs, para_id, assetHub, coretimeChain))
    refineErr(para_id, RestrictedHostFunction)
    else
    refineOk(
      para_id,
      candidate.validationCodeHash,
      headHash(s.paraInfo(para_id).headData),
      newHead,
      msgs
    )
  }

  /// Per-work-package Refine entry point (§4.1).
  ///
  /// Performs the package-level check on the authorizer config — if
  /// `len(authorized_paras) != len(workitems)` the Refine invocation
  /// aborts and every item is tagged with `AuthConfigMismatch` (§4.1
  /// step 1). Otherwise each item is refined independently via
  /// `refineItem`, with its `para_id` sourced from `authorized_paras`.
  ///
  /// Single-item packages only (per §3.2): `pvfPanic`, `newHead`, and
  /// `msgs` are per-item scalars.
  pure def refine(
    s: ParachainServiceState,
    pkg: WorkPackage,
    authConfigMalformed: bool,
    pvfPanic: bool,
    newHead: HeadData,
    msgs: List[UpwardMessage],
    assetHub: ParaId,
    coretimeChain: ParaId
  ): List[ParachainWorkResult] = {
    val errParaId =
      if (pkg.authorizedParas.length() > 0) pkg.authorizedParas[0]
      else MkParaId(0)
    if (authConfigMalformed)
    List(refineErr(errParaId, MalformedAuthorizerConfig))
    else if (pkg.items.length() != 1)
    List(refineErr(errParaId, InvalidItemCount))
    else if (pkg.authorizedParas.length() != pkg.items.length())
    List(refineErr(errParaId, AuthConfigMismatch))
    else
    List(refineItem(
      s, pkg.authorizedParas[0], pkg.items[0],
      pvfPanic, newHead, msgs, assetHub, coretimeChain
    ))
  }
}
