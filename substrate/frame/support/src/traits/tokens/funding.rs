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

//! Traits for handling funds that would otherwise be burned.
//!
//! This module provides abstractions for intercepting burns and redirecting funds in a way that
//! can be configured differently per runtime.
//!
//! ## Overview
//!
//! There are two main traits for handling funds that would otherwise be burned:
//!
//! | Trait | Entry Point | Called By |
//! |-------|-------------|-----------|
//! | `BurnHandler` | `pallet_balances::burn_from()` | After balance decreased, handles the "burn" |
//! | `OnUnbalanced` | Handler receives `Credit` imbalance | Fee handlers, revenue, slashes |
//!
//! ## When to Use Each Trait
//!
//! ### BurnHandler
//!
//! Called by `pallet_balances::burn_from()` after the source account's balance has been decreased.
//! This is the hook point for intercepting burns before total issuance is reduced.
//!
//! **Use cases:**
//! - User calls `Balances::burn` extrinsic
//! - Any pallet calling `T::Currency::burn_from()`
//!
//! **Configuration:** Set `type BurnDestination` in `pallet_balances::Config`.
//!
//! ### OnUnbalanced
//!
//! Defined in `crate::traits::tokens::imbalance`. Handles `Credit` imbalances.
//!
//! **Use cases:**
//! - Transaction fees (via `pallet_transaction_payment::OnChargeTransaction`)
//! - Coretime revenue (`pallet_broker::OnRevenue`)
//! - Staking slashes
//! - Any context producing a `Credit` that needs handling
//!
//! ## Implementation Patterns
//!
//! ### Direct Burn
//!
//! Use `DirectBurn` to reduce total issuance immediately:
//!
//! ```ignore
//! impl pallet_balances::Config for Runtime {
//!     type BurnDestination = pallet_balances::DirectBurn<Balances, AccountId>;
//! }
//! ```
//!
//! ### Buffer-Based (DAP)
//!
//! Redirect funds to a buffer account for later reuse:
//!
//! ```ignore
//! // On AssetHub (central DAP)
//! impl pallet_balances::Config for Runtime {
//!     type BurnDestination = Dap;
//! }
//!
//! // On other system chains (satellite)
//! impl pallet_balances::Config for Runtime {
//!     type BurnDestination = DapSatellite;
//! }
//! ```

use crate::traits::tokens::fungible;
use core::marker::PhantomData;
use sp_runtime::Saturating;

/// Trait for handling burned funds.
///
/// This trait is used by `pallet_balances::burn_from` to handle funds after they have been
/// removed from the source account. Implementations can either:
/// - Reduce total issuance (traditional burning via `DirectBurn`)
/// - Credit to a buffer account (for DAP systems)
pub trait BurnHandler<Balance> {
	/// Handle funds that have been burned from an account.
	///
	/// This operation is infallible.
	fn on_burned(amount: Balance);
}

/// Direct burning implementation of `BurnHandler`.
///
/// This implementation burns tokens directly by reducing total issuance.
/// Used for traditional burn systems (e.g., Kusama).
///
/// # Type Parameters
///
/// * `Currency` - The currency type that implements `fungible::Unbalanced`
/// * `AccountId` - The account identifier type
pub struct DirectBurn<Currency, AccountId>(PhantomData<(Currency, AccountId)>);

impl<Currency, AccountId> BurnHandler<Currency::Balance> for DirectBurn<Currency, AccountId>
where
	Currency: fungible::Unbalanced<AccountId>,
	AccountId: Eq,
{
	fn on_burned(amount: Currency::Balance) {
		// Reduce total issuance - funds are permanently destroyed
		Currency::set_total_issuance(Currency::total_issuance().saturating_sub(amount));
	}
}

/// No-op implementation of `BurnHandler` for unit type.
///
/// This implementation discards burned funds without reducing total issuance or crediting any
/// buffer. Useful for test configurations where burn behavior is irrelevant, but be aware that
/// tests using this won't verify realistic burn semantics.
impl<Balance> BurnHandler<Balance> for () {
	fn on_burned(_amount: Balance) {}
}
