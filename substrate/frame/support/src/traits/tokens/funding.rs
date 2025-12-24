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

//! Traits for returning funds to an issuance system.
//!
//! This module provides abstractions for returning funds (burns, slashing) in a way that can be
//! configured differently per runtime.
//!
//! Two main patterns:
//! - **Direct burn**: Traditional approach where funds are destroyed on demand
//! - **Buffer-based**: Funds are returned to a buffer for reuse

use crate::{
	defensive,
	traits::tokens::{fungible, Fortitude, Precision, Preservation},
};
use core::marker::PhantomData;
use sp_runtime::traits::Zero;

/// Trait for moving funds into an issuance buffer or burning them.
///
/// Implementations can either burn directly or transfer to a buffer for reuse.
/// This trait is infallible - implementations must handle any errors internally.
///
/// Pairs with future `FundingSource::drain()` for withdrawing from the buffer.
pub trait FundingSink<AccountId, Balance> {
	/// Fill the sink with funds from the given account.
	///
	/// This could mean burning the funds or transferring them to a buffer account.
	/// The operation is infallible - any errors are handled internally.
	///
	/// # Parameters
	/// - `from`: The account to take funds from
	/// - `amount`: The amount to fill
	/// - `preservation`: Whether to preserve the source account (Preserve = keep alive, Expendable
	///   = allow death)
	fn fill(from: &AccountId, amount: Balance, preservation: Preservation);
}

/// Direct burning implementation of `FundingSink`.
///
/// This implementation burns tokens directly, reducing total issuance.
/// Used for traditional burn systems (e.g., Kusama).
///
/// # Type Parameters
///
/// * `Currency` - The currency type that implements `Mutate`
/// * `AccountId` - The account identifier type
pub struct DirectBurn<Currency, AccountId>(PhantomData<(Currency, AccountId)>);

impl<Currency, AccountId> FundingSink<AccountId, Currency::Balance>
	for DirectBurn<Currency, AccountId>
where
	Currency: fungible::Mutate<AccountId> + fungible::Inspect<AccountId>,
	Currency::Balance: Zero,
	AccountId: Eq,
{
	fn fill(from: &AccountId, amount: Currency::Balance, preservation: Preservation) {
		// Best-effort: calculate available balance and burn that amount.
		let available = Currency::reducible_balance(from, preservation, Fortitude::Polite);

		// Burn should never fail because:
		// - can_withdraw is already satisfied by reducible_balance check
		// - we are filtering out the zero amount case
		// The only failure would be an implementation bug.
		Some(amount.min(available)).filter(|t| !t.is_zero()).map(|to_burn| {
			Currency::burn_from(from, to_burn, preservation, Precision::Exact, Fortitude::Polite)
				.inspect_err(|_| {
					defensive!(
						"🚨 DirectBurn::fill failed to burn from account - it should never happen!"
					);
				})
		});
	}
}

/// No-op implementation of `FundingSink` for unit type.
/// Used for testing or when no sink behavior is needed.
impl<AccountId, Balance> FundingSink<AccountId, Balance> for () {
	fn fill(_from: &AccountId, _amount: Balance, _preservation: Preservation) {}
}
