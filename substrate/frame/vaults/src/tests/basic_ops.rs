use crate::{
	mock::*,
	pallet::Vaults,
	tests::{rate_pct, vault_status},
};
use frame::deps::frame_support::assert_ok;
use pallet_linked_list::SortedListInterface;

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

// A `repay_for` that brings debt to zero closes the vault in
// the same op — removes it from the rate index, releases held collateral to
// the owner, deletes the Vaults row, and emits VaultClosed.
#[test]
fn repay_for_to_zero_closes_active_vault() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(5, 100)));
		let v = Vaults::<Test>::get(DOT, 1).expect("vault stored");
		let total = v.debt.principal + v.debt.interest;
		let _ = <Pusd as frame::deps::frame_support::traits::fungible::Mutate<u64>>::transfer(
			&2,
			&1,
			v.debt.interest,
			frame::deps::frame_support::traits::tokens::Preservation::Expendable,
		);
		let coll_before = collateral_balance(DOT, 1);
		assert_ok!(crate::Pallet::<Test>::repay_for(RuntimeOrigin::signed(1), 1, DOT, total));
		assert!(Vaults::<Test>::get(DOT, 1).is_none(), "vault row removed");
		assert_eq!(held(DOT, 1), 0, "held collateral released");
		assert_eq!(
			collateral_balance(DOT, 1),
			coll_before + 1_000,
			"owner received the collateral"
		);
		assert!(
			!<LinkedList as SortedListInterface<VaultList, u64>>::contains(&rate_list(DOT), &1),
			"vault removed from rate index"
		);
		System::assert_has_event(crate::mock::RuntimeEvent::Vaults(crate::Event::VaultClosed {
			collateral_id: DOT,
			owner: 1,
			recipient: 1,
		}));
	});
}

// A redemption-driven dormant
// residual that is repaid to zero auto-closes (and clears any matching
// `last_dormant_vault_owner` pointer).
#[test]
fn repay_for_to_zero_closes_dormant_vault() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(1, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(2, 100)));
		// Push acct 1 to Dormant with a small residual debt.
		assert_ok!(redeem(DOT, 3, 350));
		assert!(vault_status(DOT, 1).is_dormant());
		let v = Vaults::<Test>::get(DOT, 1).expect("dormant vault stored");
		let total = v.debt.principal + v.debt.interest;
		assert!(total > 0);
		let _ = <Pusd as frame::deps::frame_support::traits::fungible::Mutate<u64>>::transfer(
			&2,
			&1,
			total.saturating_sub(pusd_balance(1)),
			frame::deps::frame_support::traits::tokens::Preservation::Expendable,
		);
		assert_ok!(crate::Pallet::<Test>::repay_for(RuntimeOrigin::signed(1), 1, DOT, total));
		assert!(Vaults::<Test>::get(DOT, 1).is_none());
		assert_eq!(held(DOT, 1), 0);
		let bs = crate::pallet::BranchStates::<Test>::get(DOT).expect("bs");
		assert_eq!(bs.last_dormant_vault_owner, None);
	});
}
