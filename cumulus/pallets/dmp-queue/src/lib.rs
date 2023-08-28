// Copyright Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Migrates the storage from the previously deleted DMP pallet.

#![cfg_attr(not(feature = "std"), no_std)]

mod tests;

use cumulus_primitives_core::relay_chain::BlockNumber as RelayBlockNumber;
use frame_support::{
	pallet_prelude::*,
	storage_alias,
	traits::{HandleMessage, OnRuntimeUpgrade},
	weights::Weight,
};
use sp_runtime::Saturating;
use sp_std::vec::Vec;

const LOG: &str = "dmp-queue-undeploy-migration";

/// Undeploy the DMP queue pallet.
///
/// Moves all storage from the pallet to a new Queue handler. Afterwards the storage of the DMP
/// should be purged with [DeleteDmpQueue].
pub struct UndeployDmpQueue<T: MigrationConfig>(PhantomData<T>);

/// Delete the DMP pallet. Should only be used once the DMP pallet is removed from the runtime and
/// after [UndeployDmpQueue].
pub type DeleteDmpQueue<T> = frame_support::migrations::RemovePallet<
	<T as MigrationConfig>::PalletName,
	<T as MigrationConfig>::DbWeight,
>;

/// Subset of the DMP queue config required for [UndeployDmpQueue].
pub trait MigrationConfig {
	/// Name of the previously deployed DMP queue pallet.
	type PalletName: Get<&'static str>;

	/// New handler for the messages.
	type DmpHandler: HandleMessage;

	// The weight info for the runtime.
	type DbWeight: Get<frame_support::weights::RuntimeDbWeight>;
}

#[derive(Copy, Clone, Eq, PartialEq, Default, Encode, Decode, RuntimeDebug, TypeInfo)]
struct PageIndexData {
	/// The lowest used page index.
	begin_used: PageCounter,
	/// The lowest unused page index.
	end_used: PageCounter,
	/// The number of overweight messages ever recorded (and thus the lowest free index).
	overweight_count: OverweightIndex,
}

type OverweightIndex = u64;
type PageCounter = u32;

#[storage_alias(dynamic)]
type PageIndex<T: MigrationConfig> =
	StorageValue<<T as MigrationConfig>::PalletName, PageIndexData, ValueQuery>;

#[storage_alias(dynamic)]
type Pages<T: MigrationConfig> = StorageMap<
	<T as MigrationConfig>::PalletName,
	Blake2_128Concat,
	PageCounter,
	Vec<(RelayBlockNumber, Vec<u8>)>,
	ValueQuery,
>;

#[storage_alias(dynamic)]
type Overweight<T: MigrationConfig> = CountedStorageMap<
	<T as MigrationConfig>::PalletName,
	Blake2_128Concat,
	OverweightIndex,
	(RelayBlockNumber, Vec<u8>),
	OptionQuery,
>;

impl<T: MigrationConfig> OnRuntimeUpgrade for UndeployDmpQueue<T> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::DispatchError> {
		let index = PageIndex::<T>::get();

		// Check that all pages are present.
		ensure!(index.begin_used <= index.end_used, "Invalid page index");
		for p in index.begin_used..index.end_used {
			ensure!(Pages::<T>::contains_key(p), "Missing page");
			ensure!(Pages::<T>::get(p).len() > 0, "Empty page");
		}

		// Check that all overweight messages are present.
		for i in 0..index.overweight_count {
			ensure!(Overweight::<T>::contains_key(i), "Missing overweight message");
		}

		Ok(Default::default())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_: Vec<u8>) -> Result<(), sp_runtime::DispatchError> {
		let index = PageIndex::<T>::get();

		// Check that all pages are removed.
		for p in index.begin_used..index.end_used {
			ensure!(!Pages::<T>::contains_key(p), "Page should be gone");
		}
		ensure!(Pages::<T>::iter_keys().next().is_none(), "Un-indexed pages");

		// Check that all overweight messages are removed.
		for i in 0..index.overweight_count {
			ensure!(!Overweight::<T>::contains_key(i), "Overweight message should be gone");
		}
		ensure!(Overweight::<T>::iter_keys().next().is_none(), "Un-indexed overweight messages");

		Ok(())
	}

	fn on_runtime_upgrade() -> Weight {
		let index = PageIndex::<T>::get();
		log::info!(target: LOG, "Page index: {index:?}");
		let (mut messages_migrated, mut pages_migrated) = (0u32, 0u32);

		for p in index.begin_used..index.end_used {
			let page = Pages::<T>::take(p);
			log::info!(target: LOG, "Migrating page #{p} with {} messages ...", page.len());
			if page.is_empty() {
				log::error!(target: LOG, "Page #{p}: EMPTY - storage corrupted?");
			}

			for (m, (block, msg)) in page.iter().enumerate() {
				let Ok(bound) = BoundedVec::<u8, _>::try_from(msg.clone()) else {
					log::error!(target: LOG, "[Page {p}] Message #{m}: TOO LONG - ignoring");
					continue;
				};

				T::DmpHandler::handle_message(bound.as_bounded_slice());
				messages_migrated.saturating_inc();
				log::info!(target: LOG, "[Page {p}] Migrated message #{m} from block {block}");
			}
			pages_migrated.saturating_inc();
		}

		log::info!(target: LOG, "Migrated {messages_migrated} messages from {pages_migrated} pages");

		// Now migrate the overweight messages.
		let mut overweight_migrated = 0u32;
		log::info!(target: LOG, "Migrating {} overweight messages ...", index.overweight_count);

		for i in 0..index.overweight_count {
			let Some((block, msg)) = Overweight::<T>::take(i) else {
				log::error!(target: LOG, "[Overweight {i}] Message: EMPTY - storage corrupted?");
				continue;
			};
			let Ok(bound) = BoundedVec::<u8, _>::try_from(msg) else {
				log::error!(target: LOG, "[Overweight {i}] Message: TOO LONG - ignoring");
				continue;
			};

			T::DmpHandler::handle_message(bound.as_bounded_slice());
			overweight_migrated.saturating_inc();
			log::info!(target: LOG, "[Overweight {i}] Migrated message from block {block}");
		}

		Weight::zero() // FAIL-CI
	}
}
