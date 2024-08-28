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
//! Allows accounts to be rewarded for holding `fungible` asset/s, for example LP tokens.
//!
//! ## Overview
//!
//! Initiate an incentive program for a fungible asset by creating a new pool.
//!
//! During pool creation, a 'staking asset', 'reward asset', 'reward rate per block', 'expiry
//! block', and 'admin' are specified.
//!
//! Once created, holders of the 'staking asset' can 'stake' them in a corresponding pool, which
//! creates a Freeze on the asset.
//!
//! Once staked, rewards denominated in 'reward asset' begin accumulating to the staker,
//! proportional to their share of the total staked tokens in the pool.
//!
//! Reward assets pending distribution are held in an account unique to each pool.
//!
//! Care should be taken by the pool operator to keep pool accounts adequately funded with the
//! reward asset.
//!
//! The pool admin may adjust the pool configuration such as reward rate per block, expiry block,
//! and admin.
//!
//! ## Disambiguation
//!
//! While this pallet shares some terminology with the `staking-pool` and similar native staking
//! related pallets, it is distinct and is entirely unrelated to native staking.
//!
//! ## Permissioning
//!
//! Currently, pool creation and management restricted to a configured Origin.
//!
//! Future iterations of this pallet may allow permissionless creation and management of pools.
//!
//! Note: The permissioned origin must return an AccountId. This can be achieved for any Origin by
//! wrapping it with `EnsureSuccess`.
//!
//! ## Implementation Notes
//!
//! Internal logic functions such as `update_pool_and_staker_rewards` where deliberately written
//! without side-effects.
//!
//! Storage interaction such as reads and writes are instead all performed in the top level
//! pallet Call method, which while slightly more verbose, makes it easier to understand the
//! code and reason about how storage reads and writes occur in the pallet.
//!
//! ## Rewards Algorithm
//!
//! The rewards algorithm is based on the Synthetix [StakingRewards.sol](https://github.com/Synthetixio/synthetix/blob/develop/contracts/StakingRewards.sol)
//! smart contract.
//!
//! Rewards are calculated JIT (just-in-time), and all operations are O(1) making the approach
//! scalable to many pools and stakers.
//!
//! ### Resources
//!
//! - [This video series](https://www.youtube.com/watch?v=6ZO5aYg1GI8), which walks through the math
//!   of the algorithm.
//! - [This dev.to article](https://dev.to/heymarkkop/understanding-sushiswaps-masterchef-staking-rewards-1m6f),
//!   which explains the algorithm of the SushiSwap MasterChef staking. While not identical to the
//!   Synthetix approach, they are quite similar.
#![deny(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode, MaxEncodedLen};
use frame_system::pallet_prelude::BlockNumberFor;
pub use pallet::*;

use codec::Codec;
use frame_support::{
	traits::{
		fungibles::{Inspect, Mutate},
		tokens::Balance,
	},
	PalletId,
};
use scale_info::TypeInfo;
use sp_core::Get;
use sp_runtime::{
	traits::{CheckedAdd, MaybeDisplay, Zero},
	DispatchError,
};
use sp_std::boxed::Box;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
mod weights;

pub use weights::WeightInfo;

/// Unique id type for each pool.
pub type PoolId = u32;

/// Multiplier to maintain precision when calculating rewards.
pub(crate) const PRECISION_SCALING_FACTOR: u16 = 4096;

/// Convenience alias for `PoolInfo`.
pub type PoolInfoFor<T> = PoolInfo<
	<T as frame_system::Config>::AccountId,
	<T as Config>::AssetId,
	<T as Config>::Balance,
	BlockNumberFor<T>,
>;

/// The state of a staker in a pool.
#[derive(Debug, Default, Clone, Decode, Encode, MaxEncodedLen, TypeInfo)]
pub struct PoolStakerInfo<Balance> {
	/// Amount of tokens staked.
	amount: Balance,
	/// Accumulated, unpaid rewards.
	rewards: Balance,
	/// Reward per token value at the time of the staker's last interaction with the contract.
	reward_per_token_paid: Balance,
}

/// The state and configuration of an incentive pool.
#[derive(Debug, Clone, Decode, Encode, Default, PartialEq, Eq, MaxEncodedLen, TypeInfo)]
pub struct PoolInfo<AccountId, AssetId, Balance, BlockNumber> {
	/// The asset staked in this pool.
	staked_asset_id: AssetId,
	/// The asset distributed as rewards by this pool.
	reward_asset_id: AssetId,
	/// The amount of tokens rewarded per block.
	reward_rate_per_block: Balance,
	/// The block the pool will cease distributing rewards.
	expiry_block: BlockNumber,
	/// The account that can manage this pool.
	admin: AccountId,
	/// The total amount of tokens staked in this pool.
	total_tokens_staked: Balance,
	/// Total rewards accumulated per token, up to the `last_update_block`.
	reward_per_token_stored: Balance,
	/// Last block number the pool was updated.
	last_update_block: BlockNumber,
}

sp_api::decl_runtime_apis! {
	/// The runtime API for the asset rewards pallet.
	pub trait AssetRewards<Cost: MaybeDisplay + Codec> {
		/// Get the cost of creating a pool.
		///
		/// This is especially useful when the cost is dynamic.
		fn pool_creation_cost() -> Cost;
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{
		pallet_prelude::*,
		traits::{
			fungibles::MutateFreeze,
			tokens::{AssetId, Preservation},
			Consideration, Footprint,
		},
	};
	use frame_system::pallet_prelude::*;
	use sp_runtime::{
		traits::{AccountIdConversion, BadOrigin, EnsureDiv, Saturating},
		DispatchResult,
	};

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// A reason for the pallet placing a hold on funds.
	#[pallet::composite_enum]
	pub enum FreezeReason {
		/// Funds are staked in the pallet.
		#[codec(index = 0)]
		Staked,
	}

	/// A reason for the pallet placing a hold on funds.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// Cost associated with storing pool information on-chain.
		#[codec(index = 0)]
		PoolCreation,
	}

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The pallet's id, used for deriving pool account IDs.
		#[pallet::constant]
		type PalletId: Get<PalletId>;

		/// Identifier for each type of asset.
		type AssetId: AssetId + Member + Parameter;

		/// The type in which the assets are measured.
		type Balance: Balance + TypeInfo;

		/// The origin with permission to create pools.
		///
		/// The Origin must return an AccountId.
		type CreatePoolOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = Self::AccountId>;

		/// Registry of assets that can be configured to either stake for rewards, or be offered as
		/// rewards for staking.
		type Assets: Inspect<Self::AccountId, AssetId = Self::AssetId, Balance = Self::Balance>
			+ Mutate<Self::AccountId>;

		/// Freezer for the Assets.
		type AssetsFreezer: MutateFreeze<
			Self::AccountId,
			Id = Self::RuntimeFreezeReason,
			AssetId = Self::AssetId,
			Balance = Self::Balance,
		>;

		/// The overarching freeze reason.
		type RuntimeFreezeReason: From<FreezeReason>;

		/// Means for associating a cost with the on-chain storage of pool information, which
		/// is incurred by the pool creator.
		///
		/// The passed `Footprint` specifically accounts for the storage footprint of the pool's
		/// information itself, excluding any potential storage footprint related to the stakers.
		type Consideration: Consideration<Self::AccountId, Footprint>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;

		/// Helper for benchmarking.
		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper: benchmarking::BenchmarkHelper<Self::AssetId>;
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

	/// State and configuration of each staking pool.
	#[pallet::storage]
	pub type Pools<T: Config> = StorageMap<_, Blake2_128Concat, PoolId, PoolInfoFor<T>>;

	/// The cost associated with storing pool information on-chain which was incurred by the pool
	/// creator.
	///
	/// This cost may be [`None`], as determined by [`Config::Consideration`].
	#[pallet::storage]
	pub type PoolCost<T: Config> =
		StorageMap<_, Blake2_128Concat, PoolId, (T::AccountId, T::Consideration)>;

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
			caller: T::AccountId,
			/// The pool.
			pool_id: PoolId,
			/// The staked asset amount.
			amount: T::Balance,
		},
		/// An account unstaked some tokens from a pool.
		Unstaked {
			/// The account that unstaked assets.
			caller: T::AccountId,
			/// The pool.
			pool_id: PoolId,
			/// The unstaked asset amount.
			amount: T::Balance,
		},
		/// An account harvested some rewards.
		RewardsHarvested {
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
			/// The unique ID for the new pool.
			pool_id: PoolId,
			/// The staking asset.
			staked_asset_id: T::AssetId,
			/// The reward asset.
			reward_asset_id: T::AssetId,
			/// The initial reward rate per block.
			reward_rate_per_block: T::Balance,
			/// The block the pool will cease to accumulate rewards.
			expiry_block: BlockNumberFor<T>,
			/// The account allowed to modify the pool.
			admin: T::AccountId,
		},
		/// A pool reward rate was modified by the admin.
		PoolRewardRateModified {
			/// The modified pool.
			pool_id: PoolId,
			/// The new reward rate per block.
			new_reward_rate_per_block: T::Balance,
		},
		/// A pool admin was modified.
		PoolAdminModified {
			/// The modified pool.
			pool_id: PoolId,
			/// The new admin.
			new_admin: T::AccountId,
		},
		/// A pool expiry block was modified by the admin.
		PoolExpiryBlockModified {
			/// The modified pool.
			pool_id: PoolId,
			/// The new expiry block.
			new_expiry_block: BlockNumberFor<T>,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The staker does not have enough tokens to perform the operation.
		NotEnoughTokens,
		/// An operation was attempted on a non-existent pool.
		NonExistentPool,
		/// An operation was attempted for a non-existent staker.
		NonExistentStaker,
		/// An operation was attempted with a non-existent asset.
		NonExistentAsset,
		/// There was an error converting a block number.
		BlockNumberConversionError,
		/// The expiry block must be in the future.
		ExpiryBlockMustBeInTheFuture,
		/// An amount overflowed.
		Overflow,
		/// Insufficient funds to create the freeze.
		InsufficientFunds,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn integrity_test() {
			// The AccountId is at least 8 bytes to contain the unique PalletId.
			assert!(
				<frame_support::PalletId as AccountIdConversion<T::AccountId>>::try_into_account(
					&T::PalletId::get(),
				)
				.is_some()
			);
		}
	}

	/// Pallet's callable functions.
	#[pallet::call(weight(<T as Config>::WeightInfo))]
	impl<T: Config> Pallet<T> {
		/// Create a new reward pool.
		///
		/// If an explicity admin is not specified, it defaults to the caller.
		///
		/// The initial pool expiry will be calculated by summing `lifetime` with the block
		/// number at the time of execution.
		#[pallet::call_index(0)]
		pub fn create_pool(
			origin: OriginFor<T>,
			staked_asset_id: Box<T::AssetId>,
			reward_asset_id: Box<T::AssetId>,
			reward_rate_per_block: T::Balance,
			lifetime: BlockNumberFor<T>,
			admin: Option<T::AccountId>,
		) -> DispatchResult {
			// Check the origin.
			let creator = T::CreatePoolOrigin::ensure_origin(origin.clone())?;

			// Ensure the assets exist.
			ensure!(
				T::Assets::asset_exists(*staked_asset_id.clone()),
				Error::<T>::NonExistentAsset
			);
			ensure!(
				T::Assets::asset_exists(*reward_asset_id.clone()),
				Error::<T>::NonExistentAsset
			);

			// Check the expiry block.
			let expiry_block = frame_system::Pallet::<T>::block_number().saturating_add(lifetime);
			ensure!(
				expiry_block > frame_system::Pallet::<T>::block_number(),
				Error::<T>::ExpiryBlockMustBeInTheFuture
			);

			let pool_id = NextPoolId::<T>::get();

			let footprint = Self::pool_creation_footprint();
			let cost = T::Consideration::new(&creator, footprint)?;
			PoolCost::<T>::insert(pool_id, (creator.clone(), cost));

			// Get the admin, defaulting to the origin.
			let admin = admin.unwrap_or(creator.clone());

			// Create the pool.
			let pool = PoolInfoFor::<T> {
				staked_asset_id: *staked_asset_id.clone(),
				reward_asset_id: *reward_asset_id.clone(),
				reward_rate_per_block,
				total_tokens_staked: 0u32.into(),
				reward_per_token_stored: 0u32.into(),
				last_update_block: 0u32.into(),
				expiry_block,
				admin: admin.clone(),
			};

			// Insert it into storage.
			Pools::<T>::insert(pool_id, pool);

			NextPoolId::<T>::put(pool_id.checked_add(1).ok_or(Error::<T>::Overflow)?);

			// Emit created event.
			Self::deposit_event(Event::PoolCreated {
				creator,
				pool_id,
				staked_asset_id: *staked_asset_id,
				reward_asset_id: *reward_asset_id,
				reward_rate_per_block,
				expiry_block,
				admin,
			});

			Ok(())
		}

		/// Stake additional tokens in a pool.
		///
		/// A freeze is placed on the staked tokens.
		#[pallet::call_index(1)]
		pub fn stake(origin: OriginFor<T>, pool_id: PoolId, amount: T::Balance) -> DispatchResult {
			let caller = ensure_signed(origin)?;

			// Always start by updating staker and pool rewards.
			let pool_info = Pools::<T>::get(pool_id).ok_or(Error::<T>::NonExistentPool)?;
			let staker_info = PoolStakers::<T>::get(pool_id, &caller).unwrap_or_default();
			let (mut pool_info, mut staker_info) =
				Self::update_pool_and_staker_rewards(&pool_info, &staker_info)?;

			T::AssetsFreezer::increase_frozen(
				pool_info.staked_asset_id.clone(),
				&FreezeReason::Staked.into(),
				&caller,
				amount,
			)?;

			// Update Pools.
			pool_info.total_tokens_staked =
				pool_info.total_tokens_staked.checked_add(&amount).ok_or(Error::<T>::Overflow)?;

			Pools::<T>::insert(pool_id, pool_info);

			// Update PoolStakers.
			staker_info.amount.saturating_accrue(amount);
			PoolStakers::<T>::insert(pool_id, &caller, staker_info);

			// Emit event.
			Self::deposit_event(Event::Staked { caller, pool_id, amount });

			Ok(())
		}

		/// Unstake tokens from a pool.
		///
		/// Removes the freeze on the staked tokens.
		#[pallet::call_index(2)]
		pub fn unstake(
			origin: OriginFor<T>,
			pool_id: PoolId,
			amount: T::Balance,
		) -> DispatchResult {
			let caller = ensure_signed(origin)?;

			// Always start by updating the pool rewards.
			let pool_info = Pools::<T>::get(pool_id).ok_or(Error::<T>::NonExistentPool)?;
			let staker_info = PoolStakers::<T>::get(pool_id, &caller).unwrap_or_default();
			let (mut pool_info, mut staker_info) =
				Self::update_pool_and_staker_rewards(&pool_info, &staker_info)?;

			// Check the staker has enough staked tokens.
			ensure!(staker_info.amount >= amount, Error::<T>::NotEnoughTokens);

			// Unfreeze staker assets.
			T::AssetsFreezer::decrease_frozen(
				pool_info.staked_asset_id.clone(),
				&FreezeReason::Staked.into(),
				&caller,
				amount,
			)?;

			// Update Pools.
			pool_info.total_tokens_staked.saturating_reduce(amount);
			Pools::<T>::insert(pool_id, pool_info);

			// Update PoolStakers.
			staker_info.amount.saturating_reduce(amount);
			PoolStakers::<T>::insert(pool_id, &caller, staker_info);

			// Emit event.
			Self::deposit_event(Event::Unstaked { caller, pool_id, amount });

			Ok(())
		}

		/// Harvest unclaimed pool rewards for the caller.
		#[pallet::call_index(3)]
		pub fn harvest_rewards(origin: OriginFor<T>, pool_id: PoolId) -> DispatchResult {
			let staker = ensure_signed(origin)?;

			// Always start by updating the pool and staker rewards.
			let pool_info = Pools::<T>::get(pool_id).ok_or(Error::<T>::NonExistentPool)?;
			let staker_info =
				PoolStakers::<T>::get(pool_id, &staker).ok_or(Error::<T>::NonExistentStaker)?;
			let (pool_info, mut staker_info) =
				Self::update_pool_and_staker_rewards(&pool_info, &staker_info)?;

			// Transfer unclaimed rewards from the pool to the staker.
			let pool_account_id = Self::pool_account_id(&pool_id)?;
			T::Assets::transfer(
				pool_info.reward_asset_id,
				&pool_account_id,
				&staker,
				staker_info.rewards,
				// Could kill the account, but only if the pool was already almost empty.
				Preservation::Expendable,
			)?;

			// Emit event.
			Self::deposit_event(Event::RewardsHarvested {
				staker: staker.clone(),
				pool_id,
				amount: staker_info.rewards,
			});

			// Reset staker rewards.
			staker_info.rewards = 0u32.into();
			PoolStakers::<T>::insert(pool_id, &staker, staker_info);

			Ok(())
		}

		/// Modify a pool reward rate.
		///
		/// Only the pool admin may perform this operation.
		#[pallet::call_index(4)]
		pub fn set_pool_reward_rate_per_block(
			origin: OriginFor<T>,
			pool_id: PoolId,
			new_reward_rate_per_block: T::Balance,
		) -> DispatchResult {
			let caller = T::CreatePoolOrigin::ensure_origin(origin.clone())
				.or_else(|_| ensure_signed(origin))?;

			let pool_info = Pools::<T>::get(pool_id).ok_or(Error::<T>::NonExistentPool)?;
			ensure!(caller == pool_info.admin, BadOrigin);

			// Always start by updating the pool rewards.
			let rewards_per_token = Self::reward_per_token(&pool_info)?;
			let mut pool_info = Self::update_pool_rewards(&pool_info, rewards_per_token)?;

			pool_info.reward_rate_per_block = new_reward_rate_per_block;
			Pools::<T>::insert(pool_id, pool_info);

			Self::deposit_event(Event::PoolRewardRateModified {
				pool_id,
				new_reward_rate_per_block,
			});

			Ok(())
		}

		/// Modify a pool admin.
		///
		/// Only the pool admin may perform this operation.
		#[pallet::call_index(5)]
		pub fn set_pool_admin(
			origin: OriginFor<T>,
			pool_id: PoolId,
			new_admin: T::AccountId,
		) -> DispatchResult {
			let caller = T::CreatePoolOrigin::ensure_origin(origin.clone())
				.or_else(|_| ensure_signed(origin))?;

			let mut pool_info = Pools::<T>::get(pool_id).ok_or(Error::<T>::NonExistentPool)?;
			ensure!(pool_info.admin == caller, BadOrigin);
			pool_info.admin = new_admin.clone();
			Pools::<T>::insert(pool_id, pool_info);

			Self::deposit_event(Event::PoolAdminModified { pool_id, new_admin });

			Ok(())
		}

		/// Set when the pool should expire.
		///
		/// Only the pool admin may perform this operation.
		#[pallet::call_index(6)]
		pub fn set_pool_expiry_block(
			origin: OriginFor<T>,
			pool_id: PoolId,
			new_expiry_block: BlockNumberFor<T>,
		) -> DispatchResult {
			let caller = T::CreatePoolOrigin::ensure_origin(origin.clone())
				.or_else(|_| ensure_signed(origin))?;

			ensure!(
				new_expiry_block > frame_system::Pallet::<T>::block_number(),
				Error::<T>::ExpiryBlockMustBeInTheFuture
			);

			let pool_info = Pools::<T>::get(pool_id).ok_or(Error::<T>::NonExistentPool)?;
			ensure!(pool_info.admin == caller, BadOrigin);

			// Always start by updating the pool rewards.
			let reward_per_token = Self::reward_per_token(&pool_info)?;
			let mut pool_info = Self::update_pool_rewards(&pool_info, reward_per_token)?;

			pool_info.expiry_block = new_expiry_block;
			Pools::<T>::insert(pool_id, pool_info);

			Self::deposit_event(Event::PoolExpiryBlockModified { pool_id, new_expiry_block });

			Ok(())
		}

		/// Convenience method to deposit reward tokens into a pool.
		///
		/// This method is not strictly necessary (tokens could be transferred directly to the
		/// pool pot address), but is provided for convenience so manual derivation of the
		/// account id is not required.
		#[pallet::call_index(7)]
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

		/// Permissioned method to withdraw reward tokens from a pool.
		///
		/// Only the pool admin may perform this operation.
		#[pallet::call_index(8)]
		pub fn withdraw_reward_tokens(
			origin: OriginFor<T>,
			pool_id: PoolId,
			amount: T::Balance,
			dest: T::AccountId,
		) -> DispatchResult {
			let caller = T::CreatePoolOrigin::ensure_origin(origin.clone())
				.or_else(|_| ensure_signed(origin))?;

			let pool_info = Pools::<T>::get(pool_id).ok_or(Error::<T>::NonExistentPool)?;
			ensure!(pool_info.admin == caller, BadOrigin);

			T::Assets::transfer(
				pool_info.reward_asset_id,
				&Self::pool_account_id(&pool_id)?,
				&dest,
				amount,
				// Allow completely draining the account.
				Preservation::Expendable,
			)?;

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// The pool creation footprint.
		///
		/// The footprint specifically accounts for the storage footprint of the pool's information
		/// itself, excluding any potential storage footprint related to the stakers.
		pub fn pool_creation_footprint() -> Footprint {
			Footprint::from_mel::<(PoolId, PoolInfoFor<T>)>()
		}

		/// Derive a pool account ID from the pool's ID.
		pub fn pool_account_id(id: &PoolId) -> Result<T::AccountId, DispatchError> {
			if Pools::<T>::contains_key(id) {
				Ok(T::PalletId::get().into_sub_account_truncating(id))
			} else {
				Err(Error::<T>::NonExistentPool.into())
			}
		}

		/// Computes update pool and staker reward state.
		///
		/// Should be called prior to any operation involving a staker.
		///
		/// Returns the updated pool and staker info.
		///
		/// NOTE: this function has no side-effects. Side-effects such as storage modifications are
		/// the responsibility of the caller.
		pub fn update_pool_and_staker_rewards(
			pool_info: &PoolInfoFor<T>,
			staker_info: &PoolStakerInfo<T::Balance>,
		) -> Result<(PoolInfoFor<T>, PoolStakerInfo<T::Balance>), DispatchError> {
			let reward_per_token = Self::reward_per_token(&pool_info)?;
			let pool_info = Self::update_pool_rewards(pool_info, reward_per_token)?;

			let mut new_staker_info = staker_info.clone();
			new_staker_info.rewards = Self::derive_rewards(&staker_info, &reward_per_token)?;
			new_staker_info.reward_per_token_paid = pool_info.reward_per_token_stored;
			return Ok((pool_info, new_staker_info));
		}

		/// Computes update pool reward state.
		///
		/// Should be called every time the pool is adjusted, and a staker is not involved.
		///
		/// Returns the updated pool and staker info.
		///
		/// NOTE: this function has no side-effects. Side-effects such as storage modifications are
		/// the responsibility of the caller.
		pub fn update_pool_rewards(
			pool_info: &PoolInfoFor<T>,
			reward_per_token: T::Balance,
		) -> Result<PoolInfoFor<T>, DispatchError> {
			let mut new_pool_info = pool_info.clone();
			new_pool_info.last_update_block = frame_system::Pallet::<T>::block_number();
			new_pool_info.reward_per_token_stored = reward_per_token;

			Ok(new_pool_info)
		}

		/// Derives the current reward per token for this pool.
		///
		/// This is a helper function for `update_pool_rewards` and should not be called directly.
		fn reward_per_token(pool_info: &PoolInfoFor<T>) -> Result<T::Balance, DispatchError> {
			if pool_info.total_tokens_staked.is_zero() {
				return Ok(pool_info.reward_per_token_stored)
			}

			let rewardable_blocks_elapsed: u32 =
				match Self::last_block_reward_applicable(pool_info.expiry_block)
					.saturating_sub(pool_info.last_update_block)
					.try_into()
				{
					Ok(b) => b,
					Err(_) => return Err(Error::<T>::BlockNumberConversionError.into()),
				};

			Ok(pool_info.reward_per_token_stored.saturating_add(
				pool_info
					.reward_rate_per_block
					.saturating_mul(rewardable_blocks_elapsed.into())
					.saturating_mul(PRECISION_SCALING_FACTOR.into())
					.ensure_div(pool_info.total_tokens_staked)?,
			))
		}

		/// Derives the amount of rewards earned by a staker.
		///
		/// This is a helper function for `update_pool_rewards` and should not be called directly.
		fn derive_rewards(
			staker_info: &PoolStakerInfo<T::Balance>,
			reward_per_token: &T::Balance,
		) -> Result<T::Balance, DispatchError> {
			Ok(staker_info
				.amount
				.saturating_mul(reward_per_token.saturating_sub(staker_info.reward_per_token_paid))
				.ensure_div(PRECISION_SCALING_FACTOR.into())?
				.saturating_add(staker_info.rewards))
		}

		fn last_block_reward_applicable(pool_expiry_block: BlockNumberFor<T>) -> BlockNumberFor<T> {
			if frame_system::Pallet::<T>::block_number() < pool_expiry_block {
				frame_system::Pallet::<T>::block_number()
			} else {
				pool_expiry_block
			}
		}
	}
}
