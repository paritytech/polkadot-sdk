// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>

use frame_support::pallet_prelude::StorageVersion;

mod test;

pub const LOG_TARGET: &str = "runtime::storage::ethereum-client-migration";

/// Module containing the old Ethereum execution headers that should be cleaned up.

/// The in-code storage version.

pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);
pub mod v0 {
	use crate::pallet::{Config, Pallet};
	use frame_support::{
		pallet_prelude::{Decode, Encode, MaxEncodedLen, OptionQuery, TypeInfo, ValueQuery},
		storage_alias, CloneNoBound, Identity, PartialEqNoBound, RuntimeDebugNoBound,
	};
	use sp_core::H256;

	#[storage_alias]
	pub type LatestExecutionState<T: Config> =
		StorageValue<Pallet<T>, ExecutionHeaderState, ValueQuery>;

	#[storage_alias]
	pub type ExecutionHeaders<T: Config> =
		StorageMap<Pallet<T>, Identity, H256, CompactExecutionHeader, OptionQuery>;

	#[storage_alias]
	pub type ExecutionHeaderIndex<T: Config> = StorageValue<Pallet<T>, u32, ValueQuery>;

	#[storage_alias]
	pub type ExecutionHeaderMapping<T: Config> =
		StorageMap<Pallet<T>, Identity, u32, H256, ValueQuery>;

	#[derive(Copy, Clone, Default, Encode, Decode, Debug, TypeInfo, MaxEncodedLen, PartialEq)]
	pub struct ExecutionHeaderState {
		pub beacon_block_root: H256,
		pub beacon_slot: u64,
		pub block_hash: H256,
		pub block_number: u64,
	}

	#[derive(
		Default,
		Encode,
		Decode,
		CloneNoBound,
		PartialEqNoBound,
		RuntimeDebugNoBound,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub struct CompactExecutionHeader {
		pub parent_hash: H256,
		#[codec(compact)]
		pub block_number: u64,
		pub state_root: H256,
		pub receipts_root: H256,
	}
}

pub mod v0_to_v1 {
	extern crate alloc;
	use crate::{migration::LOG_TARGET, pallet::Config, WeightInfo};
	#[cfg(feature = "try-runtime")]
	use frame_support::traits::GetStorageVersion;
	use frame_support::{
		migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
		pallet_prelude::{PhantomData, StorageVersion, Weight},
		traits::OnRuntimeUpgrade,
		weights::{constants::RocksDbWeight, WeightMeter},
	};
	use sp_core::Get;
	use sp_core::H256;
	#[cfg(feature = "try-runtime")]
	use sp_runtime::TryRuntimeError;

	pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	pub const PALLET_MIGRATIONS_ID: &[u8; 26] = b"ethereum-execution-headers";

	pub struct EthereumExecutionHeaderCleanup<T: Config, W: WeightInfo, M: Get<u32>>(
		PhantomData<(T, W, M)>,
	);
	impl<T: Config, W: WeightInfo, M: Get<u32>> SteppedMigration
		for EthereumExecutionHeaderCleanup<T, W, M>
	{
		type Cursor = u32;
		type Identifier = MigrationId<26>; // Length of the migration ID PALLET_MIGRATIONS_ID

		fn id() -> Self::Identifier {
			MigrationId { pallet_id: *PALLET_MIGRATIONS_ID, version_from: 0, version_to: 1 }
		}

		fn step(
			mut cursor: Option<Self::Cursor>,
			meter: &mut WeightMeter,
		) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
			log::info!(target: LOG_TARGET, "Starting step iteration for Ethereum execution header cleanup.");
			let required = W::step();
			if meter.remaining().any_lt(required) {
				return Err(SteppedMigrationError::InsufficientWeight { required });
			}

			if let Some(last_key) = cursor {
				log::info!(target: LOG_TARGET, "Last key is {}. Max value is {}", last_key, M::get());
			} else {
				log::info!(target: LOG_TARGET, "Error getting last key");
			};

			// We loop here to do as much progress as possible per step.
			loop {
				if meter.try_consume(required).is_err() {
					log::info!(target: LOG_TARGET, "Max weight consumed, exiting loop");
					break;
				}

				let index = if let Some(last_key) = cursor {
					last_key.saturating_add(1)
				} else {
					log::info!(target: LOG_TARGET, "Cursor is 0, starting migration.");
					// If no cursor is provided, start iterating from the beginning.
					0
				};
				if index == 1 {
					let execution_hash_extra1 =
						crate::migration::v0::ExecutionHeaderMapping::<T>::get(163420);
					log::info!(target: LOG_TARGET, "Value at hardcoded index 163420 is {}.", execution_hash_extra1);
					let execution_hash_extra2 =
						crate::migration::v0::ExecutionHeaderMapping::<T>::get(163425);
					log::info!(target: LOG_TARGET, "Value at hardcoded index 163425 is {}.", execution_hash_extra2);
					let execution_hash_extra3 =
						crate::migration::v0::ExecutionHeaderMapping::<T>::get(146719);
					log::info!(target: LOG_TARGET, "Value at hardcoded index 146719 is {}.", execution_hash_extra3);
				}
				let execution_hash =
					crate::migration::v0::ExecutionHeaderMapping::<T>::get(index);

				if index >= M::get() || execution_hash == H256::zero() {
					log::info!(target: LOG_TARGET, "Ethereum execution header cleanup migration is complete. Index = {}.", index);
					crate::migration::STORAGE_VERSION.put::<crate::Pallet<T>>();
					// We are at the end of the migration, signal complete.
					log::info!(target: LOG_TARGET, "SIGNAL COMPLETE");
					cursor = None;
					break
				} else {
					crate::migration::v0::ExecutionHeaders::<T>::remove(execution_hash);
					crate::migration::v0::ExecutionHeaderMapping::<T>::remove(index);
					cursor = Some(index);
				}
			}

			if let Some(last_key) = cursor {
				log::info!(target: LOG_TARGET, "Step done, index is {}.", last_key);
			} else {
				log::info!(target: LOG_TARGET, "Step done, error getting last index");
			};

			Ok(cursor)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<alloc::vec::Vec<u8>, TryRuntimeError> {
			log::info!(target: LOG_TARGET, "Pre-upgrade execution header at index 0 is {}.", crate::migration::v0::ExecutionHeaderMapping::<T>::get(0));
			assert_eq!(crate::Pallet::<T>::on_chain_storage_version(), 0);
			let random_indexes: alloc::vec::Vec<u32> = alloc::vec![0, 700, 340, 4000, 1501];
			for i in 0..5 {
				// Check 5 random index is set
				assert!(
					H256::zero() !=
						crate::migration::v0::ExecutionHeaderMapping::<T>::get(
							random_indexes[i]
						)
				);
			}
			Ok(alloc::vec![])
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_: alloc::vec::Vec<u8>) -> Result<(), TryRuntimeError> {
			log::info!(target: LOG_TARGET, "Post-upgrade execution header at index 0 is {}.", crate::migration::v0::ExecutionHeaderMapping::<T>::get(0));
			assert_eq!(crate::Pallet::<T>::on_chain_storage_version(), STORAGE_VERSION);
			let random_indexes: alloc::vec::Vec<u32> = alloc::vec![0, 700, 340, 4000, 1501];
			for i in 0..5 {
				// Check 5 random index is cleared
				assert_eq!(
					H256::zero(),
					crate::migration::v0::ExecutionHeaderMapping::<T>::get(random_indexes[i])
				);
			}
			Ok(())
		}
	}

	pub struct ExecutionHeaderCleanup<T: Config>(PhantomData<T>);

	impl<T: Config> OnRuntimeUpgrade for ExecutionHeaderCleanup<T> {
		fn on_runtime_upgrade() -> Weight {
			log::info!(target: LOG_TARGET, "Cleaning up latest execution header state and index.");
			crate::migration::v0::LatestExecutionState::<T>::kill();
			crate::migration::v0::ExecutionHeaderIndex::<T>::kill();

			RocksDbWeight::get().reads_writes(2, 2)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<alloc::vec::Vec<u8>, TryRuntimeError> {
			let last_index = crate::migration::v0::ExecutionHeaderIndex::<T>::get();
			log::info!(target: LOG_TARGET, "Pre-upgrade execution header index is {}.", last_index);
			Ok(alloc::vec![])
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_: alloc::vec::Vec<u8>) -> Result<(), TryRuntimeError> {
			let last_index = crate::migration::v0::ExecutionHeaderIndex::<T>::get();
			log::info!(target: LOG_TARGET, "Post-upgrade execution header index is {}.", last_index);
			frame_support::ensure!(
				last_index == 0,
				"Snowbridge execution header storage has not successfully been migrated."
			);
			Ok(())
		}
	}
}
