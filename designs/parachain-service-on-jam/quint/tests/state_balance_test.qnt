// State-balance accounting: preimage solicit/forget, rescue, and
// key-value store charging (design 6.1).
module state_balance_test {
  import types.* from "../types"
  import messages.* from "../messages"
  import state.* from "../state"
  import state_balance.* from "../state_balance"
  import state_vars.* from "../state_vars"
  import main.* from "../main"
  import support.* from "./support"

  /// A parachain at its state-balance cap has its next `Solicit` rejected:
  /// - Coretime registers para 6 with `totalStateBalance = BaselineFootprint
  ///   + preimageFootprint(FixedCodeLen)`, headroom for exactly the
  ///   validation-code preimage and nothing else.
  /// - Coretime sets para 6's active code, consuming that headroom.
  /// - Para 6 emits a `Solicit` upward message for a distinct preimage,
  ///   `addReferencer` finds no headroom, appends
  ///   `InsufficientStateBalance(FromSolicit(...))` to the parachain log, and
  ///   leaves the registry and `usedStateBalance` unchanged.
  run solicitRejectedAtBalanceCapTest = {
    val para = MkParaId(6)
    val paraCode: ValidationCodeHash = { vchBytes: 0x66 }
    val soliciteeHash: Hash = { hashBytes: 0xDD }
    val soliciteeLen = 500
    val cap = BaselineFootprint + preimageFootprint(FixedCodeLen)
    init
      .then(applyHappyPath(CoretimeParaId, { vchBytes: 1 }, List(
        ParachainSetStateBalance({ paraId: para, newTotal: cap }),
        ParachainSetValidationCode({
          paraId: para,
          newValidationCodeHash: paraCode,
          newValidationCodeLen: FixedCodeLen
        })
      )))
      // After registration + set validation code: usedStateBalance is exactly at the cap.
      .then(expectThat(all {
        svc.paraInfo(para).usedStateBalance == cap,
        svc.paraInfo(para).totalStateBalance == cap
      }))
      // Para tries to solicit a new preimage: must be rejected.
      .then(applyHappyPath(para, paraCode, List(
        Solicit({ target: Parachain(para), hash: soliciteeHash, len: soliciteeLen })
      )))
      .then(expectThat(all {
        // No referencer added for the rejected preimage.
        not(svc.preimageRegistry.keys().contains((soliciteeHash, soliciteeLen))),
        // usedStateBalance stayed at the cap.
        svc.paraInfo(para).usedStateBalance == cap,
        // Accumulate log carries InsufficientStateBalance(FromSolicit(...)) for this block.
        svc.paraLog(para).foldl(false, (found, pair) =>
          found or (pair._1 == now and match pair._2 {
            | AccumulateLogEntry(evs) => evs.foldl(false, (a, e) => match e {
              | InsufficientStateBalance(reason) => match reason {
                | FromSolicit(p) => a or (p.hash == soliciteeHash and p.len == soliciteeLen)
                | FromSetKV(_) => a
              }
              | _ => a
            })
            | RefineLogEntry(_) => false
          }))
      }))
  }

  /// A never-provided preimage (`Unprovided`) is dropped by a SINGLE `forget`:
  /// no limbo, no retained referencer, refund now (JAM `ForgetImplication::Drop`).
  run forgetUnprovidedDropsInOneStepTest = {
    val para = CoretimeParaId
    val h: Hash = { hashBytes: 0x7A }
    // Solicit only — the preimage is never provided, so status is `Unprovided`.
    val d1 = initialState.addReferencer(para, h, 512)
    val d2 = d1.state.removeReferencer(para, h, 512, 0)
    all {
      assert(d1.state.preimageStatus.get((h, 512)) == Unprovided),
      // Single forget drops it: entry gone, status gone, JAM forget forwarded,
      // NOT retained (footprint refunded), and no ForgetAgainAt.
      assert(not(d2.state.preimageRegistry.keys().contains((h, 512)))),
      assert(not(d2.state.preimageStatus.keys().contains((h, 512)))),
      assert(d2.jamForget == Some(h)),
      assert(not(d2.retained)),
      assert(d2.log == None),
      init
    }
  }

  /// The expunge boundary is strict (JAM `now > unrequested_since +
  /// min_turnaround`): a second forget at `now == due` is a no-op retry, at
  /// `now == due + 1` it expunges.
  run forgetExpungeBoundaryTest = {
    val para = CoretimeParaId
    val h: Hash = { hashBytes: 0x7B }
    // Solicit, provide, then first forget at now = 0 -> Unrequested(0),
    // due = 0 + ExpungePeriod.
    val d1 = initialState.addReferencer(para, h, 512)
    val provided = markProvided(d1.state, h, 512)
    val d2 = provided.removeReferencer(para, h, 512, 0)
    // At now == due: not yet expungeable (no-op retry, entry survives).
    val atDue = d2.state.removeReferencer(para, h, 512, ExpungePeriod)
    // At now == due + 1: expunges.
    val pastDue = d2.state.removeReferencer(para, h, 512, ExpungePeriod + 1)
    all {
      assert(d2.state.preimageStatus.get((h, 512)) == Unrequested(0)),
      // now == due -> no-op, still present and retained, ForgetAgainAt re-emitted.
      assert(atDue.state.preimageRegistry.keys().contains((h, 512))),
      assert(atDue.retained),
      assert(atDue.jamForget == None),
      assert(atDue.log == Some(ForgetAgainAt({ hash: h, len: 512, due: ExpungePeriod }))),
      // now == due + 1 -> expunged.
      assert(not(pastDue.state.preimageRegistry.keys().contains((h, 512)))),
      assert(not(pastDue.state.preimageStatus.keys().contains((h, 512)))),
      assert(pastDue.jamForget == Some(h)),
      assert(not(pastDue.retained)),
      init
    }
  }

  /// Rescue: a `solicit` on an `Unrequested` preimage rescues the still-held
  /// blob back to available (JAM `Unrequested → Rerequested`), keeping the
  /// ORIGINAL unrequest slot for the next forget's gate. A DIFFERENT para (p2)
  /// soliciting p1's limbo entry triggers it, and frees the retained ghost p1:
  /// p1 had already forgotten, so its retention was only the two-step stand-in.
  /// Once p2 is the live referencer covering the single JAM entry, keeping p1
  /// charged would double-count it, so p1 is refunded and dropped (§6.1).
  run forgetRescueRerequestedTest = {
    val h: Hash = { hashBytes: 0x8A }
    val p1 = CoretimeParaId
    val p2 = AssetHubParaId
    // p1 solicits, the blob is provided, p1 forgets (last ref) at now = 5.
    val d1 = initialState.addReferencer(p1, h, 512)
    val provided = markProvided(d1.state, h, 512)
    val d2 = provided.removeReferencer(p1, h, 512, 5)
    // p2 solicits the same (h, 512) while it is Unrequested(5): rescue.
    val d3 = d2.state.addReferencer(p2, h, 512)
    all {
      assert(d2.state.preimageStatus.get((h, 512)) == Unrequested(5)),
      // p1 still charged the footprint while retained as the ghost.
      assert(d2.state.paraInfo(p1).usedStateBalance == d1.state.paraInfo(p1).usedStateBalance),
      // Rescue fires a real JAM solicit and moves to Rerequested keeping y = 5.
      assert(d3.jamSolicit == Some(h)),
      assert(d3.state.preimageStatus.get((h, 512)) == Rerequested(5)),
      // Ghost p1 is freed+refunded; p2 is the sole live referencer.
      assert(d3.state.preimageRegistry.get((h, 512)).referencers == Set(p2)),
      assert(d3.state.paraInfo(p1).usedStateBalance
        == d2.state.paraInfo(p1).usedStateBalance - preimageFootprint(512)),
      init
    }
  }

  /// NotYetUnrequest (Oracle Q6): a rescued preimage forgotten again WITHIN the
  /// original turnaround is a no-op retry; the gate keys off the original
  /// unrequest slot, NOT `now`. Proves collapsing `Rerequested` into `Provided`
  /// would be unfaithful (it would wrongly succeed with a shifted deadline).
  run forgetRerequestedNotYetTest = {
    val h: Hash = { hashBytes: 0x8B }
    val p1 = CoretimeParaId
    val p2 = AssetHubParaId
    // p1 solicit + provide + forget at now = 10 -> Unrequested(10).
    val d1 = initialState.addReferencer(p1, h, 512)
    val provided = markProvided(d1.state, h, 512)
    val d2 = provided.removeReferencer(p1, h, 512, 10)
    // p2 rescues at now = 20 -> Rerequested(10), freeing ghost p1: p2 is now
    // the sole referencer.
    val d3 = d2.state.addReferencer(p2, h, 512)
    // A forget of p1 is now a no-op (p1 was freed on rescue); p2 (last live
    // referencer) forgets at now = 25, still within the ORIGINAL turnaround
    // (25 <= 10 + ExpungePeriod), so it hits the NotYetUnrequest gate.
    val d4 = d3.state.removeReferencer(p1, h, 512, 25)
    val d5 = d4.state.removeReferencer(p2, h, 512, 25)
    all {
      assert(d3.state.preimageStatus.get((h, 512)) == Rerequested(10)),
      assert(d3.state.preimageRegistry.get((h, 512)).referencers == Set(p2)),
      // Last forget is NotYetUnrequest: no-op, entry survives, retained, no JAM
      // forget, and the retry deadline is the ORIGINAL 10 + ExpungePeriod.
      assert(d5.state.preimageRegistry.keys().contains((h, 512))),
      assert(d5.retained),
      assert(d5.jamForget == None),
      assert(d5.log == Some(ForgetAgainAt({ hash: h, len: 512, due: 10 + ExpungePeriod }))),
      // Status is unchanged (still Rerequested(10)).
      assert(d5.state.preimageStatus.get((h, 512)) == Rerequested(10)),
      init
    }
  }

  /// Retained self-rescue: the SAME para that forgot its own code (leaving it
  /// `Unrequested` with itself retained) re-solicits it; the idempotent
  /// early-return must NOT short-circuit; JAM `solicit` still fires to rescue.
  run forgetRetainedSelfRescueTest = {
    val h: Hash = { hashBytes: 0x8C }
    val para = CoretimeParaId
    val d1 = initialState.addReferencer(para, h, 512)
    val provided = markProvided(d1.state, h, 512)
    val d2 = provided.removeReferencer(para, h, 512, 7)
    // Same para re-solicits its own code sitting in limbo.
    val d3 = d2.state.addReferencer(para, h, 512)
    all {
      assert(d2.state.preimageStatus.get((h, 512)) == Unrequested(7)),
      assert(d2.state.preimageRegistry.get((h, 512)).referencers == Set(para)),
      // Idempotent branch still fires JAM solicit and rescues to Rerequested(7).
      assert(d3.jamSolicit == Some(h)),
      assert(d3.state.preimageStatus.get((h, 512)) == Rerequested(7)),
      assert(d3.state.preimageRegistry.get((h, 512)).referencers == Set(para)),
      init
    }
  }

  /// Ghost accumulation regression: repeated forget → rescue cycles by
  /// alternating paras must NOT pile up ghosts. Each rescue frees the prior
  /// ghost, so the referencer set stays size 1 and the footprint is charged
  /// exactly once, never `N × footprint` across N cycles (§6.1).
  run forgetRescueNoGhostAccumulationTest = {
    val h: Hash = { hashBytes: 0x8D }
    val p1 = CoretimeParaId
    val p2 = AssetHubParaId
    // p1 solicit + provide + forget at now=5 -> Unrequested(5), p1 ghost.
    val d1 = initialState.addReferencer(p1, h, 512)
    val prov = markProvided(d1.state, h, 512)
    val d2 = prov.removeReferencer(p1, h, 512, 5)
    // p2 rescues (frees p1) -> Rerequested(5), referencers = {p2}.
    val d3 = d2.state.addReferencer(p2, h, 512)
    // p2 forgets past the ORIGINAL deadline (now > 5 + ExpungePeriod): the
    // Rerequested forget succeeds -> Unrequested(now), p2 retained (ghost).
    val t2 = 5 + ExpungePeriod + 1
    val d4 = d3.state.removeReferencer(p2, h, 512, t2)
    // p1 rescues again (frees p2) -> Rerequested, referencers = {p1}.
    val d5 = d4.state.addReferencer(p1, h, 512)
    all {
      // After every cycle the set is a singleton; no ghost pile-up.
      assert(d3.state.preimageRegistry.get((h, 512)).referencers == Set(p2)),
      assert(d4.state.preimageStatus.get((h, 512)) == Unrequested(t2)),
      assert(d4.state.preimageRegistry.get((h, 512)).referencers == Set(p2)),
      assert(d5.state.preimageRegistry.get((h, 512)).referencers == Set(p1)),
      // Only the current sole referencer is charged the footprint; the other
      // para is back at baseline (freed on the rescue that displaced it).
      assert(d5.state.paraInfo(p2).usedStateBalance == d2.state.paraInfo(p2).usedStateBalance),
      init
    }
  }

  /// SetKV charges `kvEntryFootprint(kLen, vLen)` to `usedStateBalance`.
  run kvSetChargesTest = {
    val s = initialState
    val para = CoretimeParaId
    val key = List(1, 2, 3)
    val value = List(10, 20)
    val before = s.paraInfo(para).usedStateBalance
    val d = applySetKV(s, para, key, value)
    val expected = kvEntryFootprint(key.length(), value.length())
    all {
      assert(d.log == None),
      assert(d.state.paraInfo(para).usedStateBalance == before + expected),
      init
    }
  }

  /// RemoveKV refunds the full `kvEntryFootprint`, returning `usedStateBalance`
  /// to its pre-insert level.
  run kvRemoveRefundsTest = {
    val s = initialState
    val para = CoretimeParaId
    val key = List(1, 2, 3)
    val value = List(10, 20)
    val before = s.paraInfo(para).usedStateBalance
    val d1 = applySetKV(s, para, key, value)
    val d2 = applyRemoveKV(d1.state, para, key)
    all {
      assert(d1.log == None),
      assert(d2.log == None),
      assert(d2.state.paraInfo(para).usedStateBalance == before),
      init
    }
  }

  /// Overwriting an existing key charges only the value-size delta.
  run kvOverwriteDeltaTest = {
    val s = initialState
    val para = CoretimeParaId
    val key = List(5)
    val value1 = List(1, 2)
    val value2 = List(1, 2, 3, 4)
    val d1 = applySetKV(s, para, key, value1)
    val afterInsert = d1.state.paraInfo(para).usedStateBalance
    val d2 = applySetKV(d1.state, para, key, value2)
    // Growing overwrite: delta = (compactLen(4)+4) - (compactLen(2)+2) = 5 - 3 = 2
    val growDelta = (1 + value2.length()) - (1 + value1.length())
    all {
      assert(d1.log == None),
      assert(d2.log == None),
      assert(d2.state.paraInfo(para).usedStateBalance == afterInsert + growDelta),
      init
    }
  }

  /// A SetKV whose value would exceed the para's headroom is rejected with a log entry.
  run kvInsufficientBalanceTest = {
    val s = initialState
    val para = CoretimeParaId
    val key = List(7)
    // Build a value large enough to exceed the para's remaining headroom.
    // InitialParaBalance = BaselineFootprint + 2*preimageFootprint(FixedCodeLen) + BaselineFootprint
    // After registration, usedStateBalance = BaselineFootprint + 2*preimageFootprint(FixedCodeLen).
    // Remaining headroom ≈ BaselineFootprint. A value of length BaselineFootprint+1 bytes
    // will exceed it (kvEntryFootprint adds 39+ bytes of overhead on top).
    // We approximate with a list of length 200000 (well above any headroom).
    val v1 = List(1, 2, 3, 4, 5, 6, 7, 8, 9, 10)
    val v2 = v1.concat(v1).concat(v1).concat(v1).concat(v1)   // 50
    val v3 = v2.concat(v2).concat(v2).concat(v2).concat(v2)   // 250
    val v4 = v3.concat(v3).concat(v3).concat(v3).concat(v3)   // 1250
    val v5 = v4.concat(v4).concat(v4).concat(v4).concat(v4)   // 6250
    val v6 = v5.concat(v5).concat(v5).concat(v5).concat(v5)   // 31250
    val hugeValue = v6.concat(v6).concat(v6).concat(v6).concat(v6) // 156250
    val d = applySetKV(s, para, key, hugeValue)
    all {
      assert(d.state == s),
      assert(d.log != None),
      init
    }
  }
}
