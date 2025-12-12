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

//! Traits for funding sources and sinks in token issuance systems.
//!
//! This module provides abstractions for pulling funds (rewards, payments) and returning funds
//! (burns, slashing) in a way that can be configured differently per runtime.
//!
//! Two main patterns:
//! - **Direct mint/burn**: Traditional approach where funds are created/destroyed on demand
//! - **Buffer-based**: Funds are pre-minted into a buffer and distributed from there

use crate::traits::tokens::{fungible, Fortitude, Precision, Preservation};
use core::marker::PhantomData;
use sp_runtime::{DispatchError, DispatchResult};

/// Trait for requesting funds from an issuance system.
///
/// Implementations can either mint directly or pull from a pre-minted buffer.
pub trait FundingSource<AccountId, Balance> {
	/// Request funds to be transferred to the beneficiary.
	///
	/// Returns the actual amount transferred, which may be less than requested
	/// if the source has insufficient funds.
	fn request_funds(beneficiary: &AccountId, amount: Balance) -> Result<Balance, DispatchError>;
}

/// Trait for returning funds to an issuance system.
///
/// Implementations can either burn directly or return to a buffer for reuse.
pub trait FundingSink<AccountId, Balance> {
	/// Return funds from the given account back to the issuance system.
	///
	/// This could mean burning the funds or transferring them to a buffer account.
	///
	/// # Parameters
	/// - `from`: The account to take funds from
	/// - `amount`: The amount to return
	/// - `preservation`: Whether to preserve the source account (Preserve = keep alive, Expendable
	///   = allow death)
	fn return_funds(
		from: &AccountId,
		amount: Balance,
		preservation: Preservation,
	) -> DispatchResult;
}

/// Direct minting implementation of `FundingSource`.
///
/// This implementation mints tokens directly when funds are requested.
/// Used for traditional mint-on-demand systems (e.g., Kusama).
///
/// # Type Parameters
///
/// * `Currency` - The currency type that implements `Mutate`
/// * `AccountId` - The account identifier type
pub struct DirectMint<Currency, AccountId>(PhantomData<(Currency, AccountId)>);

impl<Currency, AccountId> FundingSource<AccountId, Currency::Balance>
	for DirectMint<Currency, AccountId>
where
	Currency: fungible::Mutate<AccountId>,
	AccountId: Eq,
{
	fn request_funds(
		beneficiary: &AccountId,
		amount: Currency::Balance,
	) -> Result<Currency::Balance, DispatchError> {
		Currency::mint_into(beneficiary, amount)?;
		Ok(amount)
	}
}

/// Direct burning implementation of `FundingSink`.
///
/// This implementation burns tokens directly when funds are returned.
/// Used for traditional burn-on-return systems (e.g., Kusama).
///
/// # Type Parameters
///
/// * `Currency` - The currency type that implements `Mutate`
/// * `AccountId` - The account identifier type
pub struct DirectBurn<Currency, AccountId>(PhantomData<(Currency, AccountId)>);

impl<Currency, AccountId> FundingSink<AccountId, Currency::Balance>
	for DirectBurn<Currency, AccountId>
where
	Currency: fungible::Mutate<AccountId>,
	AccountId: Eq,
{
	fn return_funds(
		from: &AccountId,
		amount: Currency::Balance,
		preservation: Preservation,
	) -> DispatchResult {
		Currency::burn_from(from, amount, preservation, Precision::Exact, Fortitude::Polite)?;
		Ok(())
	}
}

/// No-op implementation of `FundingSink` for unit type.
/// Used for testing or when no sink behavior is needed.
impl<AccountId, Balance> FundingSink<AccountId, Balance> for () {
	fn return_funds(
		_from: &AccountId,
		_amount: Balance,
		_preservation: Preservation,
	) -> DispatchResult {
		Ok(())
	}
}
