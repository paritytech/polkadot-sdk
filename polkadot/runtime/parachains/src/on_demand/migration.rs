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
	migrations::VersionedMigration, storage_alias, traits::UncheckedOnRuntimeUpgrade,
	weights::Weight,
};

/// Migration to V2 - Remove affinity system and simplify to single queue.
mod v2 {
	use super::*;
	use crate::on_demand::LOG_TARGET;
	use alloc::collections::BinaryHeap;
	use core::cmp::Ordering;
	use polkadot_primitives::CoreIndex;

	/// Old value of ON_DEMAND_MAX_QUEUE_MAX_SIZE from v1.
	const ON_DEMAND_MAX_QUEUE_MAX_SIZE: u32 = 1_000_000_000;

	/// Old queue index type.
	#[derive(Encode, Decode, TypeInfo, Debug, PartialEq, Clone, Eq, Copy)]
	pub(super) struct QueueIndex(pub u32);

	/// Reverse queue index type (for freed indices heap).
	#[derive(Encode, Decode, TypeInfo, Debug, PartialEq, Clone, Eq, Copy)]
	pub(super) struct ReverseQueueIndex(pub u32);

	impl Ord for QueueIndex {
		fn cmp(&self, other: &Self) -> Ordering {
			let diff = self.0.overflowing_sub(other.0).0;
			if diff == 0 {
				Ordering::Equal
			} else if diff <= ON_DEMAND_MAX_QUEUE_MAX_SIZE {
				Ordering::Greater
			} else {
				Ordering::Less
			}
		}
	}

	impl PartialOrd for QueueIndex {
		fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
			Some(self.cmp(other))
		}
	}

	impl Ord for ReverseQueueIndex {
		fn cmp(&self, other: &Self) -> Ordering {
			QueueIndex(other.0).cmp(&QueueIndex(self.0))
		}
	}

	impl PartialOrd for ReverseQueueIndex {
		fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
			Some(self.cmp(other))
		}
	}

	/// Old enqueued order with QueueIndex.
	#[derive(Encode, Decode, TypeInfo, Debug, PartialEq, Clone, Eq)]
	pub(super) struct OldEnqueuedOrder {
		pub para_id: ParaId,
		pub idx: QueueIndex,
	}

	impl PartialOrd for OldEnqueuedOrder {
		fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
			match other.idx.partial_cmp(&self.idx) {
				Some(Ordering::Equal) => other.para_id.partial_cmp(&self.para_id),
				o => o,
			}
		}
	}

	impl Ord for OldEnqueuedOrder {
		fn cmp(&self, other: &Self) -> Ordering {
			// Note: BinaryHeap is max-heap, but we want min-heap behavior for QueueIndex
			match other.idx.cmp(&self.idx) {
				Ordering::Equal => other.para_id.cmp(&self.para_id),
				o => o,
			}
		}
	}

	/// Old core affinity count.
	#[derive(Encode, Decode, TypeInfo)]
	pub(super) struct CoreAffinityCount {
		pub core_index: CoreIndex,
		pub count: u32,
	}

	/// Old QueueStatus type with affinity system.
	#[derive(Encode, Decode, TypeInfo)]
	pub(super) struct OldQueueStatus {
		pub traffic: FixedU128,
		pub next_index: QueueIndex,
		pub smallest_index: QueueIndex,
		pub freed_indices: BinaryHeap<ReverseQueueIndex>,
	}

	impl Default for OldQueueStatus {
		fn default() -> Self {
			Self {
				traffic: FixedU128::default(),
				next_index: QueueIndex(0),
				smallest_index: QueueIndex(0),
				freed_indices: BinaryHeap::new(),
			}
		}
	}

	#[storage_alias]
	pub(super) type ParaIdAffinity<T: Config> =
		StorageMap<Pallet<T>, Twox64Concat, ParaId, CoreAffinityCount, OptionQuery>;

	#[storage_alias]
	pub(super) type QueueStatus<T: Config> = StorageValue<Pallet<T>, OldQueueStatus, OptionQuery>;

	#[storage_alias]
	pub(super) type FreeEntries<T: Config> =
		StorageValue<Pallet<T>, BinaryHeap<OldEnqueuedOrder>, OptionQuery>;

	#[storage_alias]
	pub(super) type AffinityEntries<T: Config> = StorageMap<
		Pallet<T>,
		Twox64Concat,
		polkadot_primitives::CoreIndex,
		BinaryHeap<OldEnqueuedOrder>,
		OptionQuery,
	>;

	/// Migration to V2
	pub struct UncheckedMigrateToV2<T>(core::marker::PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for UncheckedMigrateToV2<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight: Weight = Weight::zero();

			let now = frame_system::Pallet::<T>::block_number();
			let old_queue_status = v2::QueueStatus::<T>::take().unwrap_or_else(|| OldQueueStatus {
				traffic: T::TrafficDefaultValue::get(),
				..Default::default()
			});
			weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));

			// Collect all orders from both free and affinity queues
			let mut all_orders = alloc::vec::Vec::new();

			// Collect from free entries
			let free_entries = v2::FreeEntries::<T>::take().unwrap_or_default();
			weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));
			for order in free_entries.into_iter() {
				all_orders.push(order);
			}

			// Collect from all affinity entries using drain for efficiency (reads + removes in one
			// op)
			let mut affinity_count = 0u64;
			for (_core_idx, affinity_heap) in v2::AffinityEntries::<T>::drain() {
				affinity_count += 1;
				for order in affinity_heap.into_iter() {
					all_orders.push(order);
				}
			}
			// drain() performs reads + writes in one operation
			weight
				.saturating_accrue(T::DbWeight::get().reads_writes(affinity_count, affinity_count));

			// Sort by QueueIndex to preserve order (ascending)
			all_orders.sort_by_key(|o| o.idx);

			// Drop ParaIdAffinity storage
			let affinity_count = v2::ParaIdAffinity::<T>::iter().count();
			let _ = v2::ParaIdAffinity::<T>::clear(u32::MAX, None);
			weight.saturating_accrue(
				T::DbWeight::get().reads_writes(affinity_count as u64, affinity_count as u64),
			);

			// Build new OrderStatus
			super::pallet::OrderStatus::<T>::mutate(|order_status| {
				// Preserve the traffic value
				order_status.traffic = old_queue_status.traffic;

				// Add all orders to the new queue
				for old_order in all_orders.iter() {
					if let Err(para_id) = order_status.queue.try_push(now, old_order.para_id) {
						log::warn!(
							target: LOG_TARGET,
							"Failed to migrate order for para_id {:?} - queue full",
							para_id
						);
					}
				}
			});
			weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));

			log::info!(
				target: LOG_TARGET,
				"Migrated on demand assigner storage to v2: {} orders migrated, {} affinity entries removed",
				all_orders.len(),
				affinity_count
			);

			weight
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<alloc::vec::Vec<u8>, sp_runtime::TryRuntimeError> {
			let old_queue_status = v2::QueueStatus::<T>::get();
			let free_entries = v2::FreeEntries::<T>::get();
			let affinity_keys: alloc::vec::Vec<_> = v2::AffinityEntries::<T>::iter_keys().collect();

			let mut total_orders = free_entries.len();
			for core_idx in affinity_keys.iter() {
				total_orders += v2::AffinityEntries::<T>::get(core_idx).len();
			}

			let affinity_count = v2::ParaIdAffinity::<T>::iter().count();

			log::info!(
				target: LOG_TARGET,
				"Before migration: {} total orders ({} free, {} in affinity queues), {} affinity mappings, traffic: {:?}",
				total_orders,
				free_entries.len(),
				total_orders - free_entries.len(),
				affinity_count,
				old_queue_status.traffic
			);

			Ok((total_orders as u32, affinity_count as u32, old_queue_status.traffic).encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: alloc::vec::Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			log::info!(target: LOG_TARGET, "Running post_upgrade() for v2");

			let (expected_orders, expected_affinity_count, expected_traffic): (
				u32,
				u32,
				FixedU128,
			) = Decode::decode(&mut &state[..]).map_err(|_| "Failed to decode pre_upgrade state")?;

			// Verify old storage is cleaned up
			ensure!(!v2::QueueStatus::<T>::exists(), "Old QueueStatus should be removed");
			ensure!(!v2::FreeEntries::<T>::exists(), "FreeEntries should be removed");
			ensure!(
				v2::AffinityEntries::<T>::iter().count() == 0,
				"AffinityEntries should be empty"
			);
			ensure!(v2::ParaIdAffinity::<T>::iter().count() == 0, "ParaIdAffinity should be empty");

			// Verify new storage
			let new_order_status = super::pallet::OrderStatus::<T>::get();
			ensure!(
				new_order_status.traffic == expected_traffic,
				"Traffic value should be preserved"
			);

			let migrated_orders = new_order_status.queue.len() as u32;
			log::info!(
				target: LOG_TARGET,
				"Successfully migrated {} orders (expected {}), removed {} affinity mappings, traffic preserved: {:?}",
				migrated_orders,
				expected_orders,
				expected_affinity_count,
				new_order_status.traffic
			);

			Ok(())
		}
	}
}

/// Migration to rename pallet from `OnDemandAssignmentProvider` to `OnDemand`.
pub struct MigrateToOnDemandPalletName<T>(core::marker::PhantomData<T>);

impl<T: Config> frame_support::traits::OnRuntimeUpgrade for MigrateToOnDemandPalletName<T> {
	fn on_runtime_upgrade() -> Weight {
		frame_support::storage::migration::move_pallet(b"OnDemandAssignmentProvider", b"OnDemand");
		// One read to check existence, one write to move the pallet
		// TODO: Moving an entire pallet is only one read and write? Let's provide a proper
		// benchmark?
		// TODO: Also can we get the actually used pallet names passed in from the runtime? (We actually don't know them here.)
		T::DbWeight::get().reads_writes(1, 1)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<alloc::vec::Vec<u8>, sp_runtime::TryRuntimeError> {
		use frame_support::ensure;
		let old_pallet_prefix = sp_io::hashing::twox_128(b"OnDemandAssignmentProvider");
		ensure!(
			sp_io::storage::next_key(&old_pallet_prefix).is_some(),
			"Old pallet OnDemandAssignmentProvider data should exist before migration"
		);
		Ok(alloc::vec::Vec::new())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: alloc::vec::Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		use frame_support::ensure;
		let old_pallet_prefix = sp_io::hashing::twox_128(b"OnDemandAssignmentProvider");
		let new_pallet_prefix = sp_io::hashing::twox_128(b"OnDemand");

		ensure!(
			sp_io::storage::next_key(&old_pallet_prefix).is_none(),
			"Old pallet OnDemandAssignmentProvider data should be removed after migration"
		);
		ensure!(
			sp_io::storage::next_key(&new_pallet_prefix).is_some(),
			"New pallet OnDemand data should exist after migration"
		);
		Ok(())
	}
}

/// Migrate `V1` to `V2` of the storage format.
pub type MigrateV1ToV2<T> = VersionedMigration<
	1,
	2,
	v2::UncheckedMigrateToV2<T>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		mock::{new_test_ext, MockGenesisConfig, Test},
		on_demand,
	};
	use alloc::collections::BinaryHeap;
	use frame_support::pallet_prelude::*;
	use polkadot_primitives::{CoreIndex, Id as ParaId};
	use sp_runtime::FixedU128;

	#[test]
	fn affinity_queue_merging_works() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			let para_1 = ParaId::from(1000);
			let para_2 = ParaId::from(1001);
			let para_3 = ParaId::from(1002);

			// Setup old storage with affinity queues
			let mut affinity_queue_core_0 = BinaryHeap::new();
			affinity_queue_core_0
				.push(v2::OldEnqueuedOrder { para_id: para_1, idx: v2::QueueIndex(1) });
			affinity_queue_core_0
				.push(v2::OldEnqueuedOrder { para_id: para_2, idx: v2::QueueIndex(3) });

			let mut affinity_queue_core_1 = BinaryHeap::new();
			affinity_queue_core_1
				.push(v2::OldEnqueuedOrder { para_id: para_3, idx: v2::QueueIndex(2) });

			v2::AffinityEntries::<Test>::insert(CoreIndex(0), affinity_queue_core_0);
			v2::AffinityEntries::<Test>::insert(CoreIndex(1), affinity_queue_core_1);

			// Setup old QueueStatus
			let old_status = v2::OldQueueStatus {
				traffic: FixedU128::from_rational(5, 10),
				next_index: v2::QueueIndex(4),
				smallest_index: v2::QueueIndex(1),
				freed_indices: BinaryHeap::new(),
			};
			v2::QueueStatus::<Test>::put(old_status);

			// Set storage version to 1
			StorageVersion::new(1).put::<on_demand::Pallet<Test>>();

			// Run migration
			let _weight = v2::UncheckedMigrateToV2::<Test>::on_runtime_upgrade();

			// Verify new storage
			let new_status = on_demand::pallet::OrderStatus::<Test>::get();
			assert_eq!(new_status.traffic, FixedU128::from_rational(5, 10));

			// Verify all orders migrated (should be 3 total: para_1, para_2, para_3)
			assert_eq!(new_status.queue.len(), 3);

			// Verify old storage is cleaned up
			assert!(!v2::QueueStatus::<Test>::exists());
			assert_eq!(v2::AffinityEntries::<Test>::iter_keys().count(), 0);
		});
	}

	#[test]
	fn free_and_affinity_queues_merged() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			let para_1 = ParaId::from(1000);
			let para_2 = ParaId::from(1001);
			let para_3 = ParaId::from(1002);
			let para_4 = ParaId::from(1003);

			// Setup free entries (no core affinity)
			let mut free_queue = BinaryHeap::new();
			free_queue.push(v2::OldEnqueuedOrder { para_id: para_1, idx: v2::QueueIndex(1) });
			free_queue.push(v2::OldEnqueuedOrder { para_id: para_2, idx: v2::QueueIndex(5) });
			v2::FreeEntries::<Test>::put(free_queue);

			// Setup affinity entries
			let mut affinity_queue = BinaryHeap::new();
			affinity_queue.push(v2::OldEnqueuedOrder { para_id: para_3, idx: v2::QueueIndex(3) });
			affinity_queue.push(v2::OldEnqueuedOrder { para_id: para_4, idx: v2::QueueIndex(7) });
			v2::AffinityEntries::<Test>::insert(CoreIndex(0), affinity_queue);

			let old_status = v2::OldQueueStatus::default();
			v2::QueueStatus::<Test>::put(old_status);

			StorageVersion::new(1).put::<on_demand::Pallet<Test>>();

			// Run migration
			v2::UncheckedMigrateToV2::<Test>::on_runtime_upgrade();

			// Verify all 4 orders merged into single queue
			let new_status = on_demand::pallet::OrderStatus::<Test>::get();
			assert_eq!(new_status.queue.len(), 4);

			// Verify old storage cleaned up
			assert!(!v2::FreeEntries::<Test>::exists());
			assert_eq!(v2::AffinityEntries::<Test>::iter_keys().count(), 0);
		});
	}

	#[test]
	fn order_preservation_by_queue_index() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			let para_1 = ParaId::from(1000);
			let para_2 = ParaId::from(1001);
			let para_3 = ParaId::from(1002);

			// Create orders with non-sequential queue indices
			let mut free_queue = BinaryHeap::new();
			free_queue.push(v2::OldEnqueuedOrder { para_id: para_2, idx: v2::QueueIndex(5) });
			free_queue.push(v2::OldEnqueuedOrder { para_id: para_1, idx: v2::QueueIndex(2) });
			free_queue.push(v2::OldEnqueuedOrder { para_id: para_3, idx: v2::QueueIndex(10) });
			v2::FreeEntries::<Test>::put(free_queue);

			let old_status = v2::OldQueueStatus::default();
			v2::QueueStatus::<Test>::put(old_status);

			StorageVersion::new(1).put::<on_demand::Pallet<Test>>();

			// Run migration
			v2::UncheckedMigrateToV2::<Test>::on_runtime_upgrade();

			// Verify orders are in queue
			// Order should be preserved based on QueueIndex (2, 5, 10)
			let new_status = on_demand::pallet::OrderStatus::<Test>::get();
			assert_eq!(new_status.queue.len(), 3);

			// Verify the order: para_1 (idx 2), para_2 (idx 5), para_3 (idx 10)
			assert_eq!(new_status.queue.queue[0].para_id, para_1);
			assert_eq!(new_status.queue.queue[1].para_id, para_2);
			assert_eq!(new_status.queue.queue[2].para_id, para_3);
		});
	}

	#[test]
	fn traffic_value_preserved() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			let traffic_value = FixedU128::from_rational(75, 100);

			let old_status = v2::OldQueueStatus {
				traffic: traffic_value,
				next_index: v2::QueueIndex(1),
				smallest_index: v2::QueueIndex(1),
				freed_indices: BinaryHeap::new(),
			};
			v2::QueueStatus::<Test>::put(old_status);

			StorageVersion::new(1).put::<on_demand::Pallet<Test>>();

			// Run migration
			v2::UncheckedMigrateToV2::<Test>::on_runtime_upgrade();

			// Verify traffic preserved
			let new_status = on_demand::pallet::OrderStatus::<Test>::get();
			assert_eq!(new_status.traffic, traffic_value);
		});
	}

	#[test]
	fn para_id_affinity_removed() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			let para_1 = ParaId::from(1000);
			let para_2 = ParaId::from(1001);

			// Setup ParaIdAffinity storage
			v2::ParaIdAffinity::<Test>::insert(
				para_1,
				v2::CoreAffinityCount { core_index: CoreIndex(0), count: 5 },
			);
			v2::ParaIdAffinity::<Test>::insert(
				para_2,
				v2::CoreAffinityCount { core_index: CoreIndex(1), count: 3 },
			);

			let old_status = v2::OldQueueStatus::default();
			v2::QueueStatus::<Test>::put(old_status);

			StorageVersion::new(1).put::<on_demand::Pallet<Test>>();

			// Run migration
			v2::UncheckedMigrateToV2::<Test>::on_runtime_upgrade();

			// Verify ParaIdAffinity completely removed
			assert_eq!(v2::ParaIdAffinity::<Test>::iter().count(), 0);
		});
	}

	#[test]
	fn empty_storage_migration() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			// Only set default QueueStatus
			let old_status = v2::OldQueueStatus::default();
			v2::QueueStatus::<Test>::put(old_status);

			StorageVersion::new(1).put::<on_demand::Pallet<Test>>();

			// Run migration with no orders
			let _weight = v2::UncheckedMigrateToV2::<Test>::on_runtime_upgrade();

			// Verify new storage has empty queue
			let new_status = on_demand::pallet::OrderStatus::<Test>::get();
			assert_eq!(new_status.queue.len(), 0);

			// Verify old storage cleaned up
			assert!(!v2::QueueStatus::<Test>::exists());
		});
	}

	#[test]
	fn multiple_affinity_cores_merged() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			// Setup affinity entries for 5 different cores
			for core_idx in 0..5 {
				let mut affinity_queue = BinaryHeap::new();
				affinity_queue.push(v2::OldEnqueuedOrder {
					para_id: ParaId::from(1000 + core_idx),
					idx: v2::QueueIndex(core_idx),
				});
				v2::AffinityEntries::<Test>::insert(CoreIndex(core_idx), affinity_queue);
			}

			let old_status = v2::OldQueueStatus::default();
			v2::QueueStatus::<Test>::put(old_status);

			StorageVersion::new(1).put::<on_demand::Pallet<Test>>();

			// Run migration
			v2::UncheckedMigrateToV2::<Test>::on_runtime_upgrade();

			// Verify all 5 orders merged into single queue
			let new_status = on_demand::pallet::OrderStatus::<Test>::get();
			assert_eq!(new_status.queue.len(), 5);

			// Verify all affinity entries removed
			assert_eq!(v2::AffinityEntries::<Test>::iter_keys().count(), 0);
		});
	}

	#[test]
	fn queue_full_handling() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			let _now = frame_system::Pallet::<Test>::block_number();

			// Try to add more orders than the queue can hold
			let mut free_queue = BinaryHeap::new();

			// Add many orders (queue might have a limit)
			for i in 0..1000 {
				free_queue.push(v2::OldEnqueuedOrder {
					para_id: ParaId::from(i),
					idx: v2::QueueIndex(i),
				});
			}

			v2::FreeEntries::<Test>::put(free_queue);

			let old_status = v2::OldQueueStatus::default();
			v2::QueueStatus::<Test>::put(old_status);

			StorageVersion::new(1).put::<on_demand::Pallet<Test>>();

			// Run migration - should not panic even if queue is full
			let _weight = v2::UncheckedMigrateToV2::<Test>::on_runtime_upgrade();

			// Verify migration completed (some orders may be dropped if queue is full)
			let new_status = on_demand::pallet::OrderStatus::<Test>::get();
			// Just verify it doesn't panic and creates some queue
			assert!(new_status.queue.len() > 0);
		});
	}
}
