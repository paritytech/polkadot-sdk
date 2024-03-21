// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Migrates the storage from the previously deleted DMP pallet.

use crate::*;
use cumulus_primitives_core::relay_chain::BlockNumber as RelayBlockNumber;
use frame_support::{pallet_prelude::*, storage_alias, traits::HandleMessage};
use sp_std::vec::Vec;

pub(crate) const LOG: &str = "runtime::dmp-queue-export-xcms";

/// The old `PageIndexData` struct.
#[derive(Copy, Clone, Eq, PartialEq, Default, Encode, Decode, RuntimeDebug, TypeInfo)]
pub struct PageIndexData {
	/// The lowest used page index.
	pub begin_used: PageCounter,
	/// The lowest unused page index.
	pub end_used: PageCounter,
	/// The number of overweight messages ever recorded (and thus the lowest free index).
	pub overweight_count: OverweightIndex,
}

/// The old `MigrationState` type.
pub type OverweightIndex = u64;
/// The old `MigrationState` type.
pub type PageCounter = u32;

/// The old `PageIndex` storage item.
#[storage_alias]
pub type PageIndex<T: Config> = StorageValue<Pallet<T>, PageIndexData, ValueQuery>;

/// The old `Pages` storage item.
#[storage_alias]
pub type Pages<T: Config> = StorageMap<
	Pallet<T>,
	Blake2_128Concat,
	PageCounter,
	Vec<(RelayBlockNumber, Vec<u8>)>,
	ValueQuery,
>;

/// The old `Overweight` storage item.
#[storage_alias]
pub type Overweight<T: Config> = CountedStorageMap<
	Pallet<T>,
	Blake2_128Concat,
	OverweightIndex,
	(RelayBlockNumber, Vec<u8>),
	OptionQuery,
>;

pub(crate) mod testing_only {
	use super::*;

	/// This alias is not used by the migration but only for testing.
	///
	/// Note that the alias type is wrong on purpose.
	#[storage_alias]
	pub type Configuration<T: Config> = StorageValue<Pallet<T>, u32>;
}

/// Migrates a single page to the `DmpSink`.
pub(crate) fn migrate_page<T: crate::Config>(p: PageCounter) -> Result<(), ()> {
	let page = Pages::<T>::take(p);
	log::debug!(target: LOG, "Migrating page #{p} with {} messages ...", page.len());
	if page.is_empty() {
		log::error!(target: LOG, "Page #{p}: EMPTY - storage corrupted?");
		return Err(())
	}

	for (m, (block, msg)) in page.iter().enumerate() {
		let Ok(bound) = BoundedVec::<u8, _>::try_from(msg.clone()) else {
			log::error!(target: LOG, "[Page {p}] Message #{m}: TOO LONG - dropping");
			continue
		};

		T::DmpSink::handle_message(bound.as_bounded_slice());
		log::debug!(target: LOG, "[Page {p}] Migrated message #{m} from block {block}");
	}

	Ok(())
}

/// Migrates a single overweight message to the `DmpSink`.
pub(crate) fn migrate_overweight<T: crate::Config>(i: OverweightIndex) -> Result<(), ()> {
	let Some((block, msg)) = Overweight::<T>::take(i) else {
		log::error!(target: LOG, "[Overweight {i}] Message: EMPTY - storage corrupted?");
		return Err(())
	};
	let Ok(bound) = BoundedVec::<u8, _>::try_from(msg) else {
		log::error!(target: LOG, "[Overweight {i}] Message: TOO LONG - dropping");
		return Err(())
	};

	T::DmpSink::handle_message(bound.as_bounded_slice());
	log::debug!(target: LOG, "[Overweight {i}] Migrated message from block {block}");

	Ok(())
}
