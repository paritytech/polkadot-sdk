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

//! # Dynamic Allocation Pool (DAP) Pallet
//!
//! This pallet implements `FundingSink` to collect funds into a buffer account instead of burning
//! them. The buffer account is created via `inc_providers` at genesis, ensuring it can receive any
//! amount including those below ED.
//!
//! For existing chains adding DAP, include `dap::migrations::v1::InitBufferAccount` in your
//! migrations tuple.
//!
//! Future phases will add:
//! - `FundingSource` (request_funds) for pulling funds
//! - Issuance curve and minting logic
//! - Distribution rules and scheduling

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use frame_support::{
	defensive,
	pallet_prelude::*,
	traits::{
		fungible::{Balanced, Credit, Inspect, Mutate},
		tokens::{Fortitude, FundingSink, Precision, Preservation},
		Imbalance, OnUnbalanced,
	},
	PalletId,
};

pub use pallet::*;

const LOG_TARGET: &str = "runtime::dap";

/// Type alias for balance.
pub type BalanceOf<T> =
	<<T as Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{sp_runtime::traits::AccountIdConversion, traits::StorageVersion};

	/// The in-code storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The currency type (new fungible traits).
		type Currency: Inspect<Self::AccountId>
			+ Mutate<Self::AccountId>
			+ Balanced<Self::AccountId>;

		/// The pallet ID used to derive the buffer account.
		///
		/// Each runtime should configure a unique ID to avoid collisions if multiple
		/// DAP instances are used.
		#[pallet::constant]
		type PalletId: Get<PalletId>;
	}

	impl<T: Config> Pallet<T> {
		/// Get the DAP buffer account
		/// NOTE: We may need more accounts in the future, for instance, to manage the strategic
		/// reserve. We will add them as necessary, generating them with additional seed.
		pub fn buffer_account() -> T::AccountId {
			T::PalletId::get().into_account_truncating()
		}

		/// Create the buffer account by incrementing its provider count.
		///
		/// Called once at genesis (for new chains) or via migration (for existing chains).
		pub(crate) fn create_buffer_account() {
			let buffer = Self::buffer_account();
			frame_system::Pallet::<T>::inc_providers(&buffer);
			log::info!(
				target: LOG_TARGET,
				"Created DAP buffer account: {buffer:?}"
			);
		}
	}

	/// Genesis config for the DAP pallet.
	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		#[serde(skip)]
		_phantom: core::marker::PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			// Create the buffer account at genesis so it can receive funds of any amount.
			Pallet::<T>::create_buffer_account();
		}
	}
}

/// Migrations for the DAP pallet.
pub mod migrations {
	use super::*;

	/// Version 1 migration.
	pub mod v1 {
		use super::*;
		use frame_support::traits::UncheckedOnRuntimeUpgrade;

		/// Inner migration that creates the buffer account.
		pub struct InitBufferAccountInner<T>(core::marker::PhantomData<T>);

		impl<T: Config> UncheckedOnRuntimeUpgrade for InitBufferAccountInner<T> {
			fn on_runtime_upgrade() -> Weight {
				Pallet::<T>::create_buffer_account();
				// Weight: 1 write (inc_providers)
				T::DbWeight::get().writes(1)
			}
		}

		/// Migration to create the DAP buffer account (version 0 → 1).
		///
		/// This migration only runs once when the on-chain storage version
		/// is 0, then updates it to 1.
		pub type InitBufferAccount<T> = frame_support::migrations::VersionedMigration<
			0,
			1,
			InitBufferAccountInner<T>,
			Pallet<T>,
			<T as frame_system::Config>::DbWeight,
		>;
	}
}

/// Type alias for credit (negative imbalance - funds that were slashed/removed).
/// This is for the `fungible::Balanced` trait as used by staking-async.
pub type CreditOf<T> = Credit<<T as frame_system::Config>::AccountId, <T as Config>::Currency>;

/// Implementation of FundingSink.
/// Use as `type Sink = Dap` in runtime config.
impl<T: Config> FundingSink<T::AccountId, BalanceOf<T>> for Pallet<T> {
	fn fill(source: &T::AccountId, amount: BalanceOf<T>, preservation: Preservation) {
		let buffer = Self::buffer_account();

		// Best-effort transfer: withdraw up to `amount` from source and deposit to buffer.
		// If source has less than `amount`, transfers whatever is available.
		if let Ok(credit) = T::Currency::withdraw(
			source,
			amount,
			Precision::BestEffort,
			preservation,
			Fortitude::Polite,
		) {
			// Resolve should never fail - buffer is pre-created at genesis or via migration.
			if !credit.peek().is_zero() && T::Currency::resolve(&buffer, credit).is_err() {
				defensive!("Failed to deposit to DAP buffer - funds burned");
			}
		}
	}
}

/// Implementation of OnUnbalanced for the fungible::Balanced trait.
/// Use as `type Slash = Dap` in staking-async config.
impl<T: Config> OnUnbalanced<CreditOf<T>> for Pallet<T> {
	fn on_nonzero_unbalanced(amount: CreditOf<T>) {
		let buffer = Self::buffer_account();
		let numeric_amount = amount.peek();

		// Resolve should never fail - buffer is pre-created at genesis or via migration.
		if T::Currency::resolve(&buffer, amount).is_err() {
			defensive!("Failed to deposit slash to DAP buffer - funds burned");
			return;
		}

		log::debug!(
			target: LOG_TARGET,
			"Deposited slash of {numeric_amount:?} to DAP buffer"
		);
	}
}

#[cfg(test)]
pub(crate) mod mock;
#[cfg(test)]
mod tests;
