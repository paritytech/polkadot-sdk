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

//! Storage migrations of the pallet.

use crate::{
	Config, ExternalValidatorSet, MaxCollators, Pallet, PendingRotation, RotationState, LOG_TARGET,
};
use frame_support::{
	migrations::VersionedMigration,
	pallet_prelude::*,
	traits::{StorageVersion, UncheckedOnRuntimeUpgrade},
};

/// The in-code storage version.
pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

/// Moves [`MaxCollators`] to `Option<u32>` with [`Config::DefaultMaxCollators`] as default.
pub mod v1 {
	use super::*;
	#[cfg(feature = "try-runtime")]
	use alloc::vec::Vec;
	#[cfg(feature = "try-runtime")]
	use codec::DecodeAll;
	#[cfg(feature = "try-runtime")]
	use frame_support::storage::unhashed;

	#[cfg(feature = "try-runtime")]
	mod v0 {
		use super::*;

		/// [`MaxCollators`] in version 0, a `u32` with no value meaning no cap.
		#[frame_support::storage_alias]
		pub type MaxCollators<T: Config> = StorageValue<Pallet<T>, u32>;
	}

	/// Rewrites a cap stored as a `u32` as `Some(cap)`.
	///
	/// Without a stored cap, [`MaxCollators`] reads [`Config::DefaultMaxCollators`]. When that
	/// default is `Some(_)` and a validator set is stored, it sets [`PendingRotation`] to
	/// [`RotationState::AwaitingQueue`], so the set is drawn again under the default, as
	/// [`Pallet::set_max_collators`] does.
	pub struct VersionUncheckedMigrateV0ToV1<T>(PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for VersionUncheckedMigrateV0ToV1<T> {
		fn on_runtime_upgrade() -> Weight {
			match MaxCollators::<T>::translate::<u32, _>(|old| old.map(Some)) {
				Ok(Some(_)) => T::DbWeight::get().reads_writes(1, 1),
				Ok(None) if T::DefaultMaxCollators::get().is_none() => {
					T::DbWeight::get().reads_writes(1, 1)
				},
				Ok(None) if ExternalValidatorSet::<T>::exists() => {
					PendingRotation::<T>::put(RotationState::AwaitingQueue);
					T::DbWeight::get().reads_writes(2, 2)
				},
				Ok(None) => T::DbWeight::get().reads_writes(2, 1),
				Err(()) => {
					log::error!(target: LOG_TARGET, "MaxCollators does not decode as a u32");
					T::DbWeight::get().reads(1)
				},
			}
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			let stored = v0::MaxCollators::<T>::exists();
			let cap =
				if stored { v0::MaxCollators::<T>::get() } else { T::DefaultMaxCollators::get() };
			let redraw = !stored &&
				T::DefaultMaxCollators::get().is_some() &&
				ExternalValidatorSet::<T>::exists();
			Ok((cap, redraw).encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			let (cap, redraw) = <(Option<u32>, bool)>::decode(&mut &state[..])
				.map_err(|_| "the pre-upgrade state does not decode")?;
			// A cap left as a `u32` can read as the default, so the stored bytes are checked too.
			if let Some(raw) = unhashed::get_raw(&MaxCollators::<T>::hashed_key()) {
				ensure!(
					Option::<u32>::decode_all(&mut &raw[..]).is_ok(),
					"MaxCollators does not decode as an Option<u32>"
				);
			}
			ensure!(MaxCollators::<T>::get() == cap, "MaxCollators changed in the migration");
			ensure!(
				!redraw || PendingRotation::<T>::get() == RotationState::AwaitingQueue,
				"the set is not drawn again under the default cap"
			);
			Ok(())
		}
	}

	/// [`VersionUncheckedMigrateV0ToV1`], run only at storage version 0.
	pub type MigrateV0ToV1<T> = VersionedMigration<
		0,
		1,
		VersionUncheckedMigrateV0ToV1<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}
