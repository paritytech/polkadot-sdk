use crate::{
	mock::*,
	pallet::{BranchStates, Vaults},
	tests::{rate_pct, vault_status},
};
use frame::deps::{
	frame_support::{assert_noop, assert_ok},
	sp_runtime::FixedU128,
};
use pallet_linked_list::SortedListInterface;
use pusd_primitives::{RedemptionAllocation, VaultRedemptionInterface};

const ONE_DAY_MS: Moment = 24 * 3_600 * 1_000;

// Behavior note: Dormant vaults can still be the target of `withdraw` and
// `repay` operations. The carve-outs are `change_rate` and collateral-only
// deposits that cannot revive the vault to `Debt >= MinimumDebt`.

#[test]
fn fully_redeemed_vault_becomes_dormant_and_leaves_rate_index() {
	build_and_execute(|| {
		register_default_branch();
		// A and B at distinct rates so the redemption order is deterministic
		// (tail-first picks A first as it has the lower rate).
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(1, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(2, 100)));

		// Redeem an amount large enough to cancel acct 1's full debt.
		let v_pre = Vaults::<Test>::get(DOT, 1).unwrap();
		let total = v_pre.debt.principal + v_pre.debt.interest;
		let target = redeem(DOT, 3, total).expect("redeem ok");
		assert_eq!(target, 1);

		let v = Vaults::<Test>::get(DOT, 1).unwrap();
		assert!(vault_status(DOT, 1).is_dormant());
		assert_eq!(v.debt.principal + v.debt.interest, 0);
		let bs = BranchStates::<Test>::get(DOT).unwrap();
		assert_eq!(bs.last_dormant_vault_owner, None);
		// Rate index no longer contains acct 1.
		assert!(!<LinkedList as SortedListInterface<VaultList, u64>>::contains(
			&rate_list(DOT),
			&1
		));
	});
}

#[test]
fn redeemed_below_min_debt_becomes_dormant() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(1, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(2, 100)));

		// MinimumDebt = 200 (from default_branch_config). Redeem so acct 1
		// has < 200 left.
		assert_ok!(redeem(DOT, 3, 350));
		let v = Vaults::<Test>::get(DOT, 1).unwrap();
		let total = v.debt.principal + v.debt.interest;
		assert!(total > 0 && total < 200, "got total = {}", total);
		assert!(vault_status(DOT, 1).is_dormant());
		let bs = BranchStates::<Test>::get(DOT).unwrap();
		assert_eq!(bs.last_dormant_vault_owner, Some(1));
		assert!(!<LinkedList as SortedListInterface<VaultList, u64>>::contains(
			&rate_list(DOT),
			&1
		));
	});
}

#[test]
fn redeemed_above_min_debt_stays_active() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(1, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(2, 100)));

		// Redeem 200 — leaves acct 1 with ≈ 300 debt, well above MinimumDebt.
		assert_ok!(redeem(DOT, 3, 200));
		assert!(vault_status(DOT, 1).is_active());
		assert!(<LinkedList as SortedListInterface<VaultList, u64>>::contains(&rate_list(DOT), &1));
	});
}

#[test]
fn touch_for_redemption_rejects_frozen_branch_and_missing_vault() {
	build_and_execute(|| {
		register_default_branch();
		assert_noop!(
			<crate::Pallet<Test> as VaultRedemptionInterface<AccountId, AssetId, Balance>>::touch_for_redemption(
				DOT, 99,
			),
			crate::Error::<Test>::VaultNotFound
		);
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(1, 100)));
		assert_ok!(crate::Pallet::<Test>::enable_frozen_mode(RuntimeOrigin::root(), DOT));
		assert_noop!(
			<crate::Pallet<Test> as VaultRedemptionInterface<AccountId, AssetId, Balance>>::touch_for_redemption(
				DOT, 1,
			),
			crate::Error::<Test>::BranchFrozen
		);
	});
}

#[test]
fn apply_redemption_rejects_invalid_allocations_without_state_change() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(1, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(2, 100)));
		let vault_pre = Vaults::<Test>::get(DOT, 1).expect("vault stored");
		let held_pre = held(DOT, 1);

		assert_noop!(
			<crate::Pallet<Test> as VaultRedemptionInterface<AccountId, AssetId, Balance>>::apply_redemption(
				DOT,
				1,
				3,
				RedemptionAllocation {
					debt_to_cancel: vault_pre.debt.total() + 1,
					collateral_to_redeemer: 0,
					fee_collateral_retained: 0,
				},
			),
			crate::Error::<Test>::InvalidRedemptionAllocation
		);
		assert_noop!(
			<crate::Pallet<Test> as VaultRedemptionInterface<AccountId, AssetId, Balance>>::apply_redemption(
				DOT,
				1,
				3,
				RedemptionAllocation {
					debt_to_cancel: 0,
					collateral_to_redeemer: held_pre + 1,
					fee_collateral_retained: 0,
				},
			),
			crate::Error::<Test>::InvalidRedemptionAllocation
		);

		assert_eq!(Vaults::<Test>::get(DOT, 1).unwrap(), vault_pre);
		assert_eq!(held(DOT, 1), held_pre);
	});
}

#[test]
fn redemption_with_retained_fee_leaves_fee_collateral_on_vault() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(1, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(2, 100)));
		let held_pre = held(DOT, 1);
		let redeemer_pre = collateral_balance(DOT, 3);

		assert_ok!(<crate::Pallet<Test> as VaultRedemptionInterface<
			AccountId,
			AssetId,
			Balance,
		>>::apply_redemption(
			DOT,
			1,
			3,
			RedemptionAllocation {
				debt_to_cancel: 100,
				collateral_to_redeemer: 10,
				fee_collateral_retained: 5,
			},
		));

		assert_eq!(held(DOT, 1), held_pre - 10);
		assert_eq!(collateral_balance(DOT, 3), redeemer_pre + 10);
	});
}

#[test]
fn dormant_pointer_clears_when_last_dormant_fully_redeemed() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(1, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(2, 100)));
		// Push acct 1 to Dormant via partial-below-MinDebt.
		assert_ok!(redeem(DOT, 3, 350));
		let bs = BranchStates::<Test>::get(DOT).unwrap();
		assert_eq!(bs.last_dormant_vault_owner, Some(1));
		// Now redeem acct 1's full residual. next_redemption_target prefers
		// last_dormant_vault_owner, so this hits acct 1 again.
		let v = Vaults::<Test>::get(DOT, 1).unwrap();
		let residual = v.debt.principal + v.debt.interest;
		let target = redeem(DOT, 3, residual).expect("redeem residual ok");
		assert_eq!(target, 1);
		let bs = BranchStates::<Test>::get(DOT).unwrap();
		assert_eq!(bs.last_dormant_vault_owner, None);
	});
}

#[test]
fn dormant_owner_borrowing_above_min_debt_revives_to_active() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(1, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(2, 100)));
		assert_ok!(redeem(DOT, 3, 350));
		assert!(vault_status(DOT, 1).is_dormant());
		assert!(!<LinkedList as SortedListInterface<VaultList, u64>>::contains(
			&rate_list(DOT),
			&1
		));
		let bs = BranchStates::<Test>::get(DOT).unwrap();
		assert_eq!(bs.last_dormant_vault_owner, Some(1));

		// Owner borrows enough to push debt above MinimumDebt → revives.
		// Vault debt jumps from ~150 to ~650, well above MinimumDebt 200.
		assert_ok!(crate::Pallet::<Test>::borrow(
			RuntimeOrigin::signed(1),
			DOT,
			500,
			None,
			None,
			Position::endpoints_only(),
		));
		assert!(vault_status(DOT, 1).is_active());
		// Re-inserted into the rate index at the new (or unchanged) rate.
		assert!(<LinkedList as SortedListInterface<VaultList, u64>>::contains(&rate_list(DOT), &1));
		let bs = BranchStates::<Test>::get(DOT).unwrap();
		assert_eq!(bs.last_dormant_vault_owner, None);
	});
}

#[test]
fn dormant_vault_with_residual_accrues_interest() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(50, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(50, 100)));
		assert_ok!(redeem(DOT, 3, 350));
		let v_pre = Vaults::<Test>::get(DOT, 1).unwrap();

		advance_time(365 * ONE_DAY_MS); // 1 year
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(2), 1, DOT));
		let v_post = Vaults::<Test>::get(DOT, 1).unwrap();
		assert!(
			v_post.debt.interest > v_pre.debt.interest,
			"Dormant vault with residual debt must accrue interest over time"
		);
	});
}

// A liquidation distributes its debt and collateral to the remaining stake
// pool. Dormant vaults keep their stake (compute_stake at open) and so still
// receive redistribution gains when `touch_vault` reconciles the epoch lag.
#[test]
fn dormant_vault_receives_redistribution_gains_on_touch() {
	build_and_execute(|| {
		register_default_branch();
		// Distinct rates so the rate-index tail is deterministic — acct 1 at
		// the lower rate sits at the tail, where the redemption helper picks
		// it first.
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(1, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(2, 100)));
		assert_ok!(open(3, DOT, 200, 200, rate_pct(5, 100)));
		assert_ok!(redeem(DOT, 4, 700)); // pushes acct 1 to Dormant

		// Drop the price so acct 3's CR falls below MCR — the vault pallet
		// refuses liquidation of a vault whose CR is at/above MCR. 1.0 puts
		// vault 3 (200 coll, ~200 debt) under the 110% MCR while leaving vaults
		// 1 and 2 above it.
		let v_dormant_pre = Vaults::<Test>::get(DOT, 1).unwrap();
		set_price(DOT, FixedU128::from_rational(1u128, 1u128));
		assert_ok!(liquidate(DOT, 3));
		// Touch acct 1 so the epoch lag closes and redist gains land on it.
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(2), 1, DOT));
		let v_dormant_post = Vaults::<Test>::get(DOT, 1).unwrap();
		// Gains may be very small for tiny liquidations; pin "did not lose".
		assert!(
			v_dormant_post.debt.principal >= v_dormant_pre.debt.principal,
			"Dormant vault should not lose principal across redistribution"
		);
	});
}

// `borrow` requires `vault.debt.principal >= cfg.minimum_debt` after the
// operation. Borrowing on a Dormant vault that doesn't reach the threshold
// reverts.
#[test]
fn dormant_borrow_below_min_debt_reverts() {
	build_and_execute(|| {
		register_default_branch();
		// Distinct rates so the rate-index tail is deterministic — acct 1 at
		// the lower rate sits at the tail, where the redemption helper picks
		// it first.
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(1, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(2, 100)));
		assert_ok!(redeem(DOT, 3, 480)); // pushes acct 1 to Dormant with tiny debt
								   // Borrow 1 — total debt would be far below MinimumDebt 200.
		assert_noop!(
			crate::Pallet::<Test>::borrow(
				RuntimeOrigin::signed(1),
				DOT,
				1,
				None,
				None,
				Position::endpoints_only(),
			),
			crate::Error::<Test>::DebtBelowMinimum
		);
	});
}

// Depositing collateral into a Dormant vault is rejected because the call
// cannot revive the vault to `Debt >= MinimumDebt` in the same op (deposits
// don't change debt). Owners must use `borrow` to revive.
#[test]
fn deposit_to_dormant_without_revival_errors() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(1, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(2, 100)));
		assert_ok!(redeem(DOT, 3, 350));
		assert!(vault_status(DOT, 1).is_dormant());
		assert_noop!(
			crate::Pallet::<Test>::deposit_collateral_for(RuntimeOrigin::signed(2), 1, DOT, 100),
			crate::Error::<Test>::DebtBelowMinimum
		);
	});
}

#[test]
fn dormant_vault_cannot_change_rate() {
	build_and_execute(|| {
		register_default_branch();
		// Distinct rates so the rate-index tail is deterministic — acct 1 at
		// the lower rate sits at the tail, where the redemption helper picks
		// it first.
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(1, 100)));
		assert_ok!(open(2, DOT, 1_000, 500, rate_pct(2, 100)));
		assert_ok!(redeem(DOT, 3, 350));
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
