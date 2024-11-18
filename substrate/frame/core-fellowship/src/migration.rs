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
	use super::*;
	use frame_system::pallet_prelude::BlockNumberFor as SystemBlockNumberFor;

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

	pub type ParamsOf<T, I> =
		ParamsType<<T as Config<I>>::Balance, SystemBlockNumberFor<T>, RANK_COUNT>;

	/// V0 type for [`crate::Params`].
	#[storage_alias]
	pub type Params<T: Config<I>, I: 'static> =
		StorageValue<Pallet<T, I>, ParamsOf<T, I>, ValueQuery>;
}

mod v1 {
	use super::*;
	use frame_system::pallet_prelude::BlockNumberFor as SystemBlockNumberFor;

	#[derive(
		Encode,
		Decode,
		CloneNoBound,
		EqNoBound,
		PartialEqNoBound,
		RuntimeDebugNoBound,
		TypeInfo,
		MaxEncodedLen,
	)]
	#[scale_info(skip_type_params(Ranks))]
	pub struct ParamsType<
		Balance: Clone + Eq + PartialEq + Debug,
		BlockNumber: Clone + Eq + PartialEq + Debug,
		Ranks: Get<u32>,
	> {
		pub active_salary: BoundedVec<Balance, Ranks>,
		pub passive_salary: BoundedVec<Balance, Ranks>,
		pub demotion_period: BoundedVec<BlockNumber, Ranks>,
		pub min_promotion_period: BoundedVec<BlockNumber, Ranks>,
		pub offboard_timeout: BlockNumber,
	}

	impl<Balance, BlockNumber, Ranks: Get<u32>> Default for ParamsType<Balance, BlockNumber, Ranks>
	where
		Balance: Default + Copy + Clone + Eq + PartialEq + Debug,
		BlockNumber: Default + Copy + Clone + Eq + PartialEq + Debug,
		Ranks: Get<u32>,
	{
		fn default() -> Self {
			let rank_count = Ranks::get() as usize;
			Self {
				active_salary: BoundedVec::defensive_truncate_from(vec![
					Balance::default();
					rank_count
				]),
				passive_salary: BoundedVec::defensive_truncate_from(vec![
					Balance::default();
					rank_count
				]),
				demotion_period: BoundedVec::defensive_truncate_from(vec![
					BlockNumber::default();
					rank_count
				]),
				min_promotion_period: BoundedVec::defensive_truncate_from(vec![
					BlockNumber::default(
					);
					rank_count
				]),
				offboard_timeout: BlockNumber::default(),
			}
		}
	}

	pub type ParamsOf<T, I> =
		ParamsType<<T as Config<I>>::Balance, SystemBlockNumberFor<T>, <T as Config<I>>::MaxRank>;

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
		let new = v1::ParamsType {
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
