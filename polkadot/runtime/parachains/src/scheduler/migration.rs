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
use crate::on_demand;
use frame_support::{
	migrations::VersionedMigration, pallet_prelude::ValueQuery, storage_alias,
	traits::UncheckedOnRuntimeUpgrade, weights::Weight,
};

// Import V4 types - these will be used directly for decoding V3 storage since they're binary-compatible
use super::assigner_coretime::{CoreDescriptor, Schedule};

/// V3 storage format - types and storage items before migration to V4.
pub(super) mod v3 {
	use super::*;
	use frame_support::pallet_prelude::{OptionQuery, Twox256};

	/// Assignment type used in V2 and V3 storage (before migration to V4).
	#[derive(Encode, Decode, TypeInfo, RuntimeDebug, Clone, PartialEq)]
	pub(crate) enum Assignment {
		/// A pool assignment (on-demand).
		Pool { para_id: ParaId, core_index: CoreIndex },
		/// A bulk assignment (from broker chain).
		Bulk(ParaId),
	}

	impl Assignment {
		pub fn para_id(&self) -> ParaId {
			match self {
				Self::Pool { para_id, .. } => *para_id,
				Self::Bulk(para_id) => *para_id,
			}
		}
	}

	#[storage_alias]
	pub(crate) type ClaimQueue<T: Config> =
		StorageValue<Pallet<T>, BTreeMap<CoreIndex, VecDeque<Assignment>>, ValueQuery>;

	/// Storage alias for the old CoreSchedules storage in the AssignerCoretime pallet.
	///
	/// NOTE: The pallet name must match the name used in the runtime's `construct_runtime!` macro.
	/// This is typically "AssignerCoretime" for Polkadot/Kusama/Rococo/Westend runtimes.
	///
	/// We can decode directly into V4 types (Schedule, PartsOf57600, etc.) because they're
	/// binary-compatible with the V3 types - field visibility doesn't affect encoding.
	#[storage_alias]
	pub(crate) type CoreSchedules<T: Config> = StorageMap<
		AssignerCoretime,
		Twox256,
		(BlockNumberFor<T>, CoreIndex),
		Schedule<BlockNumberFor<T>>,
		OptionQuery,
	>;

	/// Storage alias for the old CoreDescriptors storage in the AssignerCoretime pallet.
	///
	/// NOTE: The pallet name must match the name used in the runtime's `construct_runtime!` macro.
	#[storage_alias]
	pub(crate) type CoreDescriptors<T: Config> = StorageMap<
		AssignerCoretime,
		Twox256,
		CoreIndex,
		CoreDescriptor<BlockNumberFor<T>>,
		ValueQuery,
	>;
}

/// Migration for consolidating coretime assignment scheduling into the Scheduler pallet.
///
/// V4 completes the transition to the new coretime scheduling model by moving all coretime-related
/// storage from the deprecated top-level AssignerCoretime pallet into the Scheduler pallet.
///
/// Major changes in V4:
/// - **Removes ClaimQueue storage**: The scheduler no longer maintains a claim queue. Assignment
///   scheduling is now handled entirely through CoreSchedules managed by the assigner_coretime
///   submodule.
/// - **Migrates CoreSchedules**: Moves schedule assignments from the deprecated AssignerCoretime
///   pallet to Scheduler pallet storage, following linked-list structure via `next_schedule`
///   pointers.
/// - **Migrates CoreDescriptors**: Moves core metadata (queue descriptors and current work state)
///   from the deprecated pallet to Scheduler pallet storage.
/// - **Upgrades storage hasher**: Changes from Twox256 (non-reversible) to Twox64Concat
///   (reversible) for CoreSchedules, enabling efficient iteration and queries.
/// - **Preserves on-demand orders**: Any Pool assignments remaining in ClaimQueue are migrated to
///   the on-demand pallet queue to ensure no orders are lost.
/// - **Drops bulk assignments**: Bulk assignments (from broker chain) in ClaimQueue are
///   intentionally dropped as they will be rescheduled from CoreSchedules in the new model.
///
/// After this migration, the deprecated AssignerCoretime pallet becomes an empty stub that can be
/// removed once all networks have upgraded.
pub struct UncheckedMigrateToV4<T>(core::marker::PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for UncheckedMigrateToV4<T> {
	fn on_runtime_upgrade() -> Weight {
		let mut weight: Weight = Weight::zero();

		// Get the actual number of cores from configuration
		let num_cores = configuration::ActiveConfig::<T>::get().scheduler_params.num_cores;
		weight.saturating_accrue(T::DbWeight::get().reads(1));

		// Step 1 & 2: Migrate CoreDescriptors and CoreSchedules together by enumerating cores
		let mut schedule_count = 0u64;
		let mut descriptor_count = 0u64;
		let mut new_descriptors: BTreeMap<CoreIndex, CoreDescriptor<BlockNumberFor<T>>> =
			BTreeMap::new();

		for core_idx in 0..num_cores {
			let core_index = CoreIndex(core_idx);

			// Get the descriptor for this core
			let old_descriptor = v3::CoreDescriptors::<T>::get(core_index);
			weight.saturating_accrue(T::DbWeight::get().reads(1));

			// Check if this core has a non-default descriptor
			if old_descriptor.queue().is_none() && old_descriptor.current_work().is_none() {
				continue; // Skip empty/default descriptors
			}

			descriptor_count += 1;

			// Migrate schedules for this core by following the queue linked list
			if let Some(queue) = old_descriptor.queue() {
				let mut current_block = Some(queue.first);

				while let Some(block_number) = current_block {
					let key = (block_number, core_index);

					if let Some(schedule) = v3::CoreSchedules::<T>::take(key) {
						schedule_count += 1;
						weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));

						// Save next_schedule before moving schedule
						let next = schedule.next_schedule();

						// Insert into new storage with new hasher (Twox64Concat)
						super::CoreSchedules::<T>::insert(key, schedule);
						weight.saturating_accrue(T::DbWeight::get().writes(1));

						// Move to next schedule in queue
						current_block = next;
					} else {
						// Queue is broken or reached the end

						log::error!(
							target: super::LOG_TARGET,
							"Next queue entry was missing - this is unexpected, (core, block): {:?}",
							key,
						);
						break;
					}
				}
			}

			// Descriptor can be used as-is since types are binary-compatible
			new_descriptors.insert(core_index, old_descriptor);
		}

		// Write all descriptors at once
		super::CoreDescriptors::<T>::put(new_descriptors);
		weight.saturating_accrue(T::DbWeight::get().writes(1));

		// Step 3: Migrate ClaimQueue - preserve pool assignments
		let old_claim_queue = v3::ClaimQueue::<T>::take();
		weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));

		let mut total_assignments = 0u32;
		let mut migrated_pool_assignments = 0u32;

		// Extract and preserve only Pool (on-demand) assignments.
		// Bulk assignments will be repopulated from the broker chain via CoreSchedules.
		for (_core_idx, assignments) in old_claim_queue.iter() {
			for assignment in assignments {
				total_assignments = total_assignments.saturating_add(1);
				if let v3::Assignment::Pool { para_id, .. } = assignment {
					// Push the on-demand order back to the on-demand pallet.
					// This ensures user-paid orders are not lost.
					on_demand::Pallet::<T>::push_back_order(*para_id);
					migrated_pool_assignments = migrated_pool_assignments.saturating_add(1);
				}
				// Bulk assignments are intentionally dropped - this is
				// technically not fully correct, but will not matter in
				// practice as virtually nobody is sharing cores right now
				// and even if so, this lack in preciseness would hardly be
				// noticable.
			}
		}

		// Account for writes to on-demand storage for pool assignments
		weight.saturating_accrue(T::DbWeight::get().writes(migrated_pool_assignments as u64));

		log::info!(
			target: super::LOG_TARGET,
			"Migrated para scheduler storage to v4: {} CoreSchedules, {} CoreDescriptors migrated from AssignerCoretime to Scheduler; removed ClaimQueue ({} total assignments, {} pool assignments migrated to on-demand)",
			schedule_count,
			descriptor_count,
			total_assignments,
			migrated_pool_assignments
		);

		weight
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::DispatchError> {
		// Count schedules and descriptors by enumerating cores
		let num_cores = configuration::ActiveConfig::<T>::get().scheduler_params.num_cores;
		let mut schedule_count = 0u32;
		let mut descriptor_count = 0u32;

		for core_idx in 0..num_cores {
			let core_index = CoreIndex(core_idx);
			let descriptor = v3::CoreDescriptors::<T>::get(core_index);

			if descriptor.queue().is_some() || descriptor.current_work().is_some() {
				descriptor_count += 1;

				// Count schedules by following the queue
				if let Some(queue) = descriptor.queue() {
					let mut current_block = Some(queue.first);
					while let Some(block_number) = current_block {
						let key = (block_number, core_index);
						if let Some(schedule) = v3::CoreSchedules::<T>::get(key) {
							schedule_count += 1;
							current_block = schedule.next_schedule();
						} else {
							break;
						}
					}
				}
			}
		}

		let claim_queue = v3::ClaimQueue::<T>::get();
		let mut total_assignments = 0u32;
		let mut pool_assignments = 0u32;

		for (_core_idx, assignments) in claim_queue.iter() {
			for assignment in assignments {
				total_assignments = total_assignments.saturating_add(1);
				if matches!(assignment, v3::Assignment::Pool { .. }) {
					pool_assignments = pool_assignments.saturating_add(1);
				}
			}
		}

		log::info!(
			target: super::LOG_TARGET,
			"Before migration v4: {} CoreSchedules, {} CoreDescriptors, {} ClaimQueue assignments ({} pool, {} bulk)",
			schedule_count,
			descriptor_count,
			total_assignments,
			pool_assignments,
			total_assignments.saturating_sub(pool_assignments)
		);

		Ok((schedule_count, descriptor_count, total_assignments, pool_assignments).encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::DispatchError> {
		log::info!(target: super::LOG_TARGET, "Running post_upgrade() for v4");

		let (
			expected_schedule_count,
			expected_descriptor_count,
			total_assignments,
			expected_pool_assignments,
		): (u32, u32, u32, u32) =
			Decode::decode(&mut &state[..]).map_err(|_| "Failed to decode pre_upgrade state")?;

		// Verify old storage is cleaned up
		ensure!(!v3::ClaimQueue::<T>::exists(), "ClaimQueue storage should have been removed");

		// Check old CoreSchedules and CoreDescriptors are empty by enumerating cores
		let num_cores = configuration::ActiveConfig::<T>::get().scheduler_params.num_cores;
		for core_idx in 0..num_cores {
			let core_index = CoreIndex(core_idx);

			// Check descriptor is default/empty
			let old_descriptor = v3::CoreDescriptors::<T>::get(core_index);
			ensure!(
				old_descriptor.queue().is_none() && old_descriptor.current_work().is_none(),
				"Old CoreDescriptors should be empty"
			);

			// Check no schedules remain (by checking a few potential block numbers)
			// We can't fully verify without iterating all possible block numbers,
			// but checking the descriptor is empty should be sufficient
		}

		// Verify new storage (Twox64Concat allows iteration)
		let new_schedule_count = super::CoreSchedules::<T>::iter().count() as u32;
		ensure!(
			new_schedule_count == expected_schedule_count,
			"CoreSchedules count mismatch after migration"
		);

		let new_descriptor_count = super::CoreDescriptors::<T>::get().len() as u32;
		ensure!(
			new_descriptor_count == expected_descriptor_count,
			"CoreDescriptors count mismatch after migration"
		);

		log::info!(
			target: super::LOG_TARGET,
			"Successfully migrated v4: {} CoreSchedules, {} CoreDescriptors from AssignerCoretime to Scheduler; {} ClaimQueue assignments ({} pool pushed to on-demand)",
			new_schedule_count,
			new_descriptor_count,
			total_assignments,
			expected_pool_assignments
		);

		Ok(())
	}
}

/// Migrate `V3` to `V4` of the storage format.
pub type MigrateV3ToV4<T> = VersionedMigration<
	3,
	4,
	UncheckedMigrateToV4<T>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;

#[cfg(test)]
mod v4_tests {
	use super::*;
	use crate::{
		configuration,
		mock::{new_test_ext, MockGenesisConfig, System, Test},
		on_demand, scheduler,
	};
	use alloc::collections::BTreeMap;
	use frame_support::traits::StorageVersion;
	use pallet_broker::CoreAssignment as BrokerCoreAssignment;
	use polkadot_primitives::{CoreIndex, Id as ParaId};

	use super::assigner_coretime::{
		AssignmentState, CoreDescriptor, PartsOf57600, QueueDescriptor, Schedule, WorkState,
	};

	#[test]
	fn basic_migration_works() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			// Setup configuration with 1 core
			configuration::ActiveConfig::<Test>::mutate(|c| {
				c.scheduler_params.num_cores = 1;
			});

			// Setup: Create old storage
			let core = CoreIndex(0);
			let block_number = 10u32;
			let para_id = ParaId::from(1000);

			// Create old schedule
			let old_schedule = Schedule::new(
				vec![(
					BrokerCoreAssignment::Task(para_id.into()),
					PartsOf57600::new_saturating(28800),
				)],
				Some(100u32),
				None,
			);

			// Create old descriptor with queue pointing to this schedule
			let old_descriptor = CoreDescriptor::new(
				Some(QueueDescriptor { first: block_number, last: block_number }),
				None,
			);

			// Write to old storage using storage aliases
			v3::CoreSchedules::<Test>::insert((block_number, core), old_schedule);
			v3::CoreDescriptors::<Test>::insert(core, old_descriptor);

			// Set storage version to 3
			StorageVersion::new(3).put::<super::Pallet<Test>>();

			// Run migration
			let _weight = UncheckedMigrateToV4::<Test>::on_runtime_upgrade();

			// Verify new storage
			let new_schedule = super::CoreSchedules::<Test>::get((block_number, core))
				.expect("Schedule should be migrated");
			assert_eq!(new_schedule.assignments().len(), 1);
			assert_eq!(new_schedule.end_hint(), Some(100u32));
			assert_eq!(new_schedule.next_schedule(), None);

			let new_descriptors = super::CoreDescriptors::<Test>::get();
			let new_descriptor = new_descriptors.get(&core).expect("Descriptor should be migrated");
			assert!(new_descriptor.queue().is_some());
			assert!(new_descriptor.current_work().is_none());

			// Verify old storage is empty
			assert!(v3::CoreSchedules::<Test>::get((block_number, core)).is_none());
		});
	}

	#[test]
	fn multi_core_migration_works() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			// Setup configuration with 3 cores
			configuration::ActiveConfig::<Test>::mutate(|c| {
				c.scheduler_params.num_cores = 3;
			});

			let block_number = 10u32;

			// Setup three cores with different configurations
			for core_idx in 0..3 {
				let core = CoreIndex(core_idx);
				let para_id = ParaId::from(1000 + core_idx);

				let old_schedule = Schedule::new(
					vec![(
						BrokerCoreAssignment::Task(para_id.into()),
						PartsOf57600::new_saturating(57600),
					)],
					None,
					None,
				);

				let old_descriptor = CoreDescriptor::new(
					Some(QueueDescriptor { first: block_number, last: block_number }),
					None,
				);

				v3::CoreSchedules::<Test>::insert((block_number, core), old_schedule);
				v3::CoreDescriptors::<Test>::insert(core, old_descriptor);
			}

			StorageVersion::new(3).put::<super::Pallet<Test>>();

			// Run migration
			UncheckedMigrateToV4::<Test>::on_runtime_upgrade();

			// Verify all cores migrated
			let new_descriptors = super::CoreDescriptors::<Test>::get();
			assert_eq!(new_descriptors.len(), 3);

			for core_idx in 0..3 {
				let core = CoreIndex(core_idx);
				assert!(new_descriptors.contains_key(&core));
				assert!(super::CoreSchedules::<Test>::get((block_number, core)).is_some());
			}
		});
	}

	#[test]
	fn linked_list_migration_works() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			// Setup configuration with 1 core
			configuration::ActiveConfig::<Test>::mutate(|c| {
				c.scheduler_params.num_cores = 1;
			});

			let core = CoreIndex(0);
			let para_id = ParaId::from(1000);

			// Create a linked list: block 10 -> 20 -> 30
			let schedule_30 = Schedule::new(
				vec![(
					BrokerCoreAssignment::Task(para_id.into()),
					PartsOf57600::new_saturating(57600),
				)],
				None,
				None,
			);

			let schedule_20 = Schedule::new(
				vec![(
					BrokerCoreAssignment::Task(para_id.into()),
					PartsOf57600::new_saturating(57600),
				)],
				None,
				Some(30u32),
			);

			let schedule_10 = Schedule::new(
				vec![(
					BrokerCoreAssignment::Task(para_id.into()),
					PartsOf57600::new_saturating(57600),
				)],
				None,
				Some(20u32),
			);

			// Write schedules
			v3::CoreSchedules::<Test>::insert((10u32, core), schedule_10);
			v3::CoreSchedules::<Test>::insert((20u32, core), schedule_20);
			v3::CoreSchedules::<Test>::insert((30u32, core), schedule_30);

			// Descriptor points to first schedule
			let old_descriptor = CoreDescriptor::new(
				Some(QueueDescriptor { first: 10u32, last: 30u32 }),
				None,
			);

			v3::CoreDescriptors::<Test>::insert(core, old_descriptor);

			StorageVersion::new(3).put::<super::Pallet<Test>>();

			// Run migration
			UncheckedMigrateToV4::<Test>::on_runtime_upgrade();

			// Verify all three schedules migrated
			assert!(super::CoreSchedules::<Test>::get((10u32, core)).is_some());
			assert!(super::CoreSchedules::<Test>::get((20u32, core)).is_some());
			assert!(super::CoreSchedules::<Test>::get((30u32, core)).is_some());

			// Verify next_schedule links preserved
			let new_10 = super::CoreSchedules::<Test>::get((10u32, core)).unwrap();
			assert_eq!(new_10.next_schedule(), Some(20u32));

			let new_20 = super::CoreSchedules::<Test>::get((20u32, core)).unwrap();
			assert_eq!(new_20.next_schedule(), Some(30u32));

			let new_30 = super::CoreSchedules::<Test>::get((30u32, core)).unwrap();
			assert_eq!(new_30.next_schedule(), None);
		});
	}

	#[test]
	fn claim_queue_pool_assignments_preserved() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			let core = CoreIndex(0);
			let pool_para_1 = ParaId::from(1000);
			let pool_para_2 = ParaId::from(1001);
			let bulk_para = ParaId::from(2000);

			// Create ClaimQueue with mixed assignments
			let mut claim_queue = BTreeMap::new();
			let mut assignments = VecDeque::new();
			assignments.push_back(v3::Assignment::Pool { para_id: pool_para_1, core_index: core });
			assignments.push_back(v3::Assignment::Bulk(bulk_para));
			assignments.push_back(v3::Assignment::Pool { para_id: pool_para_2, core_index: core });
			claim_queue.insert(core, assignments);

			v3::ClaimQueue::<Test>::put(claim_queue);

			StorageVersion::new(3).put::<super::Pallet<Test>>();

			// Verify claim_queue() returns the old ClaimQueue before migration
			let claim_queue_before = super::Pallet::<Test>::claim_queue();
			assert_eq!(claim_queue_before.len(), 1, "Should have 1 core in claim queue");
			let core_queue = claim_queue_before.get(&core).expect("Core should be in claim queue");
			assert_eq!(core_queue.len(), 3, "Core should have 3 assignments");
			assert_eq!(core_queue[0], pool_para_1);
			assert_eq!(core_queue[1], bulk_para);
			assert_eq!(core_queue[2], pool_para_2);

			// Run migration
			UncheckedMigrateToV4::<Test>::on_runtime_upgrade();

			// Verify ClaimQueue is removed
			assert!(!v3::ClaimQueue::<Test>::exists());

			// Verify pool assignments went to on-demand
			// The migration calls `on_demand::Pallet::<T>::push_back_order` for each pool
			// assignment, which adds them to the on-demand queue. We verify by popping
			// assignments. Orders are ready 2 blocks after being placed (asynchronous backing).
			let mut on_demand_queue = on_demand::Pallet::<Test>::peek_order_queue();
			let now = System::block_number().saturating_add(2); // Advance 2 blocks for async backing
			let popped: Vec<ParaId> =
				on_demand_queue.pop_assignment_for_cores::<Test>(now, 2).collect();

			assert_eq!(popped.len(), 2, "Should have 2 pool assignments in on-demand queue");
			assert!(popped.contains(&pool_para_1), "pool_para_1 should be in queue");
			assert!(popped.contains(&pool_para_2), "pool_para_2 should be in queue");
		});
	}

	#[test]
	fn claim_queue_bulk_assignments_dropped() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			// Setup configuration with 1 core
			configuration::ActiveConfig::<Test>::mutate(|c| {
				c.scheduler_params.num_cores = 1;
			});

			let core = CoreIndex(0);
			let block_number = System::block_number().saturating_plus_one();

			// Create CoreSchedule with different bulk assignments (these should win)
			let descriptor_para_1 = ParaId::from(3000);
			let descriptor_para_2 = ParaId::from(3001);

			let descriptor_schedule = Schedule::new(
				vec![
					(
						BrokerCoreAssignment::Task(descriptor_para_1.into()),
						PartsOf57600::new_saturating(28800),
					),
					(
						BrokerCoreAssignment::Task(descriptor_para_2.into()),
						PartsOf57600::new_saturating(28800),
					),
				],
				None,
				None,
			);

			let descriptor = CoreDescriptor::new(
				Some(QueueDescriptor { first: block_number, last: block_number }),
				None,
			);

			v3::CoreSchedules::<Test>::insert((block_number, core), descriptor_schedule);
			v3::CoreDescriptors::<Test>::insert(core, descriptor);

			// Create ClaimQueue with different bulk assignments (these should be dropped)
			let claimqueue_para_1 = ParaId::from(2000);
			let claimqueue_para_2 = ParaId::from(2001);

			let mut claim_queue = BTreeMap::new();
			let mut assignments = VecDeque::new();
			assignments.push_back(v3::Assignment::Bulk(claimqueue_para_1));
			assignments.push_back(v3::Assignment::Bulk(claimqueue_para_2));
			claim_queue.insert(core, assignments);

			v3::ClaimQueue::<Test>::put(claim_queue);

			StorageVersion::new(3).put::<super::Pallet<Test>>();

			// Run migration
			UncheckedMigrateToV4::<Test>::on_runtime_upgrade();

			// Verify ClaimQueue is removed
			assert!(!v3::ClaimQueue::<Test>::exists());

			// Peek at the next block to see what will be scheduled
			// Should see assignments from descriptor (3000, 3001), NOT from ClaimQueue (2000, 2001)
			let peeked = scheduler::assigner_coretime::peek_next_block::<Test>(10);

			let core_assignments = peeked.get(&core).expect("Core should have assignments");
			let para_ids: Vec<ParaId> = core_assignments.iter().copied().collect();

			// Verify we see descriptor paras, not claimqueue paras
			assert!(para_ids.contains(&descriptor_para_1), "Should contain para from descriptor");
			assert!(para_ids.contains(&descriptor_para_2), "Should contain para from descriptor");
			assert!(
				!para_ids.contains(&claimqueue_para_1),
				"Should NOT contain para from old ClaimQueue"
			);
			assert!(
				!para_ids.contains(&claimqueue_para_2),
				"Should NOT contain para from old ClaimQueue"
			);
		});
	}

	#[test]
	fn empty_storage_migration_works() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			// No old storage created
			StorageVersion::new(3).put::<super::Pallet<Test>>();

			// Run migration on empty storage - should complete without panicking
			let _weight = UncheckedMigrateToV4::<Test>::on_runtime_upgrade();

			// Verify new storage is empty
			let new_descriptors = super::CoreDescriptors::<Test>::get();
			assert!(new_descriptors.is_empty());
		});
	}

	#[test]
	fn parts_of_57600_conversion_works() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			// Setup configuration with 1 core
			configuration::ActiveConfig::<Test>::mutate(|c| {
				c.scheduler_params.num_cores = 1;
			});

			let core = CoreIndex(0);
			let block_number = 10u32;
			let para_id = ParaId::from(1000);

			// Create schedule with various PartsOf57600 values
			let old_schedule = Schedule::new(
				vec![
					(
						BrokerCoreAssignment::Task(para_id.into()),
						PartsOf57600::new_saturating(14400), // 1/4
					),
					(
						BrokerCoreAssignment::Task(ParaId::from(1001).into()),
						PartsOf57600::new_saturating(28800), // 1/2
					),
					(
						BrokerCoreAssignment::Task(ParaId::from(1002).into()),
						PartsOf57600::new_saturating(14400), // 1/4
					),
				],
				None,
				None,
			);

			let old_descriptor = CoreDescriptor::new(
				Some(QueueDescriptor { first: block_number, last: block_number }),
				None,
			);

			v3::CoreSchedules::<Test>::insert((block_number, core), old_schedule);
			v3::CoreDescriptors::<Test>::insert(core, old_descriptor);

			StorageVersion::new(3).put::<super::Pallet<Test>>();

			// Run migration
			UncheckedMigrateToV4::<Test>::on_runtime_upgrade();

			// Verify assignments and their parts converted correctly
			let new_schedule = super::CoreSchedules::<Test>::get((block_number, core))
				.expect("Schedule should be migrated");
			assert_eq!(new_schedule.assignments().len(), 3);

			// Check sum of parts equals full allocation (14400 + 28800 + 14400 = 57600)
			let sum: u16 = new_schedule.assignments().iter().map(|(_, parts)| parts.value()).sum();
			assert_eq!(sum, 57600, "Sum of parts should equal full allocation");
		});
	}

	#[test]
	fn current_work_state_migrated() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			// Setup configuration with 1 core
			configuration::ActiveConfig::<Test>::mutate(|c| {
				c.scheduler_params.num_cores = 1;
			});

			let core = CoreIndex(0);
			let para_id = ParaId::from(1000);

			// Create descriptor with current_work
			let old_descriptor = CoreDescriptor::new(
				None,
				Some(WorkState {
					assignments: vec![(
						BrokerCoreAssignment::Task(para_id.into()),
						AssignmentState {
							ratio: PartsOf57600::new_saturating(57600),
							remaining: PartsOf57600::new_saturating(28800),
						},
					)],
					end_hint: Some(100u32),
					pos: 0,
					step: PartsOf57600::new_saturating(1),
				}),
			);

			v3::CoreDescriptors::<Test>::insert(core, old_descriptor);

			StorageVersion::new(3).put::<super::Pallet<Test>>();

			// Run migration
			UncheckedMigrateToV4::<Test>::on_runtime_upgrade();

			// Verify current_work migrated
			let new_descriptors = super::CoreDescriptors::<Test>::get();
			let new_descriptor = new_descriptors.get(&core).expect("Descriptor should exist");
			assert!(new_descriptor.current_work().is_some());

			let work = new_descriptor.current_work().unwrap();
			assert_eq!(work.assignments.len(), 1);

			// Verify the assignment details match what we set up
			let (assignment, state) = &work.assignments[0];
			match assignment {
				BrokerCoreAssignment::Task(task_id) => {
					assert_eq!(ParaId::from(*task_id), para_id, "ParaId should match");
				},
				_ => panic!("Expected Task assignment"),
			}

			// Verify assignment state values
			assert_eq!(state.ratio.value(), 57600, "Ratio should be full allocation");
			assert_eq!(state.remaining.value(), 28800, "Remaining should be half");

			// Verify work state metadata
			assert_eq!(work.end_hint, Some(100u32));
			assert_eq!(work.pos, 0);
			assert_eq!(work.step.value(), 1, "Step should be 1");
		});
	}
}
