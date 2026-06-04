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
//! Intercepts native token burns (transaction fees, dust removal, coretime revenue) on
//! non-AssetHub chains and redirects them into a local buffer account for eventual transfer
//! to the central DAP on AssetHub.
//!
//! Do NOT use on AssetHub (use `pallet-dap`).
//!
//! ## Usage
//!
//! - **Fees**: Use [`DealWithFeesSplit`] to split fees between DAP satellite and other handlers
//! - **Burns/Revenue**: Use `DapSatellite` as `OnUnbalanced<CreditOf>` handler (e.g., dust removal,
//!   coretime revenue)
//! Note: Direct calls to `pallet_balances::Pallet::burn()` extrinsic are not redirected to
//! the satellite buffer — they still reduce total issuance directly.
//!
//! ## Setup
//!
//! The satellite account must be pre-funded with at least existential deposit.
//! For new chains, include the satellite account in the balances genesis config.
//! For existing chains, fund it via a manual transfer.
//!
//! If the satellite account is not pre-funded, deposits below ED will be silently burned.
//!
//! ## TODO
//!
//! - Periodic XCM transfer to AssetHub DAP buffer
//! - Reconsider `active_issuance` handling: currently we don't deactivate funds on satellite chains
//!   since governance uses active issuance from AssetHub only. When XCM transfer is implemented,
//!   verify that teleport handles total issuance correctly.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
pub(crate) mod mock;
#[cfg(test)]
mod tests;

use frame_support::{
	pallet_prelude::*,
	traits::{
		fungible::{Balanced, Credit, Inspect, Unbalanced},
		Imbalance, OnUnbalanced,
	},
	PalletId,
};
use sp_runtime::{Percent, Saturating};

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

/// Implementation of `OnUnbalanced` for the `fungible::Balanced` trait.
///
/// Use this on system chains (not AssetHub) or Relay Chain to collect imbalances
/// (e.g. coretime revenue, tx fees, dust removal) that would otherwise be burned.
///
/// Only the new fungible `Credit` type is supported. An `OnUnbalanced<NegativeImbalance>` impl
/// for the old `Currency` trait is not provided because there are no active consumers: all pallets
/// that could produce `NegativeImbalance` on satellite chains (staking, identity,
/// election-provider, ...) are either deprecated, or already use the new fungible traits.
impl<T: Config> OnUnbalanced<CreditOf<T>> for Pallet<T> {
	fn on_nonzero_unbalanced(amount: CreditOf<T>) {
		let satellite = Self::satellite_account();
		let numeric_amount = amount.peek();

		// Resolve should never fail because:
		// - can_deposit on destination succeeds assuming satellite is pre-funded with ED
		// - amount is guaranteed non-zero by the trait method signature
		// The only failure would be overflow on destination or unfunded satellite.
		let _ = T::Currency::resolve(&satellite, amount).inspect_err(|_| {
			frame_support::defensive!(
				"🚨 Failed to deposit to DAP satellite - funds burned, it should never happen!"
			);
		});

		log::debug!(
			target: LOG_TARGET,
			"💸 Deposited {numeric_amount:?} to DAP satellite"
		);
	}
}
