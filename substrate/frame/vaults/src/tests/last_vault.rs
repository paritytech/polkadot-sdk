use crate::{mock::*, tests::rate_pct};
use frame::deps::{
	frame_support::{assert_noop, assert_ok},
	sp_runtime::FixedU128,
};
use pusd_primitives::{
	KeeperCompensation, LiquidationAllocation, OffsetAllocation, VaultLiquidationInterface,
};

#[test]
fn liquidate_only_vault_returns_last_vault_error() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok_open(1, DOT, 1_000, 500, rate_pct(5, 100));
		// Drop the price so the vault is severely undercollateralized — but
		// the trait-level `prepare_liquidation` still rejects on the
		// last-vault rule before any solvency check.
		set_price(DOT, FixedU128::from_rational(5u128, 100u128));
		assert_noop!(liquidate(DOT, 1), crate::Error::<Test>::LastVaultCannotBeLiquidated);
	});
}

#[test]
fn liquidate_succeeds_when_a_second_vault_exists() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok_open(1, DOT, 1_000, 500, rate_pct(5, 100));
		assert_ok_open(2, DOT, 1_000, 500, rate_pct(5, 100));
		set_price(DOT, FixedU128::from_rational(5u128, 100u128));
		// Now the last-vault guard doesn't trip — vault 2 remains as a
		// redistribution recipient.
		assert!(liquidate(DOT, 1).is_ok());
	});
}

#[test]
fn prepare_liquidation_rejects_healthy_vault() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok_open(1, DOT, 1_000, 500, rate_pct(5, 100));
		assert_ok_open(2, DOT, 1_000, 500, rate_pct(5, 100));
		// Price 10 → CR = 1000 * 10 / 500 = 20 ≫ MCR 1.1.
		assert_noop!(liquidate(DOT, 1), crate::Error::<Test>::UnsafeCollateralizationRatio);
	});
}

// Finalizing a vault that was never prepared would delete the row while its
// contribution still sits in the branch aggregates. The derived-status gate
// refuses anything that is not Dormant (prepared vaults are de-listed).
#[test]
fn finalize_without_prepare_is_rejected() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok_open(1, DOT, 1_000, 500, rate_pct(5, 100));
		assert_ok_open(2, DOT, 1_000, 500, rate_pct(5, 100));
		// Vault 1 is still Active (in the rate index): finalize must refuse.
		let alloc = LiquidationAllocation {
			offset: OffsetAllocation { recipient: 1, debt: 0, collateral: 0 },
			redistribution_collateral: 0,
			keeper: KeeperCompensation { recipient: 1, collateral: 0 },
		};
		assert_noop!(
			<crate::Pallet<Test> as VaultLiquidationInterface<AccountId, AssetId, Balance>>::finalize_liquidation(
				DOT, 1, alloc,
			),
			crate::Error::<Test>::LiquidationNotPrepared
		);
	});
}

// Liquidating the vault parked as `last_dormant_vault_owner` must clear the
// pointer along with the row, or it dangles at a missing vault.
#[test]
fn liquidating_parked_dormant_owner_clears_pointer() {
	build_and_execute(|| {
		register_default_branch();
		assert_ok_open(1, DOT, 1_000, 500, rate_pct(1, 100));
		assert_ok_open(2, DOT, 1_000, 500, rate_pct(2, 100));
		// Partial redemption drains vault 1 below MinimumDebt → Dormant, and
		// parks it as the next redemption target.
		assert_ok!(redeem(DOT, 3, 350));
		let bs = crate::pallet::BranchStates::<Test>::get(DOT).expect("branch state");
		assert_eq!(bs.last_dormant_vault_owner, Some(1));

		// Crash the price so the dormant husk is liquidatable, then liquidate.
		set_price(DOT, FixedU128::from_rational(1u128, 10u128));
		assert!(liquidate(DOT, 1).is_ok());

		assert!(crate::pallet::Vaults::<Test>::get(DOT, 1).is_none());
		let bs = crate::pallet::BranchStates::<Test>::get(DOT).expect("branch state");
		assert_eq!(bs.last_dormant_vault_owner, None, "pointer cleared with the row");
	});
}

fn assert_ok_open(who: AccountId, asset: AssetId, coll: Balance, debt: Balance, rate: FixedU128) {
	assert_ok!(open(who, asset, coll, debt, rate));
}
