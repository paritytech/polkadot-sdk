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
use crate::paras;
use alloc::vec::Vec;
use frame_support::{
	migrations::VersionedMigration, pallet_prelude::ValueQuery, storage_alias,
	traits::UncheckedOnRuntimeUpgrade, weights::Weight,
};

/// Old/legacy assignment representation (v0).
///
/// `Assignment` used to be a concrete type with the same layout V0Assignment, identical on all
/// assignment providers. This can be removed once storage has been migrated.
#[derive(Encode, Decode, RuntimeDebug, TypeInfo, PartialEq, Clone)]
struct V0Assignment {
	pub para_id: ParaId,
}

/// Assignment type used in V2 and V3 storage (before migration to V4).
#[derive(Encode, Decode, TypeInfo, RuntimeDebug, Clone, PartialEq)]
pub(crate) enum Assignment {
	/// A pool assignment (on-demand).
	Pool { para_id: ParaId, core_index: CoreIndex },
	/// A bulk assignment (from broker chain).
	Bulk(ParaId),
}

/// Old scheduler with explicit parathreads and `Scheduled` storage instead of `ClaimQueue`.
mod v0 {
	use super::*;
	use polkadot_primitives::{CollatorId, Id};

	#[storage_alias]
	pub(super) type Scheduled<T: Config> = StorageValue<Pallet<T>, Vec<CoreAssignment>, ValueQuery>;

	#[derive(Clone, Encode, Decode)]
	#[cfg_attr(feature = "std", derive(PartialEq))]
	pub struct ParathreadClaim(pub Id, pub CollatorId);

	#[derive(Clone, Encode, Decode)]
	#[cfg_attr(feature = "std", derive(PartialEq))]
	pub struct ParathreadEntry {
		/// The claim.
		pub claim: ParathreadClaim,
		/// Number of retries.
		pub retries: u32,
	}

	/// What is occupying a specific availability core.
	#[derive(Clone, Encode, Decode)]
	#[cfg_attr(feature = "std", derive(PartialEq))]
	pub enum CoreOccupied {
		/// A parathread.
		Parathread(ParathreadEntry),
		/// A parachain.
		Parachain,
	}

	/// The actual type isn't important, as we only delete the key in the state.
	#[storage_alias]
	pub(crate) type AvailabilityCores<T: Config> =
		StorageValue<Pallet<T>, Vec<Option<CoreOccupied>>, ValueQuery>;

	/// The actual type isn't important, as we only delete the key in the state.
	#[storage_alias]
	pub(super) type ParathreadQueue<T: Config> = StorageValue<Pallet<T>, (), ValueQuery>;

	#[storage_alias]
	pub(super) type ParathreadClaimIndex<T: Config> = StorageValue<Pallet<T>, (), ValueQuery>;

	/// The assignment type.
	#[derive(Clone, Encode, Decode, TypeInfo, RuntimeDebug)]
	#[cfg_attr(feature = "std", derive(PartialEq))]
	pub enum AssignmentKind {
		/// A parachain.
		Parachain,
		/// A parathread.
		Parathread(CollatorId, u32),
	}

	/// How a free core is scheduled to be assigned.
	#[derive(Clone, Encode, Decode, TypeInfo, RuntimeDebug)]
	#[cfg_attr(feature = "std", derive(PartialEq))]
	pub struct CoreAssignment {
		/// The core that is assigned.
		pub core: CoreIndex,
		/// The unique ID of the para that is assigned to the core.
		pub para_id: ParaId,
		/// The kind of the assignment.
		pub kind: AssignmentKind,
		/// The index of the validator group assigned to the core.
		pub group_idx: GroupIndex,
	}
}

// `ClaimQueue` got introduced.
//
// - Items are `Option` for some weird reason.
// - Assignments only consist of `ParaId`, `Assignment` is a concrete type (Same as V0Assignment).
mod v1 {
	use frame_support::{
		pallet_prelude::ValueQuery, storage_alias, traits::UncheckedOnRuntimeUpgrade,
		weights::Weight,
	};
	use frame_system::pallet_prelude::BlockNumberFor;

	use super::*;
	use crate::scheduler;

	#[storage_alias]
	pub(super) type ClaimQueue<T: Config> = StorageValue<
		Pallet<T>,
		BTreeMap<CoreIndex, VecDeque<Option<ParasEntry<BlockNumberFor<T>>>>>,
		ValueQuery,
	>;

	#[storage_alias]
	pub(super) type AvailabilityCores<T: Config> =
		StorageValue<Pallet<T>, Vec<CoreOccupied<BlockNumberFor<T>>>, ValueQuery>;

	#[derive(Encode, Decode, TypeInfo, RuntimeDebug, PartialEq)]
	pub(super) enum CoreOccupied<N> {
		/// No candidate is waiting availability on this core right now (the core is not occupied).
		Free,
		/// A para is currently waiting for availability/inclusion on this core.
		Paras(ParasEntry<N>),
	}

	#[derive(Encode, Decode, TypeInfo, RuntimeDebug, PartialEq)]
	pub(super) struct ParasEntry<N> {
		/// The underlying `Assignment`
		pub(super) assignment: V0Assignment,
		/// The number of times the entry has timed out in availability already.
		pub(super) availability_timeouts: u32,
		/// The block height until this entry needs to be backed.
		///
		/// If missed the entry will be removed from the claim queue without ever having occupied
		/// the core.
		pub(super) ttl: N,
	}

	impl<N> ParasEntry<N> {
		/// Create a new `ParasEntry`.
		pub(super) fn new(assignment: V0Assignment, now: N) -> Self {
			ParasEntry { assignment, availability_timeouts: 0, ttl: now }
		}

		/// Return `Id` from the underlying `Assignment`.
		pub(super) fn para_id(&self) -> ParaId {
			self.assignment.para_id
		}
	}

	fn add_to_claimqueue<T: Config>(core_idx: CoreIndex, pe: ParasEntry<BlockNumberFor<T>>) {
		ClaimQueue::<T>::mutate(|la| {
			la.entry(core_idx).or_default().push_back(Some(pe));
		});
	}

	/// Migration to V1
	pub struct UncheckedMigrateToV1<T>(core::marker::PhantomData<T>);
	impl<T: Config> UncheckedOnRuntimeUpgrade for UncheckedMigrateToV1<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight: Weight = Weight::zero();

			v0::ParathreadQueue::<T>::kill();
			v0::ParathreadClaimIndex::<T>::kill();

			let now = frame_system::Pallet::<T>::block_number();
			let scheduled = v0::Scheduled::<T>::take();
			let sched_len = scheduled.len() as u64;
			for core_assignment in scheduled {
				let core_idx = core_assignment.core;
				let assignment = V0Assignment { para_id: core_assignment.para_id };
				let pe = v1::ParasEntry::new(assignment, now);
				v1::add_to_claimqueue::<T>(core_idx, pe);
			}

			let parachains = paras::Parachains::<T>::get();
			let availability_cores = v0::AvailabilityCores::<T>::take();
			let mut new_availability_cores = Vec::new();

			for (core_index, core) in availability_cores.into_iter().enumerate() {
				let new_core = if let Some(core) = core {
					match core {
						v0::CoreOccupied::Parachain =>
							v1::CoreOccupied::Paras(v1::ParasEntry::new(
								V0Assignment { para_id: parachains[core_index] },
								now,
							)),
						v0::CoreOccupied::Parathread(entry) => v1::CoreOccupied::Paras(
							v1::ParasEntry::new(V0Assignment { para_id: entry.claim.0 }, now),
						),
					}
				} else {
					v1::CoreOccupied::Free
				};

				new_availability_cores.push(new_core);
			}

			v1::AvailabilityCores::<T>::set(new_availability_cores);

			// 2x as once for Scheduled and once for Claimqueue
			weight.saturating_accrue(T::DbWeight::get().reads_writes(2 * sched_len, 2 * sched_len));
			// reading parachains + availability_cores, writing AvailabilityCores
			weight.saturating_accrue(T::DbWeight::get().reads_writes(2, 1));
			// 2x kill
			weight.saturating_accrue(T::DbWeight::get().writes(2));

			log::info!(target: super::LOG_TARGET, "Migrated para scheduler storage to v1");

			weight
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::DispatchError> {
			let n: u32 = v0::Scheduled::<T>::get().len() as u32 +
				v0::AvailabilityCores::<T>::get().iter().filter(|c| c.is_some()).count() as u32;

			log::info!(
				target: crate::super::LOG_TARGET,
				"Number of scheduled and waiting for availability before: {n}",
			);

			Ok(n.encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::DispatchError> {
			log::info!(target: crate::super::LOG_TARGET, "Running post_upgrade()");

			ensure!(
				v0::Scheduled::<T>::get().is_empty(),
				"Scheduled should be empty after the migration"
			);

			let expected_len = u32::decode(&mut &state[..]).unwrap();
			let availability_cores_waiting = v1::AvailabilityCores::<T>::get()
				.into_iter()
				.filter(|c| !matches!(c, v1::CoreOccupied::Free))
				.count();

			ensure!(
				Pallet::<T>::claim_queue_len() as u32 + availability_cores_waiting as u32 ==
					expected_len,
				"ClaimQueue and AvailabilityCores should have the correct length",
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

/// Migration for introducing the `Assignment` enum to distinguish between Pool (on-demand) and
/// Bulk (broker chain) assignments.
///
/// V2 introduces a proper `Assignment` type to replace the simple `ParaId` assignments from V1.
/// This allows the scheduler to distinguish between:
/// - Pool assignments: on-demand parachains that use the on-demand pool
/// - Bulk assignments: parachains with bulk coretime purchased from the broker chain
///
/// The migration:
/// - Removes the `Option` wrapper from ClaimQueue entries
/// - Replaces `V0Assignment` (just a ParaId) with the new `Assignment` enum
/// - Determines assignment type based on core index (cores below parachain count are Bulk)
/// - Preserves availability_timeouts and ttl fields in the new ParasEntry structure
pub(crate) mod v2 {
	use super::*;
	use crate::scheduler;

	#[derive(Encode, Decode, TypeInfo, RuntimeDebug, PartialEq)]
	pub(crate) enum CoreOccupied<N> {
		Free,
		Paras(ParasEntry<N>),
	}

	#[derive(Encode, Decode, TypeInfo, RuntimeDebug, PartialEq)]
	pub(crate) struct ParasEntry<N> {
		pub assignment: Assignment,
		pub availability_timeouts: u32,
		pub ttl: N,
	}

	// V2 (no Option wrapper) and new [`Assignment`].
	#[storage_alias]
	pub(crate) type ClaimQueue<T: Config> = StorageValue<
		Pallet<T>,
		BTreeMap<CoreIndex, VecDeque<ParasEntry<BlockNumberFor<T>>>>,
		ValueQuery,
	>;

	#[storage_alias]
	pub(crate) type AvailabilityCores<T: Config> =
		StorageValue<Pallet<T>, Vec<CoreOccupied<BlockNumberFor<T>>>, ValueQuery>;

	fn is_bulk<T: Config>(core_index: CoreIndex) -> bool {
		core_index.0 < paras::Parachains::<T>::decode_len().unwrap_or(0) as u32
	}

	/// Migration to V2
	pub struct UncheckedMigrateToV2<T>(core::marker::PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for UncheckedMigrateToV2<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight: Weight = Weight::zero();

			let old = v1::ClaimQueue::<T>::take();
			let new = old
				.into_iter()
				.map(|(k, v)| {
					(
						k,
						v.into_iter()
							.flatten()
							.map(|p| {
								let assignment = if is_bulk::<T>(k) {
									Assignment::Bulk(p.para_id())
								} else {
									Assignment::Pool { para_id: p.para_id(), core_index: k }
								};

								ParasEntry {
									assignment,
									availability_timeouts: p.availability_timeouts,
									ttl: p.ttl,
								}
							})
							.collect::<VecDeque<_>>(),
					)
				})
				.collect::<BTreeMap<CoreIndex, VecDeque<ParasEntry<BlockNumberFor<T>>>>>();

			ClaimQueue::<T>::put(new);

			let old = v1::AvailabilityCores::<T>::get();

			let new = old
				.into_iter()
				.enumerate()
				.map(|(k, a)| match a {
					v1::CoreOccupied::Free => CoreOccupied::Free,
					v1::CoreOccupied::Paras(paras) => {
						let assignment = if is_bulk::<T>((k as u32).into()) {
							Assignment::Bulk(paras.para_id())
						} else {
							Assignment::Pool {
								para_id: paras.para_id(),
								core_index: (k as u32).into(),
							}
						};

						CoreOccupied::Paras(ParasEntry {
							assignment,
							availability_timeouts: paras.availability_timeouts,
							ttl: paras.ttl,
						})
					},
				})
				.collect::<Vec<_>>();
			AvailabilityCores::<T>::put(new);

			weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));

			log::info!(target: super::LOG_TARGET, "Migrating para scheduler storage to v2");

			weight
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::DispatchError> {
			log::trace!(
				target: crate::super::LOG_TARGET,
				"ClaimQueue before migration: {}",
				v1::ClaimQueue::<T>::get().len()
			);

			let bytes = u32::to_be_bytes(v1::ClaimQueue::<T>::get().len() as u32);

			Ok(bytes.to_vec())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::DispatchError> {
			log::trace!(target: crate::super::LOG_TARGET, "Running post_upgrade()");

			let old_len = u32::from_be_bytes(state.try_into().unwrap());
			ensure!(
				v2::ClaimQueue::<T>::get().len() as u32 == old_len,
				"Old ClaimQueue completely moved to new ClaimQueue after migration"
			);

			Ok(())
		}
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

/// Migration for simplifying the ClaimQueue structure and removing redundant availability tracking.
///
/// V3 simplifies the scheduler by removing fields that became redundant after the inclusion
/// module took over availability tracking responsibilities.
///
/// Changes in V3:
/// - Removes `ttl` (time-to-live) field from ClaimQueue entries - no longer needed as the inclusion
///   module now handles candidate lifecycle
/// - Removes `availability_timeouts` counter - availability is now tracked by the inclusion module
/// - Simplifies ClaimQueue to directly store `Assignment`s instead of wrapping them in `ParasEntry`
/// - Removes the `AvailabilityCores` storage entirely - this information is now maintained by the
///   inclusion pallet, eliminating duplication
///
/// This migration streamlines the scheduler's responsibility to just assignment scheduling,
/// while the inclusion module handles all availability-related tracking.
mod v3 {
	use super::*;
	use crate::scheduler;

	#[storage_alias]
	pub(crate) type ClaimQueue<T: Config> =
		StorageValue<Pallet<T>, BTreeMap<CoreIndex, VecDeque<Assignment>>, ValueQuery>;
	/// Migration to V3
	pub struct UncheckedMigrateToV3<T>(core::marker::PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for UncheckedMigrateToV3<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight: Weight = Weight::zero();

			// Migrate ClaimQueuee to new format.

			let old = v2::ClaimQueue::<T>::take();
			let new = old
				.into_iter()
				.map(|(k, v)| {
					(
						k,
						v.into_iter()
							.map(|paras_entry| paras_entry.assignment)
							.collect::<VecDeque<_>>(),
					)
				})
				.collect::<BTreeMap<CoreIndex, VecDeque<Assignment>>>();

			v3::ClaimQueue::<T>::put(new);

			// Clear AvailabilityCores storage
			v2::AvailabilityCores::<T>::kill();

			weight.saturating_accrue(T::DbWeight::get().reads_writes(2, 2));

			log::info!(target: super::LOG_TARGET, "Migrating para scheduler storage to v3");

			weight
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::DispatchError> {
			log::trace!(
				target: crate::super::LOG_TARGET,
				"ClaimQueue before migration: {}",
				v2::ClaimQueue::<T>::get().len()
			);

			let bytes = u32::to_be_bytes(v2::ClaimQueue::<T>::get().len() as u32);

			Ok(bytes.to_vec())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::DispatchError> {
			log::trace!(target: crate::super::LOG_TARGET, "Running post_upgrade()");

			let old_len = u32::from_be_bytes(state.try_into().unwrap());
			ensure!(
				v3::ClaimQueue::<T>::get().len() as u32 == old_len,
				"Old ClaimQueue completely moved to new ClaimQueue after migration"
			);

			ensure!(
				!v2::AvailabilityCores::<T>::exists(),
				"AvailabilityCores storage should have been completely killed"
			);

			Ok(())
		}
	}
}

/// Migrate `V2` to `V3` of the storage format.
pub type MigrateV2ToV3<T> = VersionedMigration<
	2,
	3,
	v3::UncheckedMigrateToV3<T>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;

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
#[allow(deprecated)]
mod v4 {
	use super::{
		assigner_coretime::{
			AssignmentState, CoreDescriptor, PartsOf57600, QueueDescriptor, Schedule, WorkState,
		},
		*,
	};
	use crate::{assigner_coretime as old, on_demand, scheduler};

	// V3 ClaimQueue storage (to be migrated from)
	#[storage_alias]
	pub(crate) type ClaimQueue<T: Config> =
		StorageValue<Pallet<T>, BTreeMap<CoreIndex, VecDeque<Assignment>>, ValueQuery>;

	/// Migration to V4
	pub struct UncheckedMigrateToV4<T>(core::marker::PhantomData<T>);

	impl<T: Config + old::pallet::Config> UncheckedOnRuntimeUpgrade for UncheckedMigrateToV4<T> {
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
				let old_descriptor = old::pallet::CoreDescriptors::<T>::get(core_index);
				weight.saturating_accrue(T::DbWeight::get().reads(1));

				// Check if this core has a non-default descriptor
				if old_descriptor.queue.is_none() && old_descriptor.current_work.is_none() {
					continue; // Skip empty/default descriptors
				}

				descriptor_count += 1;

				// Migrate schedules for this core by following the queue linked list
				if let Some(queue) = &old_descriptor.queue {
					let mut current_block = Some(queue.first);

					while let Some(block_number) = current_block {
						let key = (block_number, core_index);

						if let Some(old_schedule) = old::pallet::CoreSchedules::<T>::take(key) {
							schedule_count += 1;
							weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));

							// Save next_schedule before moving old_schedule
							let next = old_schedule.next_schedule;

							// Convert and insert into new storage
							let new_schedule = Schedule::new(
								old_schedule
									.assignments
									.into_iter()
									.map(|(assignment, parts)| {
										(assignment, PartsOf57600::new_saturating(parts.0))
									})
									.collect(),
								old_schedule.end_hint,
								old_schedule.next_schedule,
							);
							super::CoreSchedules::<T>::insert(key, new_schedule);
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

				// Convert descriptor from old type to new type
				let new_desc = CoreDescriptor::new(
					old_descriptor
						.queue
						.map(|q| QueueDescriptor { first: q.first, last: q.last }),
					old_descriptor.current_work.map(|w| WorkState {
						assignments: w
							.assignments
							.into_iter()
							.map(|(assignment, state)| {
								(
									assignment,
									AssignmentState {
										ratio: PartsOf57600::new_saturating(state.ratio.0),
										remaining: PartsOf57600::new_saturating(state.remaining.0),
									},
								)
							})
							.collect(),
						end_hint: w.end_hint,
						pos: w.pos,
						step: PartsOf57600::new_saturating(w.step.0),
					}),
				);
				new_descriptors.insert(core_index, new_desc);
			}

			// Write all descriptors at once
			super::CoreDescriptors::<T>::put(new_descriptors);
			weight.saturating_accrue(T::DbWeight::get().writes(1));

			// Step 3: Migrate ClaimQueue - preserve pool assignments
			let old_claim_queue = ClaimQueue::<T>::take();
			weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));

			let mut total_assignments = 0u32;
			let mut migrated_pool_assignments = 0u32;

			// Extract and preserve only Pool (on-demand) assignments.
			// Bulk assignments will be repopulated from the broker chain via CoreSchedules.
			for (_core_idx, assignments) in old_claim_queue.iter() {
				for assignment in assignments {
					total_assignments = total_assignments.saturating_add(1);
					if let Assignment::Pool { para_id, .. } = assignment {
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
				let descriptor = old::pallet::CoreDescriptors::<T>::get(core_index);

				if descriptor.queue.is_some() || descriptor.current_work.is_some() {
					descriptor_count += 1;

					// Count schedules by following the queue
					if let Some(queue) = descriptor.queue {
						let mut current_block = Some(queue.first);
						while let Some(block_number) = current_block {
							let key = (block_number, core_index);
							if let Some(schedule) = old::pallet::CoreSchedules::<T>::get(key) {
								schedule_count += 1;
								current_block = schedule.next_schedule;
							} else {
								break;
							}
						}
					}
				}
			}

			let claim_queue = ClaimQueue::<T>::get();
			let mut total_assignments = 0u32;
			let mut pool_assignments = 0u32;

			for (_core_idx, assignments) in claim_queue.iter() {
				for assignment in assignments {
					total_assignments = total_assignments.saturating_add(1);
					if matches!(assignment, Assignment::Pool { .. }) {
						pool_assignments = pool_assignments.saturating_add(1);
					}
				}
			}

			log::info!(
				target: crate::super::LOG_TARGET,
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
			log::info!(target: crate::super::LOG_TARGET, "Running post_upgrade() for v4");

			let (
				expected_schedule_count,
				expected_descriptor_count,
				total_assignments,
				expected_pool_assignments,
			): (u32, u32, u32, u32) = Decode::decode(&mut &state[..])
				.map_err(|_| "Failed to decode pre_upgrade state")?;

			// Verify old storage is cleaned up
			ensure!(!ClaimQueue::<T>::exists(), "ClaimQueue storage should have been removed");

			// Check old CoreSchedules and CoreDescriptors are empty by enumerating cores
			let num_cores = configuration::ActiveConfig::<T>::get().scheduler_params.num_cores;
			for core_idx in 0..num_cores {
				let core_index = CoreIndex(core_idx);

				// Check descriptor is default/empty
				let old_descriptor = old::pallet::CoreDescriptors::<T>::get(core_index);
				ensure!(
					old_descriptor.queue.is_none() && old_descriptor.current_work.is_none(),
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
				target: crate::super::LOG_TARGET,
				"Successfully migrated v4: {} CoreSchedules, {} CoreDescriptors from AssignerCoretime to Scheduler; {} ClaimQueue assignments ({} pool pushed to on-demand)",
				new_schedule_count,
				new_descriptor_count,
				total_assignments,
				expected_pool_assignments
			);

			Ok(())
		}
	}
}

/// Migrate `V3` to `V4` of the storage format.
pub type MigrateV3ToV4<T> = VersionedMigration<
	3,
	4,
	v4::UncheckedMigrateToV4<T>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;

#[cfg(test)]
mod v4_tests {
	use super::*;
	use crate::{
		assigner_coretime as old, configuration, on_demand,
		mock::{new_test_ext, MockGenesisConfig, System, Test},
		scheduler,
	};
	use alloc::collections::BTreeMap;
	use frame_support::traits::{OnRuntimeUpgrade, StorageVersion};
	use pallet_broker::CoreAssignment as BrokerCoreAssignment;
	use polkadot_primitives::{CoreIndex, Id as ParaId};
	use sp_core::Get;

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
			let old_schedule = old::Schedule {
				assignments: vec![(
					BrokerCoreAssignment::Task(para_id.into()),
					old::PartsOf57600(28800),
				)],
				end_hint: Some(100u32),
				next_schedule: None,
			};

			// Create old descriptor with queue pointing to this schedule
			let old_descriptor = old::CoreDescriptor {
				queue: Some(old::QueueDescriptor { first: block_number, last: block_number }),
				current_work: None,
			};

			// Write to old storage
			old::pallet::CoreSchedules::<Test>::insert((block_number, core), old_schedule);
			old::pallet::CoreDescriptors::<Test>::insert(core, old_descriptor);

			// Set storage version to 3
			StorageVersion::new(3).put::<super::Pallet<Test>>();

			// Run migration
			let _weight = v4::UncheckedMigrateToV4::<Test>::on_runtime_upgrade();

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
			assert!(old::pallet::CoreSchedules::<Test>::get((block_number, core)).is_none());
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

				let old_schedule = old::Schedule {
					assignments: vec![(
						BrokerCoreAssignment::Task(para_id.into()),
						old::PartsOf57600(57600),
					)],
					end_hint: None,
					next_schedule: None,
				};

				let old_descriptor = old::CoreDescriptor {
					queue: Some(old::QueueDescriptor { first: block_number, last: block_number }),
					current_work: None,
				};

				old::pallet::CoreSchedules::<Test>::insert((block_number, core), old_schedule);
				old::pallet::CoreDescriptors::<Test>::insert(core, old_descriptor);
			}

			StorageVersion::new(3).put::<super::Pallet<Test>>();

			// Run migration
			v4::UncheckedMigrateToV4::<Test>::on_runtime_upgrade();

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
			let schedule_30 = old::Schedule {
				assignments: vec![(
					BrokerCoreAssignment::Task(para_id.into()),
					old::PartsOf57600(57600),
				)],
				end_hint: None,
				next_schedule: None,
			};

			let schedule_20 = old::Schedule {
				assignments: vec![(
					BrokerCoreAssignment::Task(para_id.into()),
					old::PartsOf57600(57600),
				)],
				end_hint: None,
				next_schedule: Some(30u32),
			};

			let schedule_10 = old::Schedule {
				assignments: vec![(
					BrokerCoreAssignment::Task(para_id.into()),
					old::PartsOf57600(57600),
				)],
				end_hint: None,
				next_schedule: Some(20u32),
			};

			// Write schedules
			old::pallet::CoreSchedules::<Test>::insert((10u32, core), schedule_10);
			old::pallet::CoreSchedules::<Test>::insert((20u32, core), schedule_20);
			old::pallet::CoreSchedules::<Test>::insert((30u32, core), schedule_30);

			// Descriptor points to first schedule
			let old_descriptor = old::CoreDescriptor {
				queue: Some(old::QueueDescriptor { first: 10u32, last: 30u32 }),
				current_work: None,
			};

			old::pallet::CoreDescriptors::<Test>::insert(core, old_descriptor);

			StorageVersion::new(3).put::<super::Pallet<Test>>();

			// Run migration
			v4::UncheckedMigrateToV4::<Test>::on_runtime_upgrade();

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
			assignments.push_back(Assignment::Pool { para_id: pool_para_1, core_index: core });
			assignments.push_back(Assignment::Bulk(bulk_para));
			assignments.push_back(Assignment::Pool { para_id: pool_para_2, core_index: core });
			claim_queue.insert(core, assignments);

			v4::ClaimQueue::<Test>::put(claim_queue);

			StorageVersion::new(3).put::<super::Pallet<Test>>();

			// Run migration
			v4::UncheckedMigrateToV4::<Test>::on_runtime_upgrade();

			// Verify ClaimQueue is removed
			assert!(!v4::ClaimQueue::<Test>::exists());

			// Verify pool assignments went to on-demand
			// The migration calls `on_demand::Pallet::<T>::push_back_order` for each pool assignment,
			// which adds them to the on-demand queue. We verify by popping assignments.
			// Orders are ready 2 blocks after being placed (asynchronous backing).
			let mut on_demand_queue = on_demand::Pallet::<Test>::peek_order_queue();
			let now = System::block_number().saturating_add(2);  // Advance 2 blocks for async backing
			let popped: Vec<ParaId> = on_demand_queue
				.pop_assignment_for_cores::<Test>(now, 2)
				.collect();

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

			let descriptor_schedule = old::Schedule {
				assignments: vec![
					(BrokerCoreAssignment::Task(descriptor_para_1.into()), old::PartsOf57600(28800)),
					(BrokerCoreAssignment::Task(descriptor_para_2.into()), old::PartsOf57600(28800)),
				],
				end_hint: None,
				next_schedule: None,
			};

			let descriptor = old::CoreDescriptor {
				queue: Some(old::QueueDescriptor { first: block_number, last: block_number }),
				current_work: None,
			};

			old::pallet::CoreSchedules::<Test>::insert((block_number, core), descriptor_schedule);
			old::pallet::CoreDescriptors::<Test>::insert(core, descriptor);

			// Create ClaimQueue with different bulk assignments (these should be dropped)
			let claimqueue_para_1 = ParaId::from(2000);
			let claimqueue_para_2 = ParaId::from(2001);

			let mut claim_queue = BTreeMap::new();
			let mut assignments = VecDeque::new();
			assignments.push_back(Assignment::Bulk(claimqueue_para_1));
			assignments.push_back(Assignment::Bulk(claimqueue_para_2));
			claim_queue.insert(core, assignments);

			v4::ClaimQueue::<Test>::put(claim_queue);

			StorageVersion::new(3).put::<super::Pallet<Test>>();

			// Run migration
			v4::UncheckedMigrateToV4::<Test>::on_runtime_upgrade();

			// Verify ClaimQueue is removed
			assert!(!v4::ClaimQueue::<Test>::exists());

			// Peek at the next block to see what will be scheduled
			// Should see assignments from descriptor (3000, 3001), NOT from ClaimQueue (2000, 2001)
			let peeked = scheduler::assigner_coretime::peek_next_block::<Test>(10);

			let core_assignments = peeked.get(&core).expect("Core should have assignments");
			let para_ids: Vec<ParaId> = core_assignments.iter().copied().collect();

			// Verify we see descriptor paras, not claimqueue paras
			assert!(para_ids.contains(&descriptor_para_1), "Should contain para from descriptor");
			assert!(para_ids.contains(&descriptor_para_2), "Should contain para from descriptor");
			assert!(!para_ids.contains(&claimqueue_para_1), "Should NOT contain para from old ClaimQueue");
			assert!(!para_ids.contains(&claimqueue_para_2), "Should NOT contain para from old ClaimQueue");
		});
	}

	#[test]
	fn empty_storage_migration_works() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			// No old storage created
			StorageVersion::new(3).put::<super::Pallet<Test>>();

			// Run migration on empty storage - should complete without panicking
			let _weight = v4::UncheckedMigrateToV4::<Test>::on_runtime_upgrade();

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
			let old_schedule = old::Schedule {
				assignments: vec![
					(
						BrokerCoreAssignment::Task(para_id.into()),
						old::PartsOf57600(14400), // 1/4
					),
					(
						BrokerCoreAssignment::Task(ParaId::from(1001).into()),
						old::PartsOf57600(28800), // 1/2
					),
					(
						BrokerCoreAssignment::Task(ParaId::from(1002).into()),
						old::PartsOf57600(14400), // 1/4
					),
				],
				end_hint: None,
				next_schedule: None,
			};

			let old_descriptor = old::CoreDescriptor {
				queue: Some(old::QueueDescriptor { first: block_number, last: block_number }),
				current_work: None,
			};

			old::pallet::CoreSchedules::<Test>::insert((block_number, core), old_schedule);
			old::pallet::CoreDescriptors::<Test>::insert(core, old_descriptor);

			StorageVersion::new(3).put::<super::Pallet<Test>>();

			// Run migration
			v4::UncheckedMigrateToV4::<Test>::on_runtime_upgrade();

			// Verify assignments and their parts converted correctly
			let new_schedule = super::CoreSchedules::<Test>::get((block_number, core))
				.expect("Schedule should be migrated");
			assert_eq!(new_schedule.assignments().len(), 3);

			// Check sum of parts equals full allocation (14400 + 28800 + 14400 = 57600)
			let sum: u16 = new_schedule
				.assignments()
				.iter()
				.map(|(_, parts)| parts.value())
				.sum();
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
			let old_descriptor = old::CoreDescriptor {
				queue: None,
				current_work: Some(old::WorkState {
					assignments: vec![(
						BrokerCoreAssignment::Task(para_id.into()),
						old::AssignmentState {
							ratio: old::PartsOf57600(57600),
							remaining: old::PartsOf57600(28800),
						},
					)],
					end_hint: Some(100u32),
					pos: 0,
					step: old::PartsOf57600(1),
				}),
			};

			old::pallet::CoreDescriptors::<Test>::insert(core, old_descriptor);

			StorageVersion::new(3).put::<super::Pallet<Test>>();

			// Run migration
			v4::UncheckedMigrateToV4::<Test>::on_runtime_upgrade();

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
