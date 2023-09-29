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

use super::{common::V0Assignment, *};
use frame_support::{
	pallet_prelude::ValueQuery, storage_alias, traits::OnRuntimeUpgrade, weights::Weight,
};

use sp_std::convert::identity;

/// Migration for potential changes in `Assignment` representation.
pub mod assignment_version {
	use super::*;
	use crate::scheduler::{self, common::AssignmentVersion};

	pub struct MigrateAssignment<T>(sp_std::marker::PhantomData<T>);

	/// Previously used `ParasEntryType`.
	pub type OldParasEntryType<T> = ParasEntry<BlockNumberFor<T>, OldAssignmentType<T>>;

	/// Previously used assignment type:
	pub(crate) type OldAssignmentType<T> = <<T as Config>::AssignmentProvider as AssignmentProvider<
		BlockNumberFor<T>,
	>>::OldAssignmentType;

	/// ClaimQueue using old assignments.
	#[storage_alias]
	pub(crate) type ClaimQueue<T: Config> =
		StorageValue<Pallet<T>, BTreeMap<CoreIndex, VecDeque<OldParasEntryType<T>>>, ValueQuery>;

	impl<T: Config> OnRuntimeUpgrade for MigrateAssignment<T> {
		fn on_runtime_upgrade() -> Weight {
			let assignment_version = <T::AssignmentProvider as AssignmentProvider<
				BlockNumberFor<T>,
			>>::ASSIGNMENT_STORAGE_VERSION;
			// Is a migration necessary?
			if AssignmentVersion::get::<Pallet<T>>() < assignment_version {
				let mut weight = migrate_assignments::<T>();

				log::info!(target: scheduler::LOG_TARGET, "Migrating para scheduler assginments to {:?}", assignment_version);
				assignment_version.put::<Pallet<T>>();

				weight += T::DbWeight::get().reads_writes(1, 1);
				weight
			} else {
				log::trace!(target: scheduler::LOG_TARGET, "Assignments still up2date.");
				T::DbWeight::get().reads(1)
			}
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::DispatchError> {
			log::trace!(
				target: crate::scheduler::LOG_TARGET,
				"ClaimQueue before migration: {}",
				ClaimQueue::<T>::get().len()
			);

			let bytes = u32::to_be_bytes(v1::ClaimQueue::<T>::get().len() as u32);

			Ok(bytes.to_vec())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::DispatchError> {
			let assignment_version = <T::AssignmentProvider as AssignmentProvider<
				BlockNumberFor<T>,
			>>::ASSIGNMENT_STORAGE_VERSION;
			log::trace!(target: crate::scheduler::LOG_TARGET, "Running post_upgrade()");
			ensure!(
				AssignmentVersion::get::<Pallet<T>>() == assignment_version,
				"Assignment version should should match current version after the migration"
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

mod v0 {

	use super::*;

	use primitives::CollatorId;
	#[storage_alias]
	pub(super) type Scheduled<T: Config> = StorageValue<Pallet<T>, Vec<CoreAssignment>, ValueQuery>;

	#[derive(Encode, Decode)]
	pub struct QueuedParathread {
		claim: primitives::ParathreadEntry,
		core_offset: u32,
	}

	#[derive(Encode, Decode, Default)]
	pub struct ParathreadClaimQueue {
		queue: Vec<QueuedParathread>,
		next_core_offset: u32,
	}

	// Only here to facilitate the migration.
	impl ParathreadClaimQueue {
		pub fn len(self) -> usize {
			self.queue.len()
		}
	}

	#[storage_alias]
	pub(super) type ParathreadQueue<T: Config> =
		StorageValue<Pallet<T>, ParathreadClaimQueue, ValueQuery>;

	#[storage_alias]
	pub(super) type ParathreadClaimIndex<T: Config> =
		StorageValue<Pallet<T>, Vec<ParaId>, ValueQuery>;

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

pub mod v1 {
	use frame_support::{
		pallet_prelude::ValueQuery, storage_alias, traits::OnRuntimeUpgrade, weights::Weight,
	};
	use frame_system::pallet_prelude::BlockNumberFor;

	use super::*;

	#[storage_alias]
	pub(crate) type ClaimQueue<T: Config> = StorageValue<
		Pallet<T>,
		BTreeMap<CoreIndex, VecDeque<Option<ParasEntry<BlockNumberFor<T>>>>>,
		ValueQuery,
	>;

	#[derive(Encode, Decode, TypeInfo, RuntimeDebug)]
	pub struct ParasEntry<N> {
		/// The underlying `Assignment`
		pub assignment: V0Assignment,
		/// The number of times the entry has timed out in availability already.
		pub availability_timeouts: u32,
		/// The block height until this entry needs to be backed.
		///
		/// If missed the entry will be removed from the claim queue without ever having occupied
		/// the core.
		pub ttl: N,
	}

	impl<N> ParasEntry<N> {
		/// Create a new `ParasEntry`.
		pub fn new(assignment: V0Assignment, now: N) -> Self {
			ParasEntry { assignment, availability_timeouts: 0, ttl: now }
		}

		/// Return `Id` from the underlying `Assignment`.
		pub fn para_id(&self) -> ParaId {
			self.assignment.para_id()
		}
	}

	pub fn add_to_claimqueue<T: Config>(core_idx: CoreIndex, pe: ParasEntry<BlockNumberFor<T>>) {
		ClaimQueue::<T>::mutate(|la| {
			let la_deque = la.entry(core_idx).or_insert_with(|| VecDeque::new());
			la_deque.push_back(Some(pe));
		});
	}

	pub struct MigrateToV1<T>(sp_std::marker::PhantomData<T>);
	impl<T: Config> OnRuntimeUpgrade for MigrateToV1<T> {
		fn on_runtime_upgrade() -> Weight {
			if StorageVersion::get::<Pallet<T>>() == 0 {
				let weight_consumed = migrate_to_v1::<T>();

				log::info!(target: crate::scheduler::LOG_TARGET, "Migrating para scheduler storage to v1");
				StorageVersion::new(1).put::<Pallet<T>>();

				weight_consumed
			} else {
				log::warn!(target: crate::scheduler::LOG_TARGET, "Para scheduler v1 migration should be removed.");
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

			let bytes = u32::to_be_bytes(v0::Scheduled::<T>::get().len() as u32);

			Ok(bytes.to_vec())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::DispatchError> {
			log::trace!(target: crate::scheduler::LOG_TARGET, "Running post_upgrade()");
			ensure!(
				StorageVersion::get::<Pallet<T>>() >= 1,
				"Storage version should be at least `1` after the migration"
			);
			ensure!(
				v0::Scheduled::<T>::get().len() == 0,
				"Scheduled should be empty after the migration"
			);

			let sched_len = u32::from_be_bytes(state.try_into().unwrap());
			ensure!(
				Pallet::<T>::claimqueue_len() as u32 == sched_len,
				"Scheduled completely moved to ClaimQueue after migration"
			);

			Ok(())
		}
	}
}

pub mod v2 {
	use super::*;
	use crate::scheduler;
	use frame_support::traits::StorageVersion;

	// V2 (no Option wrapper), but still old Assignment format.
	//
	// Assignment migrations are handled separately. Think of it as a minor version.
	#[storage_alias]
	pub(crate) type ClaimQueue<T: Config> = StorageValue<
		Pallet<T>,
		BTreeMap<CoreIndex, VecDeque<super::v1::ParasEntry<BlockNumberFor<T>>>>,
		ValueQuery,
	>;

	pub struct MigrateToV2<T>(sp_std::marker::PhantomData<T>);

	impl<T: Config> OnRuntimeUpgrade for MigrateToV2<T> {
		fn on_runtime_upgrade() -> Weight {
			if StorageVersion::get::<Pallet<T>>() == 1 {
				let mut weight_consumed = migrate_to_v2::<T>();

				log::info!(target: scheduler::LOG_TARGET, "Migrating para scheduler storage to v2");
				StorageVersion::new(2).put::<Pallet<T>>();

				weight_consumed += T::DbWeight::get().reads_writes(1, 1);
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
				"ClaimQueue before migration: {}",
				v1::ClaimQueue::<T>::get().len()
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

			let old_len = u32::from_be_bytes(state.try_into().unwrap());
			ensure!(
				Pallet::<T>::claimqueue_len() as u32 == old_len,
				"Old ClaimQueue completely moved to new ClaimQueue after migration"
			);

			Ok(())
		}
	}
}

pub fn migrate_assignments<T: crate::scheduler::Config>() -> Weight {
	use assignment_version::ClaimQueue as OldClaimQueue;

	let mut weight: Weight = Weight::zero();

	let old = OldClaimQueue::<T>::take();
	let old_len = old.len() as u64;

	let new = old
		.into_iter()
		.map(|(core, v)| {
			(
				core,
				v.into_iter()
					.map(|old| migrate_assignment_paras_entry::<T>(core, old))
					.collect::<VecDeque<_>>(),
			)
		})
		.collect::<BTreeMap<CoreIndex, VecDeque<ParasEntryType<T>>>>();
	ClaimQueue::<T>::put(new);

	weight = weight.saturating_add(T::DbWeight::get().reads_writes(2 * old_len, 2 * old_len));

	weight
}

// Migrate to v2, but still with old assignment format.
pub fn migrate_to_v2<T: crate::scheduler::Config>() -> Weight {
	let mut weight: Weight = Weight::zero();

	let old = v1::ClaimQueue::<T>::take();
	let old_len = old.len() as u64;
	let new = old
		.into_iter()
		.map(|(k, v)| (k, v.into_iter().filter_map(identity).collect::<VecDeque<_>>()))
		.collect::<BTreeMap<CoreIndex, VecDeque<v1::ParasEntry<BlockNumberFor<T>>>>>();
	v2::ClaimQueue::<T>::put(new);

	weight = weight.saturating_add(T::DbWeight::get().reads_writes(2 * old_len, 2 * old_len));

	weight
}

pub fn migrate_to_v1<T: crate::scheduler::Config>() -> Weight {
	let mut weight: Weight = Weight::zero();

	let pq = v0::ParathreadQueue::<T>::take();
	let pq_len = pq.len() as u64;

	let pci = v0::ParathreadClaimIndex::<T>::take();
	let pci_len = pci.len() as u64;

	let now = <frame_system::Pallet<T>>::block_number();
	let scheduled = v0::Scheduled::<T>::take();
	let sched_len = scheduled.len() as u64;
	for core_assignment in scheduled {
		let core_idx = core_assignment.core;
		let assignment = V0Assignment::new(core_assignment.para_id);
		let pe = v1::ParasEntry::new(assignment, now);
		v1::add_to_claimqueue::<T>(core_idx, pe);
	}

	// 2x as once for Scheduled and once for Claimqueue
	weight = weight.saturating_add(T::DbWeight::get().reads_writes(2 * sched_len, 2 * sched_len));
	weight = weight.saturating_add(T::DbWeight::get().reads_writes(pq_len, pq_len));
	weight = weight.saturating_add(T::DbWeight::get().reads_writes(pci_len, pci_len));

	weight
}

fn migrate_assignment_paras_entry<T: crate::scheduler::Config>(
	core: CoreIndex,
	old: assignment_version::OldParasEntryType<T>,
) -> ParasEntryType<T> {
	let ParasEntry { assignment, availability_timeouts, ttl } = old;

	ParasEntry {
		assignment:
			<T::AssignmentProvider as AssignmentProvider<BlockNumberFor<T>>>::migrate_old_to_current(
				assignment, core,
			),
		availability_timeouts,
		ttl,
	}
}

// TODO: Tests!
