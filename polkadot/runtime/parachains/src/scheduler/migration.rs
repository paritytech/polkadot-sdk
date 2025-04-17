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

			log::info!(target: scheduler::LOG_TARGET, "Migrated para scheduler storage to v1");

			weight
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

			log::info!(target: scheduler::LOG_TARGET, "Migrating para scheduler storage to v2");

			weight
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

/// Migration for TTL and availability timeout retries removal.
/// AvailabilityCores storage is removed and ClaimQueue now holds `Assignment`s instead of
/// `ParasEntryType`
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

			log::info!(target: scheduler::LOG_TARGET, "Migrating para scheduler storage to v3");

			weight
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::DispatchError> {
			log::trace!(
				target: crate::scheduler::LOG_TARGET,
				"ClaimQueue before migration: {}",
				v2::ClaimQueue::<T>::get().len()
			);

			let bytes = u32::to_be_bytes(v2::ClaimQueue::<T>::get().len() as u32);

			Ok(bytes.to_vec())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::DispatchError> {
			log::trace!(target: crate::scheduler::LOG_TARGET, "Running post_upgrade()");

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
