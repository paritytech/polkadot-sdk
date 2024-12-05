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

use crate::{Config, Pallet, ReservedDmpWeightOverride, ReservedXcmpWeightOverride};
use frame_support::{
	pallet_prelude::*,
	traits::{Get, OnRuntimeUpgrade, StorageVersion},
	weights::Weight,
};

/// The current storage version.
pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(2);

/// Migrates the pallet storage to the most recent version.
pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		let mut weight: Weight = T::DbWeight::get().reads(2);

		if StorageVersion::get::<Pallet<T>>() == 0 {
			weight = weight
				.saturating_add(v1::migrate::<T>())
				.saturating_add(T::DbWeight::get().writes(1));
			StorageVersion::new(1).put::<Pallet<T>>();
		}

		if StorageVersion::get::<Pallet<T>>() == 1 {
			weight = weight
				.saturating_add(v2::migrate::<T>())
				.saturating_add(T::DbWeight::get().writes(1));
			StorageVersion::new(2).put::<Pallet<T>>();
		}

		weight
	}
}

/// V2: Migrate to 2D weights for ReservedXcmpWeightOverride and ReservedDmpWeightOverride.
mod v2 {
	use super::*;
	const DEFAULT_POV_SIZE: u64 = 64 * 1024; // 64 KB

	pub fn migrate<T: Config>() -> Weight {
		let translate = |pre: u64| -> Weight { Weight::from_parts(pre, DEFAULT_POV_SIZE) };

		if ReservedXcmpWeightOverride::<T>::translate(|pre| pre.map(translate)).is_err() {
			log::error!(
				target: "parachain_system",
				"unexpected error when performing translation of the ReservedXcmpWeightOverride type during storage upgrade to v2"
			);
		}

		if ReservedDmpWeightOverride::<T>::translate(|pre| pre.map(translate)).is_err() {
			log::error!(
				target: "parachain_system",
				"unexpected error when performing translation of the ReservedDmpWeightOverride type during storage upgrade to v2"
			);
		}

		T::DbWeight::get().reads_writes(2, 2)
	}
}

/// V1: `LastUpgrade` block number is removed from the storage since the upgrade
/// mechanism now uses signals instead of block offsets.
mod v1 {
	use crate::{Config, Pallet};
	#[allow(deprecated)]
	use frame_support::{migration::remove_storage_prefix, pallet_prelude::*};

	pub fn migrate<T: Config>() -> Weight {
		#[allow(deprecated)]
		remove_storage_prefix(<Pallet<T>>::name().as_bytes(), b"LastUpgrade", b"");
		T::DbWeight::get().writes(1)
	}
}
