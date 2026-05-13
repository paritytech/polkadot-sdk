//! Port of liquity_v2/contracts/test/borrowerOperations.t.sol (lines 19864-20068).
//!
//! Edge cases on the borrower-facing dispatchables: minimum-debt floor,
//! withdrawal caps, upfront-fee charging on open / borrow / rate change, and
//! rate-change cooldown enforcement. Polkadot's API has no `max_fee` argument
//! on any extrinsic, so the three Liquity rows that assert
//! `UpfrontFeeTooHigh` (rows 6, 8, 11) are not portable and live as `SKIPPED`
//! comments at the position they would have occupied.

use crate::{
	mock::*,
	pallet::{BranchStates, Vaults},
	tests::rate_pct,
};
use frame::deps::frame_support::{assert_noop, assert_ok};

// tests.md row 1: testCloseLastTroveReverts.
//
// Liquity reverts on `closeTrove(last)` with `OnlyOneTroveLeft`. The polkadot
// `close_vault` path requires zero debt and otherwise returns
// `InsufficientRepayment`; the actual "system needs at least one vault"
// guard lives on the liquidation path (`prepare_liquidation` returns
// `LastVaultCannotBeLiquidated`). Both behaviours are pinned here.
#[test]
fn close_last_trove_with_debt_reverts() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		assert_noop!(
			crate::Pallet::<Test>::close_vault(RuntimeOrigin::signed(1), DOT, None),
			crate::Error::<Test>::InsufficientRepayment
		);
	});
}

#[test]
fn liquidate_last_trove_reverts() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		// Drive the price down so the lone vault is undercollateralised
		// enough to look like a liquidation candidate.
		set_price(DOT, frame::deps::sp_runtime::FixedU128::from_rational(5u128, 100u128));
		assert_noop!(liquidate(DOT, 1), crate::Error::<Test>::LastVaultCannotBeLiquidated);
	});
}

// tests.md row 2: testRepayingTooMuchDebtCapsAtMinDebt (reformulated).
//
// Liquity caps the residual at `MinimumDebt`. Polkadot's `repay_for` instead
// returns `DebtWouldBecomeDust` when the post-state would land between zero
// and `MinimumDebt`. (Full repayment to zero is allowed.)
#[test]
fn repay_into_dust_window_reverts() {
	build_and_execute(|| {
		register_default_branch();
		// borrow=1000, min_debt=200. Repay 850 would leave 150 < 200.
		assert_ok!(open(1, DOT, 1_000, 1_000, rate_pct(5, 100)));
		assert_noop!(
			crate::Pallet::<Test>::repay_for(RuntimeOrigin::signed(1), 1, DOT, 850),
			crate::Error::<Test>::DebtWouldBecomeDust
		);
	});
}

// tests.md row 3: testWithdrawingTooMuchCollateralReverts.
//
// Withdrawing more collateral than is held returns `InsufficientCollateral`
// (the held-balance check fires before the CR check).
#[test]
fn withdraw_more_than_held_reverts() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		assert_noop!(
			crate::Pallet::<Test>::withdraw_collateral(RuntimeOrigin::signed(1), DOT, 2_000, None,),
			crate::Error::<Test>::InsufficientCollateral
		);
	});
}

#[test]
fn withdraw_breaking_cr_reverts() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		// open another vault so that we don't hit the last-vault rule.
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(5, 100)));
		// 1000 DOT @ $10 backs 500 pUSD — withdrawing 950 leaves
		// 50 DOT × $10 = $500, CR == 100% < ICR 120%.
		assert_noop!(
			crate::Pallet::<Test>::withdraw_collateral(RuntimeOrigin::signed(1), DOT, 950, None),
			crate::Error::<Test>::UnsafeCollateralizationRatio
		);
	});
}

// tests.md row 4: testZeroAdjustmentReverts (reformulated).
//
// Liquity's single `adjustTrove(coll, debt, rate)` reverts on all-zero. The
// polkadot per-action dispatchables instead silently no-op on zero amounts.
// This test pins that behaviour so it can't regress unnoticed: each of
// `deposit_collateral_for`, `withdraw_collateral`, `borrow`, `repay_for` is
// called with `amount=0` and must succeed.
#[test]
fn zero_amount_ops_are_no_ops() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		let pre = Vaults::<Test>::get(DOT, 1).expect("vault stored");

		assert_ok!(crate::Pallet::<Test>::deposit_collateral_for(
			RuntimeOrigin::signed(1),
			1,
			DOT,
			0,
		));
		assert_ok!(crate::Pallet::<Test>::withdraw_collateral(
			RuntimeOrigin::signed(1),
			DOT,
			0,
			None,
		));
		assert_ok!(crate::Pallet::<Test>::borrow(
			RuntimeOrigin::signed(1),
			DOT,
			0,
			None,
			None,
			Position::endpoints_only(),
		));
		assert_ok!(crate::Pallet::<Test>::repay_for(RuntimeOrigin::signed(1), 1, DOT, 0));

		let post = Vaults::<Test>::get(DOT, 1).expect("vault stored");
		assert_eq!(pre.interest_bearing_debt, post.interest_bearing_debt);
		assert_eq!(pre.accrued_interest, post.accrued_interest);
		assert_eq!(held(DOT, 1), 1_000);
	});
}

// tests.md row 5: testOpenTroveChargesUpfrontFee.
//
// The upfront fee charged at open should equal the `predict_open_upfront_fee`
// view-function quote, and the vault's recorded debt should be
// `initial_debt + upfront_fee`. The fee is added to `accrued_interest` and
// also bumps `branch.total_minted_aggregate_interest`. The fee must be
// strictly positive for the canonical (debt=10k, rate=5%) inputs — Liquity's
// test asserts this implicitly by computing it via `calcUpfrontFee`.
#[test]
fn open_trove_charges_upfront_fee() {
	build_and_execute(|| {
		register_default_branch();
		let predicted =
			crate::Pallet::<Test>::predict_open_upfront_fee(DOT, 10_000, rate_pct(5, 100));
		assert!(predicted > 0, "open at 10k @ 5% must charge a non-trivial upfront fee");
		assert_ok!(open(1, DOT, 5_000, 10_000, rate_pct(5, 100)));
		let v = Vaults::<Test>::get(DOT, 1).expect("vault stored");
		assert_eq!(v.interest_bearing_debt, 10_000);
		assert_eq!(v.accrued_interest, predicted);
		let bs = BranchStates::<Test>::get(DOT).expect("branch state");
		assert_eq!(bs.total_minted_aggregate_interest, predicted);
	});
}

// SKIPPED: row 6 `testOpenTroveRevertsIfUpfrontFeeExceedsUserProvidedLimit`
// — polkadot `open_vault` has no `max_fee` parameter; callers must read
// `predict_open_upfront_fee` off-chain and decide before submitting.

// tests.md row 7: testWithdrawBoldChargesUpfrontFee → polkadot `borrow`.
//
// Liquity's `withdrawBold` is a debt-only borrow. In polkadot it is the
// `borrow` extrinsic with no rate change. The recorded debt grows by
// `amount + upfront_fee`.
#[test]
fn borrow_charges_upfront_fee() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 5_000, 10_000, rate_pct(5, 100)));
		let v_before = Vaults::<Test>::get(DOT, 1).expect("vault stored");

		let predicted = crate::Pallet::<Test>::predict_borrow_upfront_fee(DOT, 1, 1_000, None);
		assert_ok!(crate::Pallet::<Test>::borrow(
			RuntimeOrigin::signed(1),
			DOT,
			1_000,
			None,
			None,
			Position::endpoints_only(),
		));
		let v_after = Vaults::<Test>::get(DOT, 1).expect("vault stored");
		assert_eq!(v_after.interest_bearing_debt, v_before.interest_bearing_debt + 1_000);
		assert_eq!(v_after.accrued_interest, v_before.accrued_interest + predicted);
	});
}

// SKIPPED: row 8 `testWithdrawBoldRevertsIfUpfrontFeeExceedsUserProvidedLimit`
// — same reason as row 6: no `max_fee` on `borrow`.

// tests.md row 9: testAdjustInterestRateFailsIfNotNew (reformulated).
//
// Liquity reverts on `adjustInterestRate` to the same rate. Polkadot's
// `change_rate` returns `Ok(())` early without touching state. This test
// pins the no-op semantics: storage row before and after is byte-identical.
#[test]
fn change_rate_to_same_rate_is_no_op() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 5_000, 10_000, rate_pct(5, 100)));
		let pre = Vaults::<Test>::get(DOT, 1).expect("vault stored");
		assert_ok!(crate::Pallet::<Test>::change_rate(
			RuntimeOrigin::signed(1),
			DOT,
			rate_pct(5, 100),
			Position::endpoints_only(),
		));
		let post = Vaults::<Test>::get(DOT, 1).expect("vault stored");
		assert_eq!(pre, post);
	});
}

// tests.md row 10: testAdjustInterestRateChargesUpfrontFeeWhenPremature.
//
// Two assertions:
//   - inside the cooldown window the rate change charges a strictly positive fee matching
//     `predict_rate_change_upfront_fee`,
//   - after the cooldown elapses the fee is zero.
//
// Methodology note: `change_rate` internally calls `touch_vault`, which
// folds elapsed simple interest into `vault.accrued_interest`. To isolate
// the upfront-fee delta we poke the vault first so all pending interest is
// already materialised — then `vault.accrued_interest` only grows by the
// upfront fee.
#[test]
fn change_rate_charged_fee_matches_predict() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 5_000, 10_000, rate_pct(5, 100)));
		let cfg_cooldown = 24 * 3_600 * 1_000u64; // matches default_branch_config()

		// --- Phase 1: rate change BEFORE cooldown elapses.
		advance_time(cfg_cooldown / 2);
		// Poke first so any elapsed simple interest is already in
		// `vault.accrued_interest` and the change_rate delta isolates the
		// upfront fee.
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(1), 1, DOT));
		let predicted_premature =
			crate::Pallet::<Test>::predict_rate_change_upfront_fee(DOT, 1, rate_pct(7, 100));
		assert!(
			predicted_premature > 0,
			"premature rate change must charge a non-zero upfront fee"
		);
		let v_pre = Vaults::<Test>::get(DOT, 1).expect("vault stored");
		assert_ok!(crate::Pallet::<Test>::change_rate(
			RuntimeOrigin::signed(1),
			DOT,
			rate_pct(7, 100),
			Position::endpoints_only(),
		));
		let v_mid = Vaults::<Test>::get(DOT, 1).expect("vault stored");
		assert_eq!(v_mid.annual_rate, rate_pct(7, 100));
		assert_eq!(v_mid.accrued_interest, v_pre.accrued_interest + predicted_premature);

		// --- Phase 2: same vault, this time AFTER cooldown.
		advance_time(cfg_cooldown);
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(1), 1, DOT));
		let predicted_free =
			crate::Pallet::<Test>::predict_rate_change_upfront_fee(DOT, 1, rate_pct(8, 100));
		assert_eq!(predicted_free, 0, "post-cooldown rate change must be free");
		let v_pre2 = Vaults::<Test>::get(DOT, 1).expect("vault stored");
		assert_ok!(crate::Pallet::<Test>::change_rate(
			RuntimeOrigin::signed(1),
			DOT,
			rate_pct(8, 100),
			Position::endpoints_only(),
		));
		let v_post = Vaults::<Test>::get(DOT, 1).expect("vault stored");
		assert_eq!(v_post.annual_rate, rate_pct(8, 100));
		assert_eq!(v_post.accrued_interest, v_pre2.accrued_interest);
	});
}

// SKIPPED: row 11 `testAdjustInterestRateRevertsWhenUpfrontFeeExceedsUserProvidedLimit`
// — same reason as row 6: no `max_fee` on `change_rate`.
