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

//! `Inspect`, `Mutate` and `Transfer` traits for working with vesting schedules.
//!
//! See the [`crate::traits::fungibles`] doc for more information about fungibles traits.

use crate::{dispatch::DispatchResult, traits::tokens::misc::AssetId};

pub trait Inspect<AccountId> {
	/// The quantity used to denote time; usually just a `BlockNumber`.
	type Moment;
	/// Means of identifying one asset class from another.
	type AssetId: AssetId;
	/// The balance type that this schedule applies to.
	type Balance;

	/// Get the amount that is currently being vested and cannot be transferred out of this asset
	/// account. Returns `None` if the asset account has no vesting schedule.
	fn vesting_balance(asset: Self::AssetId, who: &AccountId) -> Option<Self::Balance>;

	/// Checks if `add_vesting_schedule` would work against `who`.
	fn can_add_vesting_schedule(
		asset: Self::AssetId,
		who: &AccountId,
		locked: Self::Balance,
		per_block: Self::Balance,
		starting_block: Self::Moment,
	) -> DispatchResult;
}

/// A vesting schedule over a fungible asset class. This allows a particular currency to have
/// vesting limits applied to it.
pub trait Mutate<AccountId>: Inspect<AccountId> {
	/// Adds a vesting schedule to a given asset account.
	///
	/// If the account has `MaxVestingSchedules`, an Error is returned and nothing
	/// is updated.
	///
	/// Is a no-op if the amount to be vested is zero.
	///
	/// NOTE: This doesn't alter the free balance of the asset account.
	fn add_vesting_schedule(
		asset: Self::AssetId,
		who: &AccountId,
		locked: Self::Balance,
		per_block: Self::Balance,
		starting_block: Self::Moment,
	) -> DispatchResult;

	/// Remove a vesting schedule for a given asset account.
	///
	/// NOTE: This doesn't alter the free balance of the asset account.
	fn remove_vesting_schedule(
		asset: Self::AssetId,
		who: &AccountId,
		schedule_index: u32,
	) -> DispatchResult;
}

/// A vested transfer over an asset. This allows a transferred amount to vest over time.
pub trait Transfer<AccountId>: Inspect<AccountId> {
	/// Execute a vested transfer from `source` to `target` with the given schedule:
	/// 	- `frozen`: The amount of assets to be transferred and for the vesting schedule to apply
	///    to.
	/// 	- `per_block`: The amount to be unlocked each block. (linear vesting)
	/// 	- `starting_block`: The block where the vesting should start. This block can be in the past
	///    or future, and should adjust when the balance become available to the user.
	///
	/// Example: Assume we are on block 100. If `frozen` amount is 100, and `per_block` is 1:
	/// 	- If `starting_block` is 0, then the whole 100 tokens will be available right away as the
	///    vesting schedule started in the past and has fully completed.
	/// 	- If `starting_block` is 50, then 50 tokens are made available right away, and 50 more
	///    tokens will unlock one token at a time until block 150.
	/// 	- If `starting_block` is 100, then each block, 1 tokens will be unlocked until the whole
	///    balance is unlocked at block 200.
	/// 	- If `starting_block` is 200, then the 100 token balance will be completely locked until
	///    block 200, and then start to unlock one token at a time until block 300.
	fn vested_transfer(
		asset: Self::AssetId,
		source: &AccountId,
		target: &AccountId,
		locked: Self::Balance,
		per_block: Self::Balance,
		starting_block: Self::Moment,
	) -> DispatchResult;
}

/// A wrapper of an implementation of [Inspect], but implements [Transfer] as no-op, always failing.
/// For pallets that require this trait but users may not want to implement this functionality
pub struct NoVestedTransfers<T>(core::marker::PhantomData<T>);

impl<AccountId, T: Inspect<AccountId>> Inspect<AccountId> for NoVestedTransfers<T> {
	type Moment = T::Moment;
	type AssetId = T::AssetId;
	type Balance = T::Balance;

	fn vesting_balance(asset: Self::AssetId, who: &AccountId) -> Option<Self::Balance> {
		T::vesting_balance(asset, who)
	}

	fn can_add_vesting_schedule(
		asset: Self::AssetId,
		who: &AccountId,
		locked: Self::Balance,
		per_block: Self::Balance,
		starting_block: Self::Moment,
	) -> DispatchResult {
		T::can_add_vesting_schedule(asset, who, locked, per_block, starting_block)
	}
}

impl<AccountId, T> Transfer<AccountId> for NoVestedTransfers<T>
where
	T: Inspect<AccountId>,
{
	fn vested_transfer(
		_asset: Self::AssetId,
		_source: &AccountId,
		_target: &AccountId,
		_locked: Self::Balance,
		_per_block: Self::Balance,
		_starting_block: Self::Moment,
	) -> DispatchResult {
		Err(sp_runtime::DispatchError::Unavailable.into())
	}
}
