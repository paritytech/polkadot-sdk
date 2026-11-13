// psm.qnt
//
// Minimal Quint model of the Polkadot SDK Peg Stability Module (PSM), plus
// the negative control that proves the model can fail.
//
// Motivating problem:
// Every `try_state` check bounds `PsmDebt` from above: reserve >= debt,
// issuance >= debt, debt <= ceiling. A bug that UNDERSTATES `PsmDebt` moves
// all of them further from their boundary, so none of them can see it. The
// stateful fuzzer ran 48,000 commands against a redeem-side understatement
// without catching it.
//
// This model adds a BIDIRECTIONAL invariant:
//   PsmDebt == TotalInflow - TotalOutflow
// which pins the debt from both sides. The pallet does not state it today,
// because it keeps no inflow or outflow ledger. The model keeps one, so the
// model-based tests can hold the pallet to the invariant without the pallet
// storing it.
//
// Abstractions, kept small so model checking stays tractable:
//   - one PSM instance, one external asset. `PsmDebt` is a double map in the
//     pallet, keyed by (internal, external); every invariant modelled here is
//     per-instance, so the instance dimension adds no reachable behaviour.
//   - same decimals on both sides, zero fee, small amounts.
//     `psm_extended.qnt` drops all three.

module psm {

  // When true, `mint` records less debt than the amount that entered the
  // reserve. Instantiated false for the model, true for the control.
  const understateDebtOnMint: bool

  // ---------- State ----------

  var psmDebt: int          // PsmDebt[internal, external], in internal units
  var reserve: int          // external asset balance held by the PSM
  var totalInflow: int      // cumulative external_amount across all mints
  var totalOutflow: int     // cumulative external_to_user across all redeems
  var ceiling: int          // max_asset_debt for this pair
  var totalIssuance: int    // internal asset in circulation

  // Trace instrumentation for replay: which action produced this state.
  var last_action: str
  var last_action_args: List[int]

  // ---------- Invariants ----------
  //
  // `do_try_state` has seventeen checks, five of them advisory: an advisory
  // check warns and returns Ok, because a permitted call can create the state
  // it reports. The model keeps the same split.

  // Check 8, hard: the reserve covers the tracked debt.
  val reserveGteDebt   = reserve       >= psmDebt
  // Check 6, hard: internal issuance covers the tracked debt.
  val issuanceGteDebt  = totalIssuance >= psmDebt

  // The bidirectional invariant. Mint adds `amount` to both the inflow and
  // the debt; redeem adds `amount` to the outflow and subtracts it from the
  // debt. An understatement breaks this on the first buggy call.
  val debtMatchesFlow  = psmDebt == totalInflow - totalOutflow

  // Checks 10 and 17, advisory: `set_max_debt` and `set_asset_ceiling_weight`
  // can put the ceiling under the existing debt, so this is excluded from the
  // hard invariant.
  val debtLeqCeiling   = psmDebt <= ceiling

  val safetyInvariant =
    reserveGteDebt and issuanceGteDebt and debtMatchesFlow

  // ---------- Init ----------

  action init = all {
    psmDebt'       = 0,
    reserve'       = 0,
    totalInflow'   = 0,
    totalOutflow'  = 0,
    ceiling'       = 100,
    totalIssuance' = 0,
    last_action'   = "init",
    last_action_args' = [],
  }

  // ---------- Mint ----------
  //
  // The user deposits `amount` external, which enters the reserve in full.

  pure def recordedDebt(amount: int): int =
    if (understateDebtOnMint) amount - 1 else amount

  action mint(amount: int): bool = all {
    amount > 0,
    recordedDebt(amount) > 0,
    psmDebt + amount <= ceiling,
    psmDebt'       = psmDebt + recordedDebt(amount),
    reserve'       = reserve + amount,
    totalInflow'   = totalInflow + amount,
    totalIssuance' = totalIssuance + amount,
    totalOutflow'  = totalOutflow,
    ceiling'       = ceiling,
    last_action'   = "mint",
    last_action_args' = [amount],
  }

  // ---------- Redeem ----------
  //
  // The user burns `amount` internal and the PSM sends `amount` external out.
  action redeem(amount: int): bool = all {
    amount > 0,
    amount <= psmDebt,
    amount <= reserve,
    psmDebt'       = psmDebt - amount,
    reserve'       = reserve - amount,
    totalOutflow'  = totalOutflow + amount,
    totalIssuance' = totalIssuance - amount,
    totalInflow'   = totalInflow,
    ceiling'       = ceiling,
    last_action'   = "redeem",
    last_action_args' = [amount],
  }

  // ---------- Governance ----------
  //
  // Raise or lower the ceiling for this pair. Moves no funds.
  action setCeiling(newCeiling: int): bool = all {
    newCeiling >= 0,
    ceiling'       = newCeiling,
    psmDebt'       = psmDebt,
    reserve'       = reserve,
    totalInflow'   = totalInflow,
    totalOutflow'  = totalOutflow,
    totalIssuance' = totalIssuance,
    last_action'   = "set_ceiling",
    last_action_args' = [newCeiling],
  }

  // ---------- Step ----------

  action step = any {
    nondet a = oneOf(1.to(10)); mint(a),
    nondet a = oneOf(1.to(10)); redeem(a),
    nondet c = oneOf(0.to(100)); setCeiling(c),
  }
}

// The model. `safetyInvariant` holds.
module psm_correct {
  import psm(understateDebtOnMint = false).*
}

// The negative control. `mint` records one unit less debt than entered the
// reserve, which is the understatement class the try_state checks cannot see.
// `safetyInvariant` must fail here, and does, on the first mint.
module psm_buggy {
  import psm(understateDebtOnMint = true).*
}
