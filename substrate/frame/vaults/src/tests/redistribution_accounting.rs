//! Bug-revealing tests for the redistribution / aggregate-interest accounting
//! and FinalRecovery exit / orchestrator-trust hot spots.
//!
//! Each test names the bucket (`A1` … `A5`) from the deep-analysis report and
//! is calibrated to fail on the current `master` and pass after the matching
//! fix in Phase 1 of the plan.
//!
//! Conventions:
//! - Vaults are opened with stake == collateral (the snapshot ratio is 1:1 in the current
//!   implementation; this is invariant after Phase 2 too since `held_collateral(...)` replaces
//!   `vault.redistribution_stake`).
//! - "Recipient rate" means the recipient vault's `annual_rate`, not the liquidated vault's rate.

use crate::{
	mock::*,
	pallet::{BranchStates, Vaults},
	tests::{rate_pct, vault_status},
};
use frame::deps::{
	frame_support::{assert_err, assert_ok},
	sp_runtime::{traits::Saturating, FixedPointNumber, FixedU128},
};
use pusd_primitives::{KeeperCompensation, LiquidationAllocation, OffsetAllocation};

const ONE_YEAR_MS: Moment = 31_557_600_000;

/// `floor(x * rate)` for the recipient-rate assertions.
fn weighted(x: Balance, rate: FixedU128) -> Balance {
	rate.saturating_mul_int(x)
}

// After a redistribute-everything liquidation with recipients all at 5%, the
// branch's `debt.weighted_principal_sum` should reflect the economic
// debt at the recipient rate, not the redistributed principal at rate=1.0.
//
// Current implementation increments by `redistributed_debt * 1.0`, so the
// post-liquidation weighted sum is ~20× the correct value.
#[test]
fn weighted_sum_after_redistribution_matches_avg_recipient_rate() {
	build_and_execute(|| {
		register_default_branch();
		// Two vaults at 5% / $500 each. Both stakes are 1000.
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(5, 100)));

		// Drop price below MCR. Vault 1 is now liquidatable.
		set_price(DOT, FixedU128::from_rational(5u128, 100u128));

		let coll_1 = held(DOT, 1);
		assert_ok!(liquidate_with(DOT, 1, |_| LiquidationAllocation {
			offset: OffsetAllocation { recipient: 0, debt: 0, collateral: 0 },
			redistribution_collateral: coll_1,
			keeper: KeeperCompensation { recipient: 1, collateral: 0 },
		}));

		let bs = BranchStates::<Test>::get(DOT).expect("branch state");
		let total_econ = bs.debt.principal.saturating_add(bs.debt.pending_redist_principal);
		// Post-fix: weighted_sum ≈ total_econ * 0.05 (B's rate, the only recipient).
		let expected = weighted(total_econ, rate_pct(5, 100));
		let actual = bs.debt.weighted_principal_sum;
		// Tolerance: a couple of dust units from ceil/floor mismatches.
		assert!(
			actual.abs_diff(expected) <= 3,
			"weighted_sum after redistribution out of bounds: actual={}, expected={} (5% of {})",
			actual,
			expected,
			total_econ,
		);
	});
}

// Post-liquidation: advance one year, force a poke so `update_aggregate_interest`
// runs against the redistributed share, and check that the minted aggregate
// interest is bounded by the recipient rate (5%/yr), not by ~100%/yr.
//
// Under the bug, weighted_sum carries the redistributed principal at 1.0, so
// minted interest after 1y is roughly the entire redistributed_debt — ~20×
// what recipients owe.
#[test]
fn aggregate_interest_post_redistribution_bounded_by_recipient_rates() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(5, 100)));
		set_price(DOT, FixedU128::from_rational(5u128, 100u128));
		let coll_1 = held(DOT, 1);
		assert_ok!(liquidate_with(DOT, 1, |_| LiquidationAllocation {
			offset: OffsetAllocation { recipient: 0, debt: 0, collateral: 0 },
			redistribution_collateral: coll_1,
			keeper: KeeperCompensation { recipient: 1, collateral: 0 },
		}));

		let pre_minted = BranchStates::<Test>::get(DOT).unwrap().debt.minted_interest;
		let bs_pre = BranchStates::<Test>::get(DOT).unwrap();
		let total_econ_pre =
			bs_pre.debt.principal.saturating_add(bs_pre.debt.pending_redist_principal);

		// Advance one year, then poke vault 2 to fold pending interest into
		// `debt.minted_interest` and trigger redistribution apply.
		advance_time(ONE_YEAR_MS);
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(99), 2, DOT));

		let post_minted = BranchStates::<Test>::get(DOT).unwrap().debt.minted_interest;
		let delta = post_minted.saturating_sub(pre_minted);

		// Expected: ~5% / yr on the total economic debt at liquidation time.
		// Allow a +/- 20% band: simple_interest_ceil rounds up, and per-vault
		// attribution may slightly under- or over-attribute redistribution.
		let target = weighted(total_econ_pre, rate_pct(5, 100));
		let lower = target.saturating_mul(80).saturating_div(100);
		let upper = target.saturating_mul(120).saturating_div(100);
		assert!(
			delta >= lower && delta <= upper,
			"1y interest mint out of band: delta={}, target≈{}, total_econ={}",
			delta,
			target,
			total_econ_pre,
		);
	});
}

// Setup three vaults at different rates; liquidate one with full
// redistribution. After both surviving recipients touch, the branch
// `debt.weighted_principal_sum` should equal the sum of each
// recipient's post-touch contribution at its **own** rate, within a small
// rounding tolerance.
//
// Pre-fix this is wildly off because the rate=1.0 fold at liquidation time is
// never reconciled per-vault on touch.
#[test]
fn mixed_rate_recipients_reconcile_on_touch() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100))); // A — recipient
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(50, 100))); // B — recipient
		assert_ok!(open(3, DOT, 1_000, 500, rate_pct(10, 100))); // C — liquidated

		set_price(DOT, FixedU128::from_rational(5u128, 100u128));
		let coll_3 = held(DOT, 3);
		assert_ok!(liquidate_with(DOT, 3, |_| LiquidationAllocation {
			offset: OffsetAllocation { recipient: 0, debt: 0, collateral: 0 },
			redistribution_collateral: coll_3,
			keeper: KeeperCompensation { recipient: 3, collateral: 0 },
		}));

		// Force each survivor to touch so their redistribution share is
		// reconciled.
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(99), 1, DOT));
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(99), 2, DOT));

		let bs = BranchStates::<Test>::get(DOT).unwrap();
		let v_a = Vaults::<Test>::get(DOT, 1).unwrap();
		let v_b = Vaults::<Test>::get(DOT, 2).unwrap();
		// Expected: each vault contributes ib_debt * own_rate, summed.
		let expected = weighted(v_a.debt.principal, rate_pct(5, 100))
			.saturating_add(weighted(v_b.debt.principal, rate_pct(50, 100)));
		let actual = bs.debt.weighted_principal_sum;
		// Larger tolerance (10 units): two vaults × per-stake rounding plus the
		// avg-rate-vs-true-rate composition delta.
		assert!(
			actual.abs_diff(expected) <= 10,
			"mixed-rate weighted sum drift too large: actual={}, expected={}",
			actual,
			expected,
		);
	});
}

// A3: a follow-on `borrow` against a recipient must keep the branch
// weighted_sum consistent with each vault's own-rate contribution. Pre-fix,
// borrow recomputes `weighted_old = post_touch_ib_debt * old_rate` while the
// post-touch weighted_sum carries the redistributed share at an unrelated
// rate, so the subtract over-/under-shoots.
#[test]
fn borrow_after_redistribution_keeps_weighted_sum_consistent() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100))); // A — recipient + borrower
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(50, 100))); // B — recipient
		assert_ok!(open(3, DOT, 1_000, 500, rate_pct(10, 100))); // C — liquidated

		set_price(DOT, FixedU128::from_rational(5u128, 100u128));
		let coll_3 = held(DOT, 3);
		assert_ok!(liquidate_with(DOT, 3, |_| LiquidationAllocation {
			offset: OffsetAllocation { recipient: 0, debt: 0, collateral: 0 },
			redistribution_collateral: coll_3,
			keeper: KeeperCompensation { recipient: 3, collateral: 0 },
		}));
		// Restore price so A can borrow.
		set_price(DOT, FixedU128::from_rational(10u128, 1u128));

		// A borrows 200 more. This implicitly touches A (folding redistribution)
		// and then updates the weighted-sum bookkeeping.
		assert_ok!(crate::Pallet::<Test>::borrow(
			RuntimeOrigin::signed(1),
			DOT,
			200,
			None,
			None,
			Position::endpoints_only(),
		));
		// Touch B too so its share is reconciled.
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(99), 2, DOT));

		let bs = BranchStates::<Test>::get(DOT).unwrap();
		let v_a = Vaults::<Test>::get(DOT, 1).unwrap();
		let v_b = Vaults::<Test>::get(DOT, 2).unwrap();
		let expected = weighted(v_a.debt.principal, rate_pct(5, 100))
			.saturating_add(weighted(v_b.debt.principal, rate_pct(50, 100)));
		let actual = bs.debt.weighted_principal_sum;
		assert!(
			actual.abs_diff(expected) <= 10,
			"weighted_sum drift after borrow: actual={}, expected={}",
			actual,
			expected,
		);
	});
}

// Push a vault into FinalRecovery, raise the price so the fully-accrued CR
// goes above MCR, and `poke` it. Pre-fix, `touch_vault` auto-exits via an
// unhinted `find_position` (O(n)) — the test asserts the new behavior: poke
// leaves the vault in FinalRecovery and a dedicated `exit_final_recovery`
// extrinsic does the index re-insert with caller-supplied hints.
#[test]
fn final_recovery_exit_requires_explicit_hint() {
	build_and_execute(|| {
		register_default_branch();
		// Single vault so it's the "last eligible redistribution recipient"
		// — `enter_final_recovery` only allows the last stake holder.
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		// Drop the price below MCR so the vault becomes recovery-eligible.
		set_price(DOT, FixedU128::from_rational(5u128, 100u128));
		assert_ok!(crate::Pallet::<Test>::enter_final_recovery(RuntimeOrigin::signed(99), 1, DOT,));
		assert!(matches!(vault_status(DOT, 1), crate::types::VaultStatus::FinalRecovery));
		// Raise the price back so the vault's CR is now ≥ MCR.
		set_price(DOT, FixedU128::from_rational(10u128, 1u128));
		// Poke MUST NOT auto-exit.
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(99), 1, DOT));
		assert!(
			matches!(vault_status(DOT, 1), crate::types::VaultStatus::FinalRecovery),
			"poke should not auto-exit FinalRecovery; current code does via unhinted find_position",
		);
		// Explicit `exit_final_recovery` does.
		assert_ok!(crate::Pallet::<Test>::exit_final_recovery(
			RuntimeOrigin::signed(99),
			1,
			DOT,
			Position::endpoints_only(),
		));
		assert!(matches!(vault_status(DOT, 1), crate::types::VaultStatus::Active));
	});
}

// Test asserts the post-fix API: `OffsetAllocation` carries a `recipient`
// AccountId and `finalize_liquidation` moves `offset.collateral` to it.
// This test will fail to compile on `master` (no `recipient` field) — that
// compile error is the bug signal. Post-fix it compiles and passes.
#[test]
fn finalize_liquidation_doesnt_leak_offset_collateral_to_liquidatee() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(5, 100)));
		set_price(DOT, FixedU128::from_rational(5u128, 100u128));

		let recipient: AccountId = 999;
		let pre_recipient = collateral_balance(DOT, recipient);

		assert_ok!(liquidate_with(DOT, 1, |post_touch| LiquidationAllocation {
			offset: OffsetAllocation { recipient, debt: post_touch, collateral: 500 },
			redistribution_collateral: 0,
			keeper: KeeperCompensation { recipient: 1, collateral: 0 },
		}));

		let post_recipient = collateral_balance(DOT, recipient);
		assert_eq!(
			post_recipient.saturating_sub(pre_recipient),
			500,
			"offset.collateral should land on the offset recipient, not the liquidatee",
		);
	});
}

#[test]
fn prepare_liquidation_refuses_dust_recipient_below_floor() {
	build_and_execute(|| {
		register_default_branch(); // floor = 100
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100))); // liquidatee, stake=1_000
														  // Vault 2 needs ICR ≥ 1.2 at price 10:1, debt ≥ 200, so coll ≥ 24.
														  // Stake = 50 sits below the floor of 100.
		assert_ok!(open(2, DOT, 50, 200, rate_pct(5, 100))); // recipient, stake=50

		set_price(DOT, FixedU128::from_rational(5u128, 100u128));

		let pre_bs = BranchStates::<Test>::get(DOT).unwrap();
		let pre_redist = pre_bs.redist;
		assert_err!(liquidate(DOT, 1), crate::Error::<Test>::LastVaultCannotBeLiquidated,);
		// Floor fires inside `prepare_liquidation` before any redistribution
		// accumulator mutation; nothing about the branch state moves.
		let post_bs = BranchStates::<Test>::get(DOT).unwrap();
		let post_redist = post_bs.redist;
		assert_eq!(post_bs.stakes.total, pre_bs.stakes.total);
		assert_eq!(post_bs.debt.pending_redist_principal, 0);
		assert_eq!(post_bs.debt.weighted_principal_sum, pre_bs.debt.weighted_principal_sum,);
		assert_eq!(pre_redist, post_redist);
	});
}

#[test]
fn redist_per_stake_overflow_unit_check_for_completeness() {
	// num/denom = u128::MAX/2 / 1 → quotient * 1e18 > u128::MAX.
	let got = crate::math::redist_per_stake::<Balance>(u128::MAX / 2, 1);
	assert!(got.is_none(), "overflow must surface as None, never silently zero");
	// Boundary safety: just below the overflow threshold survives.
	let safe = crate::math::redist_per_stake::<Balance>(u128::MAX / (FixedU128::DIV * 2), 1);
	assert!(safe.is_some());
}

#[test]
fn back_to_back_near_empty_redistributions_preserve_accounting_identity() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(crate::Pallet::<Test>::set_minimum_total_stakes(
			RuntimeOrigin::root(),
			DOT,
			100,
		));
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(5, 100)));
		assert_ok!(open(3, DOT, 5_000, 500, rate_pct(5, 100)));

		set_price(DOT, FixedU128::from_rational(5u128, 100u128));

		for liquidatee in [1u64, 2u64] {
			let coll = held(DOT, liquidatee);
			assert_ok!(liquidate_with(DOT, liquidatee, |_| LiquidationAllocation {
				offset: OffsetAllocation { recipient: 0, debt: 0, collateral: 0 },
				redistribution_collateral: coll,
				keeper: KeeperCompensation { recipient: liquidatee, collateral: 0 },
			}));
			assert_accounting_identity_holds();
		}
	});
}

#[test]
fn vault_cr_view_includes_pending_redistribution() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(5, 100)));
		assert_ok!(open(3, DOT, 1_000, 500, rate_pct(5, 100)));

		set_price(DOT, FixedU128::from_rational(5u128, 100u128));
		let coll_3 = held(DOT, 3);
		assert_ok!(liquidate_with(DOT, 3, |_| LiquidationAllocation {
			offset: OffsetAllocation { recipient: 0, debt: 0, collateral: 0 },
			redistribution_collateral: coll_3,
			keeper: KeeperCompensation { recipient: 3, collateral: 0 },
		}));
		// Restore price so the view's CR is defined for vault 1.
		set_price(DOT, FixedU128::from_rational(10u128, 1u128));

		let view_pre = crate::Pallet::<Test>::vault_cr(DOT, 1).expect("cr");
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(99), 1, DOT));
		let view_post = crate::Pallet::<Test>::vault_cr(DOT, 1).expect("cr");
		// Pre- and post-touch view should agree: the view replays the same
		// pending-redistribution math `touch_vault` commits.
		assert_eq!(view_pre, view_post);
	});
}

fn assert_accounting_identity_holds() {
	let bs = BranchStates::<Test>::get(DOT).unwrap();
	let cumul = bs.redist.debt_per_stake;
	let mut sum_shares: Balance = 0;
	let mut n: u128 = 0;
	for (_owner, vault) in Vaults::<Test>::iter_prefix(DOT) {
		let snap = vault.redist_snapshot;
		let delta = cumul.saturating_sub(snap.debt_per_stake);
		sum_shares =
			sum_shares.saturating_add(delta.saturating_mul_int(vault.redistribution_stake));
		n += 1;
	}
	let tolerance: Balance = n;
	let drift = bs.debt.pending_redist_principal.abs_diff(sum_shares);
	assert!(
		drift <= tolerance,
		"pending redist principal drift: pending={}, sum_shares={}, drift={}, tol={}",
		bs.debt.pending_redist_principal,
		sum_shares,
		drift,
		tolerance,
	);
}
