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
	migrations::VersionedMigration, pallet_prelude::ValueQuery, storage_alias,
	traits::UncheckedOnRuntimeUpgrade, weights::Weight,
};

mod v0 {
	use super::*;
	use sp_std::collections::vec_deque::VecDeque;

	#[derive(Encode, Decode, TypeInfo, Debug, PartialEq, Clone)]
	pub(super) struct EnqueuedOrder {
		pub para_id: ParaId,
	}

	/// Keeps track of the multiplier used to calculate the current spot price for the on demand
	/// assigner.
	/// NOTE: Ignoring the `OnEmpty` field for the migration.
	#[storage_alias]
	pub(super) type SpotTraffic<T: Config> = StorageValue<Pallet<T>, FixedU128, ValueQuery>;

	/// The order storage entry. Uses a VecDeque to be able to push to the front of the
	/// queue from the scheduler on session boundaries.
	/// NOTE: Ignoring the `OnEmpty` field for the migration.
	#[storage_alias]
	pub(super) type OnDemandQueue<T: Config> =
		StorageValue<Pallet<T>, VecDeque<EnqueuedOrder>, ValueQuery>;
}

mod v1 {
	use super::*;

	use crate::assigner_on_demand::LOG_TARGET;

	/// Migration to V1
	pub struct UncheckedMigrateToV1<T>(sp_std::marker::PhantomData<T>);
	impl<T: Config> UncheckedOnRuntimeUpgrade for UncheckedMigrateToV1<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight: Weight = Weight::zero();

			// Migrate the current traffic value
			let config = <configuration::Pallet<T>>::config();
			QueueStatus::<T>::mutate(|mut queue_status| {
				Pallet::<T>::update_spot_traffic(&config, &mut queue_status);

				let v0_queue = v0::OnDemandQueue::<T>::take();
				// Process the v0 queue into v1.
				v0_queue.into_iter().for_each(|enqueued_order| {
					// Readding the old orders will use the new systems.
					Pallet::<T>::add_on_demand_order(
						queue_status,
						enqueued_order.para_id,
						QueuePushDirection::Back,
					);
				});
			});

			// Remove the old storage.
			v0::OnDemandQueue::<T>::kill(); // 1 write
			v0::SpotTraffic::<T>::kill(); // 1 write

			// Config read
			weight.saturating_accrue(T::DbWeight::get().reads(1));
			// QueueStatus read write (update_spot_traffic)
			weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));
			// Kill x 2
			weight.saturating_accrue(T::DbWeight::get().writes(2));

			log::info!(target: LOG_TARGET, "Migrated on demand assigner storage to v1");
			weight
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			let n: u32 = v0::OnDemandQueue::<T>::get().len() as u32;

			log::info!(
				target: LOG_TARGET,
				"Number of orders waiting in the queue before: {n}",
			);

			Ok(n.encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			log::info!(target: LOG_TARGET, "Running post_upgrade()");

			ensure!(
				v0::OnDemandQueue::<T>::get().is_empty(),
				"OnDemandQueue should be empty after the migration"
			);

			let expected_len = u32::decode(&mut &state[..]).unwrap();
			let queue_status_size = QueueStatus::<T>::get().size();
			ensure!(
				expected_len == queue_status_size,
				"Number of orders should be the same before and after migration"
			);

			let n_affinity_entries: u32 =
				AffinityEntries::<T>::iter().map(|(_index, heap)| heap.len() as u32).sum();
			let n_para_id_affinity: u32 = ParaIdAffinity::<T>::iter()
				.map(|(_para_id, affinity)| affinity.count as u32)
				.sum();
			ensure!(
				n_para_id_affinity == n_affinity_entries,
				"Number of affinity entries should be the same as the counts in ParaIdAffinity"
			);

			Ok(())
		}
	}
}

/// Migrate `V0` to `V1` of the storage format.
pub type MigrateV0ToV1<T> = VersionedMigration<
	0,
	1,
	v1::UncheckedMigrateToV1<T>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;

#[cfg(test)]
mod tests {
	use super::{v0, v1, UncheckedOnRuntimeUpgrade, Weight};
	use crate::mock::{new_test_ext, MockGenesisConfig, OnDemandAssigner, Test};
	use primitives::Id as ParaId;

	#[test]
	fn migration_to_v1_preserves_queue_ordering() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			// Place orders for paraids 1..5
			for i in 1..=5 {
				v0::OnDemandQueue::<Test>::mutate(|queue| {
					queue.push_back(v0::EnqueuedOrder { para_id: ParaId::new(i) })
				});
			}

			// Queue has 5 orders
			let old_queue = v0::OnDemandQueue::<Test>::get();
			assert_eq!(old_queue.len(), 5);
			// New queue has 0 orders
			assert_eq!(OnDemandAssigner::get_queue_status().size(), 0);

			// For tests, db weight is zero.
			assert_eq!(
				<v1::UncheckedMigrateToV1<Test> as UncheckedOnRuntimeUpgrade>::on_runtime_upgrade(),
				Weight::zero()
			);

			// New queue has 5 orders
			assert_eq!(OnDemandAssigner::get_queue_status().size(), 5);

			// Compare each entry from the old queue with the entry in the new queue.
			old_queue.iter().zip(OnDemandAssigner::get_free_entries().iter()).for_each(
				|(old_enq, new_enq)| {
					assert_eq!(old_enq.para_id, new_enq.para_id);
				},
			);
		});
	}
}
