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

//! Traits for managing reward pools.

use crate::traits::schedule::DispatchTime;
use sp_runtime::{DispatchError, DispatchResult};

/// A trait for managing a rewards pool.
pub trait RewardsPool<AccountId, PoolId, Balance> {
	type AssetId;
	type BlockNumber;

	/// Create a new reward pool.
	fn create_pool(
		creator: &AccountId,
		staked_asset_id: Self::AssetId,
		reward_asset_id: Self::AssetId,
		reward_rate_per_block: Balance,
		expiry: DispatchTime<Self::BlockNumber>,
		admin: Option<AccountId>,
	) -> Result<PoolId, DispatchError>;

	/// Modify a pool reward rate.
	fn set_pool_reward_rate_per_block(
		admin: &AccountId,
		pool_id: PoolId,
		new_reward_rate_per_block: Balance,
	) -> DispatchResult;

	/// Modify a pool admin.
	fn set_pool_admin(admin: &AccountId, pool_id: PoolId, new_admin: AccountId) -> DispatchResult;

	/// Set when the pool should expire.
	fn set_pool_expiry_block(
		admin: &AccountId,
		pool_id: PoolId,
		new_expiry: DispatchTime<Self::BlockNumber>,
	) -> DispatchResult;
}
