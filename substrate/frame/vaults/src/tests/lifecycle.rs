//! Lifecycle smoke tests: a fast layer covering branch registration, vault
//! open/close happy paths and validation rejections, multi-asset routing,
//! frozen-mode blocking, and same-rate LIFO ordering. The deeper per-area
//! coverage lives in the sibling modules.

use crate::{
	mock::*,
	pallet::{BranchStates, Vaults},
	tests::{rate_pct, vault_status},
};
use frame::deps::frame_support::{assert_err, assert_noop, assert_ok};
use pallet_linked_list::SortedListInterface;
use pusd_primitives::BranchModeProvider;

#[test]
fn register_branch_creates_state() {
	build_and_execute(|| {
		register_default_branch();
		let bs = BranchStates::<Test>::get(DOT).expect("branch registered");
		assert_eq!(bs.total_collateral, 0);
		assert!(!bs.is_frozen());
	});
}

#[test]
fn register_branch_requires_full_manager() {
	build_and_execute(|| {
		// Defensive (acct 999) cannot register a new branch — needs Full.
		assert_noop!(
			crate::Pallet::<Test>::register_branch(
				RuntimeOrigin::signed(999),
				DOT,
				default_branch_config(),
			),
			crate::Error::<Test>::InsufficientPrivilege
		);
	});
}

#[test]
fn register_branch_rejects_unknown_asset() {
	build_and_execute(|| {
		let unknown = AssetId::WithId(999_999);
		assert_noop!(
			crate::Pallet::<Test>::register_branch(
				RuntimeOrigin::root(),
				unknown,
				default_branch_config(),
			),
			crate::Error::<Test>::UnknownCollateral
		);
	});
}

#[test]
fn register_branch_rejects_duplicate_collateral() {
	build_and_execute(|| {
		register_default_branch();
		assert_noop!(
			crate::Pallet::<Test>::register_branch(
				RuntimeOrigin::root(),
				DOT,
				default_branch_config(),
			),
			crate::Error::<Test>::BranchAlreadyRegistered
		);
	});
}

#[test]
fn branches_view_lists_registered_assets_in_registration_order() {
	build_and_execute(|| {
		register_default_branch();
		register_branch_for(TOKEN_X);
		assert_eq!(crate::Pallet::<Test>::branches(), alloc::vec![DOT, TOKEN_X]);
	});
}

#[test]
fn open_vault_holds_collateral_and_mints_pusd() {
	build_and_execute(|| {
		register_default_branch();
		// 1000 DOT @ $10 = $10000 collateral; borrow 1000 pUSD with 5% rate.
		assert_ok!(open(1, DOT, 1_000, 1_000, rate_pct(5, 100)));
		let v = Vaults::<Test>::get(DOT, 1).expect("vault stored");
		assert_eq!(v.debt.principal, 1_000);
		assert!(vault_status(DOT, 1).is_active());
		assert_eq!(pusd_balance(1), 1_000);
		assert_eq!(held(DOT, 1), 1_000);
		// Rate index contains the vault.
		assert!(<LinkedList as SortedListInterface<VaultList, u64>>::contains(&rate_list(DOT), &1));
	});
}

#[test]
fn open_vault_rejects_existing_owner_vault() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		assert_noop!(
			open(1, DOT, 2_000, 500, rate_pct(5, 100)),
			crate::Error::<Test>::VaultAlreadyExists
		);
	});
}

#[test]
fn open_vault_below_min_debt_rejected() {
	build_and_execute(|| {
		register_default_branch();
		assert_noop!(
			open(1, DOT, 1_000, 100, rate_pct(5, 100)), // < min_debt 200
			crate::Error::<Test>::DebtBelowMinimum
		);
	});
}

#[test]
fn open_vault_rate_out_of_bounds_rejected() {
	build_and_execute(|| {
		register_default_branch();
		assert_noop!(
			open(1, DOT, 1_000, 500, rate_pct(0, 1)),
			crate::Error::<Test>::RateOutOfBounds
		);
		assert_noop!(
			open(1, DOT, 1_000, 500, rate_pct(101, 100)),
			crate::Error::<Test>::RateOutOfBounds
		);
	});
}

#[test]
fn open_vault_exceeds_ceiling_rejected() {
	build_and_execute(|| {
		register_default_branch();
		assert_noop!(
			open(1, DOT, 100_000_000_000, 200_000_000, rate_pct(5, 100)), // > ceiling 100M
			crate::Error::<Test>::DebtCeilingExceeded
		);
	});
}

#[test]
fn open_vault_below_icr_rejected() {
	build_and_execute(|| {
		register_default_branch();
		// 100 DOT @ $10 = $1000; borrow 1000 pUSD => CR=100% < ICR 120%.
		assert_err!(
			open(1, DOT, 100, 1_000, rate_pct(5, 100)),
			crate::Error::<Test>::UnsafeCollateralizationRatio
		);
	});
}

#[test]
fn defensive_manager_can_only_tighten_selected_risk_parameters() {
	build_and_execute(|| {
		register_default_branch();

		assert_ok!(crate::Pallet::<Test>::set_minimum_collateralization_ratio(
			RuntimeOrigin::signed(999),
			DOT,
			rate_pct(120, 100),
		));
		assert_noop!(
			crate::Pallet::<Test>::set_minimum_collateralization_ratio(
				RuntimeOrigin::signed(999),
				DOT,
				rate_pct(109, 100),
			),
			crate::Error::<Test>::DefensiveActionNotDefensive
		);

		assert_ok!(crate::Pallet::<Test>::set_debt_ceiling(
			RuntimeOrigin::signed(999),
			DOT,
			50_000_000,
		));
		assert_noop!(
			crate::Pallet::<Test>::set_debt_ceiling(RuntimeOrigin::signed(999), DOT, 200_000_000,),
			crate::Error::<Test>::DefensiveActionNotDefensive
		);

		assert_noop!(
			crate::Pallet::<Test>::set_minimum_debt(RuntimeOrigin::signed(999), DOT, 300),
			crate::Error::<Test>::InsufficientPrivilege
		);
	});
}

#[test]
fn same_rate_lifo_redemption_order() {
	build_and_execute(|| {
		register_default_branch();
		// Three vaults at the same rate, in order: 1, 2, 3.
		for who in 1u64..=3 {
			assert_ok!(open(who, DOT, 1_000, 500, rate_pct(5, 100)));
		}
		// Tail-first iteration produces 3, 2, 1 (LIFO).
		let tail =
			<LinkedList as SortedListInterface<VaultList, u64>>::iter_from_tail(&rate_list(DOT), 5);
		assert_eq!(tail, alloc::vec![3, 2, 1]);
	});
}

#[test]
fn open_vault_on_multi_asset_branch() {
	// Exercises the right-hand side of the `fungible::UnionOf`: opening a
	// vault on `TOKEN_X` (a foreign asset in `pallet-assets`) instead of
	// native DOT. Confirms the union routes hold operations to
	// `pallet-assets-holder` for non-native ids.
	build_and_execute(|| {
		register_branch_for(TOKEN_X);
		assert_ok!(open(1, TOKEN_X, 1_000, 500, rate_pct(5, 100)));
		assert_eq!(held(TOKEN_X, 1), 1_000);
	});
}

#[test]
fn frozen_branch_blocks_user_ops() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(crate::Pallet::<Test>::enable_frozen_mode(RuntimeOrigin::root(), DOT,));
		assert_noop!(
			open(1, DOT, 1_000, 500, rate_pct(5, 100)),
			crate::Error::<Test>::BranchFrozen
		);
	});
}

#[test]
fn refresh_branch_persists_frozen_on_oracle_failure() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		set_oracle_available(false);
		assert_ok!(crate::Pallet::<Test>::refresh_branch(RuntimeOrigin::signed(99), DOT));
		let bs = BranchStates::<Test>::get(DOT).expect("bs");
		let frozen = bs.frozen.expect("frozen persisted");
		assert!(matches!(frozen.reason, crate::FrozenReason::OracleFailure));
	});
}

// External observers must not see the most permissive mode while prices are
// unknowable: `mode()` reports `Frozen` for a failing oracle even before
// `refresh_branch` persists the freeze.
#[test]
fn mode_reports_frozen_while_oracle_unavailable() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		let mode = <crate::Pallet<Test> as BranchModeProvider<AssetId>>::mode;
		assert_eq!(mode(&DOT), Some(BranchMode::Normal));
		set_oracle_available(false);
		assert_eq!(mode(&DOT), Some(BranchMode::Frozen));
		set_oracle_available(true);
		assert_eq!(mode(&DOT), Some(BranchMode::Normal));
	});
}

#[test]
fn refresh_branch_clears_oracle_frozen() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		set_oracle_available(false);
		assert_ok!(crate::Pallet::<Test>::refresh_branch(RuntimeOrigin::signed(99), DOT));
		assert!(BranchStates::<Test>::get(DOT).unwrap().is_frozen());
		// Oracle still down → second refresh is a no-op (already frozen for
		// the same reason).
		assert_ok!(crate::Pallet::<Test>::refresh_branch(RuntimeOrigin::signed(99), DOT));
		assert!(BranchStates::<Test>::get(DOT).unwrap().is_frozen());
		// Restore oracle and refresh → unfreezes.
		set_oracle_available(true);
		assert_ok!(crate::Pallet::<Test>::refresh_branch(RuntimeOrigin::signed(99), DOT));
		assert!(!BranchStates::<Test>::get(DOT).unwrap().is_frozen());
	});
}

#[test]
fn refresh_branch_does_not_clear_governance_frozen() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(crate::Pallet::<Test>::enable_frozen_mode(RuntimeOrigin::root(), DOT));
		assert_ok!(crate::Pallet::<Test>::refresh_branch(RuntimeOrigin::signed(99), DOT));
		assert!(BranchStates::<Test>::get(DOT).unwrap().is_frozen());
	});
}

#[test]
fn governance_clear_clears_governance_frozen() {
	build_and_execute(|| {
		register_default_branch();
		// Defensive (acct 999) cannot clear governance Frozen — needs Full.
		assert_ok!(crate::Pallet::<Test>::enable_frozen_mode(RuntimeOrigin::root(), DOT));
		assert_noop!(
			crate::Pallet::<Test>::clear_governance_frozen_mode(RuntimeOrigin::signed(999), DOT),
			crate::Error::<Test>::InsufficientPrivilege
		);
		// Full clears governance Frozen.
		assert_ok!(crate::Pallet::<Test>::clear_governance_frozen_mode(RuntimeOrigin::root(), DOT));
		assert!(!BranchStates::<Test>::get(DOT).unwrap().is_frozen());
	});
}

#[test]
fn governance_clear_is_noop_for_oracle_frozen() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		set_oracle_available(false);
		assert_ok!(crate::Pallet::<Test>::refresh_branch(RuntimeOrigin::signed(99), DOT));
		assert!(BranchStates::<Test>::get(DOT).unwrap().is_frozen());
		// Governance clear refuses oracle-Frozen state — branch stays frozen.
		assert_ok!(crate::Pallet::<Test>::clear_governance_frozen_mode(RuntimeOrigin::root(), DOT));
		assert!(BranchStates::<Test>::get(DOT).unwrap().is_frozen());
	});
}

#[test]
fn poke_during_frozen_is_noop() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok!(open(1, DOT, 1_000, 500, rate_pct(5, 100)));
		assert_ok!(crate::Pallet::<Test>::enable_frozen_mode(RuntimeOrigin::root(), DOT));
		let bs_pre = BranchStates::<Test>::get(DOT).expect("bs");
		assert_ok!(crate::Pallet::<Test>::poke(RuntimeOrigin::signed(99), 1, DOT));
		let bs_post = BranchStates::<Test>::get(DOT).expect("bs");
		// No interest minted while frozen.
		assert_eq!(bs_pre.debt.minted_interest, bs_post.debt.minted_interest);
		assert!(bs_post.is_frozen(), "poke does not clear Frozen");
	});
}
