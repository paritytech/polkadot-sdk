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

//! Module to manage types related to vesting.

use super::*;
use fungibles::InspectFreeze;

pub(crate) type AssetFreezeReasonOf<T, I> =
	<<T as Config<I>>::Freezer as InspectFreeze<AccountIdOf<T>>>::Id;
pub(crate) type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
pub(crate) type AssetIdOf<T, I = ()> =
	<<T as Config<I>>::Assets as Inspect<AccountIdOf<T>>>::AssetId;
pub(crate) type BalanceOf<T, I = ()> =
	<<T as Config<I>>::Assets as Inspect<AccountIdOf<T>>>::Balance;
pub(crate) type AccountIdLookupOf<T> =
	<<T as frame_system::Config>::Lookup as StaticLookup>::Source;

/// Actions to take against a user's `Vesting` storage entry.
#[derive(Clone, Copy)]
pub(crate) enum VestingAction {
	/// Do not actively remove any schedules.
	Passive,
	/// Remove the schedule specified by the index.
	Remove { index: usize },
	/// Remove the two schedules, specified by index, so they can be merged.
	Merge { index1: usize, index2: usize },
}

impl VestingAction {
	/// Whether or not the filter says the schedule index should be removed.
	pub(crate) fn should_remove(&self, index: usize) -> bool {
		match self {
			Self::Passive => false,
			Self::Remove { index: index1 } => *index1 == index,
			Self::Merge { index1, index2 } => *index1 == index || *index2 == index,
		}
	}

	/// Pick the schedules that this action dictates should continue vesting undisturbed.
	pub(crate) fn pick_schedules<T: Config<I>, I: 'static>(
		&self,
		schedules: Vec<VestingInfo<BalanceOf<T, I>, BlockNumberFor<T>>>,
	) -> impl Iterator<Item = VestingInfo<BalanceOf<T, I>, BlockNumberFor<T>>> + '_ {
		schedules.into_iter().enumerate().filter_map(move |(index, schedule)| {
			if self.should_remove(index) {
				None
			} else {
				Some(schedule)
			}
		})
	}
}

// Wrapper for `T::MAX_VESTING_SCHEDULES` to satisfy `trait Get`.
pub struct MaxVestingSchedulesGet<T, I = ()>(PhantomData<(T, I)>);
impl<T: Config<I>, I: 'static> Get<u32> for MaxVestingSchedulesGet<T, I> {
	fn get() -> u32 {
		T::MAX_VESTING_SCHEDULES
	}
}

/// Struct to encode the vesting schedule of an individual account.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	Copy,
	Clone,
	PartialEq,
	Eq,
	RuntimeDebug,
	MaxEncodedLen,
	TypeInfo,
)]
pub struct VestingInfo<Balance, BlockNumber> {
	/// Locked amount at genesis.
	frozen: Balance,
	/// Amount that gets unlocked every block after `starting_block`.
	per_block: Balance,
	/// Starting block for unlocking(vesting).
	starting_block: BlockNumber,
}

impl<Balance, BlockNumber> VestingInfo<Balance, BlockNumber>
where
	Balance: AtLeast32BitUnsigned + Copy,
	BlockNumber: AtLeast32BitUnsigned + Copy + Bounded,
{
	/// Instantiate a new `VestingInfo`.
	pub fn new(
		frozen: Balance,
		per_block: Balance,
		starting_block: BlockNumber,
	) -> VestingInfo<Balance, BlockNumber> {
		VestingInfo { frozen, per_block, starting_block }
	}

	/// Validate parameters for `VestingInfo`. Note that this does not check
	/// against `MinVestedTransfer`.
	pub fn is_valid(&self) -> bool {
		!self.frozen.is_zero() && !self.raw_per_block().is_zero()
	}

	/// Locked amount at schedule creation.
	pub fn locked(&self) -> Balance {
		self.frozen
	}

	/// Amount that gets thawed every block after `starting_block`. Corrects for `per_block` of 0.
	/// We don't let `per_block` be less than 1, or else the vesting will never end.
	/// This should be used whenever accessing `per_block` unless explicitly checking for 0 values.
	pub fn per_block(&self) -> Balance {
		self.per_block.max(One::one())
	}

	/// Get the unmodified `per_block`. Generally should not be used, but is useful for
	/// validating `per_block`.
	pub(crate) fn raw_per_block(&self) -> Balance {
		self.per_block
	}

	/// Starting block for thawing(vesting).
	pub fn starting_block(&self) -> BlockNumber {
		self.starting_block
	}

	/// Amount frozen at block `n`.
	pub fn locked_at<BlockNumberToBalance: Convert<BlockNumber, Balance>>(
		&self,
		n: BlockNumber,
	) -> Balance {
		// Number of blocks that count toward vesting;
		// saturating to 0 when n < starting_block.
		let vested_block_count = n.saturating_sub(self.starting_block);
		let vested_block_count = BlockNumberToBalance::convert(vested_block_count);
		// Return amount that is still frozen in vesting.
		vested_block_count
			.checked_mul(&self.per_block()) // `per_block` accessor guarantees at least 1.
			.map(|to_unlock| self.frozen.saturating_sub(to_unlock))
			.unwrap_or(Zero::zero())
	}

	/// Block number at which the schedule ends (as type `Balance`).
	pub fn ending_block_as_balance<BlockNumberToBalance: Convert<BlockNumber, Balance>>(
		&self,
	) -> Balance {
		let starting_block = BlockNumberToBalance::convert(self.starting_block);
		let duration = if self.per_block() >= self.frozen {
			// If `per_block` is bigger than `frozen`, the schedule will end
			// the block after starting.
			One::one()
		} else {
			self.frozen / self.per_block() +
				if (self.frozen % self.per_block()).is_zero() {
					Zero::zero()
				} else {
					// `per_block` does not perfectly divide `frozen`, so we need an extra block to
					// thaw some amount less than `per_block`.
					One::one()
				}
		};

		starting_block.saturating_add(duration)
	}
}

/// Helper methods for benchmarking `pallet-assets-vesting`.
pub trait BenchmarkHelper<T: Config<I>, I: 'static> {
	/// Retrieves the asset id to be used in the benchmarking tests.
	fn asset_id() -> AssetIdOf<T, I>;
}

impl<T: Config<I>, I: 'static> BenchmarkHelper<T, I> for ()
where
	AssetIdOf<T, I>: Zero,
{
	fn asset_id() -> AssetIdOf<T, I> {
		Zero::zero()
	}
}
