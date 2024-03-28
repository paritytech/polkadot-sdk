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

/// A pool staker.
#[derive(Decode, Encode, MaxEncodedLen, TypeInfo)]
pub struct PoolStakerInfo<Balance> {
	amount: Balance,
	rewards: Balance,
	reward_debt: Balance,
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
	/// Total accumulated rewards per share. Used when calculating payouts.
	accumulated_rewards_per_share: Balance,
	/// Last block number the pool was updated. Used when calculating payouts.
	last_rewarded_block: BlockNumber,
	/// Permissioned account that can manage this pool.
	admin: AccountId,
}

#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use super::*;
	use frame_support::{pallet_prelude::*, traits::tokens::AssetId};
	use frame_system::pallet_prelude::*;
	use sp_runtime::traits::{AccountIdConversion, Saturating};

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
		/// An operation was attempted on a non-existent pool.
		NonExistentPool,
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

			// Get the admin, or try to use the origin as admin.
			let origin_acc_id = ensure_signed(origin)?;
			let admin = match admin {
				Some(admin) => admin,
				None => origin_acc_id,
			};

			// Create the pool.
			let pool = PoolInfo::<T::AccountId, T::AssetId, T::Balance, BlockNumberFor<T>> {
				staking_asset_id: *staked_asset_id.clone(),
				reward_asset_id: *reward_asset_id.clone(),
				reward_rate_per_block,
				total_tokens_staked: 0u32.into(),
				accumulated_rewards_per_share: 0u32.into(),
				last_rewarded_block: 0u32.into(),
				admin: admin.clone(),
			};

			// Insert the pool into storage.
			let pool_id = NextPoolId::<T>::get();
			Pools::<T>::insert(pool_id, pool);
			NextPoolId::<T>::put(pool_id.saturating_add(1));

			// Emit the event.
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
		pub fn stake(
			_origin: OriginFor<T>,
			_pool_id: PoolId,
			_amount: T::Balance,
		) -> DispatchResult {
			todo!()
		}

		/// Unstake tokens from a pool.
		pub fn unstake(
			_origin: OriginFor<T>,
			_pool_id: PoolId,
			_amount: T::Balance,
		) -> DispatchResult {
			todo!()
		}

		/// Harvest unclaimed pool rewards for a staker.
		pub fn harvest_rewards(
			_origin: OriginFor<T>,
			_staker: T::AccountId,
			_pool_id: PoolId,
		) -> DispatchResult {
			todo!()
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
			_origin: OriginFor<T>,
			_pool_id: PoolId,
			_amount: T::Balance,
		) -> DispatchResult {
			todo!()
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

		/// Update pool state in preparation for reward harvesting.
		fn update_pool_rewards(_staked_asset_id: T::AssetId, _reward_asset_id: T::AssetId) {
			todo!()
		}
	}
}
