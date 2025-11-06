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

//! Storage migrations for the Staking pallet. The changelog for this is maintained at
//! [CHANGELOG.md](https://github.com/paritytech/polkadot-sdk/blob/master/substrate/frame/staking/CHANGELOG.md).

use super::*;
use frame_support::{
	migrations::VersionedMigration,
	pallet_prelude::ValueQuery,
	storage_alias,
	traits::{GetStorageVersion, OnRuntimeUpgrade, UncheckedOnRuntimeUpgrade},
};

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

/// Supports the migration of Validator Disabling from pallet-staking to pallet-session
pub mod v17 {
	use super::*;

	#[frame_support::storage_alias]
	pub type DisabledValidators<T: Config> =
		StorageValue<Pallet<T>, BoundedVec<(u32, OffenceSeverity), ConstU32<333>>, ValueQuery>;

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
	use sp_staking::offence::OffenceSeverity;

	#[frame_support::storage_alias]
	pub(crate) type DisabledValidators<T: Config> =
		StorageValue<Pallet<T>, Vec<(u32, OffenceSeverity)>, ValueQuery>;

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

/// Migrating `OffendingValidators` from `Vec<(u32, bool)>` to `Vec<u32>`
pub mod v15 {
	use super::*;

	// The disabling strategy used by staking pallet
	type DefaultDisablingStrategy = pallet_session::disabling::UpToLimitDisablingStrategy;

	#[storage_alias]
	pub(crate) type DisabledValidators<T: Config> = StorageValue<Pallet<T>, Vec<u32>, ValueQuery>;

	pub struct VersionUncheckedMigrateV14ToV15<T>(core::marker::PhantomData<T>);
	impl<T: Config> UncheckedOnRuntimeUpgrade for VersionUncheckedMigrateV14ToV15<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut migrated = v14::OffendingValidators::<T>::take()
				.into_iter()
				.filter(|p| p.1) // take only disabled validators
				.map(|p| p.0)
				.collect::<Vec<_>>();

			// Respect disabling limit
			migrated.truncate(DefaultDisablingStrategy::disable_limit(
				T::SessionInterface::validators().len(),
			));

			DisabledValidators::<T>::set(migrated);

			log!(info, "v15 applied successfully.");
			T::DbWeight::get().reads_writes(1, 1)
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
			frame_support::ensure!(
				v14::OffendingValidators::<T>::decode_len().is_none(),
				"OffendingValidators is not empty after the migration"
			);
			Ok(())
		}
	}

	pub type MigrateV14ToV15<T> = VersionedMigration<
		14,
		15,
		VersionUncheckedMigrateV14ToV15<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}

/// Migration of era exposure storage items to paged exposures.
/// Changelog: [v14.](https://github.com/paritytech/substrate/blob/ankan/paged-rewards-rebased2/frame/staking/CHANGELOG.md#14)
pub mod v14 {
	use super::*;

	#[frame_support::storage_alias]
	pub(crate) type OffendingValidators<T: Config> =
		StorageValue<Pallet<T>, Vec<(u32, bool)>, ValueQuery>;

	pub struct MigrateToV14<T>(core::marker::PhantomData<T>);
	impl<T: Config> OnRuntimeUpgrade for MigrateToV14<T> {
		fn on_runtime_upgrade() -> Weight {
			let in_code = Pallet::<T>::in_code_storage_version();
			let on_chain = Pallet::<T>::on_chain_storage_version();

			if in_code == 14 && on_chain == 13 {
				in_code.put::<Pallet<T>>();

				log!(info, "staking v14 applied successfully.");
				T::DbWeight::get().reads_writes(1, 1)
			} else {
				log!(warn, "staking v14 not applied.");
				T::DbWeight::get().reads(1)
			}
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
			frame_support::ensure!(
				Pallet::<T>::on_chain_storage_version() >= 14,
				"v14 not applied"
			);
			Ok(())
		}
	}
}

pub mod v13 {
	use super::*;

	pub struct MigrateToV13<T>(core::marker::PhantomData<T>);
	impl<T: Config> OnRuntimeUpgrade for MigrateToV13<T> {
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			frame_support::ensure!(
				StorageVersion::<T>::get() == ObsoleteReleases::V12_0_0,
				"Required v12 before upgrading to v13"
			);

			Ok(Default::default())
		}

		fn on_runtime_upgrade() -> Weight {
			let in_code = Pallet::<T>::in_code_storage_version();
			let onchain = StorageVersion::<T>::get();

			if in_code == 13 && onchain == ObsoleteReleases::V12_0_0 {
				StorageVersion::<T>::kill();
				in_code.put::<Pallet<T>>();

				log!(info, "v13 applied successfully");
				T::DbWeight::get().reads_writes(1, 2)
			} else {
				log!(warn, "Skipping v13, should be removed");
				T::DbWeight::get().reads(1)
			}
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
			frame_support::ensure!(
				Pallet::<T>::on_chain_storage_version() == 13,
				"v13 not applied"
			);

			frame_support::ensure!(
				!StorageVersion::<T>::exists(),
				"Storage version not migrated correctly"
			);

			Ok(())
		}
	}
}

pub mod v12 {
	use super::*;
	use frame_support::{pallet_prelude::ValueQuery, storage_alias};

	#[storage_alias]
	type HistoryDepth<T: Config> = StorageValue<Pallet<T>, u32, ValueQuery>;

	/// Clean up `T::HistoryDepth` from storage.
	///
	/// We will be depending on the configurable value of `T::HistoryDepth` post
	/// this release.
	pub struct MigrateToV12<T>(core::marker::PhantomData<T>);
	impl<T: Config> OnRuntimeUpgrade for MigrateToV12<T> {
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			frame_support::ensure!(
				StorageVersion::<T>::get() == ObsoleteReleases::V11_0_0,
				"Expected v11 before upgrading to v12"
			);

			if HistoryDepth::<T>::exists() {
				frame_support::ensure!(
					T::HistoryDepth::get() == HistoryDepth::<T>::get(),
					"Provided value of HistoryDepth should be same as the existing storage value"
				);
			} else {
				log::info!("No HistoryDepth in storage; nothing to remove");
			}

			Ok(Default::default())
		}

		fn on_runtime_upgrade() -> frame_support::weights::Weight {
			if StorageVersion::<T>::get() == ObsoleteReleases::V11_0_0 {
				HistoryDepth::<T>::kill();
				StorageVersion::<T>::put(ObsoleteReleases::V12_0_0);

				log!(info, "v12 applied successfully");
				T::DbWeight::get().reads_writes(1, 2)
			} else {
				log!(warn, "Skipping v12, should be removed");
				T::DbWeight::get().reads(1)
			}
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
			frame_support::ensure!(
				StorageVersion::<T>::get() == ObsoleteReleases::V12_0_0,
				"v12 not applied"
			);
			Ok(())
		}
	}
}

pub mod v11 {
	use super::*;
	use frame_support::{
		storage::migration::move_pallet,
		traits::{GetStorageVersion, PalletInfoAccess},
	};
	#[cfg(feature = "try-runtime")]
	use sp_io::hashing::twox_128;

	pub struct MigrateToV11<T, P, N>(core::marker::PhantomData<(T, P, N)>);
	impl<T: Config, P: GetStorageVersion + PalletInfoAccess, N: Get<&'static str>> OnRuntimeUpgrade
		for MigrateToV11<T, P, N>
	{
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			frame_support::ensure!(
				StorageVersion::<T>::get() == ObsoleteReleases::V10_0_0,
				"must upgrade linearly"
			);
			let old_pallet_prefix = twox_128(N::get().as_bytes());

			frame_support::ensure!(
				sp_io::storage::next_key(&old_pallet_prefix).is_some(),
				"no data for the old pallet name has been detected"
			);

			Ok(Default::default())
		}

		/// Migrate the entire storage of this pallet to a new prefix.
		///
		/// This new prefix must be the same as the one set in construct_runtime. For safety, use
		/// `PalletInfo` to get it, as:
		/// `<Runtime as frame_system::Config>::PalletInfo::name::<VoterBagsList>`.
		///
		/// The migration will look into the storage version in order to avoid triggering a
		/// migration on an up to date storage.
		fn on_runtime_upgrade() -> Weight {
			let old_pallet_name = N::get();
			let new_pallet_name = <P as PalletInfoAccess>::name();

			if StorageVersion::<T>::get() == ObsoleteReleases::V10_0_0 {
				// bump version anyway, even if we don't need to move the prefix
				StorageVersion::<T>::put(ObsoleteReleases::V11_0_0);
				if new_pallet_name == old_pallet_name {
					log!(
						warn,
						"new bags-list name is equal to the old one, only bumping the version"
					);
					return T::DbWeight::get().reads(1).saturating_add(T::DbWeight::get().writes(1))
				}

				move_pallet(old_pallet_name.as_bytes(), new_pallet_name.as_bytes());
				<T as frame_system::Config>::BlockWeights::get().max_block
			} else {
				log!(warn, "v11::migrate should be removed.");
				T::DbWeight::get().reads(1)
			}
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
			frame_support::ensure!(
				StorageVersion::<T>::get() == ObsoleteReleases::V11_0_0,
				"wrong version after the upgrade"
			);

			let old_pallet_name = N::get();
			let new_pallet_name = <P as PalletInfoAccess>::name();

			// skip storage prefix checks for the same pallet names
			if new_pallet_name == old_pallet_name {
				return Ok(())
			}

			let old_pallet_prefix = twox_128(N::get().as_bytes());
			frame_support::ensure!(
				sp_io::storage::next_key(&old_pallet_prefix).is_none(),
				"old pallet data hasn't been removed"
			);

			let new_pallet_name = <P as PalletInfoAccess>::name();
			let new_pallet_prefix = twox_128(new_pallet_name.as_bytes());
			frame_support::ensure!(
				sp_io::storage::next_key(&new_pallet_prefix).is_some(),
				"new pallet data hasn't been created"
			);

			Ok(())
		}
	}
}
