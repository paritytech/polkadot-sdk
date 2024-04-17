// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//  http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! # Asset Conversion Operations Suite.
//!
//! This pallet provides operational functionalities for the Asset Conversion pallet,
//! allowing you to perform various migration and one-time-use operations. These operations
//! are designed to facilitate updates and changes to the Asset Conversion pallet without
//! breaking its API.
//!
//! ## Overview
//!
//! This suite allows you to perform the following operations:
//! - Perform migration to update account ID derivation methods for existing pools. The migration
//!   operation ensures that the required accounts are created, existing account deposits are
//!   transferred, and liquidity is moved to the new accounts.

#![deny(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
pub mod weights;
pub use pallet::*;
pub use weights::WeightInfo;

use frame_support::traits::{
	fungible::{Inspect as FungibleInspect, Mutate as FungibleMutate},
	fungibles::{roles::ResetTeam, Inspect, Mutate, Refund},
	tokens::{Fortitude, Precision, Preservation},
	AccountTouch,
};
use pallet_asset_conversion::{PoolLocator, Pools};
use sp_runtime::traits::{TryConvert, Zero};
use sp_std::boxed::Box;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config:
		pallet_asset_conversion::Config<
			PoolId = (
				<Self as pallet_asset_conversion::Config>::AssetKind,
				<Self as pallet_asset_conversion::Config>::AssetKind,
			),
		> + frame_system::Config
	{
		/// Overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Type previously used to derive the account ID for a pool. Indicates that the pool's
		/// liquidity assets are located at this account before the migration.
		type PriorAccountIdConverter: for<'a> TryConvert<
			&'a (Self::AssetKind, Self::AssetKind),
			Self::AccountId,
		>;

		/// Retrieves information about an existing deposit for a given account ID and asset from
		/// the [`pallet_asset_conversion::Config::Assets`] registry and can initiate the refund.
		type AssetsRefund: Refund<
			Self::AccountId,
			AssetId = Self::AssetKind,
			Balance = <Self::DepositAsset as FungibleInspect<Self::AccountId>>::Balance,
		>;

		/// Retrieves information about an existing deposit for a given account ID and asset from
		/// the [`pallet_asset_conversion::Config::PoolAssets`] registry and can initiate the
		/// refund.
		type PoolAssetsRefund: Refund<
			Self::AccountId,
			AssetId = Self::PoolAssetId,
			Balance = <Self::DepositAsset as FungibleInspect<Self::AccountId>>::Balance,
		>;

		/// Means to reset the team for assets from the
		/// [`pallet_asset_conversion::Config::PoolAssets`] registry.
		type PoolAssetsTeam: ResetTeam<Self::AccountId, AssetId = Self::PoolAssetId>;

		/// Registry of an asset used as an account deposit for the
		/// [`pallet_asset_conversion::Config::Assets`] and
		/// [`pallet_asset_conversion::Config::PoolAssets`] registries.
		type DepositAsset: FungibleMutate<Self::AccountId>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	// Pallet's events.
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Indicates that a pool has been migrated to the new account ID.
		MigratedToNewAccount {
			/// Pool's ID.
			pool_id: T::PoolId,
			/// Pool's prior account ID.
			prior_account: T::AccountId,
			/// Pool's new account ID.
			new_account: T::AccountId,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Provided asset pair is not supported for pool.
		InvalidAssetPair,
		/// The pool doesn't exist.
		PoolNotFound,
		/// Pool's balance cannot be zero.
		ZeroBalance,
		/// Indicates a partial transfer of balance to the new account during a migration.
		PartialTransfer,
	}

	/// Pallet's callable functions.
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Migrates an existing pool to a new account ID derivation method for a given asset pair.
		/// If the migration is successful, transaction fees are refunded to the caller.
		///
		/// Must be signed.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::migrate_to_new_account())]
		pub fn migrate_to_new_account(
			origin: OriginFor<T>,
			asset1: Box<T::AssetKind>,
			asset2: Box<T::AssetKind>,
		) -> DispatchResultWithPostInfo {
			let _ = ensure_signed(origin)?;

			let pool_id = T::PoolLocator::pool_id(&asset1, &asset2)
				.map_err(|_| Error::<T>::InvalidAssetPair)?;
			let info = Pools::<T>::get(&pool_id).ok_or(Error::<T>::PoolNotFound)?;

			let (prior_account, new_account) =
				Self::addresses(&pool_id).ok_or(Error::<T>::InvalidAssetPair)?;

			let (asset1, asset2) = pool_id.clone();

			// Assets that must be transferred to the new account id.
			let balance1 = T::Assets::total_balance(asset1.clone(), &prior_account);
			let balance2 = T::Assets::total_balance(asset2.clone(), &prior_account);
			let lp_balance = T::PoolAssets::total_balance(info.lp_token.clone(), &prior_account);

			ensure!(!balance1.is_zero(), Error::<T>::ZeroBalance);
			ensure!(!balance2.is_zero(), Error::<T>::ZeroBalance);
			ensure!(!lp_balance.is_zero(), Error::<T>::ZeroBalance);

			// Check if a deposit needs to be placed for the new account. If so, mint the
			// required deposit amount to the depositor's account to ensure the deposit can be
			// provided. Once the deposit from the prior account is returned, the minted assets will
			// be burned. Touching the new account is necessary because it's not possible to
			// transfer assets to the new account if it's required. Additionally, the deposit cannot
			// be refunded from the prior account until its balance is zero.

			let deposit_asset_ed = T::DepositAsset::minimum_balance();

			if let Some((depositor, deposit)) =
				T::AssetsRefund::deposit_held(asset1.clone(), prior_account.clone())
			{
				T::DepositAsset::mint_into(&depositor, deposit + deposit_asset_ed)?;
				T::Assets::touch(asset1.clone(), &new_account, &depositor)?;
			}

			if let Some((depositor, deposit)) =
				T::AssetsRefund::deposit_held(asset2.clone(), prior_account.clone())
			{
				T::DepositAsset::mint_into(&depositor, deposit + deposit_asset_ed)?;
				T::Assets::touch(asset2.clone(), &new_account, &depositor)?;
			}

			if let Some((depositor, deposit)) =
				T::PoolAssetsRefund::deposit_held(info.lp_token.clone(), prior_account.clone())
			{
				T::DepositAsset::mint_into(&depositor, deposit + deposit_asset_ed)?;
				T::PoolAssets::touch(info.lp_token.clone(), &new_account, &depositor)?;
			}

			// Transfer all pool related assets to the new account.

			ensure!(
				balance1 ==
					T::Assets::transfer(
						asset1.clone(),
						&prior_account,
						&new_account,
						balance1,
						Preservation::Expendable,
					)?,
				Error::<T>::PartialTransfer
			);

			ensure!(
				balance2 ==
					T::Assets::transfer(
						asset2.clone(),
						&prior_account,
						&new_account,
						balance2,
						Preservation::Expendable,
					)?,
				Error::<T>::PartialTransfer
			);

			ensure!(
				lp_balance ==
					T::PoolAssets::transfer(
						info.lp_token.clone(),
						&prior_account,
						&new_account,
						lp_balance,
						Preservation::Expendable,
					)?,
				Error::<T>::PartialTransfer
			);

			// Refund deposits from prior accounts and burn previously minted assets.

			if let Some((depositor, deposit)) =
				T::AssetsRefund::deposit_held(asset1.clone(), prior_account.clone())
			{
				T::AssetsRefund::refund(asset1.clone(), prior_account.clone())?;
				T::DepositAsset::burn_from(
					&depositor,
					deposit + deposit_asset_ed,
					Precision::Exact,
					Fortitude::Force,
				)?;
			}

			if let Some((depositor, deposit)) =
				T::AssetsRefund::deposit_held(asset2.clone(), prior_account.clone())
			{
				T::AssetsRefund::refund(asset2.clone(), prior_account.clone())?;
				T::DepositAsset::burn_from(
					&depositor,
					deposit + deposit_asset_ed,
					Precision::Exact,
					Fortitude::Force,
				)?;
			}

			if let Some((depositor, deposit)) =
				T::PoolAssetsRefund::deposit_held(info.lp_token.clone(), prior_account.clone())
			{
				T::PoolAssetsRefund::refund(info.lp_token.clone(), prior_account.clone())?;
				T::DepositAsset::burn_from(
					&depositor,
					deposit + deposit_asset_ed,
					Precision::Exact,
					Fortitude::Force,
				)?;
			}

			T::PoolAssetsTeam::reset_team(
				info.lp_token,
				new_account.clone(),
				new_account.clone(),
				new_account.clone(),
				new_account.clone(),
			)?;

			Self::deposit_event(Event::MigratedToNewAccount {
				pool_id,
				prior_account,
				new_account,
			});

			Ok(Pays::No.into())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Returns the prior and new account IDs for a given pool ID. The prior account ID comes
		/// first in the tuple.
		#[cfg(not(any(test, feature = "runtime-benchmarks")))]
		fn addresses(pool_id: &T::PoolId) -> Option<(T::AccountId, T::AccountId)> {
			match (
				T::PriorAccountIdConverter::try_convert(pool_id),
				T::PoolLocator::address(pool_id),
			) {
				(Ok(a), Ok(b)) if a != b => Some((a, b)),
				_ => None,
			}
		}

		/// Returns the prior and new account IDs for a given pool ID. The prior account ID comes
		/// first in the tuple.
		///
		/// This function is intended for use only in test and benchmark environments. The prior
		/// account ID represents the new account ID from [`Config::PoolLocator`], allowing the use
		/// of the main pallet's calls to set up a pool with liquidity placed in that account and
		/// migrate it to another account, which in this case is the result of
		/// [`Config::PriorAccountIdConverter`].
		#[cfg(any(test, feature = "runtime-benchmarks"))]
		pub(crate) fn addresses(pool_id: &T::PoolId) -> Option<(T::AccountId, T::AccountId)> {
			match (
				T::PoolLocator::address(pool_id),
				T::PriorAccountIdConverter::try_convert(pool_id),
			) {
				(Ok(a), Ok(b)) if a != b => Some((a, b)),
				_ => None,
			}
		}
	}
}
