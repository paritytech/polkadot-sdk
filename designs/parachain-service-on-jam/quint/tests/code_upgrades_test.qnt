// Parachain code-upgrade lifecycle: announce and apply (design §5.2).
module code_upgrades_test {
  import types.* from "../types"
  import messages.* from "../messages"
  import state.* from "../state"
  import state_balance.* from "../state_balance"
  import code_upgrades.* from "../code_upgrades"
  import accumulate.* from "../accumulate"
  import management.* from "../management"
  import state_vars.* from "../state_vars"
  import main.* from "../main"
  import support.* from "./support"

  pure def announcedHashOf(pi: ParaInfo): Option[ValidationCodeHash] =
    match pi.announcedUpgrade { | None => None | Some(vc) => Some(vc.hash) }

  /// 1. Coretime registers para 3 with balance headroom for the new code.
  /// 2. Coretime sets para 3's active code to `codeA`.
  /// 3. Para 3 solicits `codeB` itself; someone supplies the blob.
  /// 4. Para 3 announces `codeB`; the ACTIVE code stays `codeA`.
  /// 5. Para 3 applies `codeB`: `validationCode` flips and `announcedUpgrade`
  ///    clears. `codeA` stays referenced, and para 3 then forgets it itself.
  run codeUpgradeLifecycleTest = {
    val para = MkParaId(3)
    val codeA: ValidationCodeHash = { vchBytes: 0x11 }
    val codeB: ValidationCodeHash = { vchBytes: 0x22 }
    val balance = BaselineFootprint + preimageFootprint(FixedCodeLen) * 4
    init
      .then(applyHappyPath(CoretimeParaId, { vchBytes: 1 }, List(
        ParachainSetStateBalance({ paraId: para, newTotal: balance }),
        ParachainSetValidationCode({
          paraId: para,
          newValidationCodeHash: codeA,
          newValidationCodeLen: FixedCodeLen
        })
      )))
      .then(expectThat(all {
        svc.parachains.keys().contains(para),
        activeCodeHashOr(svc.paraInfo(para), NoCodeHash) == codeA,
        svc.paraInfo(para).announcedUpgrade == None
      }))
      // Step 1: the para solicits the new code itself and the blob is supplied.
      .then(applyHappyPath(para, codeA, List(
        Solicit({ target: Parachain(para), hash: vchAsHash(codeB), len: FixedCodeLen })
      )))
      .then(provide(vchAsHash(codeB), FixedCodeLen))
      // Step 2: announce. The active code is untouched.
      .then(applyHappyPath(para, codeA, List(
        RequestCodeUpgrade({ hash: codeB, len: FixedCodeLen, phase: Announcement })
      )))
      .then(expectThat(all {
        announcedHashOf(svc.paraInfo(para)) == Some(codeB),
        activeCodeHashOr(svc.paraInfo(para), NoCodeHash) == codeA
      }))
      // An announcement has no deadline: blocks pass and it still stands.
      .then(applyHappyPath(para, codeA, List()))
      .then(applyHappyPath(para, codeA, List()))
      .then(expectThat(announcedHashOf(svc.paraInfo(para)) == Some(codeB)))
      .then(provide(vchAsHash(codeA), FixedCodeLen))
      // Step 3: apply. The candidate carrying it is still validated with codeA.
      .then(applyHappyPath(para, codeA, List(
        RequestCodeUpgrade({ hash: codeB, len: FixedCodeLen, phase: Apply })
      )))
      .then(expectThat(all {
        activeCodeHashOr(svc.paraInfo(para), NoCodeHash) == codeB,
        svc.paraInfo(para).announcedUpgrade == None,
        svc.hasReferencer(vchAsHash(codeB), FixedCodeLen, para),
        // codeA merely UNPINS: it reverts to an ordinary solicited preimage of
        // the para, still referenced and still charged, until the para forgets it.
        not(isValidationCodeOf(svc.paraInfo(para), vchAsHash(codeA), FixedCodeLen)),
        svc.hasReferencer(vchAsHash(codeA), FixedCodeLen, para)
      }))
      // Only now can the para reclaim codeA: it is no longer validation code,
      // so the forget goes through (two-step, since codeA is available).
      .then(applyHappyPath(para, codeB, List(
        Forget({ target: Parachain(para), hash: vchAsHash(codeA), len: FixedCodeLen })
      )))
      .then(expectThat(
        isUnrequested(svc.preimageStatus.get((vchAsHash(codeA), FixedCodeLen)))
      ))
  }

  /// A candidate validated with the ANNOUNCED code is rejected: only the active
  /// code is accepted until an `Apply` lands (§5.1 step 4). The head must not
  /// move.
  run announcedCodeIsNotValidForCandidatesTest = {
    val para = CoretimeParaId
    val newHash: ValidationCodeHash = { vchBytes: 0x5151 }
    val s0 = solicitedAndProvided(initialState, para, newHash, FixedCodeLen)
    val s1 = announceCodeUpgrade(s0, para, newHash, FixedCodeLen, 0).state
    init
      .then(setSvc(s1))
      .then(expectThat(announcedHashOf(svc.paraInfo(para)) == Some(newHash)))
      .then(applyHappyPath(para, newHash, List()))
      .then(expectThat(all {
        // Rejected: head unchanged and the announcement still standing.
        svc.paraInfo(para).headData == s1.paraInfo(para).headData,
        announcedHashOf(svc.paraInfo(para)) == Some(newHash),
        activeCodeHashOr(svc.paraInfo(para), NoCodeHash)
          == activeCodeHashOr(s1.paraInfo(para), NoCodeHash)
      }))
  }

  /// An `Announcement` of a code whose preimage was solicited but never
  /// supplied is rejected: nothing is announced and the failure is logged.
  run announceRejectsUnprovidedCodeTest = {
    val para = CoretimeParaId
    val h: ValidationCodeHash = { vchBytes: 0x6161 }
    // Solicited (so the para references it) but NOT provided.
    val s0 = initialState.addReferencer(para, vchAsHash(h), FixedCodeLen).state
    val d = announceCodeUpgrade(s0, para, h, FixedCodeLen, 0)
    all {
      assert(d.state.paraInfo(para).announcedUpgrade == None),
      assert(d.log == Some(CodeUpgradeNotAvailable({ hash: vchAsHash(h), len: FixedCodeLen }))),
      assert(d.state == s0),
      init
    }
  }

  /// An `Announcement` of an available code the parachain does NOT reference is
  /// rejected. Without this, a para could announce a code another para paid for
  /// and lose its active code when that para forgets it.
  run announceRejectsCodeReferencedByOtherParaTest = {
    val para = CoretimeParaId
    val other = AssetHubParaId
    val h: ValidationCodeHash = { vchBytes: 0x7171 }
    // `other` solicits and the blob is supplied: available, but not ours.
    val s0 = solicitedAndProvided(initialState, other, h, FixedCodeLen)
    val d = announceCodeUpgrade(s0, para, h, FixedCodeLen, 0)
    all {
      assert(s0.preimageAvailable(vchAsHash(h), FixedCodeLen)),
      assert(not(s0.hasReferencer(vchAsHash(h), FixedCodeLen, para))),
      assert(d.state.paraInfo(para).announcedUpgrade == None),
      assert(d.log == Some(CodeUpgradeNotAvailable({ hash: vchAsHash(h), len: FixedCodeLen }))),
      init
    }
  }

  /// `Apply` is rejected when nothing is announced, and when the announcement
  /// names a different code. The active code is unchanged either way.
  run applyRejectsUnannouncedCodeTest = {
    val para = CoretimeParaId
    val hB: ValidationCodeHash = { vchBytes: 0x8181 }
    val hC: ValidationCodeHash = { vchBytes: 0x9191 }
    val activeBefore = activeCodeHashOr(initialState.paraInfo(para), NoCodeHash)
    // Nothing announced.
    val d0 = applyCodeUpgrade(initialState, para, hB, FixedCodeLen)
    // hB announced, but hC applied.
    val s1 = solicitedAndProvided(initialState, para, hB, FixedCodeLen)
    val s2 = announceCodeUpgrade(s1, para, hB, FixedCodeLen, 0).state
    val d1 = applyCodeUpgrade(s2, para, hC, FixedCodeLen)
    all {
      assert(d0.state == initialState),
      assert(d0.log == Some(CodeUpgradeNotAnnounced({ hash: vchAsHash(hB), len: FixedCodeLen }))),
      assert(d1.state == s2),
      assert(d1.log == Some(CodeUpgradeNotAnnounced({ hash: vchAsHash(hC), len: FixedCodeLen }))),
      assert(activeCodeHashOr(d1.state.paraInfo(para), NoCodeHash) == activeBefore),
      assert(announcedHashOf(d1.state.paraInfo(para)) == Some(hB)),
      init
    }
  }

  /// A second `Announcement` supersedes the first. The superseded code merely
  /// releases nothing: its referencer is the parachain's own solicit and stands
  /// until the parachain forgets it.
  run announcementSupersedesPreviousTest = {
    val para = CoretimeParaId
    val hB: ValidationCodeHash = { vchBytes: 0xB1B1 }
    val hC: ValidationCodeHash = { vchBytes: 0xC1C1 }
    val s0 = solicitedAndProvided(initialState, para, hB, FixedCodeLen)
    val s1 = solicitedAndProvided(s0, para, hC, FixedCodeLen)
    val s2 = announceCodeUpgrade(s1, para, hB, FixedCodeLen, 0).state
    val d3 = announceCodeUpgrade(s2, para, hC, FixedCodeLen, 0)
    all {
      assert(announcedHashOf(s2.paraInfo(para)) == Some(hB)),
      assert(announcedHashOf(d3.state.paraInfo(para)) == Some(hC)),
      // The superseded code is still the para's own solicited preimage.
      assert(d3.state.hasReferencer(vchAsHash(hB), FixedCodeLen, para)),
      assert(d3.jamForget == None),
      init
    }
  }

  /// A `Forget` of the announced code is refused while it is the announced code,
  /// and goes through once a superseding announcement has displaced it (§5.2).
  run forgetOfAnnouncedCodeIsRefusedTest = {
    val para = CoretimeParaId
    val hB: ValidationCodeHash = { vchBytes: 0xB2B2 }
    val hC: ValidationCodeHash = { vchBytes: 0xC2C2 }
    val s0 = solicitedAndProvided(initialState, para, hB, FixedCodeLen)
    val s1 = solicitedAndProvided(s0, para, hC, FixedCodeLen)
    val s2 = announceCodeUpgrade(s1, para, hB, FixedCodeLen, 0).state
    // Refused while announced: nothing changes, and the refusal is logged.
    val refused = replay(s2, para, 0,
      Forget({ target: Parachain(para), hash: vchAsHash(hB), len: FixedCodeLen }))
    val s3 = refused.state
    // Supersede, which displaces hB, and only then does the forget take effect.
    val s4 = announceCodeUpgrade(s3, para, hC, FixedCodeLen, 0).state
    val s5 = replay(s4, para, 0,
      Forget({ target: Parachain(para), hash: vchAsHash(hB), len: FixedCodeLen })).state
    all {
      assert(s3 == s2),
      assert(refused.logs == List(
        CanNotForgetValidationCode({ hash: vchAsHash(hB), len: FixedCodeLen }))),
      assert(isValidationCodeOf(s3.paraInfo(para), vchAsHash(hB), FixedCodeLen)),
      assert(announcedHashOf(s4.paraInfo(para)) == Some(hC)),
      assert(not(isValidationCodeOf(s4.paraInfo(para), vchAsHash(hB), FixedCodeLen))),
      assert(s4.hasReferencer(vchAsHash(hB), FixedCodeLen, para)),
      assert(isUnrequested(s5.preimageStatus.get((vchAsHash(hB), FixedCodeLen)))),
      init
    }
  }

  /// `Apply` never touches the preimage store: it swaps the code slots and
  /// nothing else. The displaced code keeps its referencer, its charge and its
  /// `Provided` status; only the parachain's own `Forget` can release it.
  run applyOnlySwapsCodeSlotsTest = {
    val para = CoretimeParaId
    val oldRef: ValidationCodeRef = { hash: { vchBytes: 1 }, len: FixedCodeLen }
    val oldHash = vchAsHash(oldRef.hash)
    val newHash: ValidationCodeHash = { vchBytes: 101 }
    // The old (active) code has been supplied to JAM.
    val provided = markProvided(initialState, oldHash, FixedCodeLen)
    // Announce the new code, then apply it.
    val s1 = solicitedAndProvided(provided, para, newHash, FixedCodeLen)
    val announced = announceCodeUpgrade(s1, para, newHash, FixedCodeLen, 0).state
    val applied = applyCodeUpgrade(announced, para, newHash, FixedCodeLen)
    val pi = applied.state.paraInfo(para)
    all {
      assert(pi.validationCode == Some({ hash: newHash, len: FixedCodeLen })),
      assert(pi.announcedUpgrade == None),
      // The displaced code is untouched apart from no longer being validation code.
      assert(applied.state.hasReferencer(oldHash, FixedCodeLen, para)),
      assert(applied.state.preimageStatus == announced.preimageStatus),
      assert(pi.usedStateBalance == announced.paraInfo(para).usedStateBalance),
      assert(not(isValidationCodeOf(pi, oldHash, FixedCodeLen))),
      assert(applied.jamForget == None),
      init
    }
  }
}
