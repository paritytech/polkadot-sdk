//! Port of liquity_v2/contracts/test/troveManager.t.sol row 17
//! (`testLiquidateLastTroveReverts`, line 30706).
//!
//! Polkadot encodes "the system always needs at least one vault" as
//! `LastVaultCannotBeLiquidated` from `prepare_liquidation`
//! (interfaces.rs:55-58). The check fires when the only stake on the
//! branch belongs to the vault being liquidated (proxy for "last
//! redistribution recipient").

use crate::{mock::*, tests::rate_pct};
use frame::deps::{frame_support::assert_noop, sp_runtime::FixedU128};

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

fn assert_ok_open(who: AccountId, asset: AssetId, coll: Balance, debt: Balance, rate: FixedU128) {
	use frame::deps::frame_support::assert_ok;
	assert_ok!(open(who, asset, coll, debt, rate));
}
