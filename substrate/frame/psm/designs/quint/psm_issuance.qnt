// psm_issuance.qnt
//
// Verifies Check 6: total_issuance >= total_psm_debt, with rounding.
//
// PSM mints `internal_equivalent` total per mint (split: internal_to_user + fee).
// PSM burns `effective_internal_net` per redeem.
// PsmDebt grows by `internal_equivalent` and shrinks by `effective_internal_net`.
//
// total_issuance = sum minted - sum burned = sum(internal_equivalent) - sum(effective_internal_net)
// total_psm_debt = same expression (when there's only one asset and no other minters)
//
// So Check 6 should always hold with equality if no other code mints/burns
// the internal asset. Let's verify.

module psm_issuance {

  pure val factor: int = 100

  var psmDebt: int
  var totalIssuance: int     // tracks T::InternalAsset::total_issuance()
  var aliceInternal: int
  var feeDestInternal: int   // fee destination holds internal too
  var reserveExt: int

  // Check 6: total_issuance >= total_psm_debt
  val check6 =
    totalIssuance >= psmDebt

  // Stronger: with one asset and no external minters, equality holds.
  val check6Equality =
    totalIssuance == psmDebt

  // Conservation: total_issuance == aliceInternal + feeDestInternal
  val issuanceMatchesHolders =
    totalIssuance == aliceInternal + feeDestInternal

  action init = all {
    psmDebt'         = 0,
    totalIssuance'   = 0,
    aliceInternal'   = 0,
    feeDestInternal' = 0,
    reserveExt'      = 0,
  }

  // Fees are 1% (ceiling round)
  pure def feeOf(x: int): int = (x + 99) / 100  // mul_ceil(1% of x)

  action mint(externalAmount: int): bool = all {
    externalAmount > 0,
    externalAmount % factor == 0,
    val internalEquivalent = externalAmount / factor
    val fee = feeOf(internalEquivalent)
    val internalToUser = internalEquivalent - fee
    all {
      psmDebt'         = psmDebt + internalEquivalent,
      totalIssuance'   = totalIssuance + internalEquivalent,  // mint fee + user
      aliceInternal'   = aliceInternal + internalToUser,
      feeDestInternal' = feeDestInternal + fee,
      reserveExt'      = reserveExt + externalAmount,
    }
  }

  action redeem(internalAmount: int): bool = all {
    internalAmount > 0,
    internalAmount <= aliceInternal,
    val fee = feeOf(internalAmount)
    val internalNet = internalAmount - fee
    val externalOut = internalNet / factor
    val effectiveInternalNet = externalOut * factor
    all {
      effectiveInternalNet <= psmDebt,
      externalOut <= reserveExt,
      internalNet == 0 or externalOut > 0,
      psmDebt'         = psmDebt - effectiveInternalNet,
      // total_issuance changes: fee is TRANSFERRED to feeDest (not minted/burned),
      // effective_internal_net is BURNED, dust stays with user (no movement).
      totalIssuance'   = totalIssuance - effectiveInternalNet,
      aliceInternal'   = aliceInternal - fee - effectiveInternalNet,
      feeDestInternal' = feeDestInternal + fee,
      reserveExt'      = reserveExt - externalOut,
    }
  }

  action step = any {
    nondet a = oneOf(1.to(10).map(i => i * factor)); mint(a),
    nondet a = oneOf(1.to(500)); redeem(a),
  }
}
