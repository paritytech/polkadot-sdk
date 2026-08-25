// psm_bidirectional.qnt — Counterfactual model paired with the bug-injection
// feature `inject-debt-understatement-bug` in pallet-psm.
// See docs/case-bidirectional-debt.md for the motivation.
//
// Amounts and state are in pallet units (= MinSwapAmount = 1e8 per abstract
// step), so a 1-pallet-unit understatement bug per redeem is visible on the
// very first redeem when compared against the pallet.

module psm_bidirectional {
  pure val SCALE: int = 100000000

  var debt: int
  var reserve: int
  var inflow: int
  var outflow: int

  action init = all {
    debt'    = 0,
    reserve' = 0,
    inflow'  = 0,
    outflow' = 0,
  }

  action mint(amount: int): bool = all {
    amount > 0,
    debt'    = debt + amount,
    reserve' = reserve + amount,
    inflow'  = inflow + amount,
    outflow' = outflow,
  }

  action redeem(amount: int): bool = all {
    amount > 0,
    amount <= debt,
    debt'    = debt - amount,
    reserve' = reserve - amount,
    inflow'  = inflow,
    outflow' = outflow + amount,
  }

  action step = any {
    nondet amount = oneOf(2.to(10).map(i => i * SCALE)); mint(amount),
    nondet amount = oneOf(2.to(10).map(i => i * SCALE)); redeem(amount),
  }
}
