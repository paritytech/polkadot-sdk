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

	use primitives::{CollatorId, Id};

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
			let n: u32 = v0::Scheduled::<T>::get().len() as u32 +
				v0::AvailabilityCores::<T>::get().iter().filter(|c| c.is_some()).count() as u32;

			log::info!(
				target: crate::scheduler::LOG_TARGET,
				"Number of scheduled and waiting for availability before: {n}",
			);

			Ok(n.encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::DispatchError> {
			log::info!(target: crate::scheduler::LOG_TARGET, "Running post_upgrade()");

			ensure!(
				v0::Scheduled::<T>::get().is_empty(),
				"Scheduled should be empty after the migration"
			);

			let expected_len = u32::decode(&mut &state[..]).unwrap();
			let availability_cores_waiting = super::AvailabilityCores::<T>::get()
				.iter()
				.filter(|c| !matches!(c, CoreOccupied::Free))
				.count();

			ensure!(
				Pallet::<T>::claimqueue_len() as u32 + availability_cores_waiting as u32 ==
					expected_len,
				"ClaimQueue and AvailabilityCores should have the correct length",
			);

			Ok(())
		}
	}
}

pub fn migrate_to_v1<T: crate::scheduler::Config>() -> Weight {
	let mut weight: Weight = Weight::zero();

	v0::ParathreadQueue::<T>::kill();
	v0::ParathreadClaimIndex::<T>::kill();

	let now = <frame_system::Pallet<T>>::block_number();
	let scheduled = v0::Scheduled::<T>::take();
	let sched_len = scheduled.len() as u64;
	for core_assignment in scheduled {
		let core_idx = core_assignment.core;
		let assignment = Assignment::new(core_assignment.para_id);
		let pe = ParasEntry::new(assignment, now);
		Pallet::<T>::add_to_claimqueue(core_idx, pe);
	}

	let parachains = paras::Pallet::<T>::parachains();
	let availability_cores = v0::AvailabilityCores::<T>::take();
	let mut new_availability_cores = Vec::new();

	for (core_index, core) in availability_cores.into_iter().enumerate() {
		let new_core = if let Some(core) = core {
			match core {
				v0::CoreOccupied::Parachain => CoreOccupied::Paras(ParasEntry::new(
					Assignment::new(parachains[core_index]),
					now,
				)),
				v0::CoreOccupied::Parathread(entry) =>
					CoreOccupied::Paras(ParasEntry::new(Assignment::new(entry.claim.0), now)),
			}
		} else {
			CoreOccupied::Free
		};

		new_availability_cores.push(new_core);
	}

	super::AvailabilityCores::<T>::set(new_availability_cores);

	// 2x as once for Scheduled and once for Claimqueue
	weight = weight.saturating_add(T::DbWeight::get().reads_writes(2 * sched_len, 2 * sched_len));
	// reading parachains + availability_cores, writing AvailabilityCores
	weight = weight.saturating_add(T::DbWeight::get().reads_writes(2, 1));
	// 2x kill
	weight = weight.saturating_add(T::DbWeight::get().writes(2));

	weight
}
