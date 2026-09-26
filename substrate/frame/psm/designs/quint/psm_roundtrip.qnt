// psm_roundtrip.qnt
//
// Models the redeem round-trip more faithfully:
//
//   external_out         = internal_net / factor       (floor)
//   effective_internal_net = external_out * factor    (round-trip)
//   PsmDebt -= effective_internal_net                  (NOT internal_net)
//
// The pallet uses `effective_internal_net` (post-round-trip) for the debt
// decrement and for `burn_from`. Dust (internal_net - effective_internal_net)
// stays with the user.
//
// Properties to check:
//   1. reserve >= 0 always
//   2. psmDebt >= 0 always
//   3. psmDebt + (total external withdrawn in internal units) = total minted (internal units)
//   4. Can a sequence of redeems with multi-decimal scale-down let
//      the user keep "dust" repeatedly to drain the system?

module psm_roundtrip {

  pure val factor: int = 100  // external decimals > internal decimals (DAI-like)
                              // 1 internal unit = 100 external units

  var psmDebt: int           // internal units
  var reserveExt: int        // external units, raw
  var aliceInternal: int     // internal units owned by the user
  var totalMintedInt: int    // cumulative internal units the PSM has minted
  var totalBurnedInt: int    // cumulative internal units burned (effective_internal_net)
  var totalDustKept: int     // cumulative dust the user kept on redeems (internal units)

  // Hard accounting: total ever minted = currently held by user + ever burned.
  // Dust stays with the user — it is part of aliceInternal, not separate.
  val mintedConservation =
    totalMintedInt == aliceInternal + totalBurnedInt

  // PsmDebt should equal: total minted - total burned (effective).
  // Dust is NOT subtracted from PsmDebt (it stays with user but pallet doesn't burn it).
  val debtTracksBurned =
    psmDebt == totalMintedInt - totalBurnedInt

  // The dangerous one: can the user end up with more internal than they
  // started with by exploiting round-trip dust?
  val noFreeInternal =
    aliceInternal <= totalMintedInt - totalBurnedInt

  action init = all {
    psmDebt'        = 0,
    reserveExt'     = 0,
    aliceInternal'  = 0,
    totalMintedInt' = 0,
    totalBurnedInt' = 0,
    totalDustKept'  = 0,
  }

  // Mint: user submits externalAmount. We restrict to exact multiples to
  // isolate redeem-side dust effects.
  action mint(externalAmount: int): bool = all {
    externalAmount > 0,
    externalAmount % factor == 0,
    val internalEquivalent = externalAmount / factor
    all {
      psmDebt'        = psmDebt + internalEquivalent,
      reserveExt'     = reserveExt + externalAmount,
      aliceInternal'  = aliceInternal + internalEquivalent,
      totalMintedInt' = totalMintedInt + internalEquivalent,
      totalBurnedInt' = totalBurnedInt,
      totalDustKept'  = totalDustKept,
    }
  }

  // Redeem with rounding behavior of the actual pallet:
  //   external_out = internal_amount / factor   (floor)
  //   effective_internal_net = external_out * factor
  //   dust = internal_amount - effective_internal_net  (stays with user)
  action redeem(internalAmount: int): bool = all {
    internalAmount > 0,
    internalAmount <= aliceInternal,
    val externalOut = internalAmount / factor
    val effectiveInternalNet = externalOut * factor
    val dust = internalAmount - effectiveInternalNet
    all {
      effectiveInternalNet <= psmDebt,
      externalOut <= reserveExt,
      // Pallet allows redeem with zero externalOut only when internal_net is zero.
      internalAmount == 0 or externalOut > 0,
      psmDebt'        = psmDebt - effectiveInternalNet,
      reserveExt'     = reserveExt - externalOut,
      aliceInternal'  = aliceInternal - effectiveInternalNet,  // dust stays
      totalMintedInt' = totalMintedInt,
      totalBurnedInt' = totalBurnedInt + effectiveInternalNet,
      totalDustKept'  = totalDustKept + dust,
    }
  }

  action step = any {
    nondet a = oneOf(1.to(10).map(i => i * factor)); mint(a),
    nondet a = oneOf(1.to(500)); redeem(a),
  }
}
