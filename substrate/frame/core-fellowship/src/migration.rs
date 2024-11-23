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

//! Storage migrations for the core-fellowship pallet.
use super::*;
use frame_support::{
	pallet_prelude::*,
	storage_alias,
	traits::{DefensiveTruncateFrom, UncheckedOnRuntimeUpgrade},
	BoundedVec,
};

#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;
#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

mod v0 {
	use frame_system::pallet_prelude::BlockNumberFor;

	use super::*;

	#[derive(Encode, Decode, Eq, PartialEq, Clone, TypeInfo, MaxEncodedLen, RuntimeDebug)]
	pub struct ParamsType<Balance, BlockNumber, const RANKS: usize> {
		pub active_salary: [Balance; RANKS],
		pub passive_salary: [Balance; RANKS],
		pub demotion_period: [BlockNumber; RANKS],
		pub min_promotion_period: [BlockNumber; RANKS],
		pub offboard_timeout: BlockNumber,
	}

	impl<Balance: Default + Copy, BlockNumber: Default + Copy, const RANKS: usize> Default
		for ParamsType<Balance, BlockNumber, RANKS>
	{
		fn default() -> Self {
			Self {
				active_salary: [Balance::default(); RANKS],
				passive_salary: [Balance::default(); RANKS],
				demotion_period: [BlockNumber::default(); RANKS],
				min_promotion_period: [BlockNumber::default(); RANKS],
				offboard_timeout: BlockNumber::default(),
			}
		}
	}

	/// Number of available ranks from old version.
	pub(crate) const RANK_COUNT: usize = 9;

	pub type ParamsOf<T, I> = ParamsType<<T as Config<I>>::Balance, BlockNumberFor<T>, RANK_COUNT>;

	/// V0 type for [`crate::Params`].
	#[storage_alias]
	pub type Params<T: Config<I>, I: 'static> =
		StorageValue<Pallet<T, I>, ParamsOf<T, I>, ValueQuery>;
}

mod v1 {
	use super::*;
	use frame_system::pallet_prelude::BlockNumberFor as LocalBlockNumberFor;

	pub type ParamsOf<T, I> = ParamsType<<T as Config<I>>::Balance, LocalBlockNumberFor<T>, <T as Config<I>>::MaxRank>;
	/// V1 type for [`crate::Params`].
	#[storage_alias]
	pub type Params<T: Config<I>, I: 'static> =
		StorageValue<Pallet<T, I>, ParamsOf<T, I>, ValueQuery>;
}

pub struct MigrateToV1<T, I = ()>(PhantomData<(T, I)>);
impl<T: Config<I>, I: 'static> UncheckedOnRuntimeUpgrade for MigrateToV1<T, I> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
		ensure!(
			T::MaxRank::get() >= v0::RANK_COUNT as u32,
			"pallet-core-fellowship: new bound should not truncate"
		);
		Ok(Default::default())
	}

	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		// Read the old value from storage
		let old_value = v0::Params::<T, I>::take();
		// Write the new value to storage
		let new = crate::ParamsType {
			active_salary: BoundedVec::defensive_truncate_from(old_value.active_salary.to_vec()),
			passive_salary: BoundedVec::defensive_truncate_from(old_value.passive_salary.to_vec()),
			demotion_period: BoundedVec::defensive_truncate_from(
				old_value.demotion_period.to_vec(),
			),
			min_promotion_period: BoundedVec::defensive_truncate_from(
				old_value.min_promotion_period.to_vec(),
			),
			offboard_timeout: old_value.offboard_timeout,
		};
		v1::Params::<T, I>::put(new);
		T::DbWeight::get().reads_writes(1, 1)
	}
}

mod v2 {
	use super::*;
	use crate::BlockNumberFor as NewBlockNumberFor;
	use frame_system::pallet_prelude::BlockNumberFor as LocalBlockNumberFor;

	pub trait ConvertBlockNumber<L, N>
	{
		/// Converts the local block number to the new block number type.
		fn convert(local: L) -> N;

		/// Adds an offset to new block number if necessary
		/// 
		/// For instance - if your new version uses the relay chain number, you'll want to add (RC Num - Local Num)
		fn add_offset(current: N) -> N;

		/// Returns the equivalent time duration as the previous type when represented as the new type
		/// 
		/// For instance - If you previously had 12s blocks and are now following the relay chain's 6
		/// values should be 2x the old to achieve the same duration in time
		fn equivalent_time(local: L) -> N;
	}

	pub struct MigrateToV2<T, BlockNumberConverter, I = ()>(PhantomData<(T, BlockNumberConverter, I)>);
	impl<T: Config<I>, BlockNumberConverter, I: 'static> UncheckedOnRuntimeUpgrade
		for MigrateToV2<T, I, BlockNumberConverter>
	where
		BlockNumberConverter: ConvertBlockNumber<LocalBlockNumberFor<T>, NewBlockNumberFor<T, I>>,
	{
		fn on_runtime_upgrade() -> frame_support::weights::Weight {
			// Params conversion
			let _ = Params::<T, I>::translate::<v1::ParamsOf<T, I>, _>(|x| {
				let prev = x.unwrap();
				let _ = prev.demotion_period.into_iter().map(|block_number| {
					BlockNumberConverter::equivalent_time(block_number);
					T::DbWeight::get().reads_writes(1, 1);
				});
				None
			});

			// Member conversion

			T::DbWeight::get().reads_writes(1, 1)
		}
	}
}

/// [`UncheckedOnRuntimeUpgrade`] implementation [`MigrateToV1`] wrapped in a
/// [`VersionedMigration`](frame_support::migrations::VersionedMigration), which ensures that:
/// - The migration only runs once when the on-chain storage version is 0
/// - The on-chain storage version is updated to `1` after the migration executes
/// - Reads/Writes from checking/settings the on-chain storage version are accounted for
pub type MigrateV0ToV1<T, I> = frame_support::migrations::VersionedMigration<
	0, // The migration will only execute when the on-chain storage version is 0
	1, // The on-chain storage version will be set to 1 after the migration is complete
	MigrateToV1<T, I>,
	crate::pallet::Pallet<T, I>,
	<T as frame_system::Config>::DbWeight,
>;

/// [`UncheckedOnRuntimeUpgrade`] implementation [`MigrateToV2`] wrapped in a
/// [`VersionedMigration`](frame_support::migrations::VersionedMigration), which ensures that:
/// - The migration only runs once when the on-chain storage version is `1`.
/// - The on-chain storage version is updated to `2` after the migration executes.
/// - Reads/Writes from checking/settings the on-chain storage version are accounted for.
pub type MigrateV1ToV2<T, BC, I> = frame_support::migrations::VersionedMigration<
	1, // The migration will only execute when the on-chain storage version is 0
	2, // The on-chain storage version will be set to 1 after the migration is complete
	v2::MigrateToV2<T, BC, I>,
	crate::pallet::Pallet<T, I>,
	<T as frame_system::Config>::DbWeight,
>;
