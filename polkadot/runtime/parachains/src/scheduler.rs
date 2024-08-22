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

//! The scheduler module for parachains and parathreads.
//!
//! This module is responsible for two main tasks:
//!   - Partitioning validators into groups and assigning groups to parachains and parathreads
//!   - Scheduling parachains and parathreads
//!
//! It aims to achieve these tasks with these goals in mind:
//! - It should be possible to know at least a block ahead-of-time, ideally more, which validators
//!   are going to be assigned to which parachains.
//! - Parachains that have a candidate pending availability in this fork of the chain should not be
//!   assigned.
//! - Validator assignments should not be gameable. Malicious cartels should not be able to
//!   manipulate the scheduler to assign themselves as desired.
//! - High or close to optimal throughput of parachains and parathreads. Work among validator groups
//!   should be balanced.
//!
//! The Scheduler manages resource allocation using the concept of "Availability Cores".
//! There will be one availability core for each parachain, and a fixed number of cores
//! used for multiplexing parathreads. Validators will be partitioned into groups, with the same
//! number of groups as availability cores. Validator groups will be assigned to different
//! availability cores over time.

use core::iter::Peekable;

use crate::{configuration, initializer::SessionChangeNotification, paras};
use alloc::{
	collections::{
		btree_map::{self, BTreeMap},
		btree_set::BTreeSet,
		vec_deque::VecDeque,
	},
	vec::Vec,
};
use frame_support::{pallet_prelude::*, traits::Defensive};
use frame_system::pallet_prelude::BlockNumberFor;
pub use polkadot_core_primitives::v2::BlockNumber;
use polkadot_primitives::{
	CoreIndex, GroupIndex, GroupRotationInfo, Id as ParaId, ScheduledCore, ValidatorIndex,
};
use sp_runtime::traits::One;

pub mod common;

use common::{Assignment, AssignmentProvider};

pub use pallet::*;

#[cfg(test)]
mod tests;

const LOG_TARGET: &str = "runtime::parachains::scheduler";

pub mod migration;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(2);

	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + configuration::Config + paras::Config {
		type AssignmentProvider: AssignmentProvider<BlockNumberFor<Self>>;
	}

	/// All the validator groups. One for each core. Indices are into `ActiveValidators` - not the
	/// broader set of Polkadot validators, but instead just the subset used for parachains during
	/// this session.
	///
	/// Bound: The number of cores is the sum of the numbers of parachains and parathread
	/// multiplexers. Reasonably, 100-1000. The dominant factor is the number of validators: safe
	/// upper bound at 10k.
	#[pallet::storage]
	pub type ValidatorGroups<T> = StorageValue<_, Vec<Vec<ValidatorIndex>>, ValueQuery>;

	/// The block number where the session start occurred. Used to track how many group rotations
	/// have occurred.
	///
	/// Note that in the context of parachains modules the session change is signaled during
	/// the block and enacted at the end of the block (at the finalization stage, to be exact).
	/// Thus for all intents and purposes the effect of the session change is observed at the
	/// block following the session change, block number of which we save in this storage value.
	#[pallet::storage]
	pub type SessionStartBlock<T: Config> = StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	/// One entry for each availability core. The `VecDeque` represents the assignments to be
	/// scheduled on that core. The value contained here will not be valid after the end of
	/// a block. Runtime APIs should be used to determine scheduled cores for the upcoming block.
	#[pallet::storage]
	pub type ClaimQueue<T> = StorageValue<_, BTreeMap<CoreIndex, VecDeque<Assignment>>, ValueQuery>;

	/// Availability timeout status of a core.
	pub(crate) struct AvailabilityTimeoutStatus<BlockNumber> {
		/// Is the core already timed out?
		///
		/// If this is true the core will be freed at this block.
		pub timed_out: bool,

		/// When does this core timeout.
		///
		/// The block number the core times out. If `timed_out` is true, this will correspond to
		/// now (current block number).
		pub live_until: BlockNumber,
	}
}

type PositionInClaimQueue = u32;

struct ClaimQueueIterator<E> {
	next_idx: u32,
	queue: Peekable<btree_map::IntoIter<CoreIndex, VecDeque<E>>>,
}

impl<E> Iterator for ClaimQueueIterator<E> {
	type Item = (CoreIndex, VecDeque<E>);

	fn next(&mut self) -> Option<Self::Item> {
		let (idx, _) = self.queue.peek()?;
		let val = if idx != &CoreIndex(self.next_idx) {
			log::trace!(target: LOG_TARGET, "idx did not match claim queue idx: {:?} vs {:?}", idx, self.next_idx);
			(CoreIndex(self.next_idx), VecDeque::new())
		} else {
			let (idx, q) = self.queue.next()?;
			(idx, q)
		};
		self.next_idx += 1;
		Some(val)
	}
}

impl<T: Config> Pallet<T> {
	/// Called by the initializer to initialize the scheduler pallet.
	pub(crate) fn initializer_initialize(_now: BlockNumberFor<T>) -> Weight {
		Weight::zero()
	}

	/// Called by the initializer to finalize the scheduler pallet.
	pub(crate) fn initializer_finalize() {}

	/// Called before the initializer notifies of a new session.
	pub(crate) fn pre_new_session() {
		Self::push_claim_queue_items_to_assignment_provider();
		// Self::push_occupied_cores_to_assignment_provider();
	}

	/// Called by the initializer to note that a new session has started.
	pub(crate) fn initializer_on_new_session(
		notification: &SessionChangeNotification<BlockNumberFor<T>>,
	) {
		let SessionChangeNotification { validators, new_config, .. } = notification;
		let config = new_config;

		let n_cores = core::cmp::max(
			T::AssignmentProvider::session_core_count(),
			match config.scheduler_params.max_validators_per_core {
				Some(x) if x != 0 => validators.len() as u32 / x,
				_ => 0,
			},
		);

		// shuffle validators into groups.
		if n_cores == 0 || validators.is_empty() {
			ValidatorGroups::<T>::set(Vec::new());
		} else {
			let group_base_size = validators
				.len()
				.checked_div(n_cores as usize)
				.defensive_proof("n_cores should not be 0")
				.unwrap_or(0);
			let n_larger_groups = validators
				.len()
				.checked_rem(n_cores as usize)
				.defensive_proof("n_cores should not be 0")
				.unwrap_or(0);

			// Groups contain indices into the validators from the session change notification,
			// which are already shuffled.

			let mut groups: Vec<Vec<ValidatorIndex>> = Vec::new();
			for i in 0..n_larger_groups {
				let offset = (group_base_size + 1) * i;
				groups.push(
					(0..group_base_size + 1)
						.map(|j| offset + j)
						.map(|j| ValidatorIndex(j as _))
						.collect(),
				);
			}

			for i in 0..(n_cores as usize - n_larger_groups) {
				let offset = (n_larger_groups * (group_base_size + 1)) + (i * group_base_size);
				groups.push(
					(0..group_base_size)
						.map(|j| offset + j)
						.map(|j| ValidatorIndex(j as _))
						.collect(),
				);
			}

			ValidatorGroups::<T>::set(groups);
		}

		let now = frame_system::Pallet::<T>::block_number() + One::one();
		SessionStartBlock::<T>::set(now);
	}

	/// Get an iterator into the claim queues.
	///
	/// This iterator will have an item for each and every core index up to the maximum core index
	/// found in the claim queue. In other words there will be no holes/missing core indices,
	/// between core 0 and the maximum, even if the claim queue was missing entries for particular
	/// indices in between. (The iterator will return an empty `VecDeque` for those indices.
	fn claim_queue_iterator() -> impl Iterator<Item = (CoreIndex, VecDeque<Assignment>)> {
		let queues = ClaimQueue::<T>::get();
		return ClaimQueueIterator::<Assignment> {
			next_idx: 0,
			queue: queues.into_iter().peekable(),
		}
	}

	/// Get the validators in the given group, if the group index is valid for this session.
	pub(crate) fn group_validators(group_index: GroupIndex) -> Option<Vec<ValidatorIndex>> {
		ValidatorGroups::<T>::get().get(group_index.0 as usize).map(|g| g.clone())
	}

	/// Get the group assigned to a specific core by index at the current block number. Result
	/// undefined if the core index is unknown or the block number is less than the session start
	/// index.
	pub(crate) fn group_assigned_to_core(
		core: CoreIndex,
		at: BlockNumberFor<T>,
	) -> Option<GroupIndex> {
		let config = configuration::ActiveConfig::<T>::get();
		let session_start_block = SessionStartBlock::<T>::get();

		if at < session_start_block {
			return None
		}

		let validator_groups = ValidatorGroups::<T>::get();

		if core.0 as usize >= validator_groups.len() {
			return None
		}

		let rotations_since_session_start: BlockNumberFor<T> =
			(at - session_start_block) / config.scheduler_params.group_rotation_frequency;

		let rotations_since_session_start =
			<BlockNumberFor<T> as TryInto<u32>>::try_into(rotations_since_session_start)
				.unwrap_or(0);
		// Error case can only happen if rotations occur only once every u32::max(),
		// so functionally no difference in behavior.

		let group_idx =
			(core.0 as usize + rotations_since_session_start as usize) % validator_groups.len();
		Some(GroupIndex(group_idx as u32))
	}

	/// Returns a predicate that should be used for timing out occupied cores.
	///
	/// This only ever times out cores that have been occupied across a group rotation boundary.
	pub(crate) fn availability_timeout_predicate(
	) -> impl Fn(BlockNumberFor<T>) -> AvailabilityTimeoutStatus<BlockNumberFor<T>> {
		let config = configuration::ActiveConfig::<T>::get();
		let now = frame_system::Pallet::<T>::block_number();
		let rotation_info = Self::group_rotation_info(now);

		let next_rotation = rotation_info.next_rotation_at();

		let times_out = Self::availability_timeout_check_required();

		move |pending_since| {
			let time_out_at = if times_out {
				// We are at the beginning of the rotation, here availability period is relevant.
				// Note: blocks backed in this rotation will never time out here as backed_in +
				// config.paras_availability_period will always be > now for these blocks, as
				// otherwise above condition would not be true.
				pending_since + config.scheduler_params.paras_availability_period
			} else {
				next_rotation + config.scheduler_params.paras_availability_period
			};

			AvailabilityTimeoutStatus { timed_out: time_out_at <= now, live_until: time_out_at }
		}
	}

	/// Is evaluation of `availability_timeout_predicate` necessary at the current block?
	///
	/// This can be used to avoid calling `availability_timeout_predicate` for each core in case
	/// this function returns false.
	pub(crate) fn availability_timeout_check_required() -> bool {
		let config = configuration::ActiveConfig::<T>::get();
		let now = frame_system::Pallet::<T>::block_number() + One::one();
		let rotation_info = Self::group_rotation_info(now);

		let current_window =
			rotation_info.last_rotation_at() + config.scheduler_params.paras_availability_period;
		now < current_window
	}

	/// Returns a helper for determining group rotation.
	pub(crate) fn group_rotation_info(
		now: BlockNumberFor<T>,
	) -> GroupRotationInfo<BlockNumberFor<T>> {
		let session_start_block = SessionStartBlock::<T>::get();
		let group_rotation_frequency = configuration::ActiveConfig::<T>::get()
			.scheduler_params
			.group_rotation_frequency;

		GroupRotationInfo { session_start_block, now, group_rotation_frequency }
	}

	/// Return the next thing that will be scheduled on this core assuming it is currently
	/// occupied and the candidate occupying it became available.
	pub(crate) fn next_up_on_available(core: CoreIndex) -> Option<ScheduledCore> {
		ClaimQueue::<T>::get().get(&core).and_then(|a| {
			a.front()
				.map(|assignment| ScheduledCore { para_id: assignment.para_id(), collator: None })
		})
	}

	/// Pushes occupied cores to the assignment provider.
	/// TODO: these need to get the av-cores from the paras_inherent pallet.
	// fn push_occupied_cores_to_assignment_provider() {
	// 	AvailabilityCores::<T>::mutate(|cores| {
	// 		for core in cores.iter_mut() {
	// 			match core::mem::replace(core, CoreOccupied::Free) {
	// 				CoreOccupied::Free => continue,
	// 				CoreOccupied::Paras(entry) => {
	// 					Self::maybe_push_assignment(entry);
	// 				},
	// 			}
	// 		}
	// 	});
	// }

	// on new session
	fn push_claim_queue_items_to_assignment_provider() {
		for (_, claim_queue) in ClaimQueue::<T>::take() {
			// Push back in reverse order so that when we pop from the provider again,
			// the entries in the claim queue are in the same order as they are right now.
			for para_entry in claim_queue.into_iter().rev() {
				Self::maybe_push_assignment(para_entry);
			}
		}
	}

	/// Push assignments back to the provider on session change unless the paras
	/// timed out on availability before.
	fn maybe_push_assignment(assignment: Assignment) {
		T::AssignmentProvider::push_back_assignment(assignment);
	}

	/// Frees cores and fills the free claim queue spots by popping from the `AssignmentProvider`.
	pub fn advance_claim_queue(occupied_cores: &BTreeSet<CoreIndex>) {
		// This can only happen on new sessions at which we move all assignments back to the
		// provider. Hence, there's nothing we need to do here.
		if ValidatorGroups::<T>::decode_len().map_or(true, |l| l == 0) {
			return
		}
		let n_session_cores = T::AssignmentProvider::session_core_count();
		let cq = ClaimQueue::<T>::get();
		let config = configuration::ActiveConfig::<T>::get();
		// Extra sanity, config should already never be smaller than 1:
		let n_lookahead = config.scheduler_params.lookahead.max(1);

		for core_idx in 0..n_session_cores {
			let core_idx = CoreIndex::from(core_idx);

			if !occupied_cores.contains(&core_idx) {
				let core_idx = CoreIndex::from(core_idx);
				let n_lookahead_used = cq.get(&core_idx).map_or(0, |v| v.len() as u32);

				if let Some(dropped_para) = Self::pop_from_claim_queue(&core_idx) {
					T::AssignmentProvider::report_processed(dropped_para);
				}

				for _ in n_lookahead_used..n_lookahead {
					if let Some(assignment) =
						T::AssignmentProvider::pop_assignment_for_core(core_idx)
					{
						Self::add_to_claim_queue(core_idx, assignment);
					}
				}
			}
		}
	}

	fn add_to_claim_queue(core_idx: CoreIndex, assignment: Assignment) {
		ClaimQueue::<T>::mutate(|la| {
			la.entry(core_idx).or_default().push_back(assignment);
		});
	}

	fn pop_from_claim_queue(core_idx: &CoreIndex) -> Option<Assignment> {
		ClaimQueue::<T>::mutate(|cq| cq.get_mut(&core_idx)?.pop_front())
	}

	/// Paras scheduled next in the claim queue.
	pub(crate) fn scheduled_paras() -> impl Iterator<Item = (CoreIndex, ParaId)> {
		let claim_queue = ClaimQueue::<T>::get();
		claim_queue
			.into_iter()
			.filter_map(|(core_idx, v)| v.front().map(|a| (core_idx, a.para_id())))
	}

	/// Paras that may get backed on cores.
	///
	/// 1. The para must be scheduled on core.
	/// 2. Core needs to be free, otherwise backing is not possible.
	pub(crate) fn eligible_paras<'a>(
		occupied_cores: &'a BTreeSet<CoreIndex>,
	) -> impl Iterator<Item = (CoreIndex, ParaId)> + 'a {
		Self::claim_queue_iterator().filter_map(|(core_idx, queue)| {
			if occupied_cores.contains(&core_idx) {
				return None
			}
			let next_scheduled = queue.front()?;
			Some((core_idx, next_scheduled.para_id()))
		})
	}

	#[cfg(any(feature = "try-runtime", test))]
	fn claim_queue_len() -> usize {
		ClaimQueue::<T>::get().iter().map(|la_vec| la_vec.1.len()).sum()
	}

	#[cfg(all(not(feature = "runtime-benchmarks"), test))]
	pub(crate) fn claim_queue_is_empty() -> bool {
		Self::claim_queue_len() == 0
	}

	#[cfg(test)]
	pub(crate) fn set_validator_groups(validator_groups: Vec<Vec<ValidatorIndex>>) {
		ValidatorGroups::<T>::set(validator_groups);
	}

	#[cfg(test)]
	pub(crate) fn set_claim_queue(claim_queue: BTreeMap<CoreIndex, VecDeque<ParasEntryType<T>>>) {
		ClaimQueue::<T>::set(claim_queue);
	}
}
