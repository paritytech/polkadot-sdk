// psm.qnt — Minimal Quint model for the quint-connect PoC.
//
// Single asset, single user, no fees, no governance, no decimals. The only
// purpose of this model is to demonstrate that quint-connect can:
//   1. generate traces from a Quint spec,
//   2. execute the corresponding actions against a real pallet-psm via
//      the Driver in quint_mbt.rs,
//   3. assert after each step that the model's (debt, reserve) tuple
//      matches the values read from pallet storage.
//
// Property checked: reserve == debt after every reachable state.
// (This is stronger than try_state Check 2 reserve >= debt; equality
// holds because there are no donations or fees in this minimal model.)

module psm {
  var debt: int
  var reserve: int

  // ---------- Init ----------

  action init = all {
    debt'    = 0,
    reserve' = 0,
  }

  // ---------- Mint ----------
  //
  // User deposits `amount` external units; same-decimal, no fee, so:
  //   debt    += amount
  //   reserve += amount
  action mint(amount: int): bool = all {
    amount > 0,
    debt'    = debt + amount,
    reserve' = reserve + amount,
  }

  // ---------- Redeem ----------
  //
  // User burns `amount` internal units; same-decimal, no fee, so:
  //   debt    -= amount
  //   reserve -= amount
  action redeem(amount: int): bool = all {
    amount > 0,
    amount <= debt,
    debt'    = debt - amount,
    reserve' = reserve - amount,
  }

  // ---------- Step ----------

  action step = any {
    nondet amount = oneOf(1.to(10)); mint(amount),
    nondet amount = oneOf(1.to(10)); redeem(amount),
  }
}
