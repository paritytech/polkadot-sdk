use crate::{
	mock::*,
	tests::{rate_pct, vault_status},
};
use frame::deps::{
	frame_support::{assert_noop, assert_ok, traits::fungible::Mutate as FungibleMutate},
	sp_runtime::FixedU128,
};
use pallet_linked_list::SortedListInterface;

fn fund_account(who: AccountId) {
	let _ = <Balances as FungibleMutate<AccountId>>::mint_into(&who, 1_000_000_000_000);
}

fn seed_long_rate_index() {
	for who in 1u64..=20 {
		fund_account(who);
		assert_ok!(open(who, DOT, 2_000, 500, rate_pct(20 + u128::from(who), 100)));
	}
}

// A rate-position hint never names a vault that has left the rate index (a
// redemption-Dormant vault).
#[test]
fn find_rate_position_skips_dormant_vaults() {
	build_and_execute(|| {
		register_default_branch();
		// Five vaults at 1%, 2%, 3%, 4%, 5%.
		for (who, pct) in [(1u64, 1), (2, 2), (3, 3), (4, 4), (5, 5)] {
			assert_ok!(open(who, DOT, 1_000, 500, rate_pct(pct, 100)));
		}

		// Redeem acct 1's full debt. apply_redemption transitions vault to
		// Dormant when residual debt is zero (see interfaces.rs).
		let target = redeem(DOT, 5, 600).expect("redeem ok"); // 600 > vault 1's debt to fully clear it
		assert_eq!(target, 1);
		// Vault is Dormant or its debt is below MinimumDebt — either way it
		// should be out of the rate index.
		assert!(vault_status(DOT, 1).is_dormant());
		assert!(!<LinkedList as SortedListInterface<VaultList, u64>>::contains(
			&rate_list(DOT),
			&1
		));

		// Now query a hint at a rate near acct 1's old rate. The result
		// must not name acct 1 — it's no longer in the index.
		let pos = crate::Pallet::<Test>::find_rate_position(DOT, rate_pct(15, 1000)); // 1.5%
		assert_ne!(pos.prev, Some(1));
		assert_ne!(pos.next, Some(1));
	});
}

// `repair_steps_needed` reports 0 for an already-correct hint and a positive,
// within-budget count for a stale-but-repairable one.
#[test]
fn repair_steps_needed_zero_for_valid_positive_for_stale() {
	build_and_execute(|| {
		register_default_branch();
		for (who, pct) in [(1u64, 10), (2, 20), (3, 30)] {
			assert_ok!(open(who, DOT, 2_000, 500, rate_pct(pct, 100)));
		}
		let budget = <LinkedList as SortedListInterface<VaultList, u64>>::repair_budget();
		let rate = rate_pct(25, 100);
		let good = crate::Pallet::<Test>::find_rate_position(DOT, rate);
		assert_eq!(crate::Pallet::<Test>::repair_steps_needed(DOT, rate, good), 0);
		let stale =
			crate::Pallet::<Test>::repair_steps_needed(DOT, rate, Position::endpoints_only());
		assert!(stale > 0 && stale <= budget, "stale hint must be repairable within budget");
	});
}

// A hint that would need more than the repair budget signals infeasibility by
// returning a step count strictly greater than the budget.
#[test]
fn repair_steps_needed_exceeds_budget_for_extreme_hint_in_long_index() {
	build_and_execute(|| {
		register_default_branch();
		seed_long_rate_index();
		let budget = <LinkedList as SortedListInterface<VaultList, u64>>::repair_budget();
		let steps = crate::Pallet::<Test>::repair_steps_needed(
			DOT,
			rate_pct(1, 100),
			Position::endpoints_only(),
		);
		assert!(steps > budget, "extreme stale hint in a long index is infeasible");
	});
}

// Exiting FinalRecovery back into the rate index with an unrepairable hint rolls
// the whole operation back: the vault stays in the FIFO and storage is unchanged.
#[test]
fn exit_final_recovery_invalid_hint_rolls_back() {
	build_and_execute(|| {
		register_default_branch();
		// Account 21 alone enters FinalRecovery after a price crash.
		fund_account(21);
		assert_ok!(open(21, DOT, 1_000, 500, rate_pct(1, 100)));
		set_price(DOT, FixedU128::from_rational(1u128, 10u128));
		assert_ok!(crate::Pallet::<Test>::enter_final_recovery(RuntimeOrigin::signed(99), 21, DOT));

		// Restore the price and seed a long index so the tail re-insertion at 1%
		// needs more than the repair budget.
		set_price(DOT, FixedU128::from_rational(10u128, 1u128));
		seed_long_rate_index();

		let vault_pre = crate::pallet::Vaults::<Test>::get(DOT, 21).expect("vault stored");
		assert_noop!(
			crate::Pallet::<Test>::exit_final_recovery(
				RuntimeOrigin::signed(99),
				21,
				DOT,
				Position::endpoints_only(),
			),
			crate::Error::<Test>::InvalidPositionHints
		);
		assert!(vault_status(DOT, 21).is_final_recovery());
		assert_eq!(crate::pallet::Vaults::<Test>::get(DOT, 21).unwrap(), vault_pre);
		assert_eq!(crate::Pallet::<Test>::final_recovery_queue_head(DOT, 10), alloc::vec![21]);
	});
}

#[test]
fn open_vault_invalid_hint_rolls_back_hold_mint_and_storage() {
	build_and_execute(|| {
		register_default_branch();
		seed_long_rate_index();
		fund_account(21);
		let collateral_pre = collateral_balance(DOT, 21);
		let pusd_pre = pusd_balance(21);

		assert_noop!(
			crate::Pallet::<Test>::open_vault(
				RuntimeOrigin::signed(21),
				DOT,
				1_000,
				500,
				rate_pct(1, 100),
				Position::endpoints_only(),
			),
			crate::Error::<Test>::InvalidPositionHints
		);

		assert!(crate::pallet::Vaults::<Test>::get(DOT, 21).is_none());
		assert_eq!(held(DOT, 21), 0);
		assert_eq!(collateral_balance(DOT, 21), collateral_pre);
		assert_eq!(pusd_balance(21), pusd_pre);
	});
}

#[test]
fn change_rate_invalid_hint_rolls_back_rate_fee_and_index() {
	build_and_execute(|| {
		register_default_branch();
		seed_long_rate_index();
		let vault_pre = crate::pallet::Vaults::<Test>::get(DOT, 20).expect("vault stored");
		let branch_pre = crate::pallet::BranchStates::<Test>::get(DOT).expect("branch state");
		let order_pre = <LinkedList as SortedListInterface<VaultList, u64>>::iter_from_tail(
			&rate_list(DOT),
			25,
		);

		assert_noop!(
			crate::Pallet::<Test>::change_rate(
				RuntimeOrigin::signed(20),
				DOT,
				rate_pct(1, 100),
				Position::endpoints_only(),
			),
			crate::Error::<Test>::InvalidPositionHints
		);

		assert_eq!(crate::pallet::Vaults::<Test>::get(DOT, 20).unwrap(), vault_pre);
		assert_eq!(crate::pallet::BranchStates::<Test>::get(DOT).unwrap(), branch_pre);
		let order_post = <LinkedList as SortedListInterface<VaultList, u64>>::iter_from_tail(
			&rate_list(DOT),
			25,
		);
		assert_eq!(order_post, order_pre);
	});
}
