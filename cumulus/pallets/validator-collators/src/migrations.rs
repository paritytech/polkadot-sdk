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
	CollatorDraw, Config, ExternalValidatorSet, MaxCollators, Pallet, PendingRotation,
	RotationState, LOG_TARGET,
};
use frame_support::{
	migrations::VersionedMigration,
	pallet_prelude::*,
	traits::{StorageVersion, UncheckedOnRuntimeUpgrade},
};

/// The in-code storage version.
pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

/// Moves [`MaxCollators`] to [`CollatorDraw`] with [`Config::DefaultMaxCollators`] as default.
pub mod v1 {
	use super::*;
	#[cfg(feature = "try-runtime")]
	use alloc::vec::Vec;
	use codec::DecodeAll;
	use frame_support::storage::unhashed;

	/// Rewrites a cap stored as a `u32` as [`CollatorDraw::UpTo`].
	///
	/// A stored `Option<u32>` already reads as the [`CollatorDraw`] of the same meaning, so it is
	/// left as it is.
	///
	/// Without a stored cap, [`MaxCollators`] reads [`Config::DefaultMaxCollators`]. When a
	/// validator set is stored, it sets [`PendingRotation`] to [`RotationState::AwaitingQueue`], so
	/// the set is drawn again under the default, as [`Pallet::set_max_collators`] does.
	pub struct VersionUncheckedMigrateV0ToV1<T>(PhantomData<T>);

	/// The cap if the stored bytes are exactly a `u32`.
	fn u32_cap(raw: &[u8]) -> Option<u32> {
		u32::decode_all(&mut &raw[..]).ok()
	}

	impl<T: Config> UncheckedOnRuntimeUpgrade for VersionUncheckedMigrateV0ToV1<T> {
		fn on_runtime_upgrade() -> Weight {
			match unhashed::get_raw(&MaxCollators::<T>::hashed_key()) {
				Some(raw) => match u32_cap(&raw) {
					Some(cap) => {
						MaxCollators::<T>::put(CollatorDraw::UpTo(cap));
						T::DbWeight::get().reads_writes(1, 1)
					},
					None => {
						if CollatorDraw::decode_all(&mut &raw[..]).is_err() {
							log::error!(target: LOG_TARGET, "MaxCollators does not decode");
						}
						T::DbWeight::get().reads(1)
					},
				},
				None if ExternalValidatorSet::<T>::exists() => {
					PendingRotation::<T>::put(RotationState::AwaitingQueue);
					T::DbWeight::get().reads_writes(2, 1)
				},
				None => T::DbWeight::get().reads(2),
			}
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			let raw = unhashed::get_raw(&MaxCollators::<T>::hashed_key());
			let redraw = raw.is_none() && ExternalValidatorSet::<T>::exists();
			Ok((raw, redraw).encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			let (before, redraw) = <(Option<Vec<u8>>, bool)>::decode(&mut &state[..])
				.map_err(|_| "the pre-upgrade state does not decode")?;
			let after = unhashed::get_raw(&MaxCollators::<T>::hashed_key());
			match before.as_deref().and_then(u32_cap) {
				Some(cap) => ensure!(
					MaxCollators::<T>::get() == CollatorDraw::UpTo(cap),
					"the u32 cap is not kept"
				),
				None => ensure!(after == before, "MaxCollators changed in the migration"),
			}
			if let Some(after) = after {
				ensure!(
					CollatorDraw::decode_all(&mut &after[..]).is_ok(),
					"MaxCollators does not decode as a CollatorDraw"
				);
			}
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
