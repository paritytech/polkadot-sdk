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
//! Reward assets pending distribution are held in an account derived from the Pallet's ID.
//! This pool should be adequately funded to ensure there are enough funds to make good on staker
//! claims.
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
use sp_runtime::{DispatchError, Saturating};
use sp_std::boxed::Box;

/// Unique identifier for a staking pool. (staking_asset, reward_asset).
pub type PoolId<AssetId> = (AssetId, AssetId);

/// Information on a user currently staking in a pool.
#[derive(Decode, Encode, MaxEncodedLen, TypeInfo)]
pub struct PoolStakerInfo<Balance> {
	amount: Balance,
	rewards: Balance,
	reward_debt: Balance,
}

/// Staking pool.
#[derive(Decode, Encode, MaxEncodedLen, TypeInfo)]
pub struct PoolInfo<Balance, BlockNumber> {
	reward_rate_per_block: Balance,
	total_tokens_staked: Balance,
	accumulated_rewards_per_share: Balance,
	last_rewarded_block: BlockNumber,
}

#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;
	use sp_runtime::traits::AccountIdConversion;

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
		type AssetId: Member + Parameter + Clone + MaybeSerializeDeserialize + MaxEncodedLen;

		/// The type in which the assets are measured.
		type Balance: Balance + TypeInfo;

		/// Registry of assets that can be configured to either stake for rewards, or be offered as
		/// rewards for staking.
		type Assets: Inspect<Self::AccountId, AssetId = Self::AssetId, Balance = Self::Balance>
			+ Mutate<Self::AccountId>
			+ Balanced<Self::AccountId>;
	}

	/// State of stakers in each pool.
	#[pallet::storage]
	pub type PoolStakers<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		PoolId<T::AssetId>,
		Blake2_128Concat,
		T::AccountId,
		PoolStakerInfo<T::Balance>,
	>;

	/// State and configuraiton of each staking pool.
	#[pallet::storage]
	pub type Pools<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		PoolId<T::AssetId>,
		PoolInfo<T::Balance, BlockNumberFor<T>>,
	>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An account staked some tokens in a pool.
		Staked {
			/// The account.
			who: T::AccountId,
			/// The pool.
			pool_id: PoolId<T::AssetId>,
			/// The amount.
			amount: T::Balance,
		},
		/// An account unstaked some tokens from a pool.
		Unstaked {
			/// The account.
			who: T::AccountId,
			/// The pool.
			pool_id: PoolId<T::AssetId>,
			/// The amount.
			amount: T::Balance,
		},
		/// An account harvested some rewards.
		RewardsHarvested {
			/// The account.
			who: T::AccountId,
			/// The pool.
			pool_id: PoolId<T::AssetId>,
			/// The rewarded tokens.
			amount: T::Balance,
		},
		/// A new reward pool was created.
		PoolCreated {
			/// The pool.
			pool_id: PoolId<T::AssetId>,
			/// The initial reward rate per block.
			reward_rate_per_block: T::Balance,
		},
		/// A reward pool was deleted.
		PoolDeleted {
			/// The pool.
			pool_id: PoolId<T::AssetId>,
		},
		/// A pool reward rate was been changed.
		PoolRewardRateChanged {
			/// The pool with the changed reward rate.
			pool_id: PoolId<T::AssetId>,
			/// The new reward rate of the reward rate per block distributed to stakers.
			reward_rate_per_block: T::Balance,
		},
		/// Funds were withdrawn from the Reward Pool.
		RewardPoolWithdrawal {
			/// The asset withdrawn.
			reward_asset_id: T::AssetId,
			/// The caller.
			caller: T::AccountId,
			/// The acount of reward asset withdrawn.
			amount: T::Balance,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// TODO
		TODO,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn integrity_test() {
			todo!()
		}
	}

	/// Pallet's callable functions.
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Create a new reward pool.
		pub fn create_pool(
			_origin: OriginFor<T>,
			_staked_asset_id: Box<T::AssetId>,
			_reward_asset_id: Box<T::AssetId>,
		) -> DispatchResult {
			todo!()
		}

		/// Removes an existing reward pool.
		///
		/// TODO decide how to manage clean up of stakers from a removed pool
		pub fn remove_pool(
			_origin: OriginFor<T>,
			_staked_asset_id: Box<T::AssetId>,
			_reward_asset_id: Box<T::AssetId>,
		) -> DispatchResult {
			todo!()
		}

		/// Stake tokens in a pool.
		pub fn stake(
			_origin: OriginFor<T>,
			_staked_asset_id: Box<T::AssetId>,
			_reward_asset_id: Box<T::AssetId>,
			_amount: T::Balance,
		) -> DispatchResult {
			todo!()
		}

		/// Unstake tokens from a pool.
		pub fn unstake(
			_origin: OriginFor<T>,
			_staked_asset_id: Box<T::AssetId>,
			_reward_asset_id: Box<T::AssetId>,
			_amount: T::Balance,
		) -> DispatchResult {
			todo!()
		}

		/// Harvest unclaimed pool rewards.
		pub fn harvest_rewards(
			_origin: OriginFor<T>,
			_staked_asset_id: Box<T::AssetId>,
			_reward_asset_id: Box<T::AssetId>,
		) -> DispatchResult {
			todo!()
		}

		/// Modify the reward rate of a pool.
		pub fn modify_pool(
			_origin: OriginFor<T>,
			_staked_asset_id: Box<T::AssetId>,
			_reward_asset_id: Box<T::AssetId>,
		) -> DispatchResult {
			todo!()
		}
	}

	impl<T: Config> Pallet<T> {
		/// The account ID of the reward pot.
		fn reward_pool_account_id() -> T::AccountId {
			T::PalletId::get().into_account_truncating()
		}

		/// Update pool state in preparation for reward harvesting.
		fn update_pool_rewards(_staked_asset_id: T::AssetId, _reward_asset_id: T::AssetId) {
			todo!()
		}
	}
}
