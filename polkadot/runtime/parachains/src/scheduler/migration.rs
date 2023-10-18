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
	traits::OnRuntimeUpgrade, weights::Weight,
};

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
	use super::*;
	use crate::scheduler;

	#[allow(deprecated)]
	pub type MigrateToV1<T> = VersionedMigration<
		0,
		1,
		UncheckedMigrateToV1<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;

	#[deprecated(note = "Use MigrateToV1 instead")]
	pub struct UncheckedMigrateToV1<T>(sp_std::marker::PhantomData<T>);
	#[allow(deprecated)]
	impl<T: Config> OnRuntimeUpgrade for UncheckedMigrateToV1<T> {
		fn on_runtime_upgrade() -> Weight {
			let weight_consumed = migrate_to_v1::<T>();

			log::info!(target: scheduler::LOG_TARGET, "Migrating para scheduler storage to v1");

			weight_consumed
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
		let assignment = Assignment::new(core_assignment.para_id);
		let pe = ParasEntry::new(assignment, now);
		Pallet::<T>::add_to_claimqueue(core_idx, pe);
	}

	// 2x as once for Scheduled and once for Claimqueue
	weight = weight.saturating_add(T::DbWeight::get().reads_writes(2 * sched_len, 2 * sched_len));
	weight = weight.saturating_add(T::DbWeight::get().reads_writes(pq_len, pq_len));
	weight = weight.saturating_add(T::DbWeight::get().reads_writes(pci_len, pci_len));

	weight
}
