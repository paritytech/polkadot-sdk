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

//! A module that is responsible for migration of storage.

use super::*;
use frame_support::{
	pallet_prelude::ValueQuery, storage_alias, traits::OnRuntimeUpgrade, weights::Weight,
};

use sp_std::convert::identity;

pub mod v1 {
	use frame_support::{
		pallet_prelude::ValueQuery, storage_alias, traits::OnRuntimeUpgrade, weights::Weight,
	};

	use super::*;

	#[storage_alias]
	pub(crate) type ClaimQueue<T: Config> =
		StorageValue<Pallet<T>, BTreeMap<CoreIndex, VecDeque<Option<ParasEntryType<T>>>>, ValueQuery>;

}
// TODO: Generic migration code for changed assignment type.
pub mod v2 {
	use super::*;
	use crate::scheduler;
	use frame_support::traits::StorageVersion;

	pub struct MigrateToV2<T>(sp_std::marker::PhantomData<T>);

	impl<T: Config> OnRuntimeUpgrade for MigrateToV2<T> {
		fn on_runtime_upgrade() -> Weight {
			if StorageVersion::get::<Pallet<T>>() == 1 {
				let weight_consumed = migrate_to_v2::<T>();

				log::info!(target: scheduler::LOG_TARGET, "Migrating para scheduler storage to v2");
				StorageVersion::new(2).put::<Pallet<T>>();

				weight_consumed
			} else {
				log::warn!(target: scheduler::LOG_TARGET, "Para scheduler v2 migration should be removed.");
				T::DbWeight::get().reads(1)
			}
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::DispatchError> {
			log::trace!(
				target: crate::scheduler::LOG_TARGET,
				"Scheduled before migration: {}",
				v0::Scheduled::<T>::get().len()
			);

			let bytes = u32::to_be_bytes(v1::ClaimQueue::<T>::get().len() as u32);

			Ok(bytes.to_vec())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::DispatchError> {
			log::trace!(target: crate::scheduler::LOG_TARGET, "Running post_upgrade()");
			ensure!(
				StorageVersion::get::<Pallet<T>>() >= 2,
				"Storage version should be at least `2` after the migration"
			);
			ensure!(
				v1::ClaimQueue::<T>::get().len() == Pallet<T>::ClaimQueue::<T>::get().len(),
				"ClaimQueue should be of same size still"
			);

			let old_len = u32::from_be_bytes(state.try_into().unwrap());
			ensure!(
				Pallet::<T>::claimqueue_len() as u32 == old_len,
				"Old ClaimQueue completely moved to new ClaimQueue after migration"
			);

			Ok(())
		}
	}
}

pub fn migrate_to_v2<T: crate::scheduler::Config>() -> Weight {
	let mut weight: Weight = Weight::zero();

	let old = v1::ClaimQueue::<T>::take();
	let old_len = old.len() as u64;
	let new = old.into_iter().map(|(k, v)| (k, v.into_iter().filter_map(identity).collect::<VecDeque<_>>())).collect::<BTreeMap<CoreIndex, VecDeque<ParasEntryType<T>>>>();
	ClaimQueue::<T>::put(new);

	weight = weight.saturating_add(T::DbWeight::get().reads_writes(2 * old_len, 2 * old_len));

	weight
}
