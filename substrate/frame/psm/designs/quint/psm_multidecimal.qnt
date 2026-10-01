// psm_multidecimal.qnt
//
// Models the PSM with multi-decimal external assets.
//
// PsmDebt is stored in INTERNAL units.
// The external asset has different decimals; the pallet scales between them.
//
// Question: does the bidirectional invariant from case-bidirectional-debt.md
// still hold when `TotalMintedInflow` is incremented by `external_amount`
// (in EXTERNAL units) and compared to `PsmDebt` (in INTERNAL units)?
//
// Setup: factor = 10^(ext_decimals - internal_decimals).
// We model DAI vs internal: ext_decimals=18, internal_decimals=6, factor=10^12.
//
// `external_to_internal(x) = x / factor`
// `internal_to_external(x) = x * factor`

module psm_multidecimal {

  // Use factor=100 to keep model checking tractable (analogous to 10^k).
  pure val factor: int = 100

  var psmDebt: int           // internal units
  var reserve: int           // external units (raw)
  var totalInflowExt: int    // external units — what the proposal increments
  var totalOutflowExt: int   // external units — what the proposal increments
  var totalInflowInt: int    // internal units — alternative tracking
  var totalOutflowInt: int   // internal units — alternative tracking

  // The proposal's invariant (from case-bidirectional-debt.md):
  //   `debt == inflow.saturating_sub(outflow)`
  // where inflow/outflow are tracked from `external_amount` / `external_to_user`,
  // which are in EXTERNAL units. psmDebt is in INTERNAL units.
  val proposalInvariant =
    psmDebt == totalInflowExt - totalOutflowExt

  // The fixed alternative: track inflow/outflow in INTERNAL units (i.e.
  // `internal_equivalent` on mint, `effective_internal_net` on redeem).
  val fixedInvariant =
    psmDebt == totalInflowInt - totalOutflowInt

  action init = all {
    psmDebt'         = 0,
    reserve'         = 0,
    totalInflowExt'  = 0,
    totalOutflowExt' = 0,
    totalInflowInt'  = 0,
    totalOutflowInt' = 0,
  }

  // ---------- Mint ----------
  //
  // User submits `externalAmount` in external units.
  // PSM converts via floor-div by factor (external_to_internal).
  // For simplicity, we restrict to amounts that are exact multiples of
  // `factor` so there is no scale-down dust. This is the BEST case for the
  // proposal: if the invariant fails here, it fails everywhere.
  action mint(externalAmount: int): bool = all {
    externalAmount > 0,
    externalAmount % factor == 0,                  // no dust
    val internalEquivalent = externalAmount / factor
    all {
      psmDebt'         = psmDebt + internalEquivalent,
      reserve'         = reserve + externalAmount,
      totalInflowExt'  = totalInflowExt + externalAmount,    // proposal: tracks ext
      totalInflowInt'  = totalInflowInt + internalEquivalent,// fixed: tracks int
      totalOutflowExt' = totalOutflowExt,
      totalOutflowInt' = totalOutflowInt,
    }
  }

  // ---------- Redeem ----------
  action redeem(internalAmount: int): bool = all {
    internalAmount > 0,
    internalAmount <= psmDebt,
    val externalOut = internalAmount * factor
    all {
      externalOut <= reserve,
      psmDebt'         = psmDebt - internalAmount,
      reserve'         = reserve - externalOut,
      totalOutflowExt' = totalOutflowExt + externalOut,         // proposal
      totalOutflowInt' = totalOutflowInt + internalAmount,      // fixed
      totalInflowExt'  = totalInflowExt,
      totalInflowInt'  = totalInflowInt,
    }
  }

  action step = any {
    nondet a = oneOf(1.to(10).map(i => i * factor)); mint(a),
    nondet a = oneOf(1.to(10)); redeem(a),
  }
}
