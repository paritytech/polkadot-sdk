// psm_buggy.qnt
//
// Same model as psm.qnt, but `mint` is intentionally buggy: it records a
// debt smaller than the external amount that entered the reserve. This
// is the "redeem-side" / "mint-side" understatement class of bug
// described in case-bidirectional-debt.md.
//
// Expected result:
//   `quint run --invariant=safetyInvariant` finds a counterexample
//   immediately, because `debtMatchesFlow` breaks on the very first mint.

module psm_buggy {

  var psmDebt: int
  var reserve: int
  var totalInflow: int
  var totalOutflow: int
  var ceiling: int
  var totalIssuance: int

  val reserveGteDebt   = reserve       >= psmDebt
  val issuanceGteDebt  = totalIssuance >= psmDebt
  val debtMatchesFlow  = psmDebt == totalInflow - totalOutflow
  val safetyInvariant  =
    reserveGteDebt and issuanceGteDebt and debtMatchesFlow

  action init = all {
    psmDebt'       = 0,
    reserve'       = 0,
    totalInflow'   = 0,
    totalOutflow'  = 0,
    ceiling'       = 100,
    totalIssuance' = 0,
  }

  // BUGGY mint: records `amount - 1` in psmDebt (simulating a "fee"
  // accidentally subtracted from the recorded debt) while the full
  // `amount` enters the reserve and inflow ledger.
  action mintBuggy(amount: int): bool = all {
    amount > 1,
    psmDebt + amount <= ceiling,
    psmDebt'       = psmDebt + (amount - 1),  // BUG: understates by 1
    reserve'       = reserve + amount,
    totalInflow'   = totalInflow + amount,
    totalIssuance' = totalIssuance + amount,
    totalOutflow'  = totalOutflow,
    ceiling'       = ceiling,
  }

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
  }

  action step = any {
    nondet a = oneOf(2.to(10)); mintBuggy(a),
    nondet a = oneOf(1.to(10)); redeem(a),
  }
}
