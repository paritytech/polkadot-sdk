use crate::{mock::*, pallet::Vaults, tests::rate_pct};
use frame::deps::frame_support::{assert_noop, assert_ok};

// `close_vault` requires zero debt; with debt outstanding it returns
// `DebtOutstanding`. The separate "system needs at least one vault" guard
// lives on the liquidation path — see `last_vault.rs`.
#[test]
fn close_last_vault_with_debt_reverts() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		assert_noop!(
			crate::Pallet::<Test>::close_vault(RuntimeOrigin::signed(1), DOT, None),
			crate::Error::<Test>::DebtOutstanding
		);
	});
}

// `repay_for` returns `DebtWouldBecomeDust` when the post-state would land
// strictly between zero and `MinimumDebt`. (Full repayment to zero is allowed.)
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

// The per-action dispatchables silently no-op on zero amounts rather than
// reverting. This pins that behaviour so it can't regress unnoticed: each of
// `deposit_collateral_for`, `withdraw_collateral`, `borrow`, `repay_for` is
// called with `amount=0` and must succeed, leaving state unchanged.
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
		assert_eq!(pre.debt.principal, post.debt.principal);
		assert_eq!(pre.debt.interest, post.debt.interest);
		assert_eq!(held(DOT, 1), 1_000);
	});
}

// `change_rate` to the current rate returns `Ok(())` early without touching
// state. This pins the no-op semantics: the storage row before and after is
// byte-identical.
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
