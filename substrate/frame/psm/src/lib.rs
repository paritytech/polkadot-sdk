// This file is part of Substrate.

// Copyright (C) Amforc AG.
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

//! # Peg Stability Module (PSM) Pallet
//!
//! A module enabling 1:1 swaps between pUSD and pre-approved external stablecoins.
//!
//! ## Pallet API
//!
//! See the [`pallet`] module for more information about the interfaces this pallet exposes,
//! including its configuration trait, dispatchables, storage items, events and errors.
//!
//! ## Overview
//!
//! The PSM strengthens the pUSD peg by providing arbitrage opportunities:
//! - When pUSD trades **above** $1: Users swap external stablecoins for pUSD and sell for profit
//! - When pUSD trades **below** $1: Users buy cheap pUSD and swap for external stablecoins
//!
//! This creates a price corridor bounded by the minting and redemption fees.
//!
//! ### Key Concepts
//!
//! * **Minting**: Deposit external stablecoin → receive pUSD (minus fee)
//! * **Redemption**: Burn pUSD → receive external stablecoin (minus fee)
//! * **Reserve**: External stablecoin balance held by the PSM account (derived, not stored)
//! * **PSM Debt**: Total pUSD minted through PSM, backed 1:1 by external stablecoins
//! * **Circuit Breaker**: Emergency control to disable minting or all swaps
//!
//! ### Supported Assets
//!
//! The PSM supports multiple pre-approved external stablecoins (e.g., USDC, USDT).
//! Each swap operation specifies which asset to use via the `asset_id` parameter.
//!
//! ### Fee Structure
//!
//! * **Minting Fee (`MintingFee`)**: Deducted from pUSD output during minting
//! * **Redemption Fee (`RedemptionFee`)**: Deducted from external stablecoin output during
//!   redemption
//!
//! Fees are collected in pUSD and transferred to [`Config::FeeDestination`].
//!
//! ### Example
//!
//! ```ignore
//! // Mint pUSD by depositing USDC
//! Psm::mint(RuntimeOrigin::signed(user), USDC_ASSET_ID, 1000 * UNIT)?;
//!
//! // Redeem USDC by burning pUSD
//! Psm::redeem(RuntimeOrigin::signed(user), USDC_ASSET_ID, 1000 * UNIT)?;
//! ```

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod migrations;
pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub use pallet::*;
pub use weights::WeightInfo;

/// Helper trait for benchmark setup.
///
/// Provides a way to create an external asset with the correct metadata (decimals)
/// for benchmarks, abstracting over the deposit requirements of the underlying
/// asset pallet.
#[cfg(feature = "runtime-benchmarks")]
pub trait BenchmarkHelper<AssetId, AccountId> {
	/// Create an asset with metadata matching the stable asset's decimals.
	fn create_asset(asset_id: AssetId, owner: &AccountId, decimals: u8);
}

#[frame_support::pallet]
pub mod pallet {
	pub use frame_support::traits::tokens::stable::PsmInterface;

	use alloc::collections::btree_map::BTreeMap;
	use codec::DecodeWithMemTracking;
	use frame_support::{
		pallet_prelude::*,
		traits::{
			fungible::{
				metadata::Inspect as FungibleMetadataInspect, Inspect as FungibleInspect,
				Mutate as FungibleMutate,
			},
			fungibles::{
				metadata::Inspect as FungiblesMetadataInspect, Inspect as FungiblesInspect,
				Mutate as FungiblesMutate,
			},
			tokens::{Fortitude, Precision, Preservation},
		},
		DefaultNoBound, PalletId,
	};
	use frame_system::pallet_prelude::*;
	use sp_runtime::{
		traits::{AccountIdConversion, Saturating, Zero},
		Perbill, Permill,
	};

	use crate::WeightInfo;

	/// Circuit breaker levels for emergency control.
	#[derive(
		Encode,
		Decode,
		DecodeWithMemTracking,
		MaxEncodedLen,
		TypeInfo,
		Clone,
		Copy,
		PartialEq,
		Eq,
		Debug,
		Default,
	)]
	pub enum CircuitBreakerLevel {
		/// Normal operation, all swaps enabled.
		#[default]
		AllEnabled,
		/// Minting disabled, redemptions still allowed.
		MintingDisabled,
		/// All swaps disabled.
		AllDisabled,
	}

	impl CircuitBreakerLevel {
		/// Whether this level allows minting (external → pUSD).
		pub const fn allows_minting(&self) -> bool {
			matches!(self, CircuitBreakerLevel::AllEnabled)
		}

		/// Whether this level allows redemption (pUSD → external).
		pub const fn allows_redemption(&self) -> bool {
			!matches!(self, CircuitBreakerLevel::AllDisabled)
		}
	}

	/// Privilege level returned by ManagerOrigin.
	///
	/// Enables tiered authorization where different origins have different
	/// capabilities for managing PSM parameters.
	#[derive(
		Encode,
		Decode,
		DecodeWithMemTracking,
		MaxEncodedLen,
		TypeInfo,
		Clone,
		Copy,
		PartialEq,
		Eq,
		Debug,
		Default,
	)]
	pub enum PsmManagerLevel {
		/// Full administrative access via GeneralAdmin origin.
		/// Can modify all parameters including fees, ceilings, and asset management.
		#[default]
		Full,
		/// Emergency access via EmergencyAction origin.
		/// Can modify circuit breaker status and asset ceiling weights.
		Emergency,
	}

	impl PsmManagerLevel {
		/// Whether this level allows modifying minting/redemption fees.
		pub const fn can_set_fees(&self) -> bool {
			matches!(self, PsmManagerLevel::Full)
		}

		/// Whether this level allows modifying the circuit breaker status.
		/// Both Full and Emergency levels can set circuit breaker.
		pub const fn can_set_circuit_breaker(&self) -> bool {
			true
		}

		/// Whether this level allows modifying the global PSM debt ratio.
		pub const fn can_set_max_psm_debt(&self) -> bool {
			matches!(self, PsmManagerLevel::Full)
		}

		/// Whether this level allows modifying per-asset ceiling weights.
		/// Both Full and Emergency levels can set asset ceilings.
		pub const fn can_set_asset_ceiling(&self) -> bool {
			true
		}

		/// Whether this level allows adding or removing external assets.
		pub const fn can_manage_assets(&self) -> bool {
			matches!(self, PsmManagerLevel::Full)
		}
	}

	pub(crate) type BalanceOf<T> = <<T as Config>::Fungibles as FungiblesInspect<
		<T as frame_system::Config>::AccountId,
	>>::Balance;

	/// Suggested fee of 0.5% for minting and redemption.
	pub(crate) struct DefaultFee;
	impl Get<Permill> for DefaultFee {
		fn get() -> Permill {
			Permill::from_parts(5_000)
		}
	}

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Fungibles implementation for both pUSD and external stablecoins.
		type Fungibles: FungiblesMutate<Self::AccountId, AssetId = Self::AssetId>
			+ FungiblesMetadataInspect<Self::AccountId>;

		/// Asset identifier type.
		type AssetId: Parameter + Member + Copy + MaybeSerializeDeserialize + MaxEncodedLen + Ord;

		/// Maximum allowed pUSD issuance across the entire system.
		type MaximumIssuance: Get<BalanceOf<Self>>;

		/// Origin allowed to update PSM parameters.
		///
		/// Returns `PsmManagerLevel` to distinguish privilege levels:
		/// - `Full` (via GeneralAdmin): Can modify all parameters
		/// - `Emergency` (via EmergencyAction): Can only modify circuit breaker status
		type ManagerOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = PsmManagerLevel>;

		/// A type representing the weights required by the dispatchables of this pallet.
		type WeightInfo: WeightInfo;

		/// The pUSD asset as a single-asset `fungible` type.
		///
		/// Typically `ItemOf<Asset, StablecoinAssetId, AccountId>`.
		/// Must use the same `Balance` type as `Asset`.
		type StableAsset: FungibleMutate<Self::AccountId, Balance = BalanceOf<Self>>
			+ FungibleMetadataInspect<Self::AccountId>;

		/// Account that receives pUSD fees from minting and redemption.
		///
		/// Must exist before any swap; initialized at genesis and migration
		/// via `Pallet::ensure_account_exists`.
		type FeeDestination: Get<Self::AccountId>;

		/// PalletId for deriving the PSM account.
		#[pallet::constant]
		type PalletId: Get<PalletId>;

		/// Minimum swap amount.
		#[pallet::constant]
		type MinSwapAmount: Get<BalanceOf<Self>>;

		/// Maximum number of approved external assets.
		#[pallet::constant]
		type MaxExternalAssets: Get<u32>;

		/// Helper for benchmarks to create an external asset with correct metadata.
		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper: crate::BenchmarkHelper<Self::AssetId, Self::AccountId>;
	}

	/// The in-code storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn integrity_test() {
			assert!(!T::MinSwapAmount::get().is_zero(), "MinSwapAmount must be greater than zero");
		}

		#[cfg(feature = "try-runtime")]
		fn try_state(_n: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
			Self::do_try_state()
		}
	}

	/// pUSD minted through PSM per external asset.
	#[pallet::storage]
	pub type PsmDebt<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AssetId, BalanceOf<T>, ValueQuery>;

	/// Fee for external → pUSD swaps (minting) per asset. Suggested value is 0.5%.
	#[pallet::storage]
	pub(crate) type MintingFee<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AssetId, Permill, ValueQuery, DefaultFee>;

	/// Fee for pUSD → external swaps (redemption) per asset. Suggested value is 0.5%.
	#[pallet::storage]
	pub(crate) type RedemptionFee<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AssetId, Permill, ValueQuery, DefaultFee>;

	/// Max PSM debt as percentage of MaximumIssuance (global ceiling).
	#[pallet::storage]
	pub(crate) type MaxPsmDebtOfTotal<T: Config> = StorageValue<_, Permill, ValueQuery>;

	/// Per-asset ceiling weight. Weights are normalized against the sum of all weights.
	/// Zero means minting is disabled for this asset.
	#[pallet::storage]
	pub(crate) type AssetCeilingWeight<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AssetId, Permill, ValueQuery>;

	/// Set of approved external stablecoin asset IDs with their operational status.
	/// Key existence indicates the asset is approved; the value is the circuit breaker level.
	#[pallet::storage]
	pub(crate) type ExternalAssets<T: Config> =
		CountedStorageMap<_, Blake2_128Concat, T::AssetId, CircuitBreakerLevel, OptionQuery>;

	/// Genesis configuration for the PSM pallet.
	#[pallet::genesis_config]
	#[derive(DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		/// Max PSM debt as percentage of total maximum issuance.
		pub max_psm_debt_of_total: Permill,
		/// Per-asset configuration: asset_id -> (minting_fee, redemption_fee,
		/// ceiling_weight). Keys also define the set of approved external assets.
		pub asset_configs: BTreeMap<T::AssetId, (Permill, Permill, Permill)>,
		#[serde(skip)]
		pub _marker: core::marker::PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			assert!(
				self.asset_configs.len() as u32 <= T::MaxExternalAssets::get(),
				"PSM genesis: asset_configs ({}) exceeds MaxExternalAssets ({})",
				self.asset_configs.len(),
				T::MaxExternalAssets::get(),
			);
			MaxPsmDebtOfTotal::<T>::put(self.max_psm_debt_of_total);
			let stable_decimals = T::StableAsset::decimals();
			for (asset_id, (minting_fee, redemption_fee, ceiling_weight)) in &self.asset_configs {
				assert!(
					T::Fungibles::decimals(*asset_id) == stable_decimals,
					"PSM genesis: asset {:?} decimals do not match stable asset decimals",
					asset_id,
				);
				ExternalAssets::<T>::insert(asset_id, CircuitBreakerLevel::AllEnabled);
				MintingFee::<T>::insert(asset_id, minting_fee);
				RedemptionFee::<T>::insert(asset_id, redemption_fee);
				AssetCeilingWeight::<T>::insert(asset_id, ceiling_weight);
			}
			Pallet::<T>::ensure_account_exists(&Pallet::<T>::account_id());
			Pallet::<T>::ensure_account_exists(&T::FeeDestination::get());
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// User swapped external stablecoin for pUSD.
		Minted {
			who: T::AccountId,
			asset_id: T::AssetId,
			external_amount: BalanceOf<T>,
			pusd_received: BalanceOf<T>,
			fee: BalanceOf<T>,
		},
		/// User swapped pUSD for external stablecoin.
		Redeemed {
			who: T::AccountId,
			asset_id: T::AssetId,
			pusd_paid: BalanceOf<T>,
			external_received: BalanceOf<T>,
			fee: BalanceOf<T>,
		},
		/// Minting fee updated for an asset by governance.
		MintingFeeUpdated { asset_id: T::AssetId, old_value: Permill, new_value: Permill },
		/// Redemption fee updated for an asset by governance.
		RedemptionFeeUpdated { asset_id: T::AssetId, old_value: Permill, new_value: Permill },
		/// Max PSM debt ratio updated by governance.
		MaxPsmDebtOfTotalUpdated { old_value: Permill, new_value: Permill },
		/// Per-asset debt ceiling weight updated by governance.
		AssetCeilingWeightUpdated { asset_id: T::AssetId, old_value: Permill, new_value: Permill },
		/// Per-asset circuit breaker status updated.
		AssetStatusUpdated { asset_id: T::AssetId, status: CircuitBreakerLevel },
		/// An external asset was added to the approved list.
		ExternalAssetAdded { asset_id: T::AssetId },
		/// An external asset was removed from the approved list.
		ExternalAssetRemoved { asset_id: T::AssetId },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// PSM doesn't have enough external stablecoin for redemption.
		InsufficientReserve,
		/// Swap would exceed PSM debt ceiling.
		ExceedsMaxPsmDebt,
		/// Swap amount below minimum threshold.
		BelowMinimumSwap,
		/// Minting operations are disabled (circuit breaker level >= 1).
		MintingStopped,
		/// All swap operations are disabled (circuit breaker level = 2).
		AllSwapsStopped,
		/// Asset is not an approved external stablecoin.
		UnsupportedAsset,
		/// Mint would exceed system-wide maximum pUSD issuance.
		ExceedsMaxIssuance,
		/// Asset is already in the approved list.
		AssetAlreadyApproved,
		/// Cannot remove asset: not in approved list.
		AssetNotApproved,
		/// Cannot remove asset: has non-zero PSM debt.
		AssetHasDebt,
		/// Operation requires Full manager level (GeneralAdmin), not Emergency.
		InsufficientPrivilege,
		/// Maximum number of approved external assets reached.
		TooManyAssets,
		/// External asset decimals do not match the stable asset decimals.
		DecimalsMismatch,
		/// An unexpected invariant violation occurred. This should be reported.
		Unexpected,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Swap external stablecoin for pUSD.
		///
		/// ## Dispatch Origin
		///
		/// Must be `Signed` by the user performing the swap.
		///
		/// ## Details
		///
		/// Transfers `external_amount` of the specified external stablecoin from the caller
		/// to the PSM account, then mints pUSD to the caller minus the minting fee.
		/// The fee is calculated using ceiling rounding (`mul_ceil`), ensuring the
		/// protocol never undercharges. The fee is transferred to [`Config::FeeDestination`].
		///
		/// ## Parameters
		///
		/// - `asset_id`: The external stablecoin to deposit (must be in `ExternalAssets`)
		/// - `external_amount`: Amount of external stablecoin to deposit
		///
		/// ## Errors
		///
		/// - [`Error::UnsupportedAsset`]: If `asset_id` is not an approved external stablecoin
		/// - [`Error::MintingStopped`]: If circuit breaker is at `MintingDisabled` or higher
		/// - [`Error::BelowMinimumSwap`]: If `external_amount` is below [`Config::MinSwapAmount`]
		/// - [`Error::ExceedsMaxIssuance`]: If minting would exceed system-wide pUSD issuance cap
		/// - [`Error::ExceedsMaxPsmDebt`]: If minting would exceed PSM debt ceiling (aggregate or
		///   per-asset)
		///
		/// ## Events
		///
		/// - [`Event::Minted`]: Emitted on successful mint
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::mint(T::MaxExternalAssets::get()))]
		pub fn mint(
			origin: OriginFor<T>,
			asset_id: T::AssetId,
			external_amount: BalanceOf<T>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			// Check asset is approved and minting is enabled
			let asset_status =
				ExternalAssets::<T>::get(asset_id).ok_or(Error::<T>::UnsupportedAsset)?;
			ensure!(asset_status.allows_minting(), Error::<T>::MintingStopped);

			ensure!(external_amount >= T::MinSwapAmount::get(), Error::<T>::BelowMinimumSwap);

			let fee = MintingFee::<T>::get(asset_id).mul_ceil(external_amount);
			let pusd_to_user = external_amount.saturating_sub(fee);

			// Total new issuance = pusd_to_user + fee = external_amount.
			let current_total_issuance = T::StableAsset::total_issuance();
			let max_issuance = T::MaximumIssuance::get();
			ensure!(
				current_total_issuance.saturating_add(external_amount) <= max_issuance,
				Error::<T>::ExceedsMaxIssuance
			);

			// Check aggregate PSM ceiling across all assets
			let current_total_psm_debt = Self::total_psm_debt();
			let max_psm = Self::max_psm_debt();
			ensure!(
				current_total_psm_debt.saturating_add(external_amount) <= max_psm,
				Error::<T>::ExceedsMaxPsmDebt
			);

			// Check per-asset ceiling (redistributes from disabled assets)
			let current_debt = PsmDebt::<T>::get(asset_id);
			let max_debt = Self::max_asset_debt(asset_id);
			let new_debt = current_debt.saturating_add(external_amount);
			ensure!(new_debt <= max_debt, Error::<T>::ExceedsMaxPsmDebt);

			let psm_account = Self::account_id();

			T::Fungibles::transfer(
				asset_id,
				&who,
				&psm_account,
				external_amount,
				Preservation::Expendable,
			)?;
			T::StableAsset::mint_into(&who, pusd_to_user)?;
			if !fee.is_zero() {
				T::StableAsset::mint_into(&T::FeeDestination::get(), fee)?;
			}

			PsmDebt::<T>::insert(asset_id, new_debt);

			Self::deposit_event(Event::Minted {
				who,
				asset_id,
				external_amount,
				pusd_received: pusd_to_user,
				fee,
			});

			Ok(())
		}

		/// Swap pUSD for external stablecoin.
		///
		/// ## Dispatch Origin
		///
		/// Must be `Signed` by the user performing the swap.
		///
		/// ## Details
		///
		/// Burns `pusd_amount` pUSD from the caller minus fee (transferred to
		/// [`Config::FeeDestination`]), then transfers the resulting amount in external
		/// stablecoin from PSM to the caller. The fee is calculated using ceiling rounding
		/// (`mul_ceil`), ensuring the protocol never undercharges.
		///
		/// ## Parameters
		///
		/// - `asset_id`: The external stablecoin to receive (must be in `ExternalAssets`)
		/// - `pusd_amount`: Amount of pUSD to redeem
		///
		/// ## Errors
		///
		/// - [`Error::UnsupportedAsset`]: If `asset_id` is not an approved external stablecoin
		/// - [`Error::AllSwapsStopped`]: If circuit breaker is at `AllDisabled`
		/// - [`Error::BelowMinimumSwap`]: If `pusd_amount` is below [`Config::MinSwapAmount`]
		/// - [`Error::InsufficientReserve`]: If PSM has insufficient external stablecoin
		///
		/// ## Events
		///
		/// - [`Event::Redeemed`]: Emitted on successful redemption
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::redeem())]
		pub fn redeem(
			origin: OriginFor<T>,
			asset_id: T::AssetId,
			pusd_amount: BalanceOf<T>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			// Check asset is approved and redemption is enabled
			let asset_status =
				ExternalAssets::<T>::get(asset_id).ok_or(Error::<T>::UnsupportedAsset)?;
			ensure!(asset_status.allows_redemption(), Error::<T>::AllSwapsStopped);

			ensure!(pusd_amount >= T::MinSwapAmount::get(), Error::<T>::BelowMinimumSwap);

			let fee = RedemptionFee::<T>::get(asset_id).mul_ceil(pusd_amount);
			let external_to_user = pusd_amount.saturating_sub(fee);

			// Check debt first - redemptions are limited by tracked debt, not raw reserve.
			// This prevents redemption of "donated" reserves that aren't backed by debt.
			let current_debt = PsmDebt::<T>::get(asset_id);
			ensure!(current_debt >= external_to_user, Error::<T>::InsufficientReserve);

			let reserve = Self::get_reserve(asset_id);
			if reserve < external_to_user {
				defensive!("PSM reserve is less than expected output amount");
				return Err(Error::<T>::Unexpected.into());
			}

			// Burn the redeemed portion, then transfer fee to destination.
			T::StableAsset::burn_from(
				&who,
				external_to_user,
				Preservation::Expendable,
				Precision::Exact,
				Fortitude::Polite,
			)?;
			if !fee.is_zero() {
				T::StableAsset::transfer(
					&who,
					&T::FeeDestination::get(),
					fee,
					Preservation::Expendable,
				)?;
			}

			let psm_account = Self::account_id();
			T::Fungibles::transfer(
				asset_id,
				&psm_account,
				&who,
				external_to_user,
				Preservation::Expendable,
			)?;

			PsmDebt::<T>::mutate(asset_id, |debt| {
				*debt = debt.saturating_sub(external_to_user);
			});

			Self::deposit_event(Event::Redeemed {
				who,
				asset_id,
				pusd_paid: pusd_amount,
				external_received: external_to_user,
				fee,
			});

			Ok(())
		}

		/// Set the minting fee for a specific asset (external → pUSD).
		///
		/// ## Dispatch Origin
		///
		/// Must be [`Config::ManagerOrigin`].
		///
		/// ## Parameters
		///
		/// - `asset_id`: The external stablecoin to configure
		/// - `fee`: The new minting fee as a Permill
		///
		/// ## Events
		///
		/// - [`Event::MintingFeeUpdated`]: Emitted with old and new values
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::set_minting_fee())]
		pub fn set_minting_fee(
			origin: OriginFor<T>,
			asset_id: T::AssetId,
			fee: Permill,
		) -> DispatchResult {
			let level = T::ManagerOrigin::ensure_origin(origin)?;
			ensure!(level.can_set_fees(), Error::<T>::InsufficientPrivilege);
			ensure!(ExternalAssets::<T>::contains_key(asset_id), Error::<T>::AssetNotApproved);
			let old_value = MintingFee::<T>::get(asset_id);
			MintingFee::<T>::insert(asset_id, fee);
			Self::deposit_event(Event::MintingFeeUpdated { asset_id, old_value, new_value: fee });
			Ok(())
		}

		/// Set the redemption fee for a specific asset (pUSD → external).
		///
		/// ## Dispatch Origin
		///
		/// Must be [`Config::ManagerOrigin`].
		///
		/// ## Parameters
		///
		/// - `asset_id`: The external stablecoin to configure
		/// - `fee`: The new redemption fee as a Permill
		///
		/// ## Events
		///
		/// - [`Event::RedemptionFeeUpdated`]: Emitted with old and new values
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::set_redemption_fee())]
		pub fn set_redemption_fee(
			origin: OriginFor<T>,
			asset_id: T::AssetId,
			fee: Permill,
		) -> DispatchResult {
			let level = T::ManagerOrigin::ensure_origin(origin)?;
			ensure!(level.can_set_fees(), Error::<T>::InsufficientPrivilege);
			ensure!(ExternalAssets::<T>::contains_key(asset_id), Error::<T>::AssetNotApproved);
			let old_value = RedemptionFee::<T>::get(asset_id);
			RedemptionFee::<T>::insert(asset_id, fee);
			Self::deposit_event(Event::RedemptionFeeUpdated {
				asset_id,
				old_value,
				new_value: fee,
			});
			Ok(())
		}

		/// Set the maximum PSM debt as a percentage of total maximum issuance.
		///
		/// ## Dispatch Origin
		///
		/// Must be [`Config::ManagerOrigin`].
		///
		/// ## Events
		///
		/// - [`Event::MaxPsmDebtOfTotalUpdated`]: Emitted with old and new values
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::set_max_psm_debt())]
		pub fn set_max_psm_debt(origin: OriginFor<T>, ratio: Permill) -> DispatchResult {
			let level = T::ManagerOrigin::ensure_origin(origin)?;
			ensure!(level.can_set_max_psm_debt(), Error::<T>::InsufficientPrivilege);
			let old_value = MaxPsmDebtOfTotal::<T>::get();
			MaxPsmDebtOfTotal::<T>::put(ratio);
			Self::deposit_event(Event::MaxPsmDebtOfTotalUpdated { old_value, new_value: ratio });
			Ok(())
		}

		/// Set the circuit breaker status for a specific external asset.
		///
		/// ## Dispatch Origin
		///
		/// Must be [`Config::ManagerOrigin`].
		///
		/// ## Details
		///
		/// Controls which operations are allowed for this asset:
		/// - [`CircuitBreakerLevel::AllEnabled`]: All swaps allowed
		/// - [`CircuitBreakerLevel::MintingDisabled`]: Only redemptions allowed (useful for
		///   draining debt)
		/// - [`CircuitBreakerLevel::AllDisabled`]: No swaps allowed
		///
		/// ## Parameters
		///
		/// - `asset_id`: The external stablecoin to configure
		/// - `status`: The new circuit breaker level for this asset
		///
		/// ## Errors
		///
		/// - [`Error::AssetNotApproved`]: If the asset is not in the approved list
		///
		/// ## Events
		///
		/// - [`Event::AssetStatusUpdated`]: Emitted with the asset ID and new status
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::set_asset_status())]
		pub fn set_asset_status(
			origin: OriginFor<T>,
			asset_id: T::AssetId,
			status: CircuitBreakerLevel,
		) -> DispatchResult {
			T::ManagerOrigin::ensure_origin(origin)?;
			ensure!(ExternalAssets::<T>::contains_key(asset_id), Error::<T>::AssetNotApproved);
			ExternalAssets::<T>::insert(asset_id, status);
			Self::deposit_event(Event::AssetStatusUpdated { asset_id, status });
			Ok(())
		}

		/// Set the per-asset debt ceiling weight.
		///
		/// ## Dispatch Origin
		///
		/// Must be [`Config::ManagerOrigin`].
		///
		/// ## Details
		///
		/// Ratios act as weights normalized against the sum of all asset weights:
		/// `max_asset_debt = (ratio / sum_of_all_ratios) * MaxPsmDebtOfTotal * MaximumIssuance`
		///
		/// With a single asset, the weight always normalizes to 100% of the PSM
		/// ceiling.
		///
		/// ## Parameters
		///
		/// - `asset_id`: The external stablecoin to configure
		/// - `ratio`: Weight for this asset's share of the total PSM ceiling
		///
		/// ## Events
		///
		/// - [`Event::AssetCeilingWeightUpdated`]: Emitted with old and new values
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::set_asset_ceiling_weight())]
		pub fn set_asset_ceiling_weight(
			origin: OriginFor<T>,
			asset_id: T::AssetId,
			weight: Permill,
		) -> DispatchResult {
			let level = T::ManagerOrigin::ensure_origin(origin)?;
			ensure!(level.can_set_asset_ceiling(), Error::<T>::InsufficientPrivilege);
			ensure!(ExternalAssets::<T>::contains_key(asset_id), Error::<T>::AssetNotApproved);
			let old_value = AssetCeilingWeight::<T>::get(asset_id);
			AssetCeilingWeight::<T>::insert(asset_id, weight);
			Self::deposit_event(Event::AssetCeilingWeightUpdated {
				asset_id,
				old_value,
				new_value: weight,
			});
			Ok(())
		}

		/// Add an external stablecoin to the approved list.
		///
		/// ## Dispatch Origin
		///
		/// Must be [`Config::ManagerOrigin`].
		///
		/// ## Parameters
		///
		/// - `asset_id`: The external stablecoin to add
		///
		/// ## Errors
		///
		/// - [`Error::AssetAlreadyApproved`]: If the asset is already in the approved list
		///
		/// ## Events
		///
		/// - [`Event::ExternalAssetAdded`]: Emitted on successful addition
		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::add_external_asset())]
		pub fn add_external_asset(origin: OriginFor<T>, asset_id: T::AssetId) -> DispatchResult {
			let level = T::ManagerOrigin::ensure_origin(origin)?;
			ensure!(level.can_manage_assets(), Error::<T>::InsufficientPrivilege);
			ensure!(!ExternalAssets::<T>::contains_key(asset_id), Error::<T>::AssetAlreadyApproved);
			let count = ExternalAssets::<T>::count();
			ensure!(count < T::MaxExternalAssets::get(), Error::<T>::TooManyAssets);
			ensure!(
				T::Fungibles::decimals(asset_id) == T::StableAsset::decimals(),
				Error::<T>::DecimalsMismatch
			);
			ExternalAssets::<T>::insert(asset_id, CircuitBreakerLevel::AllEnabled);
			Self::deposit_event(Event::ExternalAssetAdded { asset_id });
			Ok(())
		}

		/// Remove an external stablecoin from the approved list.
		///
		/// ## Dispatch Origin
		///
		/// Must be [`Config::ManagerOrigin`].
		///
		/// ## Details
		///
		/// The asset cannot be removed if it has non-zero PSM debt outstanding.
		/// This prevents orphaned debt that cannot be redeemed.
		///
		/// Upon removal, the associated configuration is also cleaned up:
		/// - `MintingFee` for this asset
		/// - `RedemptionFee` for this asset
		/// - `AssetCeilingWeight` for this asset
		///
		/// ## Parameters
		///
		/// - `asset_id`: The external stablecoin to remove
		///
		/// ## Errors
		///
		/// - [`Error::AssetNotApproved`]: If the asset is not in the approved list
		/// - [`Error::AssetHasDebt`]: If the asset has non-zero PSM debt
		///
		/// ## Events
		///
		/// - [`Event::ExternalAssetRemoved`]: Emitted on successful removal
		#[pallet::call_index(8)]
		#[pallet::weight(T::WeightInfo::remove_external_asset())]
		pub fn remove_external_asset(origin: OriginFor<T>, asset_id: T::AssetId) -> DispatchResult {
			let level = T::ManagerOrigin::ensure_origin(origin)?;
			ensure!(level.can_manage_assets(), Error::<T>::InsufficientPrivilege);
			ensure!(ExternalAssets::<T>::contains_key(asset_id), Error::<T>::AssetNotApproved);
			ensure!(PsmDebt::<T>::get(asset_id).is_zero(), Error::<T>::AssetHasDebt);
			ExternalAssets::<T>::remove(asset_id);

			// Clean up associated configuration
			MintingFee::<T>::remove(asset_id);
			RedemptionFee::<T>::remove(asset_id);
			AssetCeilingWeight::<T>::remove(asset_id);
			PsmDebt::<T>::remove(asset_id);
			Self::deposit_event(Event::ExternalAssetRemoved { asset_id });
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Get the PSM's derived account.
		pub(crate) fn account_id() -> T::AccountId {
			T::PalletId::get().into_account_truncating()
		}

		/// Calculate max PSM debt based on system ceiling.
		pub(crate) fn max_psm_debt() -> BalanceOf<T> {
			let max_issuance = T::MaximumIssuance::get();
			MaxPsmDebtOfTotal::<T>::get().mul_floor(max_issuance)
		}

		/// Calculate max debt for a specific asset.
		///
		/// Assumes the caller has verified the asset is approved and `AllEnabled`.
		///
		/// Returns zero if the asset has no configured weight or the weight is zero.
		///
		/// Weights are normalized against the sum of all asset weights to fill the
		/// PSM ceiling.
		pub(crate) fn max_asset_debt(asset_id: T::AssetId) -> BalanceOf<T> {
			let asset_weight = AssetCeilingWeight::<T>::get(asset_id);

			if asset_weight.is_zero() {
				return BalanceOf::<T>::zero();
			}

			let total_weight_sum: u32 = AssetCeilingWeight::<T>::iter_values()
				.map(|w| w.deconstruct())
				.fold(0u32, |acc, x| acc.saturating_add(x));

			if total_weight_sum == 0 {
				return BalanceOf::<T>::zero();
			}

			let total_psm_ceiling = Self::max_psm_debt();
			Perbill::from_rational(asset_weight.deconstruct(), total_weight_sum)
				.mul_floor(total_psm_ceiling)
		}

		/// Calculate total PSM debt across all approved assets.
		pub(crate) fn total_psm_debt() -> BalanceOf<T> {
			PsmDebt::<T>::iter_values()
				.fold(BalanceOf::<T>::zero(), |acc, debt| acc.saturating_add(debt))
		}

		/// Check if an asset is approved for PSM swaps.
		#[cfg(test)]
		pub(crate) fn is_approved_asset(asset_id: &T::AssetId) -> bool {
			ExternalAssets::<T>::contains_key(asset_id)
		}

		/// Get the reserve (balance) of an external asset held by PSM.
		pub(crate) fn get_reserve(asset_id: T::AssetId) -> BalanceOf<T> {
			T::Fungibles::balance(asset_id, &Self::account_id())
		}

		/// Ensure an account exists by incrementing its provider count if needed.
		pub(crate) fn ensure_account_exists(account: &T::AccountId) {
			if !frame_system::Pallet::<T>::account_exists(account) {
				frame_system::Pallet::<T>::inc_providers(account);
			}
		}

		#[cfg(any(feature = "try-runtime", test))]
		pub(crate) fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
			use sp_runtime::traits::CheckedAdd;

			let stable_decimals = T::StableAsset::decimals();

			// Check 1: All approved assets must have matching decimals.
			for (asset_id, _) in ExternalAssets::<T>::iter() {
				ensure!(
					T::Fungibles::decimals(asset_id) == stable_decimals,
					"External asset decimals do not match stable asset decimals"
				);
			}

			// Check 2: Per-asset reserve must be >= per-asset debt.
			// The PSM holds 1:1 backing; donated reserves may cause reserve > debt.
			for (asset_id, _) in ExternalAssets::<T>::iter() {
				let debt = PsmDebt::<T>::get(asset_id);
				let reserve = Self::get_reserve(asset_id);
				ensure!(reserve >= debt, "PSM reserve is less than tracked debt for an asset");
			}

			// Check 3: Computed total PSM debt must equal sum of per-asset debts.
			let mut sum = BalanceOf::<T>::zero();
			for (asset_id, _) in ExternalAssets::<T>::iter() {
				sum = sum
					.checked_add(&PsmDebt::<T>::get(asset_id))
					.ok_or("PSM debt overflow when summing per-asset debts")?;
			}
			ensure!(
				Self::total_psm_debt() == sum,
				"total_psm_debt() does not match sum of per-asset debts"
			);

			// Check 4: Per-asset debt should not exceed its ceiling.
			// (May be transiently violated if governance lowers ceilings, but
			// should hold under normal operation.)
			for (asset_id, status) in ExternalAssets::<T>::iter() {
				if status.allows_minting() {
					let debt = PsmDebt::<T>::get(asset_id);
					let ceiling = Self::max_asset_debt(asset_id);
					ensure!(debt <= ceiling, "Per-asset PSM debt exceeds its ceiling");
				}
			}

			Ok(())
		}
	}
}

impl<T: pallet::Config> PsmInterface for pallet::Pallet<T> {
	type Balance = pallet::BalanceOf<T>;

	fn reserved_capacity() -> Self::Balance {
		Self::max_psm_debt()
	}
}
