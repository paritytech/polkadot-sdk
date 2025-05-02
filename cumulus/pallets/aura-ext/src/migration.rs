// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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
