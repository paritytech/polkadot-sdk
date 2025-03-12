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

//! Storage migrations for the Staking-Next pallet.

use super::*;
use frame_support::{
	migrations::VersionedMigration, pallet_prelude::ValueQuery, storage_alias,
	traits::UncheckedOnRuntimeUpgrade, Twox64Concat,
};

#[cfg(feature = "try-runtime")]
use frame_support::ensure;
#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

/// Used for release versioning up to v12.
///
/// Obsolete from v13. Keeping around to make encoding/decoding of old migration code easier.
#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
enum ObsoleteReleases {
	V5_0_0,  // blockable validators.
	V6_0_0,  // removal of all storage associated with offchain phragmen.
	V7_0_0,  // keep track of number of nominators / validators in map
	V8_0_0,  // populate `VoterList`.
	V9_0_0,  // inject validators into `VoterList` as well.
	V10_0_0, // remove `EarliestUnappliedSlash`.
	V11_0_0, // Move pallet storage prefix, e.g. BagsList -> VoterBagsList
	V12_0_0, // remove `HistoryDepth`.
}

impl Default for ObsoleteReleases {
	fn default() -> Self {
		ObsoleteReleases::V12_0_0
	}
}

/// Alias to the old storage item used for release versioning. Obsolete since v13.
#[storage_alias]
type StorageVersion<T: Config> = StorageValue<Pallet<T>, ObsoleteReleases, ValueQuery>;

/// Migrates `UnappliedSlashes` to a new storage structure to support paged slashing.
/// This ensures that slashing can be processed in batches, preventing large storage operations in a
/// single block.
pub mod v17 {
	use super::*;

	#[derive(Encode, Decode, TypeInfo, MaxEncodedLen)]
	struct OldUnappliedSlash<T: Config> {
		validator: T::AccountId,
		/// The validator's own slash.
		own: BalanceOf<T>,
		/// All other slashed stakers and amounts.
		others: Vec<(T::AccountId, BalanceOf<T>)>,
		/// Reporters of the offence; bounty payout recipients.
		reporters: Vec<T::AccountId>,
		/// The amount of payout.
		payout: BalanceOf<T>,
	}

	#[frame_support::storage_alias]
	pub type OldUnappliedSlashes<T: Config> =
		StorageMap<Pallet<T>, Twox64Concat, EraIndex, Vec<OldUnappliedSlash<T>>, ValueQuery>;

	#[frame_support::storage_alias]
	pub type DisabledValidators<T: Config> =
		StorageValue<Pallet<T>, BoundedVec<(u32, OffenceSeverity), ConstU32<100>>, ValueQuery>;

	pub struct VersionUncheckedMigrateV16ToV17<T>(core::marker::PhantomData<T>);
	impl<T: Config> UncheckedOnRuntimeUpgrade for VersionUncheckedMigrateV16ToV17<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight: Weight = Weight::zero();

			OldUnappliedSlashes::<T>::drain().for_each(|(era, slashes)| {
				weight.saturating_accrue(T::DbWeight::get().reads(1));

				for slash in slashes {
					let validator = slash.validator.clone();
					let new_slash = UnappliedSlash {
						validator: validator.clone(),
						own: slash.own,
						others: WeakBoundedVec::force_from(slash.others, None),
						payout: slash.payout,
						reporter: slash.reporters.first().cloned(),
					};

					// creating a slash key which is improbable to conflict with a new offence.
					let slash_key = (validator, Perbill::from_percent(99), 9999);
					UnappliedSlashes::<T>::insert(era, slash_key, new_slash);
					weight.saturating_accrue(T::DbWeight::get().writes(1));
				}
			});

			weight
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			let mut expected_slashes: u32 = 0;
			OldUnappliedSlashes::<T>::iter().for_each(|(_, slashes)| {
				expected_slashes += slashes.len() as u32;
			});

			Ok(expected_slashes.encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
			let expected_slash_count =
				u32::decode(&mut state.as_slice()).expect("Failed to decode state");

			let actual_slash_count = UnappliedSlashes::<T>::iter().count() as u32;

			ensure!(expected_slash_count == actual_slash_count, "Slash count mismatch");

			Ok(())
		}
	}

	pub type MigrateV16ToV17<T> = VersionedMigration<
		16,
		17,
		VersionUncheckedMigrateV16ToV17<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;

	pub struct MigrateDisabledToSession<T>(core::marker::PhantomData<T>);
	impl<T: Config> pallet_session::migrations::v1::MigrateDisabledValidators
		for MigrateDisabledToSession<T>
	{
		#[cfg(feature = "try-runtime")]
		fn peek_disabled() -> Vec<(u32, OffenceSeverity)> {
			DisabledValidators::<T>::get().into()
		}

		fn take_disabled() -> Vec<(u32, OffenceSeverity)> {
			DisabledValidators::<T>::take().into()
		}
	}
}

/// Migrating `DisabledValidators` from `Vec<u32>` to `Vec<(u32, OffenceSeverity)>` to track offense
/// severity for re-enabling purposes.
pub mod v16 {
	use super::*;
	use frame_support::Twox64Concat;
	use sp_staking::offence::OffenceSeverity;

	#[frame_support::storage_alias]
	pub(crate) type Invulnerables<T: Config> =
		StorageValue<Pallet<T>, Vec<<T as frame_system::Config>::AccountId>, ValueQuery>;

	#[frame_support::storage_alias]
	pub(crate) type DisabledValidators<T: Config> =
		StorageValue<Pallet<T>, Vec<(u32, OffenceSeverity)>, ValueQuery>;

	#[frame_support::storage_alias]
	pub(crate) type ErasStakers<T: Config> = StorageDoubleMap<
		Pallet<T>,
		Twox64Concat,
		EraIndex,
		Twox64Concat,
		<T as frame_system::Config>::AccountId,
		Exposure<<T as frame_system::Config>::AccountId, BalanceOf<T>>,
		ValueQuery,
	>;

	#[frame_support::storage_alias]
	pub(crate) type ErasStakersClipped<T: Config> = StorageDoubleMap<
		Pallet<T>,
		Twox64Concat,
		EraIndex,
		Twox64Concat,
		<T as frame_system::Config>::AccountId,
		Exposure<<T as frame_system::Config>::AccountId, BalanceOf<T>>,
		ValueQuery,
	>;

	pub struct VersionUncheckedMigrateV15ToV16<T>(core::marker::PhantomData<T>);
	impl<T: Config> UncheckedOnRuntimeUpgrade for VersionUncheckedMigrateV15ToV16<T> {
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			let old_disabled_validators = v15::DisabledValidators::<T>::get();
			Ok(old_disabled_validators.encode())
		}

		fn on_runtime_upgrade() -> Weight {
			// Migrating `DisabledValidators` from `Vec<u32>` to `Vec<(u32, OffenceSeverity)>`.
			// Using max severity (PerBill 100%) for the migration which effectively makes it so
			// offenders before the migration will not be re-enabled this era unless there are
			// other 100% offenders.
			let max_offence = OffenceSeverity(Perbill::from_percent(100));
			// Inject severity
			let migrated = v15::DisabledValidators::<T>::take()
				.into_iter()
				.map(|v| (v, max_offence))
				.collect::<Vec<_>>();

			v16::DisabledValidators::<T>::set(migrated);

			log!(info, "v16 applied successfully.");
			T::DbWeight::get().reads_writes(1, 1)
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
			// Decode state to get old_disabled_validators in a format of Vec<u32>
			let old_disabled_validators =
				Vec::<u32>::decode(&mut state.as_slice()).expect("Failed to decode state");
			let new_disabled_validators = v17::DisabledValidators::<T>::get();

			// Compare lengths
			frame_support::ensure!(
				old_disabled_validators.len() == new_disabled_validators.len(),
				"DisabledValidators length mismatch"
			);

			// Compare contents
			let new_disabled_validators =
				new_disabled_validators.into_iter().map(|(v, _)| v).collect::<Vec<_>>();
			frame_support::ensure!(
				old_disabled_validators == new_disabled_validators,
				"DisabledValidator ids mismatch"
			);

			// Verify severity
			let max_severity = OffenceSeverity(Perbill::from_percent(100));
			let new_disabled_validators = v17::DisabledValidators::<T>::get();
			for (_, severity) in new_disabled_validators {
				frame_support::ensure!(severity == max_severity, "Severity mismatch");
			}

			Ok(())
		}
	}

	pub type MigrateV15ToV16<T> = VersionedMigration<
		15,
		16,
		VersionUncheckedMigrateV15ToV16<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}

pub mod v15 {
	use super::*;
	#[storage_alias]
	pub(crate) type DisabledValidators<T: Config> = StorageValue<Pallet<T>, Vec<u32>, ValueQuery>;
}
