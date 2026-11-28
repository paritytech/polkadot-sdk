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

//! Running `pallet-broker` on JAM.
//!
//! There is no relay chain on JAM: the Coretime chain decides how the cores are used, and a core
//! assignment reaches JAM as an `UpwardMessage::AssignCore` that the Parachain Service applies.
//! [`JamCoretime`] is the broker's [`CoretimeInterface`] that sends these messages.
//!
//! `AssignCore` queues authorizer hashes instead of tasks, so every task needs its hash registered
//! in [`AuthorizerHashes`]. This pallet sits next to `pallet-broker` in the runtime and keeps those
//! hashes: it registers them, schedules them for a JAM slot and activates them once the slot is
//! reached, and it sends the assignment of every core the Parachain Service rejected again.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub use pallet::*;

use alloc::vec::Vec;
use core::marker::PhantomData;
use frame_support::{traits::Get, weights::Weight};
use pallet_broker::{
	BalanceOf, CoreAssignment, CoreCountInbox, CoreIndex, CoretimeInterface, OnDemandRevenueRecord,
	PartsOf57600, RCBlockNumberOf, RCBlockNumberProviderOf, RevenueInbox, TaskId, Workload,
	CORE_MASK_BITS,
};
use parachain_service_core::{types::MessageId, upward_message::UpwardMessage};
use sp_runtime::traits::{BlockNumberProvider, SaturatedConversion, Zero};

const LOG_TARGET: &str = "runtime::jam-coretime";

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config:
		frame_system::Config<RuntimeEvent: From<Event<Self>>> + pallet_broker::Config
	{
		/// The cores whose last assignment the Parachain Service reported as rejected.
		///
		/// Every core named here that still has a workload gets its assignment sent again.
		type RejectedCores: Get<Vec<CoreIndex>>;
	}

	/// The authorizer hash of every task, which a core assignment queues for it on JAM.
	#[pallet::storage]
	pub type AuthorizerHashes<T> = StorageMap<_, Twox64Concat, TaskId, [u8; 32], OptionQuery>;

	/// The authorizer hash of every task that is scheduled for a JAM slot.
	///
	/// The hash becomes the task's current one in `AuthorizerHashes` once the JAM slot is reached.
	#[pallet::storage]
	pub type ScheduledAuthorizers<T> =
		StorageMap<_, Twox64Concat, TaskId, (u32, [u8; 32]), OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// The authorizer hash of a task was set or removed.
		AuthorizerRegistered {
			/// The task whose authorizer hash changed.
			task: TaskId,
			/// The new authorizer hash, `None` if it was removed.
			authorizer: Option<[u8; 32]>,
		},
		/// The assignment of a core the Parachain Service rejected was sent again.
		AssignmentRetried {
			/// The core whose assignment was sent again.
			core: CoreIndex,
		},
		/// The authorizer hash of a task was scheduled for a JAM slot.
		AuthorizerScheduled {
			/// The task whose authorizer hash was scheduled.
			task: TaskId,
			/// The JAM slot at which the authorizer hash takes effect.
			from_slot: u32,
			/// The scheduled authorizer hash.
			authorizer: [u8; 32],
		},
		/// A scheduled authorizer hash became the current one of a task.
		AuthorizerActivated {
			/// The task whose authorizer hash changed.
			task: TaskId,
			/// The new current authorizer hash.
			authorizer: [u8; 32],
		},
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Independent of the broker's own tick in the same block: an assignment the tick sends
		/// already uses a scheduled hash whose JAM slot was reached, and a retried core is sent its
		/// current workload, so either order leaves the Parachain Service with the same
		/// assignments.
		fn on_initialize(_now: BlockNumberFor<T>) -> Weight {
			Self::process_scheduled_authorizers().saturating_add(Self::retry_rejected_assignments())
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Set or remove the authorizer hash of a task.
		///
		/// A core assigned to a task without an authorizer hash is not sent to JAM.
		///
		/// - `origin`: Must be Root or pass the broker's `AdminOrigin`.
		/// - `task`: The task whose authorizer hash is set.
		/// - `authorizer`: The authorizer hash, `None` to remove it.
		#[pallet::call_index(0)]
		#[pallet::weight(T::DbWeight::get().writes(1))]
		pub fn register_authorizer(
			origin: OriginFor<T>,
			task: TaskId,
			authorizer: Option<[u8; 32]>,
		) -> DispatchResult {
			<T as pallet_broker::Config>::AdminOrigin::ensure_origin_or_root(origin)?;
			AuthorizerHashes::<T>::set(task, authorizer);
			Self::deposit_event(Event::<T>::AuthorizerRegistered { task, authorizer });
			Ok(())
		}

		/// Schedule the authorizer hash of a task for a JAM slot.
		///
		/// The hash becomes the task's current one once the JAM slot is reached, and the cores
		/// currently assigned to the task get an assignment taking effect at that slot.
		///
		/// - `origin`: Must be Root or pass the broker's `AdminOrigin`.
		/// - `task`: The task whose authorizer hash is scheduled.
		/// - `from_slot`: The JAM slot at which the authorizer hash takes effect.
		/// - `authorizer`: The authorizer hash.
		#[pallet::call_index(1)]
		#[pallet::weight({
			// Manual estimate: the scheduled entry plus the workload of every core.
			T::DbWeight::get().reads_writes(1_024, 2)
		})]
		pub fn schedule_authorizer(
			origin: OriginFor<T>,
			task: TaskId,
			from_slot: u32,
			authorizer: [u8; 32],
		) -> DispatchResult {
			<T as pallet_broker::Config>::AdminOrigin::ensure_origin_or_root(origin)?;
			Self::do_schedule_authorizer(task, from_slot, authorizer);
			Ok(())
		}
	}
}

/// The [`CoretimeInterface`] of a Coretime chain running on JAM.
///
/// `SlotProvider` is the JAM timeslot, which stands in for the relay chain block number.
pub struct JamCoretime<T, SlotProvider>(PhantomData<(T, SlotProvider)>);

impl<T, S> CoretimeInterface for JamCoretime<T, S>
where
	T: Config + pallet_broker::Config<Coretime = Self>,
	S: BlockNumberProvider + 'static,
{
	type AccountId = T::AccountId;
	type Balance = BalanceOf<T>;
	type RelayChainBlockNumberProvider = S;

	/// The number of cores is fixed by JAM and this chain decides how they are used, so the
	/// request is granted right away.
	fn request_core_count(count: CoreIndex) {
		CoreCountInbox::<T>::put(count);
	}

	/// There is no instantaneous coretime market on JAM, so there is never any revenue.
	fn request_revenue_info_at(when: RCBlockNumberOf<Self>) {
		RevenueInbox::<T>::put(OnDemandRevenueRecord { until: when, amount: Zero::zero() });
	}

	/// There is no instantaneous coretime market on JAM to credit.
	fn credit_account(_who: Self::AccountId, _amount: Self::Balance) {}

	/// JAM applies the assignment as soon as the Parachain Service accumulates it, so `begin` and
	/// `end_hint` have nothing to act on.
	fn assign_core(
		core: CoreIndex,
		_begin: RCBlockNumberOf<Self>,
		assignment: Vec<(CoreAssignment, PartsOf57600)>,
		_end_hint: Option<RCBlockNumberOf<Self>>,
	) {
		Pallet::<T>::send_assign_core(core, &assignment);
	}
}

/// The authorizer queue of a core running `assignment`.
///
/// Every task's authorizer hash is queued once for every 1/80th of the core it owns. The Parachain
/// Service cycles through the queue, so the tasks share the core in these proportions. Pool and
/// idle parts have no authorizer and are left out, as are tasks without an authorizer hash.
fn authorizer_queue(
	assignment: &[(CoreAssignment, PartsOf57600)],
	authorizer: impl Fn(TaskId) -> Option<[u8; 32]>,
) -> Vec<[u8; 32]> {
	let mut queue = Vec::new();
	for (task, parts) in assignment {
		let CoreAssignment::Task(task) = task else { continue };
		let Some(hash) = authorizer(*task) else {
			log::warn!(
				target: LOG_TARGET,
				"Task {task} has no authorizer hash, its part of the core is not assigned on JAM"
			);
			continue;
		};
		let entries = (*parts as usize * CORE_MASK_BITS / 57_600).max(1);
		queue.extend(core::iter::repeat(hash).take(entries));
	}
	queue
}

impl<T: Config> Pallet<T> {
	/// The JAM slot the broker's relay chain block number provider is at.
	fn jam_slot() -> u32 {
		RCBlockNumberProviderOf::<T::Coretime>::current_block_number().saturated_into()
	}

	/// The authorizer hash JAM uses for `task` from `slot` on.
	fn authorizer_at(task: TaskId, slot: u32) -> Option<[u8; 32]> {
		match ScheduledAuthorizers::<T>::get(task) {
			Some((from_slot, hash)) if from_slot <= slot => Some(hash),
			_ => AuthorizerHashes::<T>::get(task),
		}
	}

	/// The earliest JAM slot at which an authorizer hash of a task in `assignment` changes.
	fn pending_authorizer_switch(
		assignment: &[(CoreAssignment, PartsOf57600)],
		now: u32,
	) -> Option<u32> {
		assignment
			.iter()
			.filter_map(|(assignment, _)| match assignment {
				CoreAssignment::Task(task) => ScheduledAuthorizers::<T>::get(task)
					.map(|(from_slot, _)| from_slot)
					.filter(|from_slot| *from_slot > now),
				_ => None,
			})
			.min()
	}

	/// Send the assignment of `core` to the Parachain Service.
	pub(crate) fn send_assign_core(core: CoreIndex, assignment: &[(CoreAssignment, PartsOf57600)]) {
		let now = Self::jam_slot();
		Self::send_assign_core_at(core, assignment, now, |task| Self::authorizer_at(task, now));

		// The Parachain Service keeps one pending assignment per core and an inline assignment
		// replaces it, so send the scheduled one again right after.
		if let Some(from_slot) = Self::pending_authorizer_switch(assignment, now) {
			Self::send_assign_core_at(core, assignment, from_slot, |task| {
				Self::authorizer_at(task, from_slot)
			});
		}
	}

	/// Send the assignment of `core` to the Parachain Service, taking effect at `jam_slot`.
	fn send_assign_core_at(
		core: CoreIndex,
		assignment: &[(CoreAssignment, PartsOf57600)],
		jam_slot: u32,
		authorizer: impl Fn(TaskId) -> Option<[u8; 32]>,
	) {
		let queue = authorizer_queue(assignment, authorizer);
		if queue.is_empty() {
			log::warn!(
				target: LOG_TARGET,
				"No task of core {core} has an authorizer hash, nothing is sent to JAM"
			);
			return;
		}
		let message = UpwardMessage::AssignCore {
			core,
			queue,
			// The Parachain Service stays the assigner of the core.
			new_assigner: None,
			jam_slot,
		};
		// The service reports a rejected assignment as `CoreNotAssignable`, which names no core,
		// under the message id it was sent with, so the id is the core it assigns.
		let message_id = MessageId::from(u64::from(core));
		#[cfg(not(test))]
		cumulus_jam_io::send_upward_message(message_id, &message);
		#[cfg(test)]
		mock::record_upward_message(message_id, message);
	}

	/// Schedule the authorizer hash of `task` for the JAM slot `from_slot`.
	fn do_schedule_authorizer(task: TaskId, from_slot: u32, authorizer: [u8; 32]) {
		ScheduledAuthorizers::<T>::insert(task, (from_slot, authorizer));
		Self::deposit_event(Event::<T>::AuthorizerScheduled { task, from_slot, authorizer });

		// Every core currently running the task gets an assignment taking effect at `from_slot`.
		for (core, workload) in Workload::<T>::iter() {
			if !workload.iter().any(|item| item.assignment == CoreAssignment::Task(task)) {
				continue;
			}
			Self::send_assign_core_at(
				core,
				&pallet_broker::Pallet::<T>::schedule_to_assignment(workload),
				from_slot,
				|task| Self::authorizer_at(task, from_slot),
			);
		}
	}

	/// Make every scheduled authorizer hash whose JAM slot was reached the current one.
	fn process_scheduled_authorizers() -> Weight {
		let now = Self::jam_slot();
		let mut due = Vec::new();
		let mut scanned = 0u64;
		for (task, (from_slot, hash)) in ScheduledAuthorizers::<T>::iter() {
			scanned = scanned.saturating_add(1);
			if from_slot <= now {
				due.push((task, hash));
			}
		}
		for (task, authorizer) in &due {
			AuthorizerHashes::<T>::insert(task, authorizer);
			ScheduledAuthorizers::<T>::remove(task);
			Self::deposit_event(Event::<T>::AuthorizerActivated {
				task: *task,
				authorizer: *authorizer,
			});
		}
		T::DbWeight::get().reads_writes(1 + scanned, 2 * due.len() as u64)
	}

	/// Send the assignment of every core the Parachain Service rejected again.
	fn retry_rejected_assignments() -> Weight {
		// Reading the rejections is a JAM state read, so skip it while no core was ever assigned.
		if Workload::<T>::iter_keys().next().is_none() {
			return T::DbWeight::get().reads(1);
		}
		let mut retried = Vec::new();
		for core in T::RejectedCores::get() {
			if retried.contains(&core) {
				continue;
			}
			let workload = Workload::<T>::get(core);
			if workload.is_empty() {
				continue;
			}
			Self::send_assign_core(
				core,
				&pallet_broker::Pallet::<T>::schedule_to_assignment(workload),
			);
			Self::deposit_event(Event::<T>::AssignmentRetried { core });
			retried.push(core);
		}
		T::DbWeight::get().reads(1 + retried.len() as u64)
	}
}
