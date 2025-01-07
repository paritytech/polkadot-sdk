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

//! Storage migrations for the salary pallet.

use super::*;
use frame_support::{pallet_prelude::*, storage_alias, traits::UncheckedOnRuntimeUpgrade};

mod v0 {
	use super::*;
	use frame_system::pallet_prelude::BlockNumberFor as LocalBlockNumberFor;

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
	use super::{BlockNumberFor as NewBlockNumberFor, *};
	use frame_system::pallet_prelude::BlockNumberFor as LocalBlockNumberFor;

	/// Converts previous (local) block number into the new one. May just be identity functions
	/// if sticking with local block number as the provider.
	pub trait ConvertBlockNumber<L, N> {
		/// Simply converts the type from L to N
		///
		/// # Example Usage
		///
		/// ```rust,ignore
		/// // Let's say both L and N are u32, then a simple identity will suffice.
		/// fn convert(local: u32) -> u32 {
		/// 		local
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

		/// Converts to the new type and finds the equivalent moment in time as relative to the new
		/// block provider
		///
		/// For instance - if your new version uses the relay chain number, you'll want to
		/// use relay current - ((current local - local) * equivalent_block_duration).
		/// Note: This assumes consistent block times on both local chain and relay.
		///
		/// # Example usage
		///
		/// ```rust,ignore
		/// // Let's say you are a parachain and switching block providers to the relay chain.
		/// // This will return what the relay block number was at the moment the previous provider's
		/// // number was `local`.
		/// fn equivalent_moment_in_time(local: u32) -> u32 {
		/// 	// How long it's been since 'local'.
		/// 	let curr_block_number = System::block_number();
		/// 	let local_duration = curr_block_number.saturating_sub(local);
		/// 	// How many blocks that is in relay block time.
		/// 	let relay_duration = Self::equivalent_block_duration(local_duration);
		/// 	// What the relay block number must have been at that moment.
		/// 	let relay_block_number = ParachainSystem::last_relay_block_number();
		/// 	relay_block_number.saturating_sub(relay_duration)
		/// }
		/// ```
		fn equivalent_moment_in_time(local: L) -> N;

		/// Returns the equivalent number of new blocks it would take to fulfill the same
		/// amount of time in seconds as the old blocks.
		///
		/// For instance - If you previously had 12s blocks and are now following the relay chain's
		/// 6, one local block is equivalent to 2 relay blocks in duration.
		///
		///     6s         6s
		/// |---------||---------|
		///
		///          12s
		/// |--------------------|
		///
		/// ^ Two 6s blocks passed per one 12s block.
		///  
		/// # Example Usage
		///
		/// ```rust,ignore
		/// // If your previous provider updated the block number every 12 seconds and
		/// // your new updates every 6 - 2 new blocks would have taken place per one old block.
		/// fn equivalent_block_duration(local: u32) -> u32 {
		/// 	local * 2
		/// }
		/// ```
		fn equivalent_block_duration(local: L) -> N;
	}

	pub struct MigrateToV1<T, BC, I = ()>(PhantomData<(T, BC, I)>);
	impl<T: Config<I>, BC, I: 'static> UncheckedOnRuntimeUpgrade for MigrateToV1<T, BC, I>
	where
		BC: ConvertBlockNumber<LocalBlockNumberFor<T>, NewBlockNumberFor<T, I>>,
	{
		fn on_runtime_upgrade() -> frame_support::weights::Weight {
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
	}
}

/// [`UncheckedOnRuntimeUpgrade`] implementation [`MigrateToV1`] wrapped in a
/// [`VersionedMigration`](frame_support::migrations::VersionedMigration), which ensures that:
/// - The migration only runs once when the on-chain storage version is 0
/// - The on-chain storage version is updated to `1` after the migration executes
/// - Reads/Writes from checking/settings the on-chain storage version are accounted for
pub type MigrateV0ToV1<T, BC, I> = frame_support::migrations::VersionedMigration<
	0, // The migration will only execute when the on-chain storage version is 0
	1, // The on-chain storage version will be set to 1 after the migration is complete
	v1::MigrateToV1<T, BC, I>,
	crate::pallet::Pallet<T, I>,
	<T as frame_system::Config>::DbWeight,
>;
