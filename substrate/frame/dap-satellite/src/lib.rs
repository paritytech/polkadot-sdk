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
//! Collects funds into a satellite buffer on non-AssetHub chains for eventual transfer to the
//! central DAP on AssetHub.
//!
//! Use on: Relay Chain, Coretime, People, BridgeHub. Do NOT use on AssetHub (use `pallet-dap`).
//!
//! ## Usage
//!
//! - **Fees**: Use [`DealWithFeesSplit`] to split fees between DAP satellite and other handlers
//! - **Slashes/revenue**: Use `DapSatellite` as `OnUnbalanced` handler
//! - **Burn redirection**: Use [`currency::SatelliteCurrency<T>`] as `type Currency` in pallets
//!
//! Note: Direct calls to `pallet_balances::Pallet::burn()` extrinsic bypass the wrapper.
//!
//! ## Setup
//!
//! The satellite account is created at genesis with ED. For existing chains, include
//! `dap_satellite::migrations::v1::InitSatelliteAccount` in migrations.
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

extern crate alloc;

use frame_support::{
	pallet_prelude::*,
	traits::{
		fungible::{Balanced, Credit, Inspect, Mutate, Unbalanced},
		tokens::{
			Fortitude, Fortitude::Polite, Precision, Precision::BestEffort, Precision::Exact,
			Preservation, Preservation::Preserve,
		},
		Imbalance, OnUnbalanced,
	},
	PalletId,
};
use sp_runtime::{Percent, Saturating};
use xcm::prelude::*;
use xcm_executor::traits::TransactAsset;

pub use pallet::*;

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
		///
		/// `Balance` must be convertible to `u128` for constructing the XCM asset amount.
		type Currency: Inspect<Self::AccountId, Balance: Into<u128>>
			+ Mutate<Self::AccountId>
			+ Unbalanced<Self::AccountId>
			+ Balanced<Self::AccountId>;

		/// The XCM sender used to dispatch the messages to AssetHub.
		type XcmSender: SendXcm;

		/// The pallet ID used to derive the satellite account.
		type PalletId: Get<PalletId>;

		/// The location of AssetHub as seen from this chain.
		type AssetHubLocation: Get<Location>;

		/// The location of the DAP buffer account on AssetHub, used as the XCM
		/// beneficiary. Typically derived from the DAP pallet's `PalletId`.
		type DapBufferLocation: Get<InteriorLocation>;

		/// Minimum number of blocks between successive XCM transfers to AssetHub.
		/// Acts as a rate limiter to avoid sending too many XCM messages.
		#[pallet::constant]
		type TransferPeriod: Get<BlockNumberFor<Self>>;

		/// Minimum transferable balance required to trigger a transfer.
		/// This avoids the transfer of very small / negligible amounts.
		/// The satellite account always retains its existential deposit on top of this.
		#[pallet::constant]
		type MinTransferAmount: Get<BalanceOf<Self>>;

		/// The local transactor for the native asset. Used for transfers: `can_check_out`
		/// checks whether sending is allowed, and `check_out` actually records the send.
		/// - For the RC: configure as the runtime's `LocalAssetTransactor`
		/// - For parachains: configure as the runtime's native-currency transactor (the
		///   `CheckingAccount` is `()` so `can_check_out` / `check_out` are no-ops).
		type AssetTransactor: TransactAsset;

		/// The location of the native asset as seen from the current chain.
		/// - RC: `Location::here()` — the relay native token originates here.
		/// - Parachains: `Location::parent()` — the native token is the relay chain's asset.
		/// This is used to construct the XCM asset identifier before it is included in the
		/// transfer message.
		#[pallet::constant]
		type NativeAsset: Get<Location>;
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
			// We need at least one read (of LastTransferBlock) to proceed.
			let single_read = T::DbWeight::get().reads(1);
			if !remaining_weight.all_gte(single_read) {
				return Weight::zero();
			}

			// Enforce the rate limit - don't send until `TransferPeriod` blocks have passed.
			let last = LastTransferBlock::<T>::get().unwrap_or_default();
			if block.saturating_sub(last) <= T::TransferPeriod::get() {
				return single_read;
			}

			// Check how much is available above the ED.
			let balance = T::Currency::balance(&Self::satellite_account());
			let ed = T::Currency::minimum_balance();
			let available_funds = balance.saturating_sub(ed);

			// Two reads so far: `LastTransferBlock` and `balance`.
			// Since ED is constant, it doesn't require a read.
			let two_reads = T::DbWeight::get().reads(2);

			if available_funds < T::MinTransferAmount::get() {
				return two_reads;
			}

			// We update the last transfer block irrespective of the transfer result. If we
			// don't and a failure occurs repeatedly, then a transfer will be attempted on
			// every block instead of the configured period, which would be undesirable.
			LastTransferBlock::<T>::put(block);

			// Attempt the XCM transfer to the central DAP buffer.
			match Self::do_send_to_central_dap(available_funds) {
				Ok(()) => {
					Self::deposit_event(Event::SendSucceeded { amount: available_funds });
				},
				Err(e) => {
					// A warning is sufficient since the transfer will be retried later.
					log::warn!(
						target: LOG_TARGET,
						"DAP satellite XCM transfer of {:?} failed at block {:?}: {:?}",
						available_funds,
						block,
						e,
					);
					Self::deposit_event(Event::SendFailed { amount: available_funds });
				},
			}

			// Two reads and one write (that of `LastTransferBlock`).
			T::DbWeight::get().reads_writes(2, 1)
		}
	}

	impl<T: Config> Pallet<T> {
		/// Get the satellite account derived from the pallet ID.
		///
		/// This account accumulates funds locally before they are sent to AssetHub.
		pub fn satellite_account() -> T::AccountId {
			T::PalletId::get().into_account_truncating()
		}

		/// Dispatch an XCM transfer from the satellite account to the central DAP buffer.
		///
		/// Returns `Ok(())` if the XCM message was successfully enqueued, `Err(...)` otherwise.
		/// The caller is responsible for updating `LastTransferBlock` irrespective of the outcome.
		/// On failure, any funds that were burned locally are restored via `mint_into` so the
		/// satellite balance remains unchanged and ready for the next attempt.
		///
		/// # Transfer flow:
		/// 1. `can_check_out` verifies the transfer is permitted
		/// 2. `burn_from` on the satellite account destroys the local tokens, reducing
		///    `total_issuance` on this chain (the source side of the transfer).
		/// 3. Reanchor the asset location to AssetHub's perspective (e.g. the RC's
		///    `Location::here()` becomes `Location::parent()` from the AssetHub's view).
		/// 4. Send `[UnpaidExecution, ReceiveTeleportedAsset, DepositAsset]` to AssetHub.
		/// 5. `check_out` updates the transfer counter (no-op on parachains).
		fn do_send_to_central_dap(amount: BalanceOf<T>) -> Result<(), SendError> {
			let dest = T::AssetHubLocation::get();
			let asset = Asset { id: AssetId(T::NativeAsset::get()), fun: Fungible(amount.into()) };
			let check_context = XcmContext { origin: None, message_id: [0u8; 32], topic: None };

			// Step 1: Verify the transfer is allowed.
			T::AssetTransactor::can_check_out(&dest, &asset, &check_context)
				.map_err(|_| SendError::Unroutable)?;

			// Step 2: The transfer is allowed, so burn the source funds.
			let source_account = Self::satellite_account();
			T::Currency::burn_from(&source_account, amount, Preserve, Exact, Polite).map_err(
				|e| {
					log::error!(
						target: LOG_TARGET,
						"Failed to burn {:?} tokens from DAP satellite account: {:?}",
						amount,
						e,
					);
					SendError::Transport("Failed to burn from DAP satellite account")
				},
			)?;

			// Step 3: Reanchor the asset to the destination's perspective.
			let assets_for_dest =
				Assets::from(asset.clone()).reanchored(&dest, &Here.into()).map_err(|_| {
					log::error!(target: LOG_TARGET, "Failed to reanchor asset for transfer");
					let _ = T::Currency::mint_into(&source_account, amount).inspect_err(|e| {
						frame_support::defensive!(
							"Failed to restore burned funds after reanchor failure: {:?}",
							e
						);
					});
					SendError::Unroutable
				})?;

			// Step 4: Build and send the XCM message.
			let beneficiary: Location = T::DapBufferLocation::get().into_location();
			let message = Xcm(vec![
				UnpaidExecution { weight_limit: Unlimited, check_origin: None },
				ReceiveTeleportedAsset(assets_for_dest),
				DepositAsset { assets: Wild(AllCounted(1)), beneficiary },
			]);

			send_xcm::<T::XcmSender>(dest.clone(), message).map_err(|e| {
				log::error!(
					target: LOG_TARGET,
					"Failed to send {:?} tokens to AssetHub: {:?}",
					amount,
					e,
				);
				let _ = T::Currency::mint_into(&source_account, amount).inspect_err(|e| {
					frame_support::defensive!(
						"Failed to restore burned funds after XCM send failure: {:?}",
						e
					);
				});
				e
			})?;

			// Step 5: Finalise transfer tracking (no-op on parachains).
			T::AssetTransactor::check_out(&dest, &asset, &check_context);
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

			if frame_system::Pallet::<T>::providers(&satellite) > 0
				&& T::Currency::balance(&satellite) >= ed
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
	}
}

/// Fungible currency adapter module.
pub mod currency {
	use super::*;
	use frame_support::traits::{
		fungible::Dust,
		tokens::{DepositConsequence, Provenance, WithdrawConsequence},
	};
	use sp_runtime::TokenError;

	/// Fungible currency wrapper that redirects burns to the DAP satellite account.
	///
	/// Use this as `type NativeBalance =
	/// pallet_dap_satellite::currency::SatelliteCurrency<Runtime>` in runtimes that want to
	/// redirect burns to the satellite instead of reducing total issuance.
	///
	/// All fungible trait methods delegate to the inner currency except `burn_from`,
	/// which transfers funds to the satellite account.
	pub struct SatelliteCurrency<T>(core::marker::PhantomData<T>);

	impl<T: Config> Inspect<T::AccountId> for SatelliteCurrency<T> {
		type Balance = BalanceOf<T>;

		fn total_issuance() -> Self::Balance {
			T::Currency::total_issuance()
		}
		fn active_issuance() -> Self::Balance {
			T::Currency::active_issuance()
		}
		fn minimum_balance() -> Self::Balance {
			T::Currency::minimum_balance()
		}
		fn total_balance(who: &T::AccountId) -> Self::Balance {
			T::Currency::total_balance(who)
		}
		fn balance(who: &T::AccountId) -> Self::Balance {
			T::Currency::balance(who)
		}
		fn reducible_balance(
			who: &T::AccountId,
			preservation: Preservation,
			force: Fortitude,
		) -> Self::Balance {
			T::Currency::reducible_balance(who, preservation, force)
		}
		fn can_deposit(
			who: &T::AccountId,
			amount: Self::Balance,
			provenance: Provenance,
		) -> DepositConsequence {
			T::Currency::can_deposit(who, amount, provenance)
		}
		fn can_withdraw(
			who: &T::AccountId,
			amount: Self::Balance,
		) -> WithdrawConsequence<Self::Balance> {
			T::Currency::can_withdraw(who, amount)
		}
	}

	impl<T: Config> Unbalanced<T::AccountId> for SatelliteCurrency<T> {
		fn handle_dust(dust: Dust<T::AccountId, Self>) {
			T::Currency::handle_dust(Dust(dust.0));
		}
		fn write_balance(
			who: &T::AccountId,
			amount: Self::Balance,
		) -> Result<Option<Self::Balance>, DispatchError> {
			T::Currency::write_balance(who, amount)
		}
		fn set_total_issuance(amount: Self::Balance) {
			T::Currency::set_total_issuance(amount)
		}
		fn decrease_balance(
			who: &T::AccountId,
			amount: Self::Balance,
			precision: Precision,
			preservation: Preservation,
			force: Fortitude,
		) -> Result<Self::Balance, DispatchError> {
			T::Currency::decrease_balance(who, amount, precision, preservation, force)
		}
		fn increase_balance(
			who: &T::AccountId,
			amount: Self::Balance,
			precision: Precision,
		) -> Result<Self::Balance, DispatchError> {
			T::Currency::increase_balance(who, amount, precision)
		}
		fn deactivate(amount: Self::Balance) {
			T::Currency::deactivate(amount)
		}
		fn reactivate(amount: Self::Balance) {
			T::Currency::reactivate(amount)
		}
	}

	impl<T: Config> Mutate<T::AccountId> for SatelliteCurrency<T> {
		fn burn_from(
			who: &T::AccountId,
			amount: Self::Balance,
			preservation: Preservation,
			precision: Precision,
			force: Fortitude,
		) -> Result<Self::Balance, DispatchError> {
			let actual = T::Currency::reducible_balance(who, preservation, force).min(amount);
			frame_support::ensure!(
				actual == amount || precision == BestEffort,
				TokenError::FundsUnavailable
			);
			let actual =
				T::Currency::decrease_balance(who, actual, BestEffort, preservation, force)?;

			// Credit the satellite account instead of reducing total issuance.
			let satellite = Pallet::<T>::satellite_account();
			let _ =
				T::Currency::increase_balance(&satellite, actual, BestEffort).inspect_err(|e| {
					// Try to restore balance to source account.
					let _ = T::Currency::increase_balance(who, actual, BestEffort);
					frame_support::defensive!("Failed to credit DAP satellite: {:?}", e);
				});

			Ok(actual)
		}
	}

	impl<T: Config> Balanced<T::AccountId> for SatelliteCurrency<T> {
		type OnDropCredit = <T::Currency as Balanced<T::AccountId>>::OnDropCredit;
		type OnDropDebt = <T::Currency as Balanced<T::AccountId>>::OnDropDebt;
	}
}
