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

//! Trait for providing methods to mutate liquidity pools.

use frame_support::{traits::tokens::Balance, transactional};
use sp_runtime::DispatchError;

use crate::{Config, Pallet};

/// Trait for providing methods to mutate liquidity pools.
pub trait MutateLiquidity<AccountId> {
	/// Measure units of the asset classes for swapping.
	type Balance: Balance;
	/// Kind of assets that are going to be swapped.
	type AssetKind;
	/// Pool identifier.
	type PoolId;

	/// Create a new liquidity pool.
	///
	/// Returns the ID of the newly created pool.
	fn create_pool(
		creator: &AccountId,
		asset1: Self::AssetKind,
		asset2: Self::AssetKind,
	) -> Result<Self::PoolId, DispatchError>;

	/// Add liquidity to a pool.
	///
	/// Returns the amount of LP tokens minted.
	fn add_liquidity(
		who: &AccountId,
		asset1: Self::AssetKind,
		asset2: Self::AssetKind,
		amount1_desired: Self::Balance,
		amount2_desired: Self::Balance,
		amount1_min: Self::Balance,
		amount2_min: Self::Balance,
		mint_to: &AccountId,
	) -> Result<Self::Balance, DispatchError>;

	/// Remove liquidity from a pool.
	///
	/// Returns the amounts of assets withdrawn.
	fn remove_liquidity(
		who: &AccountId,
		asset1: Self::AssetKind,
		asset2: Self::AssetKind,
		lp_token_burn: Self::Balance,
		amount1_min_receive: Self::Balance,
		amount2_min_receive: Self::Balance,
		withdraw_to: &AccountId,
	) -> Result<(Self::Balance, Self::Balance), DispatchError>;
}

impl<T: Config> MutateLiquidity<T::AccountId> for Pallet<T> {
	type Balance = T::Balance;
	type AssetKind = T::AssetKind;
	type PoolId = T::PoolId;

	#[transactional]
	fn create_pool(
		creator: &T::AccountId,
		asset1: T::AssetKind,
		asset2: T::AssetKind,
	) -> Result<T::PoolId, DispatchError> {
		Self::do_create_pool(creator, asset1, asset2)
	}

	#[transactional]
	fn add_liquidity(
		who: &T::AccountId,
		asset1: T::AssetKind,
		asset2: T::AssetKind,
		amount1_desired: T::Balance,
		amount2_desired: T::Balance,
		amount1_min: T::Balance,
		amount2_min: T::Balance,
		mint_to: &T::AccountId,
	) -> Result<T::Balance, DispatchError> {
		Self::do_add_liquidity(
			who,
			asset1,
			asset2,
			amount1_desired,
			amount2_desired,
			amount1_min,
			amount2_min,
			mint_to,
		)
	}

	#[transactional]
	fn remove_liquidity(
		who: &T::AccountId,
		asset1: T::AssetKind,
		asset2: T::AssetKind,
		lp_token_burn: T::Balance,
		amount1_min_receive: T::Balance,
		amount2_min_receive: T::Balance,
		withdraw_to: &T::AccountId,
	) -> Result<(T::Balance, T::Balance), DispatchError> {
		Self::do_remove_liquidity(
			who,
			asset1,
			asset2,
			lp_token_burn,
			amount1_min_receive,
			amount2_min_receive,
			withdraw_to,
		)
	}
}
