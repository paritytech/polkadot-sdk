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

//! # FRAME Staking Rewards Pallet
//!
//! Allows rewarding fungible token holders.
//!
//! ## Overview
//!
//! Governance can create a new incentive program for a fungible asset by creating a new pool.
//!
//! When creating the pool, governance specifies a 'staking asset', 'reward asset', and 'reward rate
//! per block'.
//!
//! Once the pool is created, holders of the 'staking asset' can stake them in this pallet (creating
//! a new Freeze). Once staked, the staker begins accumulating the right to claim the 'reward asset'
//! each block, proportional to their share of the total staked tokens in the pool.
//!
//! Reward assets pending distribution are held in an account derived from the pallet ID and a
//! unique pool ID.
//!
//! Care should be taken to keep pool accounts adequately funded with the reward asset.
//!
//! ## Permissioning
//!
//! Currently, pool creation and management is permissioned and restricted to a configured Origin.
//!
//! Future iterations of this pallet may allow permissionless creation and management of pools.
//!
//! ## Implementation Notes
//!
//! The implementation is based on the [AccumulatedRewardsPerShare](https://dev.to/heymarkkop/understanding-sushiswaps-masterchef-staking-rewards-1m6f) algorithm.
//!
//! Rewards are calculated JIT (just-in-time), when a staker claims their rewards.
//!
//! All operations are O(1), allowing the approach to scale to an arbitrary amount of pools and
//! stakers.
#![deny(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode, MaxEncodedLen};
use frame_system::pallet_prelude::BlockNumberFor;
pub use pallet::*;

use frame_support::{
	traits::{
		fungibles::{Balanced, Inspect, Mutate},
		tokens::Balance,
	},
	PalletId,
};
use scale_info::TypeInfo;
use sp_core::Get;
use sp_runtime::DispatchError;
use sp_std::boxed::Box;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

/// The type of the unique id for each pool.
pub type PoolId = u32;

/// Multiplier to maintain precision when calculating rewards.
pub(crate) const PRECISION_SCALING_FACTOR: u32 = u32::MAX;

/// A pool staker.
#[derive(Default, Decode, Encode, MaxEncodedLen, TypeInfo)]
pub struct PoolStakerInfo<Balance> {
	/// Amount of tokens staked.
	amount: Balance,
	/// Accumulated, unpaid rewards.
	rewards: Balance,
	/// Reward per token value at the time of the staker's last interaction with the contract.
	reward_per_token_paid: Balance,
}

/// A staking pool.
#[derive(Debug, Decode, Encode, Default, PartialEq, Eq, MaxEncodedLen, TypeInfo)]
pub struct PoolInfo<AccountId, AssetId, Balance, BlockNumber> {
	/// The asset that is staked in this pool.
	staking_asset_id: AssetId,
	/// The asset that is distributed as rewards in this pool.
	reward_asset_id: AssetId,
	/// The amount of tokens distributed per block.
	reward_rate_per_block: Balance,
	/// The total amount of tokens staked in this pool.
	total_tokens_staked: Balance,
	/// Total rewards accumulated per token, up to the last time the rewards were updated.
	reward_per_token_stored: Balance,
	/// Last block number the pool was updated. Used when calculating payouts.
	last_update_block: BlockNumber,
	/// Permissioned account that can manage this pool.
	admin: AccountId,
}

#[frame_support::pallet(dev_mode)]
pub mod pallet {

	use super::*;
	use frame_support::{
		pallet_prelude::*,
		traits::tokens::{AssetId, Preservation},
	};
	use frame_system::pallet_prelude::*;
	use sp_runtime::traits::{AccountIdConversion, EnsureDiv, Saturating};

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The pallet's id, used for deriving its sovereign account ID.
		#[pallet::constant]
		type PalletId: Get<PalletId>;

		/// Identifier for each type of asset.
		type AssetId: AssetId + Member + Parameter;

		/// The type in which the assets are measured.
		type Balance: Balance + TypeInfo;

		/// The origin with permission to create pools. This will be removed in a later release of
		/// this pallet, which will allow permissionless pool creation.
		type PermissionedPoolCreator: EnsureOrigin<Self::RuntimeOrigin>;

		/// Registry of assets that can be configured to either stake for rewards, or be offered as
		/// rewards for staking.
		type Assets: Inspect<Self::AccountId, AssetId = Self::AssetId, Balance = Self::Balance>
			+ Mutate<Self::AccountId>
			+ Balanced<Self::AccountId>;
	}

	/// State of pool stakers.
	#[pallet::storage]
	pub type PoolStakers<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		PoolId,
		Blake2_128Concat,
		T::AccountId,
		PoolStakerInfo<T::Balance>,
	>;

	/// State and configuraiton of each staking pool.
	#[pallet::storage]
	pub type Pools<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		PoolId,
		PoolInfo<T::AccountId, T::AssetId, T::Balance, BlockNumberFor<T>>,
	>;

	/// Stores the [`PoolId`] to use for the next pool.
	///
	/// Incremented when a new pool is created.
	#[pallet::storage]
	pub type NextPoolId<T: Config> = StorageValue<_, PoolId, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An account staked some tokens in a pool.
		Staked {
			/// The account that staked assets.
			who: T::AccountId,
			/// The pool.
			pool_id: PoolId,
			/// The staked asset amount.
			amount: T::Balance,
		},
		/// An account unstaked some tokens from a pool.
		Unstaked {
			/// The account that unstaked assets.
			who: T::AccountId,
			/// The pool.
			pool_id: PoolId,
			/// The unstaked asset amount.
			amount: T::Balance,
		},
		/// An account harvested some rewards.
		RewardsHarvested {
			/// The extrinsic caller.
			who: T::AccountId,
			/// The staker whos rewards were harvested.
			staker: T::AccountId,
			/// The pool.
			pool_id: PoolId,
			/// The amount of harvested tokens.
			amount: T::Balance,
		},
		/// A new reward pool was created.
		PoolCreated {
			/// The account that created the pool.
			creator: T::AccountId,
			/// Unique ID for the new pool.
			pool_id: PoolId,
			/// The staking asset.
			staking_asset_id: T::AssetId,
			/// The reward asset.
			reward_asset_id: T::AssetId,
			/// The initial reward rate per block.
			reward_rate_per_block: T::Balance,
			/// The account allowed to modify the pool.
			admin: T::AccountId,
		},
		/// A reward pool was deleted by the admin.
		PoolDeleted {
			/// The deleted pool id.
			pool_id: PoolId,
		},
		/// A pool was modified by the admin.
		PoolModifed {
			/// The modified pool.
			pool_id: PoolId,
			/// The new reward rate.
			new_reward_rate_per_block: T::Balance,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The staker does not have enough tokens to perform the operation.
		NotEnoughTokens,
		/// An operation was attempted on a non-existent pool.
		NonExistentPool,
		/// An operation was attempted using a non-existent asset.
		NonExistentAsset,
		/// There was an error converting a block number.
		BlockNumberConversionError,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn integrity_test() {
			// TODO: Proper implementation
		}
	}

	/// Pallet's callable functions.
	///
	/// Allows optionally specifying an admin account for the pool. By default, the origin is made
	/// admin.
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Create a new reward pool.
		pub fn create_pool(
			origin: OriginFor<T>,
			staked_asset_id: Box<T::AssetId>,
			reward_asset_id: Box<T::AssetId>,
			reward_rate_per_block: T::Balance,
			admin: Option<T::AccountId>,
		) -> DispatchResult {
			// Ensure Origin is allowed to create pools.
			T::PermissionedPoolCreator::ensure_origin(origin.clone())?;

			// Ensure the assets exist.
			ensure!(
				T::Assets::asset_exists(*staked_asset_id.clone()),
				Error::<T>::NonExistentAsset
			);
			ensure!(
				T::Assets::asset_exists(*reward_asset_id.clone()),
				Error::<T>::NonExistentAsset
			);

			// Get the admin, defaulting to the origin.
			let origin_acc_id = ensure_signed(origin)?;
			let admin = match admin {
				Some(admin) => admin,
				None => origin_acc_id.clone(),
			};

			// Create the pool.
			let pool = PoolInfo::<T::AccountId, T::AssetId, T::Balance, BlockNumberFor<T>> {
				staking_asset_id: *staked_asset_id.clone(),
				reward_asset_id: *reward_asset_id.clone(),
				reward_rate_per_block,
				total_tokens_staked: 0u32.into(),
				reward_per_token_stored: 0u32.into(),
				last_update_block: 0u32.into(),
				admin: admin.clone(),
			};

			// Insert it into storage.
			let pool_id = NextPoolId::<T>::get();
			Pools::<T>::insert(pool_id, pool);
			NextPoolId::<T>::put(pool_id.saturating_add(1));

			// Emit created event.
			Self::deposit_event(Event::PoolCreated {
				creator: origin_acc_id,
				pool_id,
				staking_asset_id: *staked_asset_id,
				reward_asset_id: *reward_asset_id,
				reward_rate_per_block,
				admin,
			});

			Ok(())
		}

		/// Removes an existing reward pool.
		///
		/// TODO decide how to manage clean up of stakers from a removed pool.
		pub fn remove_pool(_origin: OriginFor<T>, _pool_id: PoolId) -> DispatchResult {
			todo!()
		}

		/// Stake tokens in a pool.
		pub fn stake(origin: OriginFor<T>, pool_id: PoolId, amount: T::Balance) -> DispatchResult {
			let caller = ensure_signed(origin)?;

			// Always start by updating the pool rewards.
			Self::update_pool_rewards(&pool_id, &caller)?;

			// Try to freeze the staker assets.
			// TODO: (blocked https://github.com/paritytech/polkadot-sdk/issues/3342)

			// Update Pools.
			let mut pool = Pools::<T>::get(pool_id).ok_or(Error::<T>::NonExistentPool)?;
			pool.total_tokens_staked.saturating_accrue(amount);
			Pools::<T>::insert(pool_id, pool);

			// Update PoolStakers.
			let mut staker = PoolStakers::<T>::get(pool_id, &caller).unwrap_or_default();
			staker.amount.saturating_accrue(amount);
			PoolStakers::<T>::insert(pool_id, &caller, staker);

			Ok(())
		}

		/// Unstake tokens from a pool.
		pub fn unstake(
			origin: OriginFor<T>,
			pool_id: PoolId,
			amount: T::Balance,
		) -> DispatchResult {
			let caller = ensure_signed(origin)?;

			// Always start by updating the pool rewards.
			Self::update_pool_rewards(&pool_id, &caller)?;

			// Check the staker has enough staked tokens.
			let mut staker = PoolStakers::<T>::get(pool_id, &caller).unwrap_or_default();
			ensure!(staker.amount >= amount, Error::<T>::NotEnoughTokens);

			// Unfreeze staker assets.
			// TODO: (blocked https://github.com/paritytech/polkadot-sdk/issues/3342)

			// Update Pools.
			let mut pool = Pools::<T>::get(pool_id).ok_or(Error::<T>::NonExistentPool)?;
			pool.total_tokens_staked.saturating_reduce(amount);
			Pools::<T>::insert(pool_id, pool);

			// Update PoolStakers.
			staker.amount.saturating_reduce(amount);
			PoolStakers::<T>::insert(pool_id, &caller, staker);

			Ok(())
		}

		/// Harvest unclaimed pool rewards for a staker.
		pub fn harvest_rewards(
			origin: OriginFor<T>,
			pool_id: PoolId,
			staker: Option<T::AccountId>,
		) -> DispatchResult {
			let caller = ensure_signed(origin)?;

			let staker = match staker {
				Some(staker) => staker,
				None => caller.clone(),
			};

			// Always start by updating the pool rewards.
			Self::update_pool_rewards(&pool_id, &staker)?;

			// Transfer unclaimed rewards from the pool to the staker.
			let mut staker_info = PoolStakers::<T>::get(pool_id, &caller).unwrap_or_default();
			let pool_info = Pools::<T>::get(pool_id).ok_or(Error::<T>::NonExistentPool)?;
			let pool_account_id = Self::pool_account_id(&pool_id)?;

			T::Assets::transfer(
				pool_info.reward_asset_id,
				&pool_account_id,
				&staker,
				staker_info.rewards,
				Preservation::Preserve,
			)?;

			// Reset staker unclaimed rewards.
			staker_info.rewards = 0u32.into();

			Ok(())
		}

		/// Modify the reward rate of a pool.
		pub fn modify_pool(
			_origin: OriginFor<T>,
			_pool_id: PoolId,
			_new_reward_rate: T::Balance,
		) -> DispatchResult {
			todo!()
		}

		/// Convinience method to deposit reward tokens into a pool.
		///
		/// This method is not strictly necessary (tokens could be transferred directly to the
		/// pool pot address), but is provided for convenience so manual derivation of the
		/// account id is not required.
		pub fn deposit_reward_tokens(
			origin: OriginFor<T>,
			pool_id: PoolId,
			amount: T::Balance,
		) -> DispatchResult {
			let caller = ensure_signed(origin)?;
			let pool_info = Pools::<T>::get(pool_id).ok_or(Error::<T>::NonExistentPool)?;
			let pool_account_id = Self::pool_account_id(&pool_id)?;
			T::Assets::transfer(
				pool_info.reward_asset_id,
				&caller,
				&pool_account_id,
				amount,
				Preservation::Preserve,
			)?;
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Derive a pool account ID from the pallet's ID.
		fn pool_account_id(id: &PoolId) -> Result<T::AccountId, DispatchError> {
			if Pools::<T>::contains_key(id) {
				Ok(T::PalletId::get().into_sub_account_truncating(id))
			} else {
				Err(Error::<T>::NonExistentPool.into())
			}
		}

		/// Update pool reward state.
		fn update_pool_rewards(pool_id: &PoolId, staker: &T::AccountId) -> DispatchResult {
			let reward_per_token = Self::reward_per_token(pool_id)?;

			let mut pool_info = Pools::<T>::get(pool_id).ok_or(Error::<T>::NonExistentPool)?;
			pool_info.last_update_block = frame_system::Pallet::<T>::block_number();
			Pools::<T>::insert(pool_id, pool_info);

			let mut staker_info = PoolStakers::<T>::get(pool_id, staker).unwrap_or_default();
			staker_info.rewards = Self::derive_rewards(pool_id, staker)?;
			staker_info.reward_per_token_paid = reward_per_token;
			PoolStakers::<T>::insert(pool_id, staker, staker_info);

			Ok(())
		}

		/// Derives the current reward per token for this pool.
		///
		/// Helper function for update_pool_rewards. Should not be called directly.
		fn reward_per_token(pool_id: &PoolId) -> Result<T::Balance, DispatchError> {
			let pool_info = Pools::<T>::get(pool_id).ok_or(Error::<T>::NonExistentPool)?;

			if pool_info.total_tokens_staked.eq(&0u32.into()) {
				return Ok(0u32.into());
			}

			let blocks_elapsed: u32 = match frame_system::Pallet::<T>::block_number()
				.saturating_sub(pool_info.last_update_block)
				.try_into()
			{
				Ok(b) => b,
				Err(_) => return Err(Error::<T>::BlockNumberConversionError.into()),
			};

			Ok(pool_info
				.reward_per_token_stored
				.saturating_add(
					pool_info
						.reward_rate_per_block
						.saturating_mul(blocks_elapsed.into())
						.saturating_mul(PRECISION_SCALING_FACTOR.into()),
				)
				.ensure_div(pool_info.total_tokens_staked)?)
		}

		/// Derives the amount of rewards earned by a staker.
		///
		/// Helper function for update_pool_rewards. Should not be called directly.
		fn derive_rewards(
			pool_id: &PoolId,
			staker: &T::AccountId,
		) -> Result<T::Balance, DispatchError> {
			let reward_per_token = Self::reward_per_token(pool_id)?;
			let staker_info = PoolStakers::<T>::get(pool_id, staker).unwrap_or_default();

			Ok(staker_info
				.amount
				.saturating_mul(reward_per_token.saturating_sub(staker_info.reward_per_token_paid))
				.ensure_div(PRECISION_SCALING_FACTOR.into())?
				.saturating_add(staker_info.rewards))
		}
	}
}
