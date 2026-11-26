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

//! Forwards fungible asset definitions to a sibling chain over XCM.
//!
//! Any signed account can replicate an asset of the local registry, [`Config::Assets`], on the
//! destination chain. The pallet sends an XCM `Transact` that force-creates the replica with the
//! asset's current minimum balance and sufficiency, owned by this pallet's sovereign account on the
//! destination. Nobody controls that account, so the replica's issuance can only change through
//! XCM transfers backed by this chain. Metadata is not forwarded.
//!
//! The XCM program requests unpaid execution, so the destination's barrier must grant it to this
//! chain, and dispatches `force_create` from this pallet's location, which the destination's
//! `pallet-assets` instance must accept as `ForceOrigin`. That instance must use
//! `xcm::v5::Location` as asset id parameter (see [`RemoteAssetsCall`]); the replica's id is
//! [`Config::AssetIdToLocation`] reanchored to the destination.
//!
//! The caller pays the XCM delivery fees and holds [`Config::ForwardDeposit`], which should cover
//! at least the destination's asset deposit since the replica is created without one. Only
//! [`Config::ManagerOrigin`] can release it, with [`Pallet::remove_forwarded_asset`].
//!
//! After the asset's minimum balance or sufficiency changes, anyone can re-send them with
//! [`Pallet::sync_asset_status`].
//!
//! The forwarding is one-shot, so execution on the destination is not verified or reported back. A
//! forward that fails on the destination chain leaves a [`ForwardedAssets`] entry without a
//! replica. It can be removed by [`Config::ManagerOrigin`] so the asset can be forwarded again. A
//! sync that fails there leaves the record ahead of the replica until the destination's force
//! origin corrects it and the next local change re-enables the sync.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;
pub mod weights;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub use pallet::*;
pub use weights::WeightInfo;

use alloc::vec;
use codec::{Encode, HasCompact};
use frame_support::{
	pallet_prelude::*,
	traits::{
		fungible::{Inspect as FungibleInspect, Mutate as FungibleMutate, MutateHold},
		fungibles::Inspect as FungiblesInspect,
		tokens::{DepositConsequence, Precision, Provenance},
	},
};
use sp_runtime::{
	traits::{MaybeEquivalence, TryConvert, Zero},
	MultiAddress,
};
use xcm::v5::{
	validate_send, ExecuteXcm, InteriorLocation, Junction, Location, MaybeErrorCode, OriginKind,
	Reanchorable, SendXcm, WeightLimit, Xcm, XcmHash,
};
use xcm_executor::traits::{ConvertLocation, FeeManager, FeeReason};

/// Balance type of the currency backing the forward deposit.
pub type NativeBalanceOf<T> =
	<<T as Config>::Currency as FungibleInspect<<T as frame_system::Config>::AccountId>>::Balance;

/// Asset id type of the local asset registry.
pub type AssetIdOf<T> =
	<<T as Config>::Assets as FungiblesInspect<<T as frame_system::Config>::AccountId>>::AssetId;

/// Balance type of the local asset registry.
pub type AssetBalanceOf<T> =
	<<T as Config>::Assets as FungiblesInspect<<T as frame_system::Config>::AccountId>>::Balance;

/// Record of a forwarded asset.
#[derive(Clone, Debug, Decode, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo)]
pub struct ForwardInfo<AccountId, Balance, AssetBalance> {
	/// The account that forwarded the asset and pays the deposit.
	pub depositor: AccountId,
	/// Deposit held from `depositor`.
	pub deposit: Balance,
	/// The asset's minimum balance last sent to the destination.
	pub min_balance: AssetBalance,
	/// The asset's sufficiency last sent to the destination.
	pub is_sufficient: bool,
}

/// The destination's `pallet-assets` calls this pallet sends, with their call indices.
///
/// Decoding assumes the destination instance uses `xcm::v5::Location` as asset id parameter and
/// `MultiAddress` lookups over this chain's account id type.
#[derive(Clone, Debug, Encode, Eq, PartialEq)]
pub enum RemoteAssetsCall<AccountId, Balance: HasCompact> {
	#[codec(index = 1)]
	ForceCreate {
		id: Location,
		owner: MultiAddress<AccountId, ()>,
		is_sufficient: bool,
		#[codec(compact)]
		min_balance: Balance,
	},
	#[codec(index = 21)]
	ForceAssetStatus {
		id: Location,
		owner: MultiAddress<AccountId, ()>,
		issuer: MultiAddress<AccountId, ()>,
		admin: MultiAddress<AccountId, ()>,
		freezer: MultiAddress<AccountId, ()>,
		#[codec(compact)]
		min_balance: Balance,
		is_sufficient: bool,
		is_frozen: bool,
	},
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching hold reason type.
		type RuntimeHoldReason: From<HoldReason>;

		/// Currency the forward deposit is held in.
		type Currency: FungibleMutate<Self::AccountId>
			+ MutateHold<Self::AccountId, Reason = Self::RuntimeHoldReason>;

		/// Registry of the assets that can be forwarded, e.g. a `pallet-assets` instance or a
		/// `fungibles::UnionOf` over several. Must implement `fungibles::Create` under
		/// `runtime-benchmarks`.
		type Assets: FungiblesInspect<Self::AccountId>;

		/// Location of an asset id from this chain's perspective, via `convert_back`.
		///
		/// Use `xcm_builder::AsPrefixedGeneralIndex` for a local `pallet-assets` instance and
		/// `sp_runtime::traits::Identity` for location-keyed assets.
		type AssetIdToLocation: MaybeEquivalence<Location, AssetIdOf<Self>>;

		/// Deposit held per forwarded asset until [`Config::ManagerOrigin`] removes its entry.
		type ForwardDeposit: Get<NativeBalanceOf<Self>>;

		/// Origin allowed to remove [`ForwardedAssets`] entries and release their deposits.
		type ManagerOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Location of the destination chain, as seen from this chain.
		type Destination: Get<Location>;

		/// Index of the target `pallet-assets` instance in the destination's runtime call enum.
		type RemoteAssetsPalletIndex: Get<u8>;

		/// Universal location of this chain, used to reanchor locations to the destination.
		type UniversalLocation: Get<InteriorLocation>;

		/// Location-to-account converter matching the destination's; derives this pallet's
		/// sovereign account there.
		type DestinationAccountOf: ConvertLocation<Self::AccountId>;

		/// Converts the caller's origin into the location charged for XCM delivery fees.
		type OriginToLocation: TryConvert<Self::RuntimeOrigin, Location>;

		/// Router that delivers XCM to the destination.
		type XcmSender: SendXcm;

		/// Executor used to charge XCM delivery fees to the caller.
		type XcmExecutor: ExecuteXcm<Self::RuntimeCall> + FeeManager;

		/// Weight information for this pallet.
		type WeightInfo: WeightInfo;

		/// Runtime-specific benchmark setup, see [`benchmarking::BenchmarkHelper`].
		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper: crate::benchmarking::BenchmarkHelper<AssetIdOf<Self>>;
	}

	/// Reasons for holding funds.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// Deposit backing a [`ForwardedAssets`] entry.
		#[codec(index = 0)]
		ForwardDeposit,
	}

	/// Assets already forwarded to the destination chain.
	#[pallet::storage]
	pub type ForwardedAssets<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		AssetIdOf<T>,
		ForwardInfo<T::AccountId, NativeBalanceOf<T>, AssetBalanceOf<T>>,
		OptionQuery,
	>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An asset was forwarded to the destination chain.
		AssetForwarded {
			asset_id: AssetIdOf<T>,
			remote_asset_id: Location,
			depositor: T::AccountId,
			deposit: NativeBalanceOf<T>,
			min_balance: AssetBalanceOf<T>,
			is_sufficient: bool,
			message_id: XcmHash,
		},
		/// The status of a forwarded asset was re-sent to the destination chain.
		AssetStatusSynced {
			asset_id: AssetIdOf<T>,
			min_balance: AssetBalanceOf<T>,
			is_sufficient: bool,
			message_id: XcmHash,
		},
		/// A forwarded asset's record was removed and its deposit released to the depositor.
		ForwardRemoved {
			asset_id: AssetIdOf<T>,
			depositor: T::AccountId,
			released: NativeBalanceOf<T>,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The asset does not exist in the local registry or is being destroyed.
		UnknownAsset,
		/// The asset's minimum balance is zero, which the destination refuses to create an asset
		/// with.
		MinBalanceZero,
		/// The asset was already forwarded.
		AlreadyForwarded,
		/// The asset was not forwarded yet.
		NotForwarded,
		/// The asset's status equals the last-sent one.
		StatusUnchanged,
		/// The asset id or the pallet location cannot be expressed from the destination's
		/// perspective, or the asset is native to the destination.
		InvalidAssetLocation,
		/// The caller's origin or this pallet's location cannot be converted.
		LocationConversionFailed,
		/// The XCM delivery fees cannot be charged to the caller.
		FeesNotPaid,
		/// The message cannot be delivered to the destination.
		SendFailed,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Forward the asset `id` to the destination chain. The caller pays the XCM delivery fees
		/// and [`Config::ForwardDeposit`].
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::forward_asset())]
		pub fn forward_asset(origin: OriginFor<T>, id: AssetIdOf<T>) -> DispatchResult {
			let who = ensure_signed(origin.clone())?;
			ensure!(!ForwardedAssets::<T>::contains_key(&id), Error::<T>::AlreadyForwarded);
			let (min_balance, is_sufficient) = Self::asset_status(&id, &who)?;
			// A min balance of `0` cannot be force created on the destination chain, so this call
			// would only freeze the caller's deposit for no benefit.
			ensure!(!min_balance.is_zero(), Error::<T>::MinBalanceZero);
			let remote_asset_id = Self::remote_asset_id(id.clone())?;
			let owner = Self::remote_owner_account()?;

			let create = RemoteAssetsCall::ForceCreate {
				id: remote_asset_id.clone(),
				owner: MultiAddress::Id(owner),
				is_sufficient,
				min_balance,
			};

			let deposit = T::ForwardDeposit::get();
			<T as Config>::Currency::hold(&HoldReason::ForwardDeposit.into(), &who, deposit)?;

			let message = Self::build_remote_xcm(&create);
			let message_id = Self::send_remote_xcm(origin, message)?;

			ForwardedAssets::<T>::insert(
				&id,
				ForwardInfo { depositor: who.clone(), deposit, min_balance, is_sufficient },
			);
			Self::deposit_event(Event::AssetForwarded {
				asset_id: id,
				remote_asset_id,
				depositor: who,
				deposit,
				min_balance,
				is_sufficient,
				message_id,
			});
			Ok(())
		}

		/// Re-send the minimum balance and sufficiency of a forwarded asset after they changed. The
		/// caller pays the XCM delivery fees. Also resets the replica's team to this pallet's
		/// sovereign account and unfreezes it.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::sync_asset_status())]
		pub fn sync_asset_status(origin: OriginFor<T>, id: AssetIdOf<T>) -> DispatchResult {
			let who = ensure_signed(origin.clone())?;
			let mut record = ForwardedAssets::<T>::get(&id).ok_or(Error::<T>::NotForwarded)?;
			let (min_balance, is_sufficient) = Self::asset_status(&id, &who)?;
			ensure!(
				record.min_balance != min_balance || record.is_sufficient != is_sufficient,
				Error::<T>::StatusUnchanged
			);

			let remote_asset_id = Self::remote_asset_id(id.clone())?;
			let owner = Self::remote_owner_account()?;

			let status = RemoteAssetsCall::ForceAssetStatus {
				id: remote_asset_id,
				owner: MultiAddress::Id(owner.clone()),
				issuer: MultiAddress::Id(owner.clone()),
				admin: MultiAddress::Id(owner.clone()),
				freezer: MultiAddress::Id(owner),
				min_balance,
				is_sufficient,
				is_frozen: false,
			};

			let message = Self::build_remote_xcm(&status);
			let message_id = Self::send_remote_xcm(origin, message)?;

			record.min_balance = min_balance;
			record.is_sufficient = is_sufficient;
			ForwardedAssets::<T>::insert(&id, record);

			Self::deposit_event(Event::AssetStatusSynced {
				asset_id: id,
				min_balance,
				is_sufficient,
				message_id,
			});
			Ok(())
		}

		/// Remove the [`ForwardedAssets`] entry of asset `id` and release its deposit. Requires
		/// [`Config::ManagerOrigin`]. The replica on the destination is untouched, so forwarding
		/// the asset again fails there while it exists.
		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::remove_forwarded_asset())]
		pub fn remove_forwarded_asset(origin: OriginFor<T>, id: AssetIdOf<T>) -> DispatchResult {
			T::ManagerOrigin::ensure_origin(origin)?;
			let record = ForwardedAssets::<T>::take(&id).ok_or(Error::<T>::NotForwarded)?;

			let released = <T as Config>::Currency::release(
				&HoldReason::ForwardDeposit.into(),
				&record.depositor,
				record.deposit,
				Precision::BestEffort,
			)?;

			Self::deposit_event(Event::ForwardRemoved {
				asset_id: id,
				depositor: record.depositor,
				released,
			});
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// This pallet's interior location, `PalletInstance(<pallet index>)`.
		pub fn pallet_location() -> InteriorLocation {
			let index = <Self as frame_support::traits::PalletInfoAccess>::index() as u8;
			[Junction::PalletInstance(index)].into()
		}

		/// Minimum balance and sufficiency of `id`. Fails for unknown and destroying assets, both
		/// of which `can_deposit` reports as `UnknownAsset`.
		fn asset_status(
			id: &AssetIdOf<T>,
			who: &T::AccountId,
		) -> Result<(AssetBalanceOf<T>, bool), Error<T>> {
			if let DepositConsequence::UnknownAsset =
				T::Assets::can_deposit(id.clone(), who, Zero::zero(), Provenance::Extant)
			{
				return Err(Error::<T>::UnknownAsset);
			}
			Ok((T::Assets::minimum_balance(id.clone()), T::Assets::is_sufficient(id.clone())))
		}

		/// Location of `asset_id` from the destination's perspective. Fails for locations interior
		/// to the destination, which name its own assets.
		pub fn remote_asset_id(asset_id: AssetIdOf<T>) -> Result<Location, Error<T>> {
			let remote = T::AssetIdToLocation::convert_back(&asset_id)
				.ok_or(Error::<T>::InvalidAssetLocation)?
				.reanchored(&T::Destination::get(), &T::UniversalLocation::get())
				.map_err(|_| Error::<T>::InvalidAssetLocation)?;
			ensure!(remote.parent_count() != 0, Error::<T>::InvalidAssetLocation);
			Ok(remote)
		}

		/// This pallet's sovereign account on the destination, which owns the replicas.
		pub fn remote_owner_account() -> Result<T::AccountId, Error<T>> {
			let pallet_location: Location = Self::pallet_location().into();
			let reanchored = pallet_location
				.reanchored(&T::Destination::get(), &T::UniversalLocation::get())
				.map_err(|_| Error::<T>::InvalidAssetLocation)?;
			T::DestinationAccountOf::convert_location(&reanchored)
				.ok_or(Error::<T>::LocationConversionFailed)
		}

		/// Builds the program executed on the destination.
		fn build_remote_xcm(call: &RemoteAssetsCall<T::AccountId, AssetBalanceOf<T>>) -> Xcm<()> {
			let encoded = (T::RemoteAssetsPalletIndex::get(), call).encode();
			Xcm(vec![
				xcm::v5::Instruction::UnpaidExecution {
					weight_limit: WeightLimit::Unlimited,
					check_origin: None,
				},
				xcm::v5::Instruction::DescendOrigin(Self::pallet_location()),
				xcm::v5::Instruction::Transact {
					origin_kind: OriginKind::Xcm,
					fallback_max_weight: None,
					call: encoded.into(),
				},
				xcm::v5::Instruction::ExpectTransactStatus(MaybeErrorCode::Success),
			])
		}

		/// Delivers `message` to the destination, charging the delivery fees to `origin` unless
		/// the fee manager waives them.
		fn send_remote_xcm(
			origin: T::RuntimeOrigin,
			message: Xcm<()>,
		) -> Result<XcmHash, DispatchError> {
			let fee_payer = T::OriginToLocation::try_convert(origin)
				.map_err(|_| Error::<T>::LocationConversionFailed)?;
			let (ticket, price) = validate_send::<T::XcmSender>(T::Destination::get(), message)
				.map_err(|_| Error::<T>::SendFailed)?;
			if !<T::XcmExecutor as FeeManager>::is_waived(Some(&fee_payer), FeeReason::ChargeFees) {
				T::XcmExecutor::charge_fees(fee_payer, price)
					.map_err(|_| Error::<T>::FeesNotPaid)?;
			}
			let message_id = T::XcmSender::deliver(ticket).map_err(|_| Error::<T>::SendFailed)?;
			Ok(message_id)
		}
	}
}
