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
//!
//! # Types
//!
//! - [`DebtComponents`]: Breakdown of debt (principal, interest, penalty) for liquidations
//! - [`PaymentBreakdown`]: How a payment is distributed during auction takes
//!
//! # Traits
//!
//! - [`AuctionsHandler`]: Vaults → Auctions (start liquidation auctions)
//! - [`CollateralManager`]: Auctions → Vaults (execute purchases, complete auctions)
//! - [`VaultsInterface`]: PSM → Vaults (query debt ceiling)

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	pallet_prelude::{DispatchError, DispatchResult},
	traits::tokens::Balance,
};
use scale_info::TypeInfo;
use sp_runtime::{FixedPointOperand, FixedU128, Saturating};
/// Debt components for liquidation auctions.
///
/// Represents the breakdown of debt that must be recovered during a liquidation auction.
/// Used when starting auctions and internally by the auctions pallet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct DebtComponents<Balance> {
	/// Principal debt - burned to maintain pUSD peg.
	pub principal: Balance,
	/// Accrued interest - burned (was already minted to Insurance Fund during accrual).
	pub interest: Balance,
	/// Liquidation penalty - transferred to Insurance Fund.
	pub penalty: Balance,
}

impl<Balance: Saturating + Copy> DebtComponents<Balance> {
	/// Create new debt components.
	pub const fn new(principal: Balance, interest: Balance, penalty: Balance) -> Self {
		Self { principal, interest, penalty }
	}

	/// Total debt to recover from the auction.
	pub fn total(&self) -> Balance {
		self.principal.saturating_add(self.interest).saturating_add(self.penalty)
	}
}

/// Breakdown of how a payment is distributed during auction `take()`.
///
/// Mirrors [`DebtComponents`] structure - tracks how much of each component was paid.
/// Use the computed methods [`burn()`](Self::burn) and [`insurance_fund()`](Self::insurance_fund)
/// to determine how to process the payment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct PaymentBreakdown<Balance> {
	/// Principal portion paid (for `CurrentLiquidationAmount` tracking).
	pub principal_paid: Balance,
	/// Interest portion paid (burned; was already minted to IF during accrual).
	pub interest_paid: Balance,
	/// Penalty portion paid (transferred to Insurance Fund).
	pub penalty_paid: Balance,
}

impl<Balance: Saturating + Copy> PaymentBreakdown<Balance> {
	/// Create new payment breakdown.
	pub const fn new(
		principal_paid: Balance,
		interest_paid: Balance,
		penalty_paid: Balance,
	) -> Self {
		Self { principal_paid, interest_paid, penalty_paid }
	}

	/// Amount to burn (principal + interest).
	///
	/// Interest is burned because it was already minted to the Insurance Fund
	/// when it accrued. Burning it on repayment balances the supply.
	pub fn burn(&self) -> Balance {
		self.principal_paid.saturating_add(self.interest_paid)
	}

	/// Amount to transfer to Insurance Fund (penalty).
	///
	/// The penalty is transferred to the IF, which temporarily holds the keeper's
	/// share until auction completion.
	pub const fn insurance_fund(&self) -> Balance {
		self.penalty_paid
	}

	/// Total payment amount.
	pub fn total(&self) -> Balance {
		self.burn().saturating_add(self.penalty_paid)
	}
}

/// Trait for the Vaults pallet to delegate auction lifecycle to the Auctions pallet.
///
/// Implemented by the Auctions pallet, called by the Vaults pallet when a vault
/// needs to be liquidated.
pub trait AuctionsHandler<AccountId, Balance> {
	/// Start a new auction for liquidating vault collateral.
	///
	/// Called by the Vaults pallet when a vault becomes undercollateralized.
	/// Returns the auction ID on success.
	///
	/// # Parameters
	///
	/// - `vault_owner`: Account whose vault is being liquidated
	/// - `collateral_amount`: Amount of collateral to auction
	/// - `debt`: Debt breakdown to recover (principal, interest, penalty)
	/// - `keeper`: Account that triggered liquidation (receives keeper incentive)
	///
	/// # Errors
	///
	/// Returns an error if the circuit breaker is active or the oracle price is unavailable.
	fn start_auction(
		vault_owner: AccountId,
		collateral_amount: Balance,
		debt: DebtComponents<Balance>,
		keeper: AccountId,
	) -> Result<u32, DispatchError>;
}

/// Trait for the Auctions pallet to call back into Vaults for asset operations.
///
/// This trait decouples the auction logic from the asset management:
/// - Auctions pallet manages auction state (price decay, staleness, incentives computation)
/// - Vaults pallet handles all asset operations (holds, transfers, pricing, minting/burning)
pub trait CollateralManager<AccountId> {
	/// The balance type used for collateral and debt amounts.
	type Balance: Balance + FixedPointOperand;

	/// Get current collateral price from oracle.
	///
	/// Returns the normalized price: `smallest_pUSD_units / smallest_collateral_unit`.
	/// Used by auctions for `restart_auction()` to set new starting price.
	fn get_dot_price() -> Option<FixedU128>;

	/// Execute a purchase: collect pUSD from buyer, transfer collateral to recipient.
	///
	/// Called during `take()`. This function:
	/// 1. Burns `payment.burn()` pUSD from the buyer (principal + interest)
	/// 2. Transfers `payment.insurance_fund()` pUSD from buyer to Insurance Fund
	/// 3. Releases `collateral_amount` from the vault owner's Seized hold
	/// 4. Transfers the collateral to the recipient
	/// 5. Reduces `CurrentLiquidationAmount` by `payment.principal_paid`
	///
	/// # Errors
	///
	/// Returns an error if the buyer has insufficient pUSD or the collateral transfer fails.
	fn execute_purchase(
		buyer: &AccountId,
		collateral_amount: Self::Balance,
		payment: PaymentBreakdown<Self::Balance>,
		recipient: &AccountId,
		vault_owner: &AccountId,
	) -> DispatchResult;

	/// Complete an auction: pay keeper, return excess collateral, record any shortfall.
	///
	/// Called when auction finishes (tab satisfied or lot exhausted).
	///
	/// # Parameters
	///
	/// - `vault_owner`: Original vault owner (receives excess collateral)
	/// - `remaining_collateral`: Excess collateral to return to owner
	/// - `shortfall`: Uncollected debt (becomes bad debt)
	/// - `keeper`: Account that triggered/restarted the auction
	/// - `keeper_incentive`: pUSD amount to pay keeper (from IF, funded by penalty)
	///
	/// # Errors
	///
	/// Returns an error if the keeper payment or collateral release fails.
	fn complete_auction(
		vault_owner: &AccountId,
		remaining_collateral: Self::Balance,
		shortfall: Self::Balance,
		keeper: &AccountId,
		keeper_incentive: Self::Balance,
	) -> DispatchResult;

	/// Execute a surplus auction purchase: buyer sends collateral, receives pUSD from IF.
	///
	/// Called during `take_surplus()`. This function:
	/// 1. Transfers `pusd_amount` pUSD from the Insurance Fund to the recipient
	/// 2. Transfers `collateral_amount` from the buyer to the `FeeHandler`
	///
	/// # Errors
	///
	/// Returns an error if the buyer has insufficient collateral or IF has insufficient pUSD.
	fn execute_surplus_purchase(
		buyer: &AccountId,
		recipient: &AccountId,
		pusd_amount: Self::Balance,
		collateral_amount: Self::Balance,
	) -> DispatchResult;

	/// Get the Insurance Fund's pUSD balance.
	///
	/// Used to check if surplus auctions can be started (IF balance > threshold).
	fn get_insurance_fund_balance() -> Self::Balance;

	/// Get the total pUSD supply.
	///
	/// Used with `get_insurance_fund_balance()` to calculate whether the
	/// Insurance Fund exceeds the surplus auction threshold.
	fn get_total_pusd_supply() -> Self::Balance;

	/// Transfer surplus pUSD from Insurance Fund via configured handler.
	///
	/// Used in DirectTransfer mode to send surplus directly to treasury
	/// without going through an auction. The destination is determined
	/// by the runtime's `SurplusHandler` configuration.
	///
	/// # Parameters
	/// - `amount`: Amount of pUSD to transfer from Insurance Fund
	///
	/// # Errors
	/// Returns an error if the Insurance Fund has insufficient pUSD.
	fn transfer_surplus(amount: Self::Balance) -> DispatchResult;
}
