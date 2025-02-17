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

//! `VestingSchedule` and `VestedTransfer` traits for working with vesting schedules.
//!
//! See the [`crate::traits::fungible`] doc for more information about fungible traits.

use crate::dispatch::DispatchResult;

/// A vesting schedule over a fungible type. This allows a particular currency to have vesting
/// limits applied to it.
pub trait VestingSchedule<AccountId> {
	/// The quantity used to denote time; usually just a `BlockNumber`.
	type Moment;

	/// The balance type that this schedule applies to.
	type Balance;

	/// Get the amount that is currently being vested and cannot be transferred out of this account.
	/// Returns `None` if the account has no vesting schedule.
	fn vesting_balance(who: &AccountId) -> Option<Self::Balance>;

	/// Adds a vesting schedule to a given account.
	///
	/// If the account has `MaxVestingSchedules`, an Error is returned and nothing
	/// is updated.
	///
	/// Is a no-op if the amount to be vested is zero.
	///
	/// NOTE: This doesn't alter the free balance of the account.
	fn add_vesting_schedule(
		who: &AccountId,
		locked: Self::Balance,
		per_block: Self::Balance,
		starting_block: Self::Moment,
	) -> DispatchResult;

	/// Checks if `add_vesting_schedule` would work against `who`.
	fn can_add_vesting_schedule(
		who: &AccountId,
		locked: Self::Balance,
		per_block: Self::Balance,
		starting_block: Self::Moment,
	) -> DispatchResult;

	/// Remove a vesting schedule for a given account.
	///
	/// NOTE: This doesn't alter the free balance of the account.
	fn remove_vesting_schedule(who: &AccountId, schedule_index: u32) -> DispatchResult;
}

/// A vested transfer over a token. This allows a transferred amount to vest over time.
pub trait VestedTransfer<AccountId> {
	/// The quantity used to denote time; usually just a `BlockNumber`.
	type Moment;

	/// The balance type that this schedule applies to.
	type Balance;

	/// Execute a vested transfer from `source` to `target` with the given schedule:
	/// 	- `frozen`: The amount of tokens to be transferred and for the vesting schedule to apply
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
		source: &AccountId,
		target: &AccountId,
		locked: Self::Balance,
		per_block: Self::Balance,
		starting_block: Self::Moment,
	) -> DispatchResult;
}

// A no-op implementation of `VestedTransfer` for pallets that require this trait, but users may
// not want to implement this functionality
pub struct NoVestedTransfers<B>(core::marker::PhantomData<B>);

impl<AccountId, Balance> VestedTransfer<AccountId> for NoVestedTransfers<Balance> {
	type Moment = ();
	type Balance = Balance;

	fn vested_transfer(
		_source: &AccountId,
		_target: &AccountId,
		_locked: Self::Balance,
		_per_block: Self::Balance,
		_starting_block: Self::Moment,
	) -> DispatchResult {
		Err(sp_runtime::DispatchError::Unavailable.into())
	}
}
