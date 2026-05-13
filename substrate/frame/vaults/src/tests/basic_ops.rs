//! Port of liquity_v2/contracts/test/basicOps.t.sol (lines 18972-19188).
//!
//! Smoke tests for the vault open/close/adjust path. Per `tests.md` "Pallet
//! ownership", `pallet-vaults` owns rows 1-5; rows 6-9 (`testRedeem`,
//! `testLiquidation`, `testSPDeposit`, `testSPWithdrawal`) belong to
//! `pallet-redemptions` and `pallet-stability-pool` and are out of scope here.

use crate::{
	mock::*,
	pallet::Vaults,
	tests::{rate_pct, vault_status},
};
use frame::deps::frame_support::assert_ok;

// SKIPPED: row 1 `testOpenTroveFailsWithoutAllowance` — polkadot does not
// have an ERC-20 allowance model; collateral is moved by a signed origin
// authorising itself, so there is nothing to check.

// tests.md row 2: testOpenTroveFailsWithoutBalance.
//
// In polkadot the underlying `fungible::hold` call fails with a token-layer
// error when the caller's free balance is below the requested collateral
// amount. We use account 100 which is not funded by genesis (only 1..=10 are).
#[test]
fn open_trove_fails_without_balance() {
	build_and_execute(|| {
		register_default_branch();
		assert!(open(100, DOT, 1_000, 500, rate_pct(5, 100)).is_err());
	});
}

// tests.md row 3: testOpenTrove.
//
// Vault count is one after opening; storage row stamped Active with the
// declared debt.
#[test]
fn open_trove() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 1_000, rate_pct(5, 100)));
		assert_eq!(Vaults::<Test>::iter_prefix(DOT).count(), 1);
		assert!(vault_status(DOT, 1).is_active());
	});
}

// tests.md row 4: testCloseTrove.
//
// Open A and B; B closes (after repaying its full debt — including the
// upfront fee that landed in `accrued_interest`) and the surviving vault
// count is one.
#[test]
fn close_trove() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(5, 100)));
		let v = Vaults::<Test>::get(DOT, 2).expect("vault stored");
		let total = v.debt.principal + v.debt.interest;
		// Caller needs enough pUSD to cover the upfront fee on top of their
		// borrowed principal — top them up from acct 1's mint.
		let _ = <Pusd as frame::deps::frame_support::traits::fungible::Mutate<u64>>::transfer(
			&1,
			&2,
			v.debt.interest,
			frame::deps::frame_support::traits::tokens::Preservation::Expendable,
		);
		assert_ok!(crate::Pallet::<Test>::repay_for(RuntimeOrigin::signed(2), 2, DOT, total));
		assert_ok!(crate::Pallet::<Test>::close_vault(RuntimeOrigin::signed(2), DOT, None));
		assert_eq!(Vaults::<Test>::iter_prefix(DOT).count(), 1);
	});
}

// tests.md row 5: testAdjustTrove (reformulated).
//
// Polkadot has no single `adjust_vault` extrinsic — adjustments are applied
// through the per-action dispatchables (`deposit_collateral_for`, `borrow`,
// `withdraw_collateral`, `repay_for`). This test exercises a +collateral
// then +debt sequence and asserts the end-state shows both deltas.
#[test]
fn adjust_trove_via_deposit_then_borrow() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		// +200 collateral.
		assert_ok!(crate::Pallet::<Test>::deposit_collateral_for(
			RuntimeOrigin::signed(1),
			1,
			DOT,
			200,
		));
		// +300 debt (no rate change).
		assert_ok!(crate::Pallet::<Test>::borrow(
			RuntimeOrigin::signed(1),
			DOT,
			300,
			None,
			None,
			Position::endpoints_only(),
		));
		assert_eq!(held(DOT, 1), 1_200);
		let v = Vaults::<Test>::get(DOT, 1).expect("vault stored");
		assert_eq!(v.debt.principal, 800);
		// pUSD net to user: initial 500 + 300 borrowed (fees go to fee handler
		// dropper, not the user).
		assert_eq!(pusd_balance(1), 800);
	});
}

// (rows 6-9 owned by pallet-redemptions / pallet-stability-pool)
