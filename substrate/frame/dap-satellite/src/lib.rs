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
//! ## Total Issuance
//!
//! Satellite funds are burnt when sent via XCM (reducing `total_issuance` there) and the same
//! funds are minted in the AssetHub DAP buffer when the XCM message is received.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
pub(crate) mod mock;
#[cfg(test)]
mod tests;

use frame_support::{
	pallet_prelude::*,
	traits::{
		fungible::{Balanced, Credit, Inspect, Mutate, Unbalanced},
		tokens::{Fortitude::Polite, Precision::Exact, Preservation::Preserve},
		Imbalance, OnUnbalanced,
	},
	weights::WeightMeter,
	PalletId,
};
use sp_runtime::{Percent, Saturating};

pub use pallet::*;

/// Trait for dispatching the XCM transfer to the DAP buffer on AssetHub.
///
/// The pallet burns tokens from the satellite account before calling [`SendToDap::send`].
/// Implementations should construct and dispatch the appropriate XCM message for `amount`
/// tokens. On error, the pallet restores the burned tokens via `mint_into`.
pub trait SendToDap<Balance> {
	/// The error type returned when sending fails. Must implement [`core::fmt::Debug`] so the
	/// pallet can log the failure reason.
	type Error: core::fmt::Debug;

	/// Send `amount` (already burned from the satellite account) to the DAP buffer via XCM.
	///
	/// Returns `Ok(())` if the message was successfully enqueued, `Err(Self::Error)` otherwise.
	fn send(amount: Balance) -> Result<(), Self::Error>;
}

const LOG_TARGET: &str = "runtime::dap-satellite";

/// Type alias for balance.
pub type BalanceOf<T> =
	<<T as Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::sp_runtime::traits::AccountIdConversion;
	use frame_system::pallet_prelude::BlockNumberFor;

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
		type PalletId: Get<PalletId>;

		/// The implementation responsible for sending accumulated funds to the DAP buffer
		/// on AssetHub via XCM. All XCM construction and dispatch logic lives here,
		/// keeping this pallet free of XCM dependencies.
		type SendToDap: super::SendToDap<BalanceOf<Self>>;

		/// Minimum number of blocks between successive XCM transfers to AssetHub.
		/// Acts as a rate limiter to avoid sending too many XCM messages.
		#[pallet::constant]
		type TransferPeriod: Get<BlockNumberFor<Self>>;

		/// Minimum transferable balance required to trigger a transfer.
		/// This avoids the transfer of very small / negligible amounts.
		/// The satellite account always retains its existential deposit on top of this.
		#[pallet::constant]
		type MinTransferAmount: Get<BalanceOf<Self>>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Successfully sent funds to the AssetHub DAP buffer via XCM.
		SendSucceeded { amount: BalanceOf<T> },
		/// Failed to send funds via XCM. They will remain in the satellite account
		/// and sending will be retried after the next `TransferPeriod`.
		SendFailed { amount: BalanceOf<T> },
	}

	/// The block at which the last XCM transfer to the AssetHub DAP was made. This is set to
	/// `None` if no transfer has been dispatched yet. Use `OptionQuery` to distinguish between
	/// "never transferred" (None) and "transferred at block 0" (Some(0)).
	#[pallet::storage]
	pub type LastTransferBlock<T: Config> = StorageValue<_, BlockNumberFor<T>, OptionQuery>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_idle(block: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			let mut meter = WeightMeter::with_limit(remaining_weight);

			// We need at least one read (of LastTransferBlock) to proceed.
			if meter.try_consume(T::DbWeight::get().reads(1)).is_err() {
				return meter.consumed();
			}

			// Enforce the rate limit - don't send until `TransferPeriod` blocks have passed.
			let last = LastTransferBlock::<T>::get().unwrap_or_default();
			if block.saturating_sub(last) <= T::TransferPeriod::get() {
				return meter.consumed();
			}

			// Check how much is available above the ED.
			// Since the ED is constant, only the balance read counts towards the weight here.
			if meter.try_consume(T::DbWeight::get().reads(1)).is_err() {
				return meter.consumed();
			}
			let balance = T::Currency::balance(&Self::satellite_account());
			let ed = T::Currency::minimum_balance();
			let available_funds = balance.saturating_sub(ed);

			if available_funds < T::MinTransferAmount::get() {
				return meter.consumed();
			}

			// We update the last transfer block irrespective of the transfer result. If we
			// don't and a failure occurs repeatedly, then a transfer will be attempted on
			// every block instead of the configured period, which would be undesirable.
			if meter.try_consume(T::DbWeight::get().writes(1)).is_err() {
				return meter.consumed();
			}
			LastTransferBlock::<T>::put(block);

			// Attempt the transfer to the central DAP buffer.
			match Self::do_send_to_central_dap(available_funds) {
				Ok(()) => {
					Self::deposit_event(Event::SendSucceeded { amount: available_funds });
				},
				Err(e) => {
					// A warning is sufficient since the transfer will be retried later.
					log::warn!(
						target: LOG_TARGET,
						"DAP satellite transfer of {:?} failed at block {:?}: {:?}",
						available_funds,
						block,
						e
					);
					Self::deposit_event(Event::SendFailed { amount: available_funds });
				},
			}

			meter.consumed()
		}
	}

	/// Internal error variants for [`Pallet::do_send_to_central_dap`].
	#[derive(Debug)]
	enum DoSendError {
		/// Failed to burn tokens from the satellite account before sending.
		BurnFailed,
		/// The [`Config::SendToDap`] implementation failed to dispatch the XCM.
		SendFailed,
	}

	impl<T: Config> Pallet<T> {
		/// Get the satellite account derived from the pallet ID.
		///
		/// This account accumulates funds locally before they are sent to AssetHub.
		pub fn satellite_account() -> T::AccountId {
			T::PalletId::get().into_account_truncating()
		}

		/// Burns `amount` from the satellite account then delegates to [`Config::SendToDap`].
		///
		/// On failure, any burned funds are restored via `mint_into` so the satellite balance
		/// remains unchanged and the next scheduled attempt can retry.
		/// The caller is responsible for updating `LastTransferBlock` irrespective of the outcome.
		fn do_send_to_central_dap(amount: BalanceOf<T>) -> Result<(), DoSendError> {
			let source = Self::satellite_account();

			T::Currency::burn_from(&source, amount, Preserve, Exact, Polite).map_err(|e| {
				log::error!(
					target: LOG_TARGET,
					"Failed to burn {:?} tokens from DAP satellite account: {:?}",
					amount,
					e,
				);
				DoSendError::BurnFailed
			})?;

			T::SendToDap::send(amount).map_err(|e| {
				log::warn!(
					target: LOG_TARGET,
					"DAP satellite XCM send of {:?} failed: {:?}",
					amount,
					e,
				);
				let _ = T::Currency::mint_into(&source, amount).inspect_err(|e| {
					frame_support::defensive!(
						"Failed to restore burned funds after send failure: {:?}",
						e
					);
				});
				DoSendError::SendFailed
			})?;

			Ok(())
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
					log::info!(
						target: LOG_TARGET,
						"🛰️ Created DAP satellite account: {satellite:?}"
					);
				},
				Err(e) => {
					frame_support::defensive!("Failed to mint ED into DAP satellite: {:?}", e);
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

/// Implements [`SendToDap`] for a runtime via XCM teleport to AssetHub.
///
/// Generates a `SendToDapError` enum and the `SendToDap<Balance>` impl for the given `$runtime`.
///
/// # Parameters
///
/// - `$runtime`: The runtime type (e.g. `Runtime`).
/// - `$asset_transactor`: Type implementing `xcm_executor::traits::TransactAsset`.
/// - `$xcm_router`: Type implementing `xcm::prelude::SendXcm`.
/// - `$dest`: Expression returning the [`xcm::prelude::Location`] of AssetHub.
/// - `$native_asset`: Expression returning the [`xcm::prelude::Location`] of the native token.
///
/// # Requirements:
///
/// The following must be in scope at the call site:
/// - `Balance`: the chain's native balance type.
/// - `DapBufferLocation`: a `parameter_types!`-generated type whose `get()` returns the
///   [`xcm::prelude::InteriorLocation`] of the DAP buffer account on AssetHub.
///
/// # Example:
///
/// ```ignore
/// pallet_dap_satellite::impl_send_to_dap_via_xcm!(
///     Runtime,
///     xcm_config::FungibleTransactor,
///     xcm_config::XcmRouter,
///     testnet_parachains_constants::westend::locations::AssetHubLocation::get(),
///     xcm_config::TokenRelayLocation::get(),
/// );
/// ```
#[macro_export]
macro_rules! impl_send_to_dap_via_xcm {
	($runtime:ty, $asset_transactor:ty, $xcm_router:ty, $dest:expr, $native_asset:expr $(,)?) => {
		/// Error variants for the XCM-based [`pallet_dap_satellite::SendToDap`] implementation.
		#[derive(Debug)]
		pub enum SendToDapError {
			/// The asset transactor rejected the outgoing check-out.
			AssetCheckOutFailed,
			/// Failed to reanchor assets for the destination chain.
			ReanchorFailed,
			/// The XCM router failed to dispatch the message.
			SendXcmFailed,
		}

		impl $crate::SendToDap<Balance> for $runtime {
			type Error = SendToDapError;

			fn send(amount: Balance) -> Result<(), SendToDapError> {
				use xcm::prelude::*;
				use xcm_executor::traits::TransactAsset;

				let dest = $dest;
				let asset = Asset { id: AssetId($native_asset), fun: Fungible(amount) };
				let check_context = XcmContext { origin: None, message_id: [0u8; 32], topic: None };

				<$asset_transactor>::can_check_out(&dest, &asset, &check_context)
					.map_err(|_| SendToDapError::AssetCheckOutFailed)?;

				let assets_for_dest = Assets::from(asset.clone())
					.reanchored(&dest, &Here.into())
					.map_err(|_| SendToDapError::ReanchorFailed)?;

				let beneficiary: Location = DapBufferLocation::get().into_location();
				let message = Xcm(vec![
					UnpaidExecution { weight_limit: Unlimited, check_origin: None },
					ReceiveTeleportedAsset(assets_for_dest),
					DepositAsset { assets: Wild(AllCounted(1)), beneficiary },
				]);

				send_xcm::<$xcm_router>(dest.clone(), message)
					.map_err(|_| SendToDapError::SendXcmFailed)?;

				<$asset_transactor>::check_out(&dest, &asset, &check_context);
				Ok(())
			}
		}
	};
}
