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

//! Storage migrations for `pallet_salary`.

use super::*;
use frame::{
	deps::frame_support::migrations::VersionedMigration, storage_alias,
	traits::UncheckedOnRuntimeUpgrade,
};

#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;
#[cfg(feature = "try-runtime")]
use frame::try_runtime::TryRuntimeError;

mod v0 {
	use super::*;
	use frame::prelude::BlockNumberFor as LocalBlockNumberFor;

	// V0 types.
	pub type CycleIndexOf<T> = LocalBlockNumberFor<T>;
	pub type StatusOf<T, I> = StatusType<CycleIndexOf<T>, LocalBlockNumberFor<T>, BalanceOf<T, I>>;
	pub type ClaimantStatusOf<T, I> = ClaimantStatus<CycleIndexOf<T>, BalanceOf<T, I>, IdOf<T, I>>;

	/// V0 alias for [`crate::Status`].
	#[storage_alias]
	pub type Status<T: Config<I>, I: 'static> =
		StorageValue<Pallet<T, I>, StatusOf<T, I>, OptionQuery>;

	/// V0 alias for [`crate::Claimant`].
	#[storage_alias]
	pub type Claimant<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Twox64Concat,
		<T as frame_system::Config>::AccountId,
		ClaimantStatusOf<T, I>,
		OptionQuery,
	>;
}

pub mod v1 {
	use super::{pallet::BlockNumberFor as NewBlockNumberFor, *};
	use frame::prelude::{BlockNumberFor as LocalBlockNumberFor, BlockNumberProvider, Saturating};

	/// Converts the old block-number type into the new block-number type.
	///
	/// The old provider was implicitly `frame_system::Pallet`. The new provider is selected by
	/// the runtime through [`Config::BlockNumberProvider`].
	pub trait ConvertBlockNumber<L, N> {
		/// Converts an old stored block-number value into the new type.
		///
		/// # Example Usage
		///
		/// ```rust,ignore
		/// // Same type: identity conversion is enough.
		/// fn convert(old: u32) -> u32 {
		/// 	old
		/// }
		///
		/// // Different type: use explicit bounds or fallback behavior.
		/// fn convert(old: u64) -> u32 {
		/// 	u32::try_from(old).unwrap_or(u32::MAX)
		/// }
		/// ```
		fn convert(old: L) -> N;

		/// Converts an old block-number moment into the equivalent moment for the new provider.
		///
		/// # Example Usage
		///
		/// ```rust,ignore
		/// // A parachain switching salary timing from local block numbers to relay-chain block
		/// // numbers can map an old local moment onto the relay block at the same wall-clock time.
		/// fn equivalent_moment_in_time(old_local_moment: u32) -> u32 {
		/// 	let current_local_block = System::block_number();
		/// 	let local_duration = u32::abs_diff(current_local_block, old_local_moment);
		/// 	let relay_duration = Self::equivalent_block_duration(local_duration);
		/// 	let current_relay_block = ParachainSystem::last_relay_block_number();
		///
		/// 	if current_local_block >= old_local_moment {
		/// 		current_relay_block.saturating_sub(relay_duration)
		/// 	} else {
		/// 		current_relay_block.saturating_add(relay_duration)
		/// 	}
		/// }
		/// ```
		fn equivalent_moment_in_time(old_moment: L) -> N;

		/// Converts an old block duration into an equivalent duration for the new provider.
		///
		/// For example, if the old provider used 12 second blocks and the new provider uses 6
		/// second blocks, one old block is equivalent to two new blocks.
		fn equivalent_block_duration(old_duration: L) -> N;
	}

	/// Converts block numbers between providers with the same block number type and block
	/// duration.
	///
	/// This is useful when switching from [`frame_system::Pallet`] to another block number
	/// provider on a chain where both providers advance at the same rate. Cycle indexes are copied
	/// directly, while stored moments are translated relative to the current moment of each
	/// provider.
	pub struct SameBlockDurationConverter<OldProvider, NewProvider>(
		PhantomData<(OldProvider, NewProvider)>,
	);

	impl<OldProvider, NewProvider, BlockNumber> ConvertBlockNumber<BlockNumber, BlockNumber>
		for SameBlockDurationConverter<OldProvider, NewProvider>
	where
		OldProvider: BlockNumberProvider<BlockNumber = BlockNumber>,
		NewProvider: BlockNumberProvider<BlockNumber = BlockNumber>,
		BlockNumber: Copy + Ord + Saturating,
	{
		fn convert(old: BlockNumber) -> BlockNumber {
			old
		}

		fn equivalent_moment_in_time(old_moment: BlockNumber) -> BlockNumber {
			let old_block_number = OldProvider::current_block_number();
			let old_duration = Self::equivalent_block_duration(if old_block_number >= old_moment {
				old_block_number.saturating_sub(old_moment)
			} else {
				old_moment.saturating_sub(old_block_number)
			});
			let new_block_number = NewProvider::current_block_number();
			if old_block_number >= old_moment {
				new_block_number.saturating_sub(old_duration)
			} else {
				new_block_number.saturating_add(old_duration)
			}
		}

		fn equivalent_block_duration(old_duration: BlockNumber) -> BlockNumber {
			old_duration
		}
	}

	pub struct MigrateToV1<T, BC, I = ()>(PhantomData<(T, BC, I)>);

	impl<T: Config<I>, BC, I: 'static> UncheckedOnRuntimeUpgrade for MigrateToV1<T, BC, I>
	where
		BC: ConvertBlockNumber<LocalBlockNumberFor<T>, NewBlockNumberFor<T, I>>,
	{
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			let status_exists = v0::Status::<T, I>::exists();
			let claimant_count = v0::Claimant::<T, I>::iter().count() as u32;
			Ok((status_exists, claimant_count).encode())
		}

		fn on_runtime_upgrade() -> Weight {
			let mut transactions = 0u64;

			if let Some(old_status) = v0::Status::<T, I>::take() {
				let new_status = crate::StatusOf::<T, I> {
					cycle_index: BC::convert(old_status.cycle_index),
					cycle_start: BC::equivalent_moment_in_time(old_status.cycle_start),
					budget: old_status.budget,
					total_registrations: old_status.total_registrations,
					total_unregistered_paid: old_status.total_unregistered_paid,
				};
				crate::Status::<T, I>::put(new_status);
				transactions = transactions.saturating_add(1);
			}

			crate::Claimant::<T, I>::translate::<v0::ClaimantStatusOf<T, I>, _>(
				|_, old_claimant| {
					transactions = transactions.saturating_add(1);
					Some(crate::ClaimantStatusOf::<T, I> {
						last_active: BC::convert(old_claimant.last_active),
						status: old_claimant.status,
					})
				},
			);

			T::DbWeight::get().reads_writes(transactions, transactions)
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
			let (status_existed, pre_claimant_count): (bool, u32) =
				Decode::decode(&mut &state[..]).expect("pre_upgrade provides valid state; qed");

			ensure!(
				crate::Status::<T, I>::exists() == status_existed,
				"Status storage existence should remain the same before and after the upgrade."
			);

			let post_claimant_count = crate::Claimant::<T, I>::iter().count() as u32;
			ensure!(
				post_claimant_count == pre_claimant_count,
				"Claimant count should remain the same before and after the upgrade."
			);
			Ok(())
		}
	}
}

/// [`UncheckedOnRuntimeUpgrade`] implementation [`MigrateToV1`](v1::MigrateToV1) wrapped in a
/// [`VersionedMigration`], which ensures that:
///
/// - The migration only runs once when the on-chain storage version is 0.
/// - The on-chain storage version is updated to 1 after the migration executes.
/// - Reads and writes from checking and setting the on-chain storage version are accounted for.
pub type MigrateV0ToV1<T, BC, I> = VersionedMigration<
	0,
	1,
	v1::MigrateToV1<T, BC, I>,
	crate::pallet::Pallet<T, I>,
	<T as frame_system::Config>::DbWeight,
>;

/// Salary v0 to v1 migration for runtimes switching from [`frame_system::Pallet`] to
/// [`Config::BlockNumberProvider`] where both providers use the same block number type and block
/// duration.
pub type MigrateV0ToV1SameBlockDuration<T, I> = MigrateV0ToV1<
	T,
	v1::SameBlockDurationConverter<frame_system::Pallet<T>, <T as Config<I>>::BlockNumberProvider>,
	I,
>;
