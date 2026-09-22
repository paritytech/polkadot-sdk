// Parachain validation-code upgrade lifecycle (design §5.2).
//
// The parachain solicits the new code through the ordinary `Solicit` path, then
// drives the switch itself with `Announcement` and `Apply`.
module code_upgrades {
  import types.* from "./types"
  import messages.* from "./messages"
  import state.* from "./state"
  import state_balance.* from "./state_balance"

  /// A `PreimageDelta` that changes nothing.
  pure def unchangedDelta(s: ParachainServiceState): PreimageDelta =
    { state: s, jamSolicit: None, jamForget: None, retained: false, log: None }

  /// Announce the code a later `Apply` may switch to (§5.2). Replayed from a
  /// `RequestCodeUpgrade` upward message during Accumulate, so it runs
  /// on-chain, not inside Refine. Availability is judged as of `lookupAnchor`,
  /// the anchor of the work report carrying the message.
  pure def announceCodeUpgrade(
    s: ParachainServiceState,
    para_id: ParaId,
    newHash: ValidationCodeHash,
    codeLen: int,
    lookupAnchor: Timeslot
  ): PreimageDelta = {
    val h = vchAsHash(newHash)
    val pi = s.paraInfo(para_id)
    val newRef = { hash: newHash, len: codeLen }
    val isActive =
      match pi.validationCode { | Some(vc) => vc == newRef | None => false }
    if (isActive) {
      // Announcing the running code is a no-op, so an announcement never
      // shadows the active code.
      unchangedDelta(s)
    } else if (not(s.hasReferencer(h, codeLen, para_id))
               or not(s.preimageAvailableAt(h, codeLen, lookupAnchor))) {
      { state: s, jamSolicit: None, jamForget: None, retained: false,
        log: Some(CodeUpgradeNotAvailable({ hash: h, len: codeLen })) }
    } else {
      unchangedDelta({
        ...s,
        parachains: s.parachains.put(
          para_id, { ...pi, announcedUpgrade: Some(newRef) }
        )
      })
    }
  }

  /// Switch `validationCode` to the announced code (§5.2).
  pure def applyCodeUpgrade(
    s: ParachainServiceState,
    para_id: ParaId,
    newHash: ValidationCodeHash,
    codeLen: int
  ): PreimageDelta = {
    val pi = s.paraInfo(para_id)
    val newRef = { hash: newHash, len: codeLen }
    val notAnnounced =
      { state: s, jamSolicit: None, jamForget: None, retained: false,
        log: Some(CodeUpgradeNotAnnounced({ hash: vchAsHash(newHash), len: codeLen })) }
    match pi.announcedUpgrade {
      | None => notAnnounced
      | Some(vc) =>
        if (vc != newRef) {
          notAnnounced
        } else {
          unchangedDelta({
            ...s,
            parachains: s.parachains.put(
              para_id, { ...pi, validationCode: Some(vc), announcedUpgrade: None }
            )
          })
        }
    }
  }
}
