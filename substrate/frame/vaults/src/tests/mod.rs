//! `pallet-vaults` test suite.

mod bad_debt;
mod basic_ops;
mod borrower_operations;
mod critical_threshold;
mod debt_in_front;
mod events;
mod final_recovery;
mod hint_helpers;
mod interest_rate;
mod last_vault;
mod lifecycle;
mod redemptions;
mod redistribution_accounting;
mod sorted_troves;

use frame::deps::sp_runtime::FixedU128;

use crate::mock::{AccountId, AssetId, Test};

pub(super) fn rate_pct(num: u128, denom: u128) -> FixedU128 {
	FixedU128::from_rational(num, denom)
}

pub(super) fn vault_status(asset: AssetId, owner: AccountId) -> crate::types::VaultStatus {
	crate::Pallet::<Test>::vault_status(asset, owner).expect("vault status")
}
