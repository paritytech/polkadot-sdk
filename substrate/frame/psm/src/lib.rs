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
//! A module hosting one or more Peg Stability Modules. Each PSM enables 1:1 swaps between a
//! specific internal stablecoin and that PSM's pre-approved external stablecoins.
//!
//! ## Pallet API
//!
//! See the [`pallet`] module for more information about the interfaces this pallet exposes,
//! including its configuration trait, dispatchables, storage items, events and errors.
//!
//! ## Terminology
//!
//! Throughout this pallet two distinct token roles are referenced:
//!
//! * **Internal** — the stablecoin a PSM issues and burns (e.g. a runtime's pUSD). Each PSM
//!   instance is keyed by its internal asset id; multiple instances can coexist, each with its own
//!   reserve, debt ceiling, fee destination and approved externals. Mint operations credit the user
//!   with the internal asset; redeem operations burn it. Fees are collected in the internal asset
//!   and forwarded to that instance's [`PsmInfo::fee_destination`].
//! * **External** — third-party stablecoins (e.g. USDC, USDT) approved on a specific PSM via
//!   [`Pallet::add_external_asset`] and held in that PSM's reserve. Users deposit external to mint
//!   internal, and burn internal to redeem external. A PSM may approve multiple externals, each
//!   identified by `asset_id`.
//!
//! ## Overview
//!
//! A PSM strengthens its internal asset's peg by providing arbitrage opportunities:
//! - When the internal asset trades **above** $1: Users swap external stablecoins for the internal
//!   asset and sell for profit.
//! - When the internal asset trades **below** $1: Users buy cheap internal asset and swap for
//!   external stablecoins.
//!
//! This creates a price corridor bounded by the minting and redemption fees.
//!
//! ### Key Concepts
//!
//! * **PSM instance**: A configured Peg Stability Module, keyed by its internal asset id and
//!   described by [`PsmInfo`]. Each instance has its own reserve account derived as
//!   `PalletId::into_sub_account_truncating(internal_asset)`.
//! * **Minting**: Deposit external stablecoin → receive internal asset (minus fee).
//! * **Redemption**: Burn internal asset → receive external stablecoin (minus fee).
//! * **Reserve**: External stablecoin balance held by a PSM's reserve account (derived, not stored).
//! * **PSM Debt**: Total internal asset minted through a PSM, backed 1:1 by external stablecoins
//!   in that PSM's reserve.
//! * **Circuit Breaker**: Per-external emergency control to disable minting or all swaps.
//!
//! ### Fee Structure
//!
//! * **Minting Fee (`MintingFee`)**: Deducted from internal-asset output during minting,
//!   configured per `(internal_asset, external_asset)` pair.
//! * **Redemption Fee (`RedemptionFee`)**: Deducted from external stablecoin output during
//!   redemption, configured per `(internal_asset, external_asset)` pair.
//!
//! Fees are collected in the internal asset and transferred to the instance's
//! [`PsmInfo::fee_destination`].
//!
//! ### Example
//!
//! ```ignore
//! // Mint internal asset by depositing USDC on the pUSD PSM
//! Psm::mint(RuntimeOrigin::signed(user), PUSD_ASSET_ID, USDC_ASSET_ID, 1000 * UNIT)?;
//!
//! // Redeem USDC by burning pUSD
//! Psm::redeem(RuntimeOrigin::signed(user), PUSD_ASSET_ID, USDC_ASSET_ID, 1000 * UNIT)?;
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
	/// Get the asset ID for a given asset index.
	fn get_asset_id(asset_index: u32) -> AssetId;
	/// Create an asset with metadata matching the internal asset's decimals.
	fn create_asset(asset_id: AssetId, owner: &AccountId, decimals: u8);
}

#[frame_support::pallet]
pub mod pallet {
	pub use frame_support::traits::tokens::stable::PsmInterface;

	use codec::DecodeWithMemTracking;
	use frame_support::{
		pallet_prelude::*,
		traits::{
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
		traits::{AccountIdConversion, CheckedDiv, CheckedMul, Saturating, Zero},
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
		/// Whether this level allows minting (external → internal).
		pub const fn allows_minting(&self) -> bool {
			matches!(self, CircuitBreakerLevel::AllEnabled)
		}

		/// Whether this level allows redemption (internal → external).
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
			matches!(self, PsmManagerLevel::Full | PsmManagerLevel::Emergency)
		}

		/// Whether this level allows modifying the PSM debt ceiling.
		/// Both Full and Emergency levels can set the max debt.
		pub const fn can_set_max_debt(&self) -> bool {
			matches!(self, PsmManagerLevel::Full | PsmManagerLevel::Emergency)
		}

		/// Whether this level allows modifying per-asset ceiling weights.
		/// Both Full and Emergency levels can set asset ceilings.
		pub const fn can_set_asset_ceiling(&self) -> bool {
			matches!(self, PsmManagerLevel::Full | PsmManagerLevel::Emergency)
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

	/// Maximum absolute difference between an external asset's decimals and the internal
	/// asset's decimals. Bounds the scaling factor `10^diff` well below `u128::MAX`
	/// so realistic balances cannot overflow during conversion.
	pub const MAX_DECIMALS_DIFF: u32 = 24;

	/// On-chain record of a PSM instance.
	#[derive(
		Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, Debug,
	)]
	#[scale_info(skip_type_params(T))]
	pub struct PsmInfo<T: Config> {
		/// Account receiving minting and redemption fees, denominated in the internal asset.
		pub fee_destination: T::AccountId,
		/// Absolute internal-asset debt ceiling.
		pub max_debt: BalanceOf<T>,
		/// Snapshot of the internal asset's decimals at install time.
		pub internal_decimals: u8,
		/// Number of approved external assets attached to this instance.
		pub external_count: u32,
	}

	/// On-chain record of an external asset approved on a PSM instance.
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
	)]
	pub struct ExternalAssetInfo {
		/// Per-external circuit breaker status.
		pub status: CircuitBreakerLevel,
		/// Snapshot of the external asset's decimals at registration time.
		pub decimals: u8,
	}

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Fungibles implementation for both internal and external stablecoins.
		type Fungibles: FungiblesMutate<Self::AccountId, AssetId = Self::AssetId>
			+ FungiblesMetadataInspect<Self::AccountId>;

		/// Asset identifier type.
		type AssetId: Parameter + Member + Clone + MaybeSerializeDeserialize + MaxEncodedLen + Ord;

		/// Origin allowed to update PSM parameters.
		///
		/// Returns `PsmManagerLevel` to distinguish privilege levels:
		/// - `Full` (via GeneralAdmin): Can modify all parameters
		/// - `Emergency` (via EmergencyAction): Can modify circuit breaker status, per-asset
		///   ceiling weights, and the PSM debt ceiling.
		type ManagerOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = PsmManagerLevel>;

		/// A type representing the weights required by the dispatchables of this pallet.
		type WeightInfo: WeightInfo;

		/// PalletId for deriving each PSM instance's reserve sub-account.
		#[pallet::constant]
		type PalletId: Get<PalletId>;

		/// Minimum swap amount, in internal-asset units.
		#[pallet::constant]
		type MinSwapAmount: Get<BalanceOf<Self>>;

		/// Maximum number of approved external assets per PSM instance.
		#[pallet::constant]
		type MaxExternalAssetsPerPsm: Get<u32>;

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

	/// Registered PSM instances, keyed by the internal asset id.
	#[pallet::storage]
	pub type Psms<T: Config> = StorageMap<_, Blake2_128Concat, T::AssetId, PsmInfo<T>, OptionQuery>;

	/// Internal-asset debt minted through PSM, per `(internal, external)` pair.
	#[pallet::storage]
	pub type PsmDebt<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		T::AssetId,
		Blake2_128Concat,
		T::AssetId,
		BalanceOf<T>,
		ValueQuery,
	>;

	/// Fee for external → internal swaps (minting), per `(internal, external)` pair.
	/// Defaults to 0.5%.
	#[pallet::storage]
	pub(crate) type MintingFee<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		T::AssetId,
		Blake2_128Concat,
		T::AssetId,
		Permill,
		ValueQuery,
		DefaultFee,
	>;

	/// Fee for internal → external swaps (redemption), per `(internal, external)` pair.
	/// Defaults to 0.5%.
	#[pallet::storage]
	pub(crate) type RedemptionFee<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		T::AssetId,
		Blake2_128Concat,
		T::AssetId,
		Permill,
		ValueQuery,
		DefaultFee,
	>;

	/// Per-external ceiling weight within a PSM, normalised against the sum of weights
	/// for the same instance. Zero disables minting for that external.
	#[pallet::storage]
	pub(crate) type AssetCeilingWeight<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		T::AssetId,
		Blake2_128Concat,
		T::AssetId,
		Permill,
		ValueQuery,
	>;

	/// Approved external assets per PSM.
	#[pallet::storage]
	pub(crate) type ExternalAssets<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		T::AssetId,
		Blake2_128Concat,
		T::AssetId,
		ExternalAssetInfo,
		OptionQuery,
	>;

	/// Genesis configuration for the PSM pallet.
	///
	/// Each entry in `psms` declares one PSM instance; `externals` attaches approved
	/// externals to those instances. Externals whose `internal_asset` has no matching
	/// `psms` entry are rejected.
	#[pallet::genesis_config]
	#[derive(DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		/// `(internal_asset, fee_destination, max_debt)`.
		pub psms: alloc::vec::Vec<(T::AssetId, T::AccountId, BalanceOf<T>)>,
		/// `(internal_asset, external_asset, minting_fee, redemption_fee, ceiling_weight)`.
		pub externals: alloc::vec::Vec<(T::AssetId, T::AssetId, Permill, Permill, Permill)>,
		#[serde(skip)]
		pub _marker: core::marker::PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			for (internal, fee_destination, max_debt) in &self.psms {
				let internal_decimals = T::Fungibles::decimals(internal.clone());
				Psms::<T>::insert(
					internal,
					PsmInfo::<T> {
						fee_destination: fee_destination.clone(),
						max_debt: *max_debt,
						internal_decimals,
						external_count: 0,
					},
				);
				Pallet::<T>::ensure_account_exists(&Pallet::<T>::psm_account(internal));
				Pallet::<T>::ensure_account_exists(fee_destination);
			}

			for (internal, external, minting_fee, redemption_fee, ceiling_weight) in &self.externals
			{
				Psms::<T>::mutate(internal, |maybe| {
					let info = maybe.as_mut().unwrap_or_else(|| {
						panic!("PSM genesis: external configured for unregistered instance")
					});
					assert!(
						info.external_count < T::MaxExternalAssetsPerPsm::get(),
						"PSM genesis: externals on an instance exceed MaxExternalAssetsPerPsm",
					);

					let asset_decimals = T::Fungibles::decimals(external.clone());
					let diff = asset_decimals.abs_diff(info.internal_decimals) as u32;
					assert!(
						diff <= MAX_DECIMALS_DIFF,
						"PSM genesis: asset {:?} decimals diff ({}) exceeds MAX_DECIMALS_DIFF ({})",
						external,
						diff,
						MAX_DECIMALS_DIFF,
					);

					ExternalAssets::<T>::insert(
						internal,
						external,
						ExternalAssetInfo {
							status: CircuitBreakerLevel::AllEnabled,
							decimals: asset_decimals,
						},
					);
					MintingFee::<T>::insert(internal, external, minting_fee);
					RedemptionFee::<T>::insert(internal, external, redemption_fee);
					AssetCeilingWeight::<T>::insert(internal, external, ceiling_weight);
					info.external_count = info.external_count.saturating_add(1);
				});
			}
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// User swapped external stablecoin for internal.
		Minted {
			internal_asset: T::AssetId,
			who: T::AccountId,
			asset_id: T::AssetId,
			external_amount: BalanceOf<T>,
			received: BalanceOf<T>,
			fee: BalanceOf<T>,
		},
		/// User swapped internal for external stablecoin.
		Redeemed {
			internal_asset: T::AssetId,
			who: T::AccountId,
			asset_id: T::AssetId,
			paid: BalanceOf<T>,
			external_received: BalanceOf<T>,
			fee: BalanceOf<T>,
		},
		/// Minting fee updated for an asset by governance.
		MintingFeeUpdated {
			internal_asset: T::AssetId,
			asset_id: T::AssetId,
			old_value: Permill,
			new_value: Permill,
		},
		/// Redemption fee updated for an asset by governance.
		RedemptionFeeUpdated {
			internal_asset: T::AssetId,
			asset_id: T::AssetId,
			old_value: Permill,
			new_value: Permill,
		},
		/// PSM debt ceiling updated by governance.
		MaxDebtUpdated {
			internal_asset: T::AssetId,
			old_value: BalanceOf<T>,
			new_value: BalanceOf<T>,
		},
		/// Per-asset debt ceiling weight updated by governance.
		AssetCeilingWeightUpdated {
			internal_asset: T::AssetId,
			asset_id: T::AssetId,
			old_value: Permill,
			new_value: Permill,
		},
		/// Per-asset circuit breaker status updated.
		AssetStatusUpdated {
			internal_asset: T::AssetId,
			asset_id: T::AssetId,
			status: CircuitBreakerLevel,
		},
		/// An external asset was added to the approved list.
		ExternalAssetAdded { internal_asset: T::AssetId, asset_id: T::AssetId },
		/// An external asset was removed from the approved list.
		ExternalAssetRemoved { internal_asset: T::AssetId, asset_id: T::AssetId },
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
		/// No PSM instance is registered for the given internal asset.
		PsmNotFound,
		/// Asset is already in the approved list.
		AssetAlreadyApproved,
		/// Asset does not exist.
		AssetDoesNotExist,
		/// Cannot remove asset: not in approved list.
		AssetNotApproved,
		/// Cannot remove asset: has non-zero PSM debt.
		AssetHasDebt,
		/// Operation requires Full manager level (GeneralAdmin), not Emergency.
		InsufficientPrivilege,
		/// Maximum number of approved external assets reached.
		TooManyAssets,
		/// Live decimals diverged from the snapshot taken at registration or genesis.
		DecimalsMismatch,
		/// The asset's decimal precision is outside the supported range.
		DecimalsRangeExceeded,
		/// Decimal scaling produced an arithmetic overflow.
		ConversionOverflow,
		/// Conversion to the counter-asset rounds to zero; swap would transfer nothing.
		AmountTooSmallAfterConversion,
		/// An unexpected invariant violation occurred. This should be reported.
		Unexpected,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Swap external stablecoin for internal on a specific PSM instance.
		///
		/// ## Dispatch Origin
		///
		/// Must be `Signed` by the user performing the swap.
		///
		/// ## Details
		///
		/// Transfers `external_amount` of `asset_id` from the caller to the
		/// `internal_asset`'s PSM reserve account, then mints `internal_asset` to the
		/// caller minus the minting fee. The fee is calculated using ceiling rounding
		/// (`mul_ceil`), ensuring the protocol never undercharges. The fee is
		/// transferred to [`PsmInfo::fee_destination`] of the targeted instance.
		///
		/// ## Parameters
		///
		/// - `internal_asset`: The internal stablecoin that identifies the PSM instance.
		/// - `asset_id`: The external stablecoin to deposit (must be approved on `internal_asset`).
		/// - `external_amount`: Amount of external stablecoin to deposit.
		///
		/// ## Errors
		///
		/// - [`Error::PsmNotFound`]: If no PSM is registered for `internal_asset`.
		/// - [`Error::UnsupportedAsset`]: If `asset_id` is not approved on this PSM.
		/// - [`Error::MintingStopped`]: If the per-external circuit breaker is at `MintingDisabled`
		///   or higher.
		/// - [`Error::BelowMinimumSwap`]: If `external_amount` is below [`Config::MinSwapAmount`].
		/// - [`Error::ExceedsMaxPsmDebt`]: If minting would exceed this PSM's debt ceiling
		///   (aggregate or per-asset).
		/// - [`Error::DecimalsMismatch`]: If live decimals diverged from the snapshot taken at
		///   registration.
		/// - [`Error::AmountTooSmallAfterConversion`]: If the conversion to the counter-asset
		///   rounds to zero; swap would transfer nothing.
		///
		/// ## Events
		///
		/// - [`Event::Minted`]: Emitted on successful mint.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::mint(T::MaxExternalAssetsPerPsm::get()))]
		pub fn mint(
			origin: OriginFor<T>,
			internal_asset: T::AssetId,
			asset_id: T::AssetId,
			external_amount: BalanceOf<T>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let info = Psms::<T>::get(&internal_asset).ok_or(Error::<T>::PsmNotFound)?;

			let external = ExternalAssets::<T>::get(&internal_asset, &asset_id)
				.ok_or(Error::<T>::UnsupportedAsset)?;
			ensure!(external.status.allows_minting(), Error::<T>::MintingStopped);

			let (ext_decimals, internal_decimals) =
				Self::ensure_decimals_match(&info, &internal_asset, &asset_id, &external)?;

			let internal_equivalent =
				Self::external_to_internal(external_amount, ext_decimals, internal_decimals)?;
			ensure!(!internal_equivalent.is_zero(), Error::<T>::AmountTooSmallAfterConversion);
			ensure!(internal_equivalent >= T::MinSwapAmount::get(), Error::<T>::BelowMinimumSwap);

			let effective_external =
				Self::internal_to_external(internal_equivalent, ext_decimals, internal_decimals)?;

			let fee =
				MintingFee::<T>::get(&internal_asset, &asset_id).mul_ceil(internal_equivalent);
			let internal_to_user = internal_equivalent.saturating_sub(fee);

			let current_total_psm_debt = Self::total_psm_debt(&internal_asset);
			ensure!(
				current_total_psm_debt.saturating_add(internal_equivalent) <= info.max_debt,
				Error::<T>::ExceedsMaxPsmDebt
			);

			let current_debt = PsmDebt::<T>::get(&internal_asset, &asset_id);
			let max_debt = Self::max_asset_debt(&internal_asset, &asset_id, &info);
			let new_debt = current_debt.saturating_add(internal_equivalent);
			ensure!(new_debt <= max_debt, Error::<T>::ExceedsMaxPsmDebt);

			let psm_account = Self::psm_account(&internal_asset);
			T::Fungibles::transfer(
				asset_id.clone(),
				&who,
				&psm_account,
				effective_external,
				Preservation::Expendable,
			)?;
			T::Fungibles::mint_into(internal_asset.clone(), &who, internal_to_user)?;
			if !fee.is_zero() {
				T::Fungibles::mint_into(internal_asset.clone(), &info.fee_destination, fee)?;
			}

			PsmDebt::<T>::insert(&internal_asset, &asset_id, new_debt);

			Self::deposit_event(Event::Minted {
				internal_asset,
				who,
				asset_id,
				external_amount: effective_external,
				received: internal_to_user,
				fee,
			});
			Ok(())
		}

		/// Swap internal for external stablecoin on a specific PSM instance.
		///
		/// ## Dispatch Origin
		///
		/// Must be `Signed` by the user performing the swap.
		///
		/// ## Details
		///
		/// Burns `amount` of `internal_asset` from the caller minus fee (transferred to
		/// the instance's [`PsmInfo::fee_destination`]), then transfers the resulting
		/// amount in `asset_id` from the PSM reserve to the caller. The fee is
		/// calculated using ceiling rounding (`mul_ceil`), ensuring the protocol never
		/// undercharges.
		///
		/// ## Parameters
		///
		/// - `internal_asset`: The internal stablecoin that identifies the PSM instance.
		/// - `asset_id`: The external stablecoin to receive (must be approved on `internal_asset`).
		/// - `amount`: Amount of `internal_asset` to redeem.
		///
		/// ## Errors
		///
		/// - [`Error::PsmNotFound`]: If no PSM is registered for `internal_asset`.
		/// - [`Error::UnsupportedAsset`]: If `asset_id` is not approved on this PSM.
		/// - [`Error::AllSwapsStopped`]: If the per-external circuit breaker is at `AllDisabled`.
		/// - [`Error::BelowMinimumSwap`]: If `amount` is below [`Config::MinSwapAmount`].
		/// - [`Error::InsufficientReserve`]: If the PSM holds less of `asset_id` than the
		///   redemption requires.
		/// - [`Error::DecimalsMismatch`]: If live decimals diverged from the snapshot taken at
		///   registration.
		/// - [`Error::AmountTooSmallAfterConversion`]: If the conversion to the counter-asset
		///   rounds to zero; swap would transfer nothing.
		///
		/// ## Events
		///
		/// - [`Event::Redeemed`]: Emitted on successful redemption.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::redeem())]
		pub fn redeem(
			origin: OriginFor<T>,
			internal_asset: T::AssetId,
			asset_id: T::AssetId,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let info = Psms::<T>::get(&internal_asset).ok_or(Error::<T>::PsmNotFound)?;

			let external = ExternalAssets::<T>::get(&internal_asset, &asset_id)
				.ok_or(Error::<T>::UnsupportedAsset)?;
			ensure!(external.status.allows_redemption(), Error::<T>::AllSwapsStopped);

			let (ext_decimals, internal_decimals) =
				Self::ensure_decimals_match(&info, &internal_asset, &asset_id, &external)?;

			ensure!(amount >= T::MinSwapAmount::get(), Error::<T>::BelowMinimumSwap);

			let fee = RedemptionFee::<T>::get(&internal_asset, &asset_id).mul_ceil(amount);
			let internal_net = amount.saturating_sub(fee);

			let external_out =
				Self::internal_to_external(internal_net, ext_decimals, internal_decimals)?;
			ensure!(
				internal_net.is_zero() || !external_out.is_zero(),
				Error::<T>::AmountTooSmallAfterConversion
			);
			let effective_internal_net =
				Self::external_to_internal(external_out, ext_decimals, internal_decimals)?;

			let current_debt = PsmDebt::<T>::get(&internal_asset, &asset_id);
			ensure!(current_debt >= effective_internal_net, Error::<T>::InsufficientReserve);

			let reserve = Self::get_reserve(&internal_asset, &asset_id);
			if reserve < external_out {
				defensive!("PSM reserve is less than expected output amount");
				return Err(Error::<T>::Unexpected.into());
			}

			if !fee.is_zero() {
				T::Fungibles::transfer(
					internal_asset.clone(),
					&who,
					&info.fee_destination,
					fee,
					Preservation::Expendable,
				)?;
			}

			if !effective_internal_net.is_zero() {
				T::Fungibles::burn_from(
					internal_asset.clone(),
					&who,
					effective_internal_net,
					Preservation::Expendable,
					Precision::Exact,
					Fortitude::Polite,
				)?;
			}

			let psm_account = Self::psm_account(&internal_asset);
			if !external_out.is_zero() {
				T::Fungibles::transfer(
					asset_id.clone(),
					&psm_account,
					&who,
					external_out,
					Preservation::Expendable,
				)?;
			}

			PsmDebt::<T>::mutate(&internal_asset, &asset_id, |debt| {
				*debt = debt.saturating_sub(effective_internal_net);
			});

			Self::deposit_event(Event::Redeemed {
				internal_asset,
				who,
				asset_id,
				paid: effective_internal_net.saturating_add(fee),
				external_received: external_out,
				fee,
			});
			Ok(())
		}

		/// Set the minting fee for an `(internal_asset, asset_id)` pair.
		///
		/// ## Dispatch Origin
		///
		/// Must be [`Config::ManagerOrigin`] at the `Full` level.
		///
		/// ## Parameters
		///
		/// - `internal_asset`: The PSM instance to configure.
		/// - `asset_id`: The external stablecoin whose minting fee is being updated.
		/// - `fee`: The new minting fee.
		///
		/// ## Errors
		///
		/// - [`Error::InsufficientPrivilege`]: If the origin only has `Emergency` privileges.
		/// - [`Error::AssetNotApproved`]: If `asset_id` is not approved on `internal_asset`.
		///
		/// ## Events
		///
		/// - [`Event::MintingFeeUpdated`]: Emitted with old and new values.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::set_minting_fee())]
		pub fn set_minting_fee(
			origin: OriginFor<T>,
			internal_asset: T::AssetId,
			asset_id: T::AssetId,
			fee: Permill,
		) -> DispatchResult {
			let level = T::ManagerOrigin::ensure_origin(origin)?;
			ensure!(level.can_set_fees(), Error::<T>::InsufficientPrivilege);
			ensure!(
				ExternalAssets::<T>::contains_key(&internal_asset, &asset_id),
				Error::<T>::AssetNotApproved
			);
			let old_value = MintingFee::<T>::get(&internal_asset, &asset_id);
			MintingFee::<T>::insert(&internal_asset, &asset_id, fee);
			Self::deposit_event(Event::MintingFeeUpdated {
				internal_asset,
				asset_id,
				old_value,
				new_value: fee,
			});
			Ok(())
		}

		/// Set the redemption fee for an `(internal_asset, asset_id)` pair.
		///
		/// ## Dispatch Origin
		///
		/// Must be [`Config::ManagerOrigin`] at the `Full` level.
		///
		/// ## Parameters
		///
		/// - `internal_asset`: The PSM instance to configure.
		/// - `asset_id`: The external stablecoin whose redemption fee is being updated.
		/// - `fee`: The new redemption fee.
		///
		/// ## Errors
		///
		/// - [`Error::InsufficientPrivilege`]: If the origin only has `Emergency` privileges.
		/// - [`Error::AssetNotApproved`]: If `asset_id` is not approved on `internal_asset`.
		///
		/// ## Events
		///
		/// - [`Event::RedemptionFeeUpdated`]: Emitted with old and new values.
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::set_redemption_fee())]
		pub fn set_redemption_fee(
			origin: OriginFor<T>,
			internal_asset: T::AssetId,
			asset_id: T::AssetId,
			fee: Permill,
		) -> DispatchResult {
			let level = T::ManagerOrigin::ensure_origin(origin)?;
			ensure!(level.can_set_fees(), Error::<T>::InsufficientPrivilege);
			ensure!(
				ExternalAssets::<T>::contains_key(&internal_asset, &asset_id),
				Error::<T>::AssetNotApproved
			);
			let old_value = RedemptionFee::<T>::get(&internal_asset, &asset_id);
			RedemptionFee::<T>::insert(&internal_asset, &asset_id, fee);
			Self::deposit_event(Event::RedemptionFeeUpdated {
				internal_asset,
				asset_id,
				old_value,
				new_value: fee,
			});
			Ok(())
		}

		/// Set the absolute PSM debt ceiling of a specific PSM instance.
		///
		/// ## Dispatch Origin
		///
		/// Must be [`Config::ManagerOrigin`]. Both `Full` and `Emergency` levels may use
		/// this call.
		///
		/// ## Parameters
		///
		/// - `internal_asset`: The PSM instance to configure.
		/// - `value`: The new absolute debt ceiling, in internal-asset units.
		///
		/// ## Errors
		///
		/// - [`Error::InsufficientPrivilege`]: If the origin level cannot set the debt ceiling.
		/// - [`Error::PsmNotFound`]: If no PSM is registered for `internal_asset`.
		///
		/// ## Events
		///
		/// - [`Event::MaxDebtUpdated`]: Emitted with old and new values.
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::set_max_debt())]
		pub fn set_max_debt(
			origin: OriginFor<T>,
			internal_asset: T::AssetId,
			value: BalanceOf<T>,
		) -> DispatchResult {
			let level = T::ManagerOrigin::ensure_origin(origin)?;
			ensure!(level.can_set_max_debt(), Error::<T>::InsufficientPrivilege);
			Psms::<T>::try_mutate(&internal_asset, |maybe| -> DispatchResult {
				let info = maybe.as_mut().ok_or(Error::<T>::PsmNotFound)?;
				let old_value = info.max_debt;
				info.max_debt = value;
				Self::deposit_event(Event::MaxDebtUpdated {
					internal_asset: internal_asset.clone(),
					old_value,
					new_value: value,
				});
				Ok(())
			})
		}

		/// Set the per-external circuit breaker on a PSM instance.
		///
		/// ## Dispatch Origin
		///
		/// Must be [`Config::ManagerOrigin`]. Both `Full` and `Emergency` levels may use
		/// this call.
		///
		/// ## Parameters
		///
		/// - `internal_asset`: The PSM instance to configure.
		/// - `asset_id`: The external stablecoin whose status is being updated.
		/// - `status`: The new circuit breaker level for that external.
		///
		/// ## Errors
		///
		/// - [`Error::AssetNotApproved`]: If `asset_id` is not approved on `internal_asset`.
		///
		/// ## Events
		///
		/// - [`Event::AssetStatusUpdated`]: Emitted on a successful update.
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::set_asset_status())]
		pub fn set_asset_status(
			origin: OriginFor<T>,
			internal_asset: T::AssetId,
			asset_id: T::AssetId,
			status: CircuitBreakerLevel,
		) -> DispatchResult {
			T::ManagerOrigin::ensure_origin(origin)?;
			ExternalAssets::<T>::try_mutate(
				&internal_asset,
				&asset_id,
				|maybe| -> DispatchResult {
					let info = maybe.as_mut().ok_or(Error::<T>::AssetNotApproved)?;
					info.status = status;
					Ok(())
				},
			)?;
			Self::deposit_event(Event::AssetStatusUpdated { internal_asset, asset_id, status });
			Ok(())
		}

		/// Set the per-external ceiling weight on a PSM instance.
		///
		/// Weights are normalised against the sum of weights within the same instance:
		/// `max_asset_debt = (weight / sum_of_weights) * info.max_debt`.
		///
		/// ## Dispatch Origin
		///
		/// Must be [`Config::ManagerOrigin`]. Both `Full` and `Emergency` levels may use
		/// this call.
		///
		/// ## Parameters
		///
		/// - `internal_asset`: The PSM instance to configure.
		/// - `asset_id`: The external stablecoin whose ceiling weight is being updated.
		/// - `weight`: The new ceiling weight. Zero disables minting for this external.
		///
		/// ## Errors
		///
		/// - [`Error::InsufficientPrivilege`]: If the origin level cannot set ceiling weights.
		/// - [`Error::AssetNotApproved`]: If `asset_id` is not approved on `internal_asset`.
		///
		/// ## Events
		///
		/// - [`Event::AssetCeilingWeightUpdated`]: Emitted with old and new values.
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::set_asset_ceiling_weight())]
		pub fn set_asset_ceiling_weight(
			origin: OriginFor<T>,
			internal_asset: T::AssetId,
			asset_id: T::AssetId,
			weight: Permill,
		) -> DispatchResult {
			let level = T::ManagerOrigin::ensure_origin(origin)?;
			ensure!(level.can_set_asset_ceiling(), Error::<T>::InsufficientPrivilege);
			ensure!(
				ExternalAssets::<T>::contains_key(&internal_asset, &asset_id),
				Error::<T>::AssetNotApproved
			);
			let old_value = AssetCeilingWeight::<T>::get(&internal_asset, &asset_id);
			AssetCeilingWeight::<T>::insert(&internal_asset, &asset_id, weight);
			Self::deposit_event(Event::AssetCeilingWeightUpdated {
				internal_asset,
				asset_id,
				old_value,
				new_value: weight,
			});
			Ok(())
		}

		/// Approve an external stablecoin on a PSM instance.
		///
		/// Snapshots the external asset's live decimals at registration time and
		/// increments [`PsmInfo::external_count`].
		///
		/// ## Dispatch Origin
		///
		/// Must be [`Config::ManagerOrigin`] at the `Full` level.
		///
		/// ## Parameters
		///
		/// - `internal_asset`: The PSM instance to approve the external on.
		/// - `asset_id`: The external stablecoin to approve.
		///
		/// ## Errors
		///
		/// - [`Error::InsufficientPrivilege`]: If the origin only has `Emergency` privileges.
		/// - [`Error::PsmNotFound`]: If no PSM is registered for `internal_asset`.
		/// - [`Error::TooManyAssets`]: If the PSM is already at
		///   [`Config::MaxExternalAssetsPerPsm`].
		/// - [`Error::AssetAlreadyApproved`]: If `asset_id` is already approved on this PSM.
		/// - [`Error::AssetDoesNotExist`]: If `asset_id` does not exist in the underlying fungibles
		///   backend.
		/// - [`Error::DecimalsMismatch`]: If the internal asset's live decimals diverged from the
		///   snapshot in [`PsmInfo`].
		/// - [`Error::DecimalsRangeExceeded`]: If `|asset_decimals − internal_decimals|` exceeds
		///   [`MAX_DECIMALS_DIFF`].
		///
		/// ## Events
		///
		/// - [`Event::ExternalAssetAdded`]: Emitted on a successful approval.
		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::add_external_asset())]
		pub fn add_external_asset(
			origin: OriginFor<T>,
			internal_asset: T::AssetId,
			asset_id: T::AssetId,
		) -> DispatchResult {
			let level = T::ManagerOrigin::ensure_origin(origin)?;
			ensure!(level.can_manage_assets(), Error::<T>::InsufficientPrivilege);
			Psms::<T>::try_mutate(&internal_asset, |maybe| -> DispatchResult {
				let info = maybe.as_mut().ok_or(Error::<T>::PsmNotFound)?;
				ensure!(
					info.external_count < T::MaxExternalAssetsPerPsm::get(),
					Error::<T>::TooManyAssets
				);
				ensure!(
					!ExternalAssets::<T>::contains_key(&internal_asset, &asset_id),
					Error::<T>::AssetAlreadyApproved
				);
				ensure!(
					T::Fungibles::asset_exists(asset_id.clone()),
					Error::<T>::AssetDoesNotExist
				);

				let asset_decimals = T::Fungibles::decimals(asset_id.clone());
				ensure!(
					T::Fungibles::decimals(internal_asset.clone()) == info.internal_decimals,
					Error::<T>::DecimalsMismatch
				);
				ensure!(
					(asset_decimals.abs_diff(info.internal_decimals) as u32) <= MAX_DECIMALS_DIFF,
					Error::<T>::DecimalsRangeExceeded
				);

				ExternalAssets::<T>::insert(
					&internal_asset,
					&asset_id,
					ExternalAssetInfo {
						status: CircuitBreakerLevel::AllEnabled,
						decimals: asset_decimals,
					},
				);
				info.external_count = info.external_count.saturating_add(1);
				Self::deposit_event(Event::ExternalAssetAdded {
					internal_asset: internal_asset.clone(),
					asset_id,
				});
				Ok(())
			})
		}

		/// Remove an external stablecoin from a PSM instance.
		///
		/// Wipes the external's per-instance state (status, decimals, fees, ceiling
		/// weight, debt counter) and decrements [`PsmInfo::external_count`]. The
		/// external must have zero outstanding debt on this instance.
		///
		/// ## Dispatch Origin
		///
		/// Must be [`Config::ManagerOrigin`] at the `Full` level.
		///
		/// ## Parameters
		///
		/// - `internal_asset`: The PSM instance to remove the external from.
		/// - `asset_id`: The external stablecoin to remove.
		///
		/// ## Errors
		///
		/// - [`Error::InsufficientPrivilege`]: If the origin only has `Emergency` privileges.
		/// - [`Error::PsmNotFound`]: If no PSM is registered for `internal_asset`.
		/// - [`Error::AssetNotApproved`]: If `asset_id` is not approved on this PSM.
		/// - [`Error::AssetHasDebt`]: If the external still has non-zero outstanding debt.
		///
		/// ## Events
		///
		/// - [`Event::ExternalAssetRemoved`]: Emitted on a successful removal.
		#[pallet::call_index(8)]
		#[pallet::weight(T::WeightInfo::remove_external_asset())]
		pub fn remove_external_asset(
			origin: OriginFor<T>,
			internal_asset: T::AssetId,
			asset_id: T::AssetId,
		) -> DispatchResult {
			let level = T::ManagerOrigin::ensure_origin(origin)?;
			ensure!(level.can_manage_assets(), Error::<T>::InsufficientPrivilege);
			Psms::<T>::try_mutate(&internal_asset, |maybe| -> DispatchResult {
				let info = maybe.as_mut().ok_or(Error::<T>::PsmNotFound)?;
				ensure!(
					ExternalAssets::<T>::contains_key(&internal_asset, &asset_id),
					Error::<T>::AssetNotApproved
				);
				ensure!(
					PsmDebt::<T>::get(&internal_asset, &asset_id).is_zero(),
					Error::<T>::AssetHasDebt
				);
				ExternalAssets::<T>::remove(&internal_asset, &asset_id);
				MintingFee::<T>::remove(&internal_asset, &asset_id);
				RedemptionFee::<T>::remove(&internal_asset, &asset_id);
				AssetCeilingWeight::<T>::remove(&internal_asset, &asset_id);
				PsmDebt::<T>::remove(&internal_asset, &asset_id);
				info.external_count = info.external_count.saturating_sub(1);
				Self::deposit_event(Event::ExternalAssetRemoved {
					internal_asset: internal_asset.clone(),
					asset_id,
				});
				Ok(())
			})
		}
	}

	impl<T: Config> Pallet<T> {
		/// Derive the reserve account for a PSM instance.
		pub fn psm_account(internal_asset: &T::AssetId) -> T::AccountId {
			T::PalletId::get().into_sub_account_truncating(internal_asset)
		}

		/// PSM debt ceiling for an instance, read from the stored [`PsmInfo`]. Returns
		/// zero if no PSM is installed for `internal_asset`.
		#[cfg(test)]
		pub(crate) fn max_psm_debt(internal_asset: &T::AssetId) -> BalanceOf<T> {
			Psms::<T>::get(internal_asset).map(|p| p.max_debt).unwrap_or_default()
		}

		/// Calculate max debt for a specific external on a PSM.
		///
		/// Weights are normalised against the sum of weights within the same instance to
		/// fill the instance's `max_debt` ceiling. Returns zero if the external has no
		/// configured weight or weights sum to zero.
		pub(crate) fn max_asset_debt(
			internal_asset: &T::AssetId,
			asset_id: &T::AssetId,
			info: &PsmInfo<T>,
		) -> BalanceOf<T> {
			let asset_weight = AssetCeilingWeight::<T>::get(internal_asset, asset_id);

			if asset_weight.is_zero() {
				return BalanceOf::<T>::zero();
			}

			let total_weight_sum: u32 = AssetCeilingWeight::<T>::iter_prefix(internal_asset)
				.map(|(_, w)| w.deconstruct())
				.fold(0u32, |acc, x| acc.saturating_add(x));

			if total_weight_sum == 0 {
				return BalanceOf::<T>::zero();
			}

			Perbill::from_rational(asset_weight.deconstruct(), total_weight_sum)
				.mul_floor(info.max_debt)
		}

		/// Total internal-asset debt minted through a PSM instance.
		pub(crate) fn total_psm_debt(internal_asset: &T::AssetId) -> BalanceOf<T> {
			PsmDebt::<T>::iter_prefix_values(internal_asset)
				.fold(BalanceOf::<T>::zero(), |acc, debt| acc.saturating_add(debt))
		}

		/// Whether an external is approved on a PSM instance.
		#[cfg(test)]
		pub(crate) fn is_approved_asset(
			internal_asset: &T::AssetId,
			asset_id: &T::AssetId,
		) -> bool {
			ExternalAssets::<T>::contains_key(internal_asset, asset_id)
		}

		/// Balance of an external held by a PSM instance's reserve account.
		pub(crate) fn get_reserve(
			internal_asset: &T::AssetId,
			asset_id: &T::AssetId,
		) -> BalanceOf<T> {
			T::Fungibles::balance(asset_id.clone(), &Self::psm_account(internal_asset))
		}

		/// Convert an amount denominated in external-asset units into internal units.
		///
		/// Scales by `10^(ext_decimals - internal_decimals)` — multiplies up when internal has more
		/// decimals, floor-divides when it has fewer. Returns [`Error::ConversionOverflow`] if
		/// the scaling factor or the product does not fit in the balance type.
		pub(crate) fn external_to_internal(
			amount: BalanceOf<T>,
			ext_decimals: u8,
			internal_decimals: u8,
		) -> Result<BalanceOf<T>, Error<T>> {
			use core::cmp::Ordering::*;
			match ext_decimals.cmp(&internal_decimals) {
				Equal => Ok(amount),
				Less => {
					let diff = (internal_decimals - ext_decimals) as u32;
					let factor = Self::pow10(diff)?;
					amount.checked_mul(&factor).ok_or(Error::<T>::ConversionOverflow)
				},
				Greater => {
					let diff = (ext_decimals - internal_decimals) as u32;
					let factor = Self::pow10(diff)?;
					Ok(amount.checked_div(&factor).unwrap_or_else(BalanceOf::<T>::zero))
				},
			}
		}

		/// Convert an amount denominated in internal units into external-asset units.
		///
		/// Inverse of [`Self::external_to_internal`]. Floor-divides when internal has more
		/// decimals, multiplies up when it has fewer.
		pub(crate) fn internal_to_external(
			amount: BalanceOf<T>,
			ext_decimals: u8,
			internal_decimals: u8,
		) -> Result<BalanceOf<T>, Error<T>> {
			use core::cmp::Ordering::*;
			match ext_decimals.cmp(&internal_decimals) {
				Equal => Ok(amount),
				Less => {
					let diff = (internal_decimals - ext_decimals) as u32;
					let factor = Self::pow10(diff)?;
					Ok(amount.checked_div(&factor).unwrap_or_else(BalanceOf::<T>::zero))
				},
				Greater => {
					let diff = (ext_decimals - internal_decimals) as u32;
					let factor = Self::pow10(diff)?;
					amount.checked_mul(&factor).ok_or(Error::<T>::ConversionOverflow)
				},
			}
		}

		/// Compute `10^exp` as a [`BalanceOf`]. Returns [`Error::ConversionOverflow`] if the result
		/// does not fit in `u128` or in `BalanceOf<T>`.
		fn pow10(exp: u32) -> Result<BalanceOf<T>, Error<T>> {
			let factor_u128 = 10u128.checked_pow(exp).ok_or(Error::<T>::ConversionOverflow)?;
			factor_u128.try_into().map_err(|_| Error::<T>::ConversionOverflow)
		}

		/// Verify the live decimals for an external still match the snapshot taken at
		/// registration on this PSM, and that the internal asset's live decimals still
		/// match the snapshot stored in [`PsmInfo`].
		pub(crate) fn ensure_decimals_match(
			info: &PsmInfo<T>,
			internal_asset: &T::AssetId,
			asset_id: &T::AssetId,
			external: &ExternalAssetInfo,
		) -> Result<(u8, u8), DispatchError> {
			ensure!(
				T::Fungibles::decimals(asset_id.clone()) == external.decimals,
				Error::<T>::DecimalsMismatch
			);
			ensure!(
				T::Fungibles::decimals(internal_asset.clone()) == info.internal_decimals,
				Error::<T>::DecimalsMismatch
			);
			Ok((external.decimals, info.internal_decimals))
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

			for (internal_asset, info) in Psms::<T>::iter() {
				// 1. Live internal decimals must match the snapshot.
				ensure!(
					T::Fungibles::decimals(internal_asset.clone()) == info.internal_decimals,
					"Internal asset live decimals diverged from the snapshot"
				);

				let mut counted = 0u32;
				for (asset_id, external) in ExternalAssets::<T>::iter_prefix(&internal_asset) {
					ensure!(
						T::Fungibles::decimals(asset_id.clone()) == external.decimals,
						"External asset live decimals diverged from the snapshot"
					);
					counted = counted.saturating_add(1);

					// 2. Per-external reserve covers tracked debt.
					let debt = PsmDebt::<T>::get(&internal_asset, &asset_id);
					let reserve = Self::get_reserve(&internal_asset, &asset_id);
					let debt_as_external =
						Self::internal_to_external(debt, external.decimals, info.internal_decimals)
							.map_err(|_| "Failed to convert tracked debt to external units")?;
					ensure!(
						reserve >= debt_as_external,
						"PSM reserve is less than tracked debt for an asset"
					);
				}

				// 3. Cached `external_count` matches the iterated externals.
				ensure!(
					info.external_count == counted,
					"PsmInfo.external_count does not match the approved externals"
				);

				// 4. Sum of per-asset debts equals the aggregate helper.
				let mut sum = BalanceOf::<T>::zero();
				for (_, debt) in PsmDebt::<T>::iter_prefix(&internal_asset) {
					sum = sum.checked_add(&debt).ok_or("PSM debt overflow when summing")?;
				}
				ensure!(
					sum == Self::total_psm_debt(&internal_asset),
					"sum of per-asset debts disagrees with total_psm_debt"
				);

				// 5. Aggregate debt within the configured ceiling.
				ensure!(sum <= info.max_debt, "Aggregate PSM debt exceeds the instance's max_debt");

				// 6. Per-asset debt within its (normalised) ceiling when minting is enabled.
				for (asset_id, external) in ExternalAssets::<T>::iter_prefix(&internal_asset) {
					if external.status.allows_minting() {
						let debt = PsmDebt::<T>::get(&internal_asset, &asset_id);
						let ceiling = Self::max_asset_debt(&internal_asset, &asset_id, &info);
						ensure!(debt <= ceiling, "Per-asset PSM debt exceeds its ceiling");
					}
				}
			}

			// 7. No orphaned per-asset state outside registered PSMs.
			for (internal_asset, _, _) in ExternalAssets::<T>::iter() {
				ensure!(
					Psms::<T>::contains_key(&internal_asset),
					"Orphaned ExternalAssets row without parent PSM"
				);
			}
			for (internal_asset, _, _) in PsmDebt::<T>::iter() {
				ensure!(
					Psms::<T>::contains_key(&internal_asset),
					"Orphaned PsmDebt row without parent PSM"
				);
			}

			Ok(())
		}
	}
}

impl<T: pallet::Config> PsmInterface for pallet::Pallet<T> {
	type AssetId = T::AssetId;
	type Balance = pallet::BalanceOf<T>;

	fn reserved_capacity(asset: Self::AssetId) -> Self::Balance {
		pallet::Psms::<T>::get(asset).map(|p| p.max_debt).unwrap_or_default()
	}
}
