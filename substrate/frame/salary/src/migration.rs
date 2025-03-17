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

//! Storage migrations for the `pallet_salary`.

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
	use frame::prelude::BlockNumberFor as LocalBlockNumberFor;

	/// Converts previous (local) block number into the new one. May just be identity functions
	/// if sticking with local block number as the block provider.
	pub trait ConvertBlockNumber<L, N> {
		/// Simply converts the type from L to N
		///
		/// # Example Usage
		///
		/// ```rust,ignore
		/// // Let's say both L and N are u32, then a simple identity will suffice.
		/// fn convert(local: u32) -> u32 {
		/// 	local
		/// }
		///
		/// // But if L is u64 and N is u32, or some other problematic variation,
		/// // you may need to do some checks.
		/// fn convert(local: u64) -> u32 {
		/// 	let new = u32::try_from(local);
		/// 	match new {
		///    		Ok(v) => v,
		///    		Err(_) => u32::MAX // Or likely some custom logic.
		/// 	}
		/// }
		/// ```
		fn convert(local: L) -> N;

		/// Converts to the new type and finds the equivalent moment in time as from the view of the
		/// new block provider
		///
		/// # Example usage
		///
		/// ```rust,ignore
		/// // Let's say you are a parachain and switching block providers to the relay chain.
		/// // This will return what the relay block number was at the moment the previous provider's
		/// // number was `local_moment`, assuming consistent block times on both chains.
		/// fn equivalent_moment_in_time(local_moment: u32) -> u32 {
		/// 	// How long it's been since 'local_moment' from the parachains pov.
		/// 	let local_block_number = System::block_number();
		/// 	let local_duration = u32::abs_diff(local_block_number, local_moment);
		/// 	// How many blocks that is from the relay's pov.
		/// 	let relay_duration = Self::equivalent_block_duration(local_duration);
		/// 	// What the relay block number must have been at 'local_moment'.
		/// 	let relay_block_number = ParachainSystem::last_relay_block_number();
		/// 	if local_block_number >= local_moment {
		/// 		// Moment was in past.
		/// 		relay_block_number.saturating_sub(relay_duration)
		/// 	} else {
		/// 		// Moment is in future.
		/// 		relay_block_number.saturating_add(relay_duration)
		/// 	}
		/// }
		/// ```
		fn equivalent_moment_in_time(local_moment: L) -> N;

		/// Returns the equivalent number of new blocks it would take to fulfill the same
		/// amount of time in seconds as the old blocks.
		///
		/// For instance - If you previously had 12s blocks and are now following the relay chain's
		/// 6, one local block is equivalent to 2 relay blocks in duration.
		///
		/// # Visualized
		///
		/// ```text
		///     6s         6s
		/// |---------||---------|
		///
		///          12s
		/// |--------------------|
		///
		/// ^ Two 6s relay blocks passed per one 12s local block.
		/// ```
		///
		/// # Example Usage
		///
		/// ```rust,ignore
		/// // Following the scenerio above.
		/// fn equivalent_block_duration(local_duration: u32) -> u32 {
		/// 	local_duration.saturating_mul(2)
		/// }
		/// ```
		fn equivalent_block_duration(local_duration: L) -> N;
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
			let mut transactions = 0;

			// Status storage option
			if let Some(old_status) = v0::Status::<T, I>::take() {
				let new_status = crate::StatusOf::<T, I> {
					cycle_index: BC::convert(old_status.cycle_index),
					cycle_start: BC::equivalent_moment_in_time(old_status.cycle_start),
					budget: old_status.budget,
					total_registrations: old_status.total_registrations,
					total_unregistered_paid: old_status.total_unregistered_paid,
				};
				crate::Status::<T, I>::put(new_status);
				transactions.saturating_inc();
			}

			// Claimant map
			crate::Claimant::<T, I>::translate::<v0::ClaimantStatusOf<T, I>, _>(
				|_, old_claimant| {
					transactions.saturating_inc();
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
			let (status_existed, pre_claimaint_count): (bool, u32) =
				Decode::decode(&mut &state[..]).expect("pre_upgrade provides a valid state; qed");

			ensure!(crate::Status::<T, I>::exists() == status_existed, "The Status' storage existence should remain the same before and after the upgrade.");
			let post_claimant_count = crate::Claimant::<T, I>::iter().count() as u32;
			ensure!(
				post_claimant_count == pre_claimaint_count,
				"The Claimant count should remain the same before and after the upgrade."
			);
			Ok(())
		}
	}
}

/// [`UncheckedOnRuntimeUpgrade`] implementation [`MigrateToV1`](v1::MigrateToV1) wrapped in a
/// [`VersionedMigration`], which ensures that:
/// - The migration only runs once when the on-chain storage version is 0
/// - The on-chain storage version is updated to `1` after the migration executes
/// - Reads/Writes from checking/settings the on-chain storage version are accounted for
pub type MigrateV0ToV1<T, BC, I> = VersionedMigration<
	0, // The migration will only execute when the on-chain storage version is 0
	1, // The on-chain storage version will be set to 1 after the migration is complete
	v1::MigrateToV1<T, BC, I>,
	crate::pallet::Pallet<T, I>,
	<T as frame_system::Config>::DbWeight,
>;
