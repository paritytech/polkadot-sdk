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

use super::PALLET_MIGRATIONS_ID;
use crate::{pallet::Config, Multisig, Multisigs};
use frame_support::{
	migrations::{SteppedMigration, SteppedMigrationError},
	pallet_prelude::PhantomData,
	weights::WeightMeter,
};

mod benchmarks;
mod tests;
pub mod weights;

/// Module containing the OLD (v1) storage items.
///
/// Before running this migration, the storage alias defined here represents the
/// `on_chain` storage.
mod v1 {
	use frame_support::{pallet_prelude::*, storage_alias};
	use frame_system::Config as SystemConfig;

	use crate::{pallet::Config, BalanceOf, BlockNumberFor, Pallet, Timepoint};

	/// An open multisig operation.
	#[derive(Decode, Encode)]
	pub struct OldMultisig<BlockNumber, Balance, AccountId, MaxApprovals>
	where
		MaxApprovals: Get<u32>,
	{
		/// The extrinsic when the multisig operation was opened.
		pub when: Timepoint<BlockNumber>,
		/// The amount held in reserve of the `depositor`, to be returned once the operation ends.
		pub deposit: Balance,
		/// The account who opened it (i.e. the first to approve it).
		pub depositor: AccountId,
		/// The approvals achieved so far, including the depositor. Always sorted.
		pub approvals: BoundedVec<AccountId, MaxApprovals>,
	}

	#[storage_alias]
	pub type Multisigs<T: Config> = StorageDoubleMap<
		Pallet<T>,
		Twox64Concat,
		<T as SystemConfig>::AccountId,
		Blake2_128Concat,
		[u8; 32],
		OldMultisig<
			BlockNumberFor<T>,
			BalanceOf<T>,
			<T as SystemConfig>::AccountId,
			<T as Config>::MaxSignatories,
		>,
	>;
}

use crate::{Decode, Encode, MaxEncodedLen};
// TODO: did not want to touch frame/support/src/migrations.rs
// so I moved it here, please double check
#[derive(MaxEncodedLen, Encode, Decode)]
pub struct MigrationId<const N: usize> {
	pub pallet_id: [u8; N],
	pub version_from: u8,
	pub version_to: u8,
}

/// Migrates the items of the [`crate::Multisigs`] map, by wrapping the timepoint value with `Some`,
/// to support new optional timepoint feature.
///
/// The `step` function will be called once per block. It is very important that this function
/// *never* panics and never uses more weight than it got in its meter. The migrations should also
/// try to make maximal progress per step, so that the total time it takes to migrate stays low.
struct LazyMigrationV2<T: Config, W: weights::WeightInfo>(PhantomData<(T, W)>);
impl<T: Config, W: weights::WeightInfo> SteppedMigration for LazyMigrationV2<T, W> {
	type Cursor = (T::AccountId, [u8; 32]);
	// Without the explicit length here the construction of the ID would not be infallible.
	type Identifier = MigrationId<15>;

	/// The identifier of this migration. Which should be globally unique.
	fn id() -> Self::Identifier {
		MigrationId { pallet_id: *PALLET_MIGRATIONS_ID, version_from: 1, version_to: 2 }
	}

	/// The actual logic of the migration.
	///
	/// This function is called repeatedly until it returns `Ok(None)`, indicating that the
	/// migration is complete. Ideally, the migration should be designed in such a way that each
	/// step consumes as much weight as possible. However, this is simplified to perform one stored
	/// value mutation per block.
	fn step(
		mut cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		let required = W::step();
		// If there is not enough weight for a single step, return an error. This case can be
		// problematic if it is the first migration that ran in this block. But there is nothing
		// that we can do about it here.
		if meter.remaining().any_lt(required) {
			return Err(SteppedMigrationError::InsufficientWeight { required });
		}

		// We loop here to do as much progress as possible per step.
		loop {
			if meter.try_consume(required).is_err() {
				break;
			}

			let mut iter = if let Some(last_key) = cursor {
				// If a cursor is provided, start iterating from the stored value
				// corresponding to the last key processed in the previous step.
				// Note that this only works if the old and the new map use the same way to hash
				// storage keys.

				v1::Multisigs::<T>::iter_from(v1::Multisigs::<T>::hashed_key_for(
					last_key.0, last_key.1,
				))
			} else {
				// If no cursor is provided, start iterating from the beginning.
				v1::Multisigs::<T>::iter()
			};

			// If there's a next item in the iterator, perform the migration.
			if let Some((last_key1, last_key2, value)) = iter.next() {
				// Migrate the `when` field (`Timepoint<BlockNumber>`) -> `maybe_when`
				// (`Option<Timepoint<BlockNumber>>`)
				let new_multisig = Multisig {
					maybe_when: Some(value.when),
					deposit: value.deposit,
					depositor: value.depositor,
					approvals: value.approvals,
				};

				// We can just insert here since the old and the new map share the same key-space.
				// Otherwise it would have to invert the concat hash function and re-hash it.
				Multisigs::<T>::insert(last_key1.clone(), last_key2, new_multisig);
				cursor = Some((last_key1, last_key2)) // Return the processed key as the new cursor.
			} else {
				cursor = None; // Signal that the migration is complete (no more items to process).
				break;
			}
		}
		Ok(cursor)
	}
}
