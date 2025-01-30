// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.
extern crate alloc;

use crate::{Config, Pallet};
#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;
use frame_support::{migrations::VersionedMigration, pallet_prelude::StorageVersion};

/// The in-code storage version.
pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

mod v0 {
	use super::*;
	use frame_support::{pallet_prelude::OptionQuery, storage_alias};
	use sp_consensus_aura::Slot;

	/// Current slot paired with a number of authored blocks.
	///
	/// Updated on each block initialization.
	#[storage_alias]
	pub(super) type SlotInfo<T: Config> = StorageValue<Pallet<T>, (Slot, u32), OptionQuery>;
}
mod v1 {
	use super::*;
	use frame_support::{pallet_prelude::*, traits::UncheckedOnRuntimeUpgrade};

	pub struct UncheckedMigrationToV1<T: Config>(PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for UncheckedMigrationToV1<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight: Weight = Weight::zero();
			weight += migrate::<T>();
			weight
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			Ok(Vec::new())
		}
		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			ensure!(!v0::SlotInfo::<T>::exists(), "SlotInfo should not exist");
			Ok(())
		}
	}

	pub fn migrate<T: Config>() -> Weight {
		v0::SlotInfo::<T>::kill();
		T::DbWeight::get().writes(1)
	}
}

/// Migrate `V0` to `V1`.
pub type MigrateV0ToV1<T> = VersionedMigration<
	0,
	1,
	v1::UncheckedMigrationToV1<T>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;
