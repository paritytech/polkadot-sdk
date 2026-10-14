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

//! Running the broker on JAM.
//!
//! There is no relay chain on JAM: the Coretime chain decides how the cores are used, and a core
//! assignment reaches JAM as an `UpwardMessage::AssignCore` that the Parachain Service applies.
//! `AssignCore` queues authorizer hashes instead of tasks, so every task needs its hash registered
//! in [`AuthorizerHashes`].

use crate::*;
use alloc::vec::Vec;
use codec::Encode;
use core::marker::PhantomData;
use frame_support::{traits::Get, weights::Weight};
use parachain_service_core::upward_message::UpwardMessage;
use sp_runtime::traits::{BlockNumberProvider, SaturatedConversion, Zero};

/// The [`CoretimeInterface`] of a Coretime chain running on JAM.
///
/// `SlotProvider` is the JAM timeslot, which stands in for the relay chain block number.
pub struct JamCoretime<T, SlotProvider>(PhantomData<(T, SlotProvider)>);

impl<T, S> CoretimeInterface for JamCoretime<T, S>
where
	T: Config<Coretime = Self>,
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
pub fn authorizer_queue(
	assignment: &[(CoreAssignment, PartsOf57600)],
	authorizer: impl Fn(TaskId) -> Option<[u8; 32]>,
) -> Vec<[u8; 32]> {
	let mut queue = Vec::new();
	for (task, parts) in assignment {
		let CoreAssignment::Task(task) = task else { continue };
		let Some(hash) = authorizer(*task) else {
			log::warn!(
				target: crate::LOG_TARGET,
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
	/// Send the assignment of `core` to the Parachain Service.
	pub(crate) fn send_assign_core(core: CoreIndex, assignment: &[(CoreAssignment, PartsOf57600)]) {
		let queue = authorizer_queue(assignment, AuthorizerHashes::<T>::get);
		if queue.is_empty() {
			log::warn!(
				target: crate::LOG_TARGET,
				"No task of core {core} has an authorizer hash, nothing is sent to JAM"
			);
			return;
		}
		let message = UpwardMessage::AssignCore {
			core,
			queue,
			// The Parachain Service stays the assigner of the core.
			new_assigner: None,
			jam_slot: RCBlockNumberProviderOf::<T::Coretime>::current_block_number()
				.saturated_into(),
		};
		cumulus_jam_state_reader::jam_upward::send_upward_message(&message.encode());
	}

	/// Send the assignment of every core the Parachain Service rejected again.
	pub(crate) fn retry_rejected_assignments() -> Weight {
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
			Self::send_assign_core(core, &Self::schedule_to_assignment(workload));
			Self::deposit_event(Event::<T>::AssignmentRetried { core });
			retried.push(core);
		}
		T::DbWeight::get().reads(1 + retried.len() as u64)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::*;
	use frame_support::{assert_noop, assert_ok};
	use sp_runtime::DispatchError;

	const A: [u8; 32] = [1; 32];
	const B: [u8; 32] = [2; 32];

	fn registered(task: TaskId) -> Option<[u8; 32]> {
		match task {
			1 => Some(A),
			2 => Some(B),
			_ => None,
		}
	}

	#[test]
	fn a_full_core_queues_its_task_80_times_works() {
		assert_eq!(authorizer_queue(&[(CoreAssignment::Task(1), 57_600)], registered), vec![A; 80]);
	}

	#[test]
	fn a_shared_core_queues_tasks_by_their_parts_works() {
		let queue = authorizer_queue(
			&[(CoreAssignment::Task(1), 14_400), (CoreAssignment::Task(2), 43_200)],
			registered,
		);
		assert_eq!(queue, [vec![A; 20], vec![B; 60]].concat());
	}

	#[test]
	fn pool_idle_and_unregistered_parts_are_left_out_works() {
		let queue = authorizer_queue(
			&[
				(CoreAssignment::Idle, 720),
				(CoreAssignment::Pool, 720),
				(CoreAssignment::Task(3), 720),
				(CoreAssignment::Task(2), 720),
			],
			registered,
		);
		assert_eq!(queue, vec![B]);
	}

	#[test]
	fn register_authorizer_works() {
		TestExt::new().execute_with(|| {
			assert_ok!(Broker::register_authorizer(RuntimeOrigin::root(), 1, Some(A)));
			assert_eq!(AuthorizerHashes::<Test>::get(1), Some(A));
			assert_ok!(Broker::register_authorizer(RuntimeOrigin::root(), 1, None));
			assert_eq!(AuthorizerHashes::<Test>::get(1), None);
		});
	}

	#[test]
	fn register_authorizer_needs_admin_origin_errors() {
		TestExt::new().execute_with(|| {
			assert_noop!(
				Broker::register_authorizer(RuntimeOrigin::signed(2), 1, Some(A)),
				DispatchError::BadOrigin
			);
		});
	}
}
