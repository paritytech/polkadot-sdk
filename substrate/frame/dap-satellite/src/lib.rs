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
//! Intercepts native token burns (transaction fees, dust removal, coretime revenue) on system
//! parachains that do not have a central DAP and redirects them into a local buffer account for
//! eventual transfer to the central DAP.
//!
//! Important: The system chain(s) that employ a central DAP must use `pallet-dap`instead!
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
//! ## Total Issuance
//!
//! Satellite funds are burnt upon sending (reducing `total_issuance` here) and the same
//! funds are minted in the central DAP when the sent message is received.

#![cfg_attr(not(feature = "std"), no_std)]

pub mod migrations;

#[cfg(test)]
pub(crate) mod mock;
#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;
pub use weights::WeightInfo;

use frame_support::{
	pallet_prelude::*,
	sp_runtime::traits::Zero,
	traits::{
		fungible::{Balanced, Credit, Inspect, Unbalanced},
		tokens::{Fortitude, Preservation},
		Currency, Imbalance, OnUnbalanced,
	},
	weights::WeightMeter,
	PalletId,
};
use sp_runtime::{traits::BlockNumberProvider, Percent, Saturating};

pub use pallet::*;

pub use sp_dap::DAP_PALLET_ID;

pub use sp_dap::SendToDap;

const LOG_TARGET: &str = "runtime::dap-satellite";

/// Type alias for balance.
pub type BalanceOf<T> =
	<<T as Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::sp_runtime::traits::AccountIdConversion;
	use frame_system::pallet_prelude::BlockNumberFor as SystemBlockNumberFor;

	/// The in-code storage version.
	const STORAGE_VERSION: frame_support::traits::StorageVersion =
		frame_support::traits::StorageVersion::new(1);

	/// Block number type derived from the configured [`Config::BlockNumberProvider`].
	pub type BlockNumberFor<T> =
		<<T as Config>::BlockNumberProvider as BlockNumberProvider>::BlockNumber;

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
		type PalletId: Get<PalletId>;

		/// The implementation responsible for sending accumulated funds to the central DAP.
		/// Message construction and dispatch logic lives here, keeping this pallet free of
		/// message-related dependencies.
		type SendToDap: super::SendToDap<Self::AccountId, BalanceOf<Self>>;

		/// Minimum number of blocks between successive transfers to the central DAP.
		/// Acts as a rate limiter to avoid sending too many messages.
		#[pallet::constant]
		type TransferPeriod: Get<BlockNumberFor<Self>>;

		/// Minimum transferable balance required to trigger a transfer.
		/// This avoids the transfer of very small / negligible amounts.
		/// The satellite account always retains its existential deposit on top of this.
		#[pallet::constant]
		type MinTransferAmount: Get<BalanceOf<Self>>;

		/// Block number provider. Use `RelaychainDataProvider` on parachains so that
		/// `TransferPeriod` is expressed in relay chain blocks, keeping the cadence stable.
		type BlockNumberProvider: BlockNumberProvider;

		/// Weight information for the pallet's operations.
		type WeightInfo: weights::WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Successfully sent funds to the central DAP.
		SendSucceeded { amount: BalanceOf<T> },
		/// Failed to send funds. They will remain in the satellite account
		/// and sending will be retried after another `TransferPeriod` blocks.
		SendFailed { amount: BalanceOf<T> },
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<SystemBlockNumberFor<T>> for Pallet<T> {
		fn on_idle(_block: SystemBlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			// Only attempt transfers on blocks that are exact multiples of `TransferPeriod`.
			let block = T::BlockNumberProvider::current_block_number();
			if (block % T::TransferPeriod::get()) != Zero::zero() {
				return Weight::zero();
			}

			let mut meter = WeightMeter::with_limit(remaining_weight);

			// Need one read for the balance check.
			if meter.try_consume(T::DbWeight::get().reads(1)).is_err() {
				return meter.consumed();
			}

			let satellite_account = Self::satellite_account();
			// We use `reducible_balance` with `Preservation::Preserve` to get the
			// usable balance (excluding the ED).
			let available_funds = T::Currency::reducible_balance(
				&satellite_account,
				Preservation::Preserve,
				Fortitude::Polite,
			);

			if available_funds < T::MinTransferAmount::get() {
				return meter.consumed();
			}

			// Ensure there is enough weight budget for the full XCM send.
			if meter.try_consume(T::WeightInfo::send_native()).is_err() {
				return meter.consumed();
			}

			// Attempt the transfer to the central DAP.
			match T::SendToDap::send_native(satellite_account, available_funds) {
				Ok(()) => {
					Self::deposit_event(Event::SendSucceeded { amount: available_funds });
				},
				Err(()) => {
					log::debug!(
						target: LOG_TARGET,
						"DAP satellite transfer of {:?} failed at block {:?}",
						available_funds,
						block,
					);
					Self::deposit_event(Event::SendFailed { amount: available_funds });
				},
			}

			meter.consumed()
		}

		fn integrity_test() {
			assert!(
				!T::TransferPeriod::get().is_zero(),
				"TransferPeriod must not be zero (would cause division by zero in on_idle)"
			);
			assert!(
				T::PalletId::get() == sp_dap::DAP_SATELLITE_PALLET_ID,
				"PalletId must match sp_dap::DAP_SATELLITE_PALLET_ID"
			);
		}
	}

	impl<T: Config> Pallet<T> {
		/// Get the satellite account derived from the pallet ID.
		///
		/// This account accumulates funds locally before they are sent to the central DAP.
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
/// Use this on system chains that don't have a central DAP along with the Relay Chain to collect
/// imbalances (e.g. coretime revenue, tx fees, dust removal) that would otherwise be burned.
///
/// For pallets still using the legacy `Currency` trait (e.g. `pallet_identity`), use
/// [`DapSatelliteLegacyAdapter`] instead.
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

/// Type alias for legacy `NegativeImbalance` from the `Currency` trait.
type LegacyNegativeImbalance<A, C> = <C as Currency<A>>::NegativeImbalance;

/// Adapter that redirects `NegativeImbalance` from the legacy `Currency` trait to the DAP
/// satellite.
///
/// Cannot be implemented directly on `Pallet<T>` because the compiler cannot prove that
/// `<C as Currency>::NegativeImbalance` and `fungible::Credit` are always distinct types,
/// so two `OnUnbalanced` impls on the same struct are rejected.
///
/// Will be removed once all consumer pallets migrate to fungible traits.
///
/// # Example
/// ```ignore
/// type Slashed = pallet_dap_satellite::DapSatelliteLegacyAdapter<Runtime, Balances>;
/// ```
pub struct DapSatelliteLegacyAdapter<T, C>(core::marker::PhantomData<(T, C)>);

impl<T: Config, C> OnUnbalanced<LegacyNegativeImbalance<T::AccountId, C>>
	for DapSatelliteLegacyAdapter<T, C>
where
	C: Currency<T::AccountId>,
{
	fn on_nonzero_unbalanced(amount: LegacyNegativeImbalance<T::AccountId, C>) {
		let satellite = Pallet::<T>::satellite_account();
		let numeric_amount = amount.peek();
		// NOTE: resolve_creating is infallible.
		C::resolve_creating(&satellite, amount);
		log::debug!(
			target: LOG_TARGET,
			"💸 Deposited (legacy) {numeric_amount:?} to DAP satellite"
		);
	}
}
