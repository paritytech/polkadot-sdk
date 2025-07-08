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

//! The lockable currency trait and some associated types.

use super::{super::misc::WithdrawReasons, Currency};
use crate::{dispatch::DispatchResult, traits::misc::Get};

/// An identifier for a lock. Used for disambiguating different locks so that
/// they can be individually replaced or removed.
pub type LockIdentifier = [u8; 8];

/// A currency whose accounts can have liquidity restrictions.
pub trait LockableCurrency<AccountId>: Currency<AccountId> {
	/// The quantity used to denote time; usually just a `BlockNumber`.
	type Moment;

	/// The maximum number of locks a user should have on their account.
	type MaxLocks: Get<u32>;

	/// Create a new balance lock on account `who`.
	///
	/// If the new lock is valid (i.e. not already expired), it will push the struct to
	/// the `Locks` vec in storage. Note that you can lock more funds than a user has.
	///
	/// If the lock `id` already exists, this will update it.
	fn set_lock(
		id: LockIdentifier,
		who: &AccountId,
		amount: Self::Balance,
		reasons: WithdrawReasons,
	);

	/// Changes a balance lock (selected by `id`) so that it becomes less liquid in all
	/// parameters or creates a new one if it does not exist.
	///
	/// Calling `extend_lock` on an existing lock `id` differs from `set_lock` in that it
	/// applies the most severe constraints of the two, while `set_lock` replaces the lock
	/// with the new parameters. As in, `extend_lock` will set:
	/// - maximum `amount`
	/// - bitwise mask of all `reasons`
	fn extend_lock(
		id: LockIdentifier,
		who: &AccountId,
		amount: Self::Balance,
		reasons: WithdrawReasons,
	);

	/// Remove an existing lock.
	fn remove_lock(id: LockIdentifier, who: &AccountId);
}

/// A inspect interface for a currency whose accounts can have liquidity restrictions.
pub trait InspectLockableCurrency<AccountId>: LockableCurrency<AccountId> {
	/// Amount of funds locked for `who` associated with `id`.
	fn balance_locked(id: LockIdentifier, who: &AccountId) -> Self::Balance;
}

/// A vesting schedule over a currency. This allows a particular currency to have vesting limits
/// applied to it.
pub trait VestingSchedule<AccountId> {
	/// The quantity used to denote time; usually just a `BlockNumber`.
	type Moment;

	/// The currency that this schedule applies to.
	type Currency: Currency<AccountId>;

	/// Get the amount that is currently being vested and cannot be transferred out of this account.
	/// Returns `None` if the account has no vesting schedule.
	fn vesting_balance(who: &AccountId)
		-> Option<<Self::Currency as Currency<AccountId>>::Balance>;

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
		locked: <Self::Currency as Currency<AccountId>>::Balance,
		per_block: <Self::Currency as Currency<AccountId>>::Balance,
		starting_block: Self::Moment,
	) -> DispatchResult;

	/// Checks if `add_vesting_schedule` would work against `who`.
	fn can_add_vesting_schedule(
		who: &AccountId,
		locked: <Self::Currency as Currency<AccountId>>::Balance,
		per_block: <Self::Currency as Currency<AccountId>>::Balance,
		starting_block: Self::Moment,
	) -> DispatchResult;

	/// Remove a vesting schedule for a given account.
	///
	/// NOTE: This doesn't alter the free balance of the account.
	fn remove_vesting_schedule(who: &AccountId, schedule_index: u32) -> DispatchResult;
}

/// A vested transfer over a currency. This allows a transferred amount to vest over time.
pub trait VestedTransfer<AccountId> {
	/// The quantity used to denote time; usually just a `BlockNumber`.
	type Moment;

	/// The currency that this schedule applies to.
	type Currency: Currency<AccountId>;

	/// Execute a vested transfer from `source` to `target` with the given schedule:
	/// 	- `locked`: The amount to be transferred and for the vesting schedule to apply to.
	/// 	- `per_block`: The amount to be unlocked each block. (linear vesting)
	/// 	- `starting_block`: The block where the vesting should start. This block can be in the past
	///    or future, and should adjust when the tokens become available to the user.
	///
	/// Example: Assume we are on block 100. If `locked` amount is 100, and `per_block` is 1:
	/// 	- If `starting_block` is 0, then the whole 100 tokens will be available right away as the
	///    vesting schedule started in the past and has fully completed.
	/// 	- If `starting_block` is 50, then 50 tokens are made available right away, and 50 more
	///    tokens will unlock one token at a time until block 150.
	/// 	- If `starting_block` is 100, then each block, 1 token will be unlocked until the whole
	///    balance is unlocked at block 200.
	/// 	- If `starting_block` is 200, then the 100 token balance will be completely locked until
	///    block 200, and then start to unlock one token at a time until block 300.
	fn vested_transfer(
		source: &AccountId,
		target: &AccountId,
		locked: <Self::Currency as Currency<AccountId>>::Balance,
		per_block: <Self::Currency as Currency<AccountId>>::Balance,
		starting_block: Self::Moment,
	) -> DispatchResult;
}

// An no-op implementation of `VestedTransfer` for pallets that require this trait, but users may
// not want to implement this functionality
pub struct NoVestedTransfers<C> {
	phantom: core::marker::PhantomData<C>,
}

impl<AccountId, C: Currency<AccountId>> VestedTransfer<AccountId> for NoVestedTransfers<C> {
	type Moment = ();
	type Currency = C;

	fn vested_transfer(
		_source: &AccountId,
		_target: &AccountId,
		_locked: <Self::Currency as Currency<AccountId>>::Balance,
		_per_block: <Self::Currency as Currency<AccountId>>::Balance,
		_starting_block: Self::Moment,
	) -> DispatchResult {
		Err(sp_runtime::DispatchError::Unavailable.into())
	}
}
