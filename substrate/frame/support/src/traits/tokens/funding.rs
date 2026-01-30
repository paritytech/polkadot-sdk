// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Traits for customizing burn behavior in fungible token systems.
//!
//! This module provides the [`BurnHandler`] trait which allows runtimes to customize what happens
//! when tokens are burned via `fungible::Mutate::burn_from`.
//!
//! ## Overview
//!
//! The `BurnHandler` trait controls the **entire** `burn_from` operation, including:
//! - Validating the burn amount against the account's reducible balance
//! - Decreasing the account balance
//! - Handling total issuance (reduce it, or not)
//! - Any additional side effects (e.g., crediting a buffer account)
//!
//! ## Implementations
//!
//! | Implementation | Behavior | Use Case |
//! |----------------|----------|----------|
//! | [`DirectBurn`] | Reduces balance and total issuance | Traditional burning |
//! | `Dap` / `DapSatellite` | Moves funds to buffer, total issuance unchanged | DAP system chains |
//! | `()` | No-op, discards funds | Testing only |
//!
//! ## Configuration
//!
//! Configure via `pallet_balances::Config::BurnHandler`:
//!
//! ```ignore
//! impl pallet_balances::Config for Runtime {
//!     // Traditional burning - exact same behavior as before this trait existed
//!     type BurnHandler = DirectBurn<Balances>;
//!
//!     // Or for DAP satellite chains:
//!     // type BurnHandler = DapSatellite;
//! }
//! ```

use crate::traits::tokens::{
	fungible::{Inspect, Unbalanced},
	Fortitude, Precision,
	Precision::BestEffort,
	Preservation,
};
use core::marker::PhantomData;
use sp_runtime::{traits::CheckedSub, ArithmeticError, DispatchError, Saturating, TokenError};

/// Handler for `fungible::Mutate::burn_from` operations.
///
/// This trait allows customization of the entire burn flow. Implementations control:
/// - Balance validation and reduction
/// - Total issuance handling
/// - Any additional side effects
///
/// See [`DirectBurn`] for the standard implementation that reduces total issuance.
pub trait BurnHandler<AccountId, Balance> {
	/// Execute a burn operation.
	///
	/// This method is called by `pallet_balances`'s implementation of
	/// `fungible::Mutate::burn_from`.
	///
	/// # Parameters
	/// - `who`: Account to burn from
	/// - `amount`: Requested amount to burn
	/// - `preservation`: Whether to keep the account alive
	/// - `precision`: Whether to burn exactly the amount or best-effort
	/// - `force`: Whether to force past frozen funds
	///
	/// # Returns
	/// The actual amount burned on success.
	fn burn_from(
		who: &AccountId,
		amount: Balance,
		preservation: Preservation,
		precision: Precision,
		force: Fortitude,
	) -> Result<Balance, DispatchError>;
}

/// Standard burn implementation that reduces total issuance.
///
/// This is the default handler that maintains backward compatibility. It:
/// 1. Validates the burn amount against reducible balance
/// 2. Checks for total issuance underflow
/// 3. Decreases the account balance
/// 4. Reduces total issuance by the burned amount
///
/// Use this for all runtimes that want traditional burn behavior.
///
/// # Type Parameters
///
/// * `Currency` - The currency type (typically `Balances` pallet)
///
/// # Example
///
/// ```ignore
/// impl pallet_balances::Config for Runtime {
///     type BurnHandler = DirectBurn<Balances>;
/// }
/// ```
pub struct DirectBurn<Currency>(PhantomData<Currency>);

impl<Currency, AccountId> BurnHandler<AccountId, Currency::Balance> for DirectBurn<Currency>
where
	Currency: Inspect<AccountId> + Unbalanced<AccountId>,
	AccountId: Eq,
{
	fn burn_from(
		who: &AccountId,
		amount: Currency::Balance,
		preservation: Preservation,
		precision: Precision,
		force: Fortitude,
	) -> Result<Currency::Balance, DispatchError> {
		let actual = Currency::reducible_balance(who, preservation, force).min(amount);
		frame_support::ensure!(
			actual == amount || precision == BestEffort,
			TokenError::FundsUnavailable
		);
		Currency::total_issuance()
			.checked_sub(&actual)
			.ok_or(ArithmeticError::Overflow)?;
		let actual = Currency::decrease_balance(who, actual, BestEffort, preservation, force)?;
		Currency::set_total_issuance(Currency::total_issuance().saturating_sub(actual));
		Ok(actual)
	}
}

/// No-op implementation for testing.
///
/// Discards burned funds without affecting balances or issuance. Only use in tests where burn
/// behavior is irrelevant. Tests needing realistic burn semantics should use [`DirectBurn`] or
/// a custom implementation.
impl<AccountId, Balance: Default> BurnHandler<AccountId, Balance> for () {
	fn burn_from(
		_who: &AccountId,
		_amount: Balance,
		_preservation: Preservation,
		_precision: Precision,
		_force: Fortitude,
	) -> Result<Balance, DispatchError> {
		Ok(Default::default())
	}
}
