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

//! # DAP Satellite Pallet
//!
//! This pallet is meant to be used on **system chains other than AssetHub** (e.g., Coretime,
//! People, BridgeHub) or on the **Relay Chain**. It should NOT be deployed on AssetHub, which
//! hosts the central DAP pallet (`pallet-dap`).
//!
//! The DAP Satellite collects funds that would otherwise be burned (e.g., transaction fees and
//! coretime revenue) into a local satellite account. These funds are accumulated
//! locally and will eventually be transferred via XCM to the central DAP buffer on AssetHub.
//!
//! ## Implementation
//!
//! This is a minimal implementation that only accumulates funds locally. The periodic XCM
//! transfer to AssetHub is NOT yet implemented.
//!
//! In this first iteration, the pallet provides the following trait implementations:
//! - `BurnHandler`: Called by `pallet_balances::burn_from` to redirect burned funds to satellite.
//! - `OnUnbalanced`: For the `fungible::Balanced` trait, useful for coretime revenue and fee
//!   splits.
//!
//! `BurnHandler`: frame_support::traits::tokens::BurnHandler
//! `OnUnbalanced`: frame_support::traits::OnUnbalanced
//!
//! For existing chains adding DAP Satellite, include
//! `dap_satellite::migrations::v1::InitSatelliteAccount` in your migrations tuple.
//!
//! **TODO:**
//! - Periodic XCM transfer to AssetHub DAP buffer
//! - Configuration for XCM period and destination
//!
//! ## Usage
//!
//! On system chains (not AssetHub) or Relay Chain, configure pallets to use the satellite:
//!
//! ```ignore
//! // In runtime configuration for Coretime/People/BridgeHub/RelayChain
//! impl pallet_balances::Config for Runtime {
//!     type BurnHandler = DapSatellite;
//! }
//!
//! // For coretime revenue (pallet-broker)
//! impl pallet_broker::Config for Runtime {
//!     type OnRevenue = DapSatellite;
//! }
//! ```

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
pub(crate) mod mock;
#[cfg(test)]
mod tests;

extern crate alloc;

use frame_support::{
	pallet_prelude::*,
	traits::{
		fungible::{Balanced, Credit, Inspect, Mutate, Unbalanced},
		tokens::{BurnHandler, Fortitude, Precision, Precision::BestEffort, Preservation},
		Imbalance, OnUnbalanced,
	},
	PalletId,
};
use sp_runtime::{Percent, Saturating, TokenError};

pub use pallet::*;

const LOG_TARGET: &str = "runtime::dap-satellite";

/// Type alias for balance.
pub type BalanceOf<T> =
	<<T as Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::sp_runtime::traits::AccountIdConversion;

	/// The in-code storage version.
	const STORAGE_VERSION: frame_support::traits::StorageVersion =
		frame_support::traits::StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The currency type.
		type Currency: Inspect<Self::AccountId>
			+ Mutate<Self::AccountId>
			+ Unbalanced<Self::AccountId>
			+ Balanced<Self::AccountId>;

		/// The pallet ID used to derive the satellite account.
		///
		/// Each runtime should configure a unique ID to avoid collisions if multiple
		/// DAP satellite instances are used.
		#[pallet::constant]
		type PalletId: Get<PalletId>;
	}

	impl<T: Config> Pallet<T> {
		/// Get the satellite account derived from the pallet ID.
		///
		/// This account accumulates funds locally before they are sent to AssetHub.
		pub fn satellite_account() -> T::AccountId {
			T::PalletId::get().into_account_truncating()
		}

		/// Create the satellite account with a provider reference and fund it with ED.
		///
		/// Called once at genesis (for new chains and test/benchmark setup) or via migration
		/// (for existing chains). Safe to call multiple times - will early exit if account
		/// already exists with sufficient balance.
		pub fn create_satellite_account() {
			let satellite = Self::satellite_account();
			let ed = T::Currency::minimum_balance();

			if frame_system::Pallet::<T>::providers(&satellite) > 0 &&
				T::Currency::balance(&satellite) >= ed
			{
				log::debug!(
					target: LOG_TARGET,
					"DAP satellite account already initialized: {satellite:?}"
				);
				return;
			}

			// Ensure the account exists by incrementing its provider count.
			frame_system::Pallet::<T>::inc_providers(&satellite);

			// Fund the account with ED so it can receive deposits of any amount.
			// Without this, deposits smaller than ED would fail.
			log::info!(
				target: LOG_TARGET,
				"Attempting to mint ED ({ed:?}) into DAP satellite: {satellite:?}"
			);

			match T::Currency::mint_into(&satellite, ed) {
				Ok(_) => {
					// Mark ED as inactive so it doesn't participate in governance.
					T::Currency::deactivate(ed);
					log::info!(
						target: LOG_TARGET,
						"🛰️ Created DAP satellite account: {satellite:?}"
					);
				},
				Err(e) => {
					log::error!(
						target: LOG_TARGET,
						"🚨 Failed to mint ED into DAP satellite: {e:?}"
					);
				},
			}
		}
	}

	/// Genesis config for the DAP Satellite pallet.
	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		#[serde(skip)]
		_phantom: core::marker::PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			// Create and fund the satellite account at genesis.
			Pallet::<T>::create_satellite_account();
		}
	}
}

/// Migrations for the DAP Satellite pallet.
pub mod migrations {
	use super::*;

	/// Version 1 migration.
	pub mod v1 {
		use super::*;

		mod inner {
			use super::*;
			use frame_support::traits::UncheckedOnRuntimeUpgrade;

			/// Inner migration that creates the satellite account.
			pub struct InitSatelliteAccountInner<T>(core::marker::PhantomData<T>);

			impl<T: Config> UncheckedOnRuntimeUpgrade for InitSatelliteAccountInner<T> {
				fn on_runtime_upgrade() -> Weight {
					Pallet::<T>::create_satellite_account();
					// Weight: inc_providers (1 read, 1 write) + mint_into (2 reads, 2 writes)
					T::DbWeight::get().reads_writes(3, 3)
				}
			}
		}

		/// Migration to create the DAP satellite account (version 0 → 1).
		pub type InitSatelliteAccount<T> = frame_support::migrations::VersionedMigration<
			0,
			1,
			inner::InitSatelliteAccountInner<T>,
			Pallet<T>,
			<T as frame_system::Config>::DbWeight,
		>;
	}
}

/// Type alias for credit (negative imbalance - funds that were removed).
/// This is for the `fungible::Balanced` trait.
pub type CreditOf<T> = Credit<<T as frame_system::Config>::AccountId, <T as Config>::Currency>;

/// A configurable fee handler that splits fees between DAP satellite and another destination.
///
/// - `DapPercent`: Percentage of fees to send to DAP satellite (e.g., `Percent::from_percent(0)`)
/// - `OtherHandler`: Where to send the remaining fees (e.g., `ToAuthor`, `DealWithFees`)
///
/// Tips always go 100% to `OtherHandler`.
///
/// # Example
///
/// ```ignore
/// parameter_types! {
///     pub const DapSatelliteFeePercent: Percent = Percent::from_percent(0); // 0% to DAP
/// }
///
/// type DealWithFeesSatellite = pallet_dap_satellite::DealWithFeesSplit<
///     Runtime,
///     DapSatelliteFeePercent,
///     DealWithFees<Runtime>, // Or ToAuthor<Runtime> for relay chain
/// >;
///
/// impl pallet_transaction_payment::Config for Runtime {
///     type OnChargeTransaction = FungibleAdapter<Balances, DealWithFeesSatellite>;
/// }
/// ```
pub struct DealWithFeesSplit<T, DapPercent, OtherHandler>(
	core::marker::PhantomData<(T, DapPercent, OtherHandler)>,
);

impl<T, DapPercent, OtherHandler> OnUnbalanced<CreditOf<T>>
	for DealWithFeesSplit<T, DapPercent, OtherHandler>
where
	T: Config,
	DapPercent: Get<Percent>,
	OtherHandler: OnUnbalanced<CreditOf<T>>,
{
	fn on_unbalanceds(mut fees_then_tips: impl Iterator<Item = CreditOf<T>>) {
		if let Some(fees) = fees_then_tips.next() {
			let dap_percent = DapPercent::get();
			let other_percent = Percent::one().saturating_sub(dap_percent);
			let mut split =
				fees.ration(dap_percent.deconstruct() as u32, other_percent.deconstruct() as u32);
			if let Some(tips) = fees_then_tips.next() {
				// Tips go 100% to other handler.
				tips.merge_into(&mut split.1);
			}
			if !dap_percent.is_zero() {
				<Pallet<T> as OnUnbalanced<_>>::on_unbalanced(split.0);
			}
			OtherHandler::on_unbalanced(split.1);
		}
	}
}

/// Implementation of BurnHandler for pallet.
///
/// Moves burned funds to the satellite account instead of reducing total issuance.
/// Total issuance remains unchanged; funds are marked as inactive for governance.
impl<T: Config> BurnHandler<T::AccountId, BalanceOf<T>> for Pallet<T> {
	fn burn_from(
		who: &T::AccountId,
		amount: BalanceOf<T>,
		preservation: Preservation,
		precision: Precision,
		force: Fortitude,
	) -> Result<BalanceOf<T>, DispatchError> {
		let actual = T::Currency::reducible_balance(who, preservation, force).min(amount);
		ensure!(actual == amount || precision == BestEffort, TokenError::FundsUnavailable);
		let actual = T::Currency::decrease_balance(who, actual, BestEffort, preservation, force)?;

		// Credit the satellite account instead of reducing total issuance.
		let satellite = Self::satellite_account();
		let _ = T::Currency::increase_balance(&satellite, actual, BestEffort).inspect_err(|_| {
			frame_support::defensive!(
				"Failed to credit DAP satellite - Loss of funds due to overflow"
			);
		});

		// Mark funds as inactive so they don't participate in governance voting.
		// TODO: When implementing XCM transfer to AssetHub, call `reactivate(amount)` before
		// sending.
		T::Currency::deactivate(actual);

		Ok(actual)
	}
}

/// Implementation of `OnUnbalanced` for the `fungible::Balanced` trait.
///
/// Use this on system chains (not AssetHub) or Relay Chain to collect imbalances
/// (e.g., coretime revenue) that would otherwise be burned.
///
/// # Example
///
/// ```ignore
/// impl pallet_broker::Config for Runtime {
///     type OnRevenue = DapSatellite;
/// }
/// ```
impl<T: Config> OnUnbalanced<CreditOf<T>> for Pallet<T> {
	fn on_nonzero_unbalanced(amount: CreditOf<T>) {
		let satellite = Self::satellite_account();
		let numeric_amount = amount.peek();

		// Resolve should never fail because:
		// - can_deposit on destination succeeds since satellite exists (created with provider at
		//   genesis/runtime upgrade so no ED issue)
		// - amount is guaranteed non-zero by the trait method signature
		// The only failure would be overflow on destination.
		let _ = T::Currency::resolve(&satellite, amount).inspect_err(|_| {
			frame_support::defensive!(
				"🚨 Failed to deposit to DAP satellite - funds burned, it should never happen!"
			);
		});

		log::debug!(
			target: LOG_TARGET,
			"💸 Deposited {numeric_amount:?} to DAP satellite"
		);

		// Mark funds as inactive so they don't participate in governance voting.
		// TODO: When implementing XCM transfer to AssetHub, call `reactivate(amount)` before
		// sending.
		T::Currency::deactivate(numeric_amount);
	}
}
