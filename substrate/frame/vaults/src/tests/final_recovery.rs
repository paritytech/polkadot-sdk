//! FinalRecovery queue behavior backed by `pallet-linked-list`.

use crate::{
	mock::*,
	tests::{rate_pct, vault_status},
};
use frame::deps::{
	frame_support::{assert_noop, assert_ok},
	sp_runtime::{FixedPointNumber, FixedU128},
};
use pusd_primitives::{RedemptionAllocation, VaultRedemptionInterface};

fn low_recovery_price() -> FixedU128 {
	FixedU128::from_rational(1u128, 10u128)
}

fn enter_recovery(who: AccountId, rate: FixedU128) {
	set_price(DOT, FixedU128::from_rational(10u128, 1u128));
	assert_ok!(open(who, DOT, 1_000, 500, rate));
	set_price(DOT, low_recovery_price());
	assert_ok!(crate::Pallet::<Test>::enter_final_recovery(RuntimeOrigin::signed(99), who, DOT,));
}

fn direct_redeem(owner: AccountId, redeemer: AccountId, amount: Balance) {
	let post_touch = <crate::Pallet<Test> as VaultRedemptionInterface<
		AccountId,
		AssetId,
		Balance,
	>>::touch_for_redemption(DOT, owner)
	.expect("touch target");
	let debt_to_cancel = core::cmp::min(amount, post_touch);
	let price = MockPrices::get().get(&DOT).copied().expect("price set");
	let collateral_to_redeemer =
		(FixedU128::saturating_from_integer(debt_to_cancel) / price).saturating_mul_int(1u128);
	assert_ok!(
		<crate::Pallet<Test> as VaultRedemptionInterface<AccountId, AssetId, Balance>>::apply_redemption(
			DOT,
			owner,
			redeemer,
			RedemptionAllocation {
				debt_to_cancel,
				collateral_to_redeemer,
				fee_collateral_retained: 0,
			},
		)
	);
}

#[test]
fn final_recovery_queue_is_fifo_across_multiple_vaults() {
	build_and_execute(|| {
		register_default_branch();

		enter_recovery(1, rate_pct(1, 100));
		enter_recovery(2, rate_pct(2, 100));
		enter_recovery(3, rate_pct(3, 100));

		assert_eq!(crate::Pallet::<Test>::final_recovery_queue_head(DOT, 10), alloc::vec![1, 2, 3]);
		assert_eq!(
			<crate::Pallet<Test> as VaultRedemptionInterface<AccountId, AssetId, Balance>>::next_redemption_target(
				DOT, None,
			),
			Some(1)
		);
	});
}

#[test]
fn enter_final_recovery_rejects_non_last_eligible_vault() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(5, 100)));
		set_price(DOT, low_recovery_price());

		assert_noop!(
			crate::Pallet::<Test>::enter_final_recovery(RuntimeOrigin::signed(99), 1, DOT),
			crate::Error::<Test>::NotLastEligibleVault
		);
	});
}

#[test]
fn enter_final_recovery_rejects_vault_above_mcr() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));

		assert_noop!(
			crate::Pallet::<Test>::enter_final_recovery(RuntimeOrigin::signed(99), 1, DOT),
			crate::Error::<Test>::UnsafeCollateralizationRatio
		);
	});
}

#[test]
fn final_recovery_middle_exit_splices_queue() {
	build_and_execute(|| {
		register_default_branch();

		enter_recovery(1, rate_pct(1, 100));
		enter_recovery(2, rate_pct(2, 100));
		enter_recovery(3, rate_pct(3, 100));

		set_price(DOT, FixedU128::from_rational(10u128, 1u128));
		assert_ok!(crate::Pallet::<Test>::exit_final_recovery(
			RuntimeOrigin::signed(42),
			2,
			DOT,
			Position::endpoints_only(),
		));

		assert!(vault_status(DOT, 2).is_active());
		assert_eq!(crate::Pallet::<Test>::final_recovery_queue_head(DOT, 10), alloc::vec![1, 3]);
	});
}

#[test]
fn exit_final_recovery_rejects_when_cr_still_below_mcr() {
	build_and_execute(|| {
		register_default_branch();
		enter_recovery(1, rate_pct(5, 100));

		assert_noop!(
			crate::Pallet::<Test>::exit_final_recovery(
				RuntimeOrigin::signed(99),
				1,
				DOT,
				Position::endpoints_only(),
			),
			crate::Error::<Test>::UnsafeCollateralizationRatio
		);
		assert!(vault_status(DOT, 1).is_final_recovery());
		assert_eq!(crate::Pallet::<Test>::final_recovery_queue_head(DOT, 10), alloc::vec![1]);
	});
}

#[test]
fn exit_final_recovery_rejects_non_final_recovery_vault() {
	build_and_execute(|| {
		register_default_branch();
		// A plain Active vault is not in the FIFO, so exiting it is invalid.
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		assert_noop!(
			crate::Pallet::<Test>::exit_final_recovery(
				RuntimeOrigin::signed(99),
				1,
				DOT,
				Position::endpoints_only(),
			),
			crate::Error::<Test>::InvalidVaultStatus
		);
	});
}

#[test]
fn frozen_branch_rejects_enter_final_recovery() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		assert_ok!(crate::Pallet::<Test>::enable_frozen_mode(RuntimeOrigin::root(), DOT));
		// The frozen check precedes the CR / last-eligible checks.
		assert_noop!(
			crate::Pallet::<Test>::enter_final_recovery(RuntimeOrigin::signed(99), 1, DOT),
			crate::Error::<Test>::BranchFrozen
		);
	});
}

#[test]
fn frozen_branch_rejects_exit_final_recovery() {
	build_and_execute(|| {
		register_default_branch();
		enter_recovery(1, rate_pct(5, 100));
		assert_ok!(crate::Pallet::<Test>::enable_frozen_mode(RuntimeOrigin::root(), DOT));
		assert_noop!(
			crate::Pallet::<Test>::exit_final_recovery(
				RuntimeOrigin::signed(99),
				1,
				DOT,
				Position::endpoints_only(),
			),
			crate::Error::<Test>::BranchFrozen
		);
		assert!(vault_status(DOT, 1).is_final_recovery());
	});
}

#[test]
fn redemption_zeroing_final_recovery_vault_makes_it_dormant() {
	build_and_execute(|| {
		register_default_branch();
		enter_recovery(1, rate_pct(5, 100));
		// Restore the price so the redeemer's collateral payout is affordable.
		set_price(DOT, FixedU128::from_rational(10u128, 1u128));
		let full = crate::pallet::Vaults::<Test>::get(DOT, 1).expect("vault stored").debt.total();

		// Cancelling the entire debt pulls the vault out of the FIFO; it settles
		// to Dormant with its stake re-synced to the still-held collateral.
		direct_redeem(1, 7, full);

		assert!(vault_status(DOT, 1).is_dormant());
		assert!(crate::Pallet::<Test>::final_recovery_queue_head(DOT, 10).is_empty());
		let vault = crate::pallet::Vaults::<Test>::get(DOT, 1).expect("vault stored");
		assert_eq!(vault.debt.total(), 0);
		assert_eq!(vault.redistribution_stake, held(DOT, 1));
		let bs = crate::pallet::BranchStates::<Test>::get(DOT).expect("bs");
		assert_eq!(bs.stakes.total, held(DOT, 1));
	});
}

#[test]
fn final_recovery_blocks_borrow_repay_withdraw_and_change_rate() {
	build_and_execute(|| {
		register_default_branch();
		enter_recovery(1, rate_pct(5, 100));

		assert_noop!(
			crate::Pallet::<Test>::borrow(
				RuntimeOrigin::signed(1),
				DOT,
				100,
				None,
				None,
				Position::endpoints_only(),
			),
			crate::Error::<Test>::VaultInFinalRecovery
		);
		assert_noop!(
			crate::Pallet::<Test>::repay_for(RuntimeOrigin::signed(1), 1, DOT, 100),
			crate::Error::<Test>::VaultInFinalRecovery
		);
		assert_noop!(
			crate::Pallet::<Test>::withdraw_collateral(RuntimeOrigin::signed(1), DOT, 1, None),
			crate::Error::<Test>::VaultInFinalRecovery
		);
		assert_noop!(
			crate::Pallet::<Test>::change_rate(
				RuntimeOrigin::signed(1),
				DOT,
				rate_pct(7, 100),
				Position::endpoints_only(),
			),
			crate::Error::<Test>::InvalidVaultStatus
		);
	});
}

#[test]
fn exit_final_recovery_to_dormant_when_debt_below_minimum() {
	build_and_execute(|| {
		register_default_branch();
		enter_recovery(1, rate_pct(5, 100));
		enter_recovery(2, rate_pct(5, 100));
		// Restore price so CR sits comfortably above MCR after partial redemption.
		set_price(DOT, FixedU128::from_rational(10u128, 1u128));
		// Redeem most of vault 1's debt — pulls it below MinimumDebt (200) but
		// leaves a non-zero residual, so it stays in the FR FIFO.
		direct_redeem(1, 99, 350);
		let v = crate::pallet::Vaults::<Test>::get(DOT, 1).expect("vault stored");
		assert!(v.debt.total() > 0);
		assert!(v.debt.total() < 200);
		// The Dormant-exit branch never re-inserts into the rate index, so the
		// hint is ignored. Pass a deliberately invalid one to prove it.
		assert_ok!(crate::Pallet::<Test>::exit_final_recovery(
			RuntimeOrigin::signed(99),
			1,
			DOT,
			Position::between(999, 998),
		));
		// Vault is no longer in FR FIFO and not in the rate index — it's Dormant.
		assert!(vault_status(DOT, 1).is_dormant());
		assert_eq!(crate::Pallet::<Test>::final_recovery_queue_head(DOT, 10), alloc::vec![2]);
		let bs = crate::pallet::BranchStates::<Test>::get(DOT).expect("bs");
		assert_eq!(bs.last_dormant_vault_owner, Some(1));
	});
}

#[test]
fn deposit_into_final_recovery_keeps_stake_zero() {
	build_and_execute(|| {
		register_default_branch();
		enter_recovery(1, rate_pct(5, 100));

		let before = crate::pallet::BranchStates::<Test>::get(DOT).expect("branch state");
		assert_ok!(crate::Pallet::<Test>::deposit_collateral_for(
			RuntimeOrigin::signed(2),
			1,
			DOT,
			10_000,
		));

		// The collateral lands on the hold and in the branch total, but the
		// vault stays excluded from stake accounting while in the FIFO.
		let vault = crate::pallet::Vaults::<Test>::get(DOT, 1).expect("vault stored");
		assert_eq!(vault.redistribution_stake, 0);
		let after = crate::pallet::BranchStates::<Test>::get(DOT).expect("branch state");
		assert_eq!(after.stakes.total, before.stakes.total);
		assert_eq!(after.total_collateral, before.total_collateral + 10_000);
		assert_eq!(held(DOT, 1), 1_000 + 10_000);
		assert!(vault_status(DOT, 1).is_final_recovery());
	});
}

#[test]
fn final_recovery_rescue_deposit_then_exit() {
	build_and_execute(|| {
		register_default_branch();
		enter_recovery(1, rate_pct(5, 100));

		// At the crash price the vault is deep underwater; top up enough
		// collateral that the fully-accrued CR clears the MCR again, then
		// exit. Stake re-syncs from the hold on the way out.
		assert_ok!(crate::Pallet::<Test>::deposit_collateral_for(
			RuntimeOrigin::signed(2),
			1,
			DOT,
			10_000,
		));
		assert_ok!(crate::Pallet::<Test>::exit_final_recovery(
			RuntimeOrigin::signed(99),
			1,
			DOT,
			Position::endpoints_only(),
		));

		assert!(vault_status(DOT, 1).is_active());
		let vault = crate::pallet::Vaults::<Test>::get(DOT, 1).expect("vault stored");
		assert_eq!(vault.redistribution_stake, held(DOT, 1));
		assert!(crate::Pallet::<Test>::final_recovery_queue_head(DOT, 10).is_empty());
	});
}

#[test]
fn redemption_queue_composes_recovery_dormant_and_rate_index() {
	build_and_execute(|| {
		register_default_branch();

		enter_recovery(1, rate_pct(1, 100));
		enter_recovery(2, rate_pct(2, 100));

		set_price(DOT, FixedU128::from_rational(10u128, 1u128));
		assert_ok!(open(3, DOT, 1_000, 500, rate_pct(1, 100)));
		assert_ok!(open(4, DOT, 1_000, 500, rate_pct(2, 100)));
		assert_ok!(open(5, DOT, 1_000, 500, rate_pct(3, 100)));

		direct_redeem(3, 10, 350);
		assert!(vault_status(DOT, 3).is_dormant());

		assert_eq!(
			crate::Pallet::<Test>::redemption_queue_head(DOT, 10),
			alloc::vec![1, 2, 3, 4, 5]
		);
	});
}
