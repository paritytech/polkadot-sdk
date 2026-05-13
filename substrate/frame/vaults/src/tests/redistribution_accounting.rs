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
//!   `vault.stake`).
//! - "Recipient rate" means the recipient vault's `annual_rate`, not the liquidated vault's rate.

use crate::{
	mock::*,
	pallet::{BranchStates, Vaults},
	tests::rate_pct,
};
use frame::deps::{
	frame_support::assert_ok,
	sp_runtime::{FixedPointNumber, FixedU128},
};
use pusd_primitives::{KeeperCompensation, LiquidationAllocation, OffsetAllocation};

const ONE_YEAR_MS: Moment = 31_557_600_000;

/// `floor(x * rate)` for the recipient-rate assertions.
fn weighted(x: Balance, rate: FixedU128) -> Balance {
	rate.saturating_mul_int(x)
}

// After a redistribute-everything liquidation with recipients all at 5%, the
// branch's `weighted_interest_bearing_debt_sum` should reflect the economic
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
		let total_econ =
			bs.total_interest_bearing_debt.saturating_add(bs.pending_redistribution_debt);
		// Post-fix: weighted_sum ≈ total_econ * 0.05 (B's rate, the only recipient).
		let expected = weighted(total_econ, rate_pct(5, 100));
		let actual = bs.weighted_interest_bearing_debt_sum;
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

		let pre_minted = BranchStates::<Test>::get(DOT).unwrap().total_minted_aggregate_interest;
		let bs_pre = BranchStates::<Test>::get(DOT).unwrap();
		let total_econ_pre = bs_pre
			.total_interest_bearing_debt
			.saturating_add(bs_pre.pending_redistribution_debt);

		// Advance one year, then poke vault 2 to fold pending interest into
		// `total_minted_aggregate_interest` and trigger redistribution apply.
		advance_time(ONE_YEAR_MS);
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(99), 2, DOT));

		let post_minted = BranchStates::<Test>::get(DOT).unwrap().total_minted_aggregate_interest;
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
// `weighted_interest_bearing_debt_sum` should equal the sum of each
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
		let expected = weighted(v_a.interest_bearing_debt, rate_pct(5, 100))
			.saturating_add(weighted(v_b.interest_bearing_debt, rate_pct(50, 100)));
		let actual = bs.weighted_interest_bearing_debt_sum;
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
			None,
			None,
		));
		// Touch B too so its share is reconciled.
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(99), 2, DOT));

		let bs = BranchStates::<Test>::get(DOT).unwrap();
		let v_a = Vaults::<Test>::get(DOT, 1).unwrap();
		let v_b = Vaults::<Test>::get(DOT, 2).unwrap();
		let expected = weighted(v_a.interest_bearing_debt, rate_pct(5, 100))
			.saturating_add(weighted(v_b.interest_bearing_debt, rate_pct(50, 100)));
		let actual = bs.weighted_interest_bearing_debt_sum;
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
		assert!(matches!(
			Vaults::<Test>::get(DOT, 1).unwrap().status,
			crate::types::VaultStatus::FinalRecovery
		));
		// Raise the price back so the vault's CR is now ≥ MCR.
		set_price(DOT, FixedU128::from_rational(10u128, 1u128));
		// Poke MUST NOT auto-exit.
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(99), 1, DOT));
		assert!(
			matches!(
				Vaults::<Test>::get(DOT, 1).unwrap().status,
				crate::types::VaultStatus::FinalRecovery
			),
			"poke should not auto-exit FinalRecovery; current code does via unhinted find_position",
		);
		// Explicit `exit_final_recovery` does.
		assert_ok!(crate::Pallet::<Test>::exit_final_recovery(
			RuntimeOrigin::signed(99),
			1,
			DOT,
			None,
			None,
		));
		assert!(matches!(
			Vaults::<Test>::get(DOT, 1).unwrap().status,
			crate::types::VaultStatus::Active
		));
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
