// This file is part of Substrate.

// Copyright (C) 2020-2025 Amforc AG.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Shared primitives for pUSD pallets.
//!
//! This crate provides common types and traits used by the vaults, auctions and PSM pallets.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	pallet_prelude::{DispatchError, DispatchResult},
	traits::tokens::Balance,
};
use scale_info::TypeInfo;
use sp_runtime::{traits::Saturating, FixedPointOperand, FixedU128};

/// Breakdown of how a payment is distributed during auction `take()`.
///
/// Returned by `Tab::apply_payment()` and consumed by `PurchaseParams::new()`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct PaymentBreakdown<Balance> {
	/// Amount to burn (principal debt repayment)
	pub burn: Balance,
	/// Amount to transfer to keeper (incentive)
	pub keeper: Balance,
	/// Amount to transfer to Insurance Fund (interest + penalty net of keeper)
	pub insurance_fund: Balance,
}

impl<Balance: Default + Saturating + Copy> PaymentBreakdown<Balance> {
	/// Calculate the total payment amount.
	pub fn total(&self) -> Balance {
		self.burn.saturating_add(self.keeper).saturating_add(self.insurance_fund)
	}
}

/// Parameters for executing a collateral purchase during auction `take()`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PurchaseParams<AccountId, Balance> {
	/// Original vault owner (collateral is released from their seized hold)
	pub vault_owner: AccountId,
	/// Account paying pUSD for the collateral
	pub buyer: AccountId,
	/// Account receiving the collateral (may differ from buyer)
	pub recipient: AccountId,
	/// Account receiving the keeper incentive
	pub keeper: AccountId,
	/// Amount of collateral to transfer to recipient
	pub collateral_amount: Balance,
	/// How the pUSD payment is distributed
	pub payment: PaymentBreakdown<Balance>,
}

impl<AccountId, Balance> PurchaseParams<AccountId, Balance> {
	/// Create new purchase parameters.
	///
	/// # Arguments
	/// * `vault_owner` - Original vault owner
	/// * `buyer` - Account paying pUSD
	/// * `recipient` - Account receiving collateral
	/// * `keeper` - Account receiving keeper incentive
	/// * `collateral_amount` - Amount of collateral to transfer
	/// * `payment` - Payment breakdown from `Tab::apply_payment()`
	pub const fn new(
		vault_owner: AccountId,
		buyer: AccountId,
		recipient: AccountId,
		keeper: AccountId,
		collateral_amount: Balance,
		payment: PaymentBreakdown<Balance>,
	) -> Self {
		Self { vault_owner, buyer, recipient, keeper, collateral_amount, payment }
	}
}

/// Trait for the Vaults pallet to delegate auction lifecycle to the Auctions pallet.
///
/// This trait is implemented by the Auctions pallet and called by the Vaults pallet
/// when a vault needs to be liquidated.
pub trait AuctionsHandler<AccountId, Balance> {
	/// Start a new auction for liquidating vault collateral.
	///
	/// Called by the Vaults pallet when a vault becomes undercollateralized.
	///
	/// # Arguments
	/// * `vault_owner` - Account whose vault is being liquidated
	/// * `collateral_amount` - Amount of collateral to auction
	/// * `principal` - Principal debt to recover (gets burned)
	/// * `accrued_interest` - Accumulated interest (goes to Insurance Fund)
	/// * `penalty` - Liquidation penalty (goes to Insurance Fund)
	/// * `keeper` - The account that triggered liquidation (receives keeper incentive)
	///
	/// Returns the auction ID on success.
	fn start_auction(
		vault_owner: &AccountId,
		collateral_amount: Balance,
		principal: Balance,
		accrued_interest: Balance,
		penalty: Balance,
		keeper: &AccountId,
	) -> Result<u32, DispatchError>;
}

/// Trait for the Auctions pallet to call back into Vaults for asset operations.
///
/// This trait decouples the auction logic from the asset management:
/// - Auctions pallet manages auction state (price decay, staleness, incentives computation)
/// - Vaults pallet handles all asset operations (holds, transfers, pricing, minting/burning)
///
///
/// # Keeper Payment
///
/// Keepers are paid at the end of the auction through buyer purchases. When buyers
/// call `take()` and principal hits 0, we pay the keeper. The
/// keeper receives their portion directly from the buyer via [`execute_purchase`].
pub trait CollateralManager<AccountId> {
	/// The balance type used for collateral and debt amounts.
	type Balance: Balance + FixedPointOperand;

	/// Get current collateral price from oracle.
	///
	/// Returns the normalized price: smallest_pUSD_units / smallest_collateral_unit.
	/// Used by auctions for `restart_auction()` to set new starting price.
	///
	/// The collateral asset is determined by the implementing pallet's configuration.
	fn get_dot_price() -> Option<FixedU128>;

	/// Execute a purchase: collect pUSD from buyer, transfer collateral to recipient.
	///
	/// Called during `take()`. This function:
	/// 1. Burns `burn_amount` pUSD from the buyer (principal debt)
	/// 2. Transfers `insurance_fund_amount` pUSD from buyer to Insurance Fund
	/// 3. Transfers `keeper_amount` pUSD from buyer to keeper if principal hits 0.
	/// 4. Releases `collateral_amount` from the vault owner's Seized hold
	/// 5. Transfers the collateral to the recipient
	///
	/// The buyer pays the full tab: `burn_amount + insurance_fund_amount + keeper_amount`.
	///
	/// Returns an error if the buyer has insufficient pUSD or if the collateral
	/// transfer fails.
	fn execute_purchase(params: PurchaseParams<AccountId, Self::Balance>) -> DispatchResult;

	/// Complete an auction: return excess collateral and record any shortfall.
	///
	/// Called when auction finishes (tab satisfied or lot exhausted).
	///
	/// # Arguments
	/// * `vault_owner` - Original vault owner (receives excess collateral)
	/// * `remaining_collateral` - Excess collateral to return to owner
	/// * `shortfall` - Uncollected debt (becomes bad debt)
	fn complete_auction(
		vault_owner: &AccountId,
		remaining_collateral: Self::Balance,
		shortfall: Self::Balance,
	) -> DispatchResult;
}
