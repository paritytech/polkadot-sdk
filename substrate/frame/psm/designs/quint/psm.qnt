// psm.qnt
//
// Minimal Quint model of the Polkadot SDK Peg Stability Module (PSM).
//
// Motivating problem (from case-bidirectional-debt.md):
// All 12 existing `try_state` checks bound `PsmDebt` from ABOVE.
// A bug that UNDERSTATES `PsmDebt` makes every one of them easier to pass.
// The stateful fuzzer ran 48,000+ commands against a redeem-side
// understatement bug without catching it.
//
// This model encodes a BIDIRECTIONAL invariant:
//   PsmDebt == TotalInflow - TotalOutflow
// and lets us model-check it exhaustively over a bounded state space.
//
// Abstractions (kept small so model checking stays tractable):
//   - one external asset
//   - same decimals on both sides
//   - zero fee
//   - amounts bounded to a small range

module psm {

  // ---------- State ----------

  var psmDebt: int          // PsmDebt<asset> in internal units
  var reserve: int          // external asset balance held by PSM
  var totalInflow: int      // cumulative external_amount across all mints
  var totalOutflow: int     // cumulative external_to_user across all redeems
  var ceiling: int          // per-asset debt ceiling
  var totalIssuance: int    // internal asset (pUSD) in circulation

  // Trace instrumentation for QC replay: which action produced this state.
  // itf_to_rust.py reads these fields to reconstruct the action sequence.
  var last_action: str
  var last_action_args: List[int]

  // ---------- Invariants ----------
  //
  // The real PSM `try_state` distinguishes HARD checks (always hold) from
  // WARN checks (governance may transiently violate them by lowering a
  // ceiling below existing debt). The model mirrors that distinction.

  // HARD: reserve never falls below tracked debt.
  val reserveGteDebt   = reserve       >= psmDebt
  // HARD: total internal issuance never falls below tracked debt.
  val issuanceGteDebt  = totalIssuance >= psmDebt

  // The NEW bidirectional invariant (also HARD).
  //
  // Mint adds `external_amount` to inflow and `external_amount` to psmDebt.
  // Redeem adds `external_to_user` to outflow and subtracts the same from psmDebt.
  // So at every reachable state:
  //   psmDebt == totalInflow - totalOutflow
  //
  // An understatement bug (recording `pusd_to_user` instead of `external_amount`,
  // or subtracting `pusd_amount` instead of `external_to_user`) breaks this
  // invariant on the very first buggy mint/redeem.
  val debtMatchesFlow  = psmDebt == totalInflow - totalOutflow

  // WARN: can be transiently violated by `setCeiling` lowering the ceiling
  // below current debt. Excluded from the hard safety invariant.
  val debtLeqCeiling   = psmDebt <= ceiling

  // Hard safety invariant: must hold in every reachable state.
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
  // User deposits `amount` external. Full `amount` enters reserve.
  // psmDebt and totalInflow each increase by `amount`.
  action mint(amount: int): bool = all {
    amount > 0,
    psmDebt + amount <= ceiling,
    psmDebt'       = psmDebt + amount,
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
  // User burns `amount` internal. PSM sends `amount` external back out.
  // psmDebt decreases by `amount`. totalOutflow grows by `amount`.
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
  // Lower or raise the per-asset ceiling. Does not move funds.
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
