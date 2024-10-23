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

use crate::{configuration, initializer::SessionChangeNotification, paras};
use alloc::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet, vec_deque::VecDeque},
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

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(3);

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
	/// scheduled on that core.
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

impl<T: Config> Pallet<T> {
	/// Called by the initializer to initialize the scheduler pallet.
	pub(crate) fn initializer_initialize(_now: BlockNumberFor<T>) -> Weight {
		Weight::zero()
	}

	/// Called by the initializer to finalize the scheduler pallet.
	pub(crate) fn initializer_finalize() {}

	/// Called by the initializer to note that a new session has started.
	pub(crate) fn initializer_on_new_session(
		notification: &SessionChangeNotification<BlockNumberFor<T>>,
	) {
		let SessionChangeNotification { validators, new_config, prev_config, .. } = notification;
		let config = new_config;
		let assigner_cores = config.scheduler_params.num_cores;

		let n_cores = core::cmp::max(
			assigner_cores,
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

		// Resize and populate claim queue.
		Self::maybe_resize_claim_queue(prev_config.scheduler_params.num_cores, assigner_cores);
		Self::populate_claim_queue_after_session_change();

		let now = frame_system::Pallet::<T>::block_number() + One::one();
		SessionStartBlock::<T>::set(now);
	}

	/// Get the validators in the given group, if the group index is valid for this session.
	pub(crate) fn group_validators(group_index: GroupIndex) -> Option<Vec<ValidatorIndex>> {
		ValidatorGroups::<T>::get().get(group_index.0 as usize).map(|g| g.clone())
	}

	/// Get the number of cores.
	pub(crate) fn num_availability_cores() -> usize {
		ValidatorGroups::<T>::decode_len().unwrap_or(0)
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
		// Since this is being called from a runtime API, we need to workaround for #64.
		if Self::on_chain_storage_version() == StorageVersion::new(2) {
			migration::v2::ClaimQueue::<T>::get()
				.get(&core)
				.and_then(|a| a.front().map(|entry| entry.assignment.para_id()))
		} else {
			ClaimQueue::<T>::get()
				.get(&core)
				.and_then(|a| a.front().map(|assignment| assignment.para_id()))
		}
		.map(|para_id| ScheduledCore { para_id, collator: None })
	}

	// Since this is being called from a runtime API, we need to workaround for #64.
	pub(crate) fn get_claim_queue() -> BTreeMap<CoreIndex, VecDeque<Assignment>> {
		if Self::on_chain_storage_version() == StorageVersion::new(2) {
			migration::v2::ClaimQueue::<T>::get()
				.into_iter()
				.map(|(core_index, entries)| {
					(core_index, entries.into_iter().map(|e| e.assignment).collect())
				})
				.collect()
		} else {
			ClaimQueue::<T>::get()
		}
	}

	/// For each core that isn't part of the `except_for` set, pop the first item of the claim queue
	/// and fill the queue from the assignment provider.
	pub(crate) fn advance_claim_queue(except_for: &BTreeSet<CoreIndex>) {
		let config = configuration::ActiveConfig::<T>::get();
		let num_assigner_cores = config.scheduler_params.num_cores;
		// Extra sanity, config should already never be smaller than 1:
		let n_lookahead = config.scheduler_params.lookahead.max(1);

		for core_idx in 0..num_assigner_cores {
			let core_idx = CoreIndex::from(core_idx);

			if !except_for.contains(&core_idx) {
				let core_idx = CoreIndex::from(core_idx);

				if let Some(dropped_para) = Self::pop_front_of_claim_queue(&core_idx) {
					T::AssignmentProvider::report_processed(dropped_para);
				}

				Self::fill_claim_queue(core_idx, n_lookahead);
			}
		}
	}

	// on new session
	fn maybe_resize_claim_queue(old_core_count: u32, new_core_count: u32) {
		if new_core_count < old_core_count {
			ClaimQueue::<T>::mutate(|cq| {
				let to_remove: Vec<_> = cq
					.range(CoreIndex(new_core_count)..CoreIndex(old_core_count))
					.map(|(k, _)| *k)
					.collect();
				for key in to_remove {
					if let Some(dropped_assignments) = cq.remove(&key) {
						Self::push_back_to_assignment_provider(dropped_assignments.into_iter());
					}
				}
			});
		}
	}

	// Populate the claim queue. To be called on new session, after all the other modules were
	// initialized.
	fn populate_claim_queue_after_session_change() {
		let config = configuration::ActiveConfig::<T>::get();
		// Extra sanity, config should already never be smaller than 1:
		let n_lookahead = config.scheduler_params.lookahead.max(1);
		let new_core_count = config.scheduler_params.num_cores;

		for core_idx in 0..new_core_count {
			let core_idx = CoreIndex::from(core_idx);
			Self::fill_claim_queue(core_idx, n_lookahead);
		}
	}

	/// Push some assignments back to the provider.
	fn push_back_to_assignment_provider(
		assignments: impl core::iter::DoubleEndedIterator<Item = Assignment>,
	) {
		// Push back in reverse order so that when we pop from the provider again,
		// the entries in the claim queue are in the same order as they are right
		// now.
		for assignment in assignments.rev() {
			T::AssignmentProvider::push_back_assignment(assignment);
		}
	}

	fn fill_claim_queue(core_idx: CoreIndex, n_lookahead: u32) {
		ClaimQueue::<T>::mutate(|la| {
			let cq = la.entry(core_idx).or_default();

			let mut n_lookahead_used = cq.len() as u32;

			// If the claim queue used to be empty, we need to double the first assignment.
			// Otherwise, the para will only be able to get the collation in right at the next block
			// (synchronous backing).
			// Only do this if the configured lookahead is greater than 1. Otherwise, it doesn't
			// make sense.
			if n_lookahead_used == 0 && n_lookahead > 1 {
				if let Some(assignment) = T::AssignmentProvider::pop_assignment_for_core(core_idx) {
					T::AssignmentProvider::assignment_duplicated(&assignment);
					cq.push_back(assignment.clone());
					cq.push_back(assignment);
					n_lookahead_used += 2;
				}
			}

			for _ in n_lookahead_used..n_lookahead {
				if let Some(assignment) = T::AssignmentProvider::pop_assignment_for_core(core_idx) {
					cq.push_back(assignment);
				} else {
					break
				}
			}

			// If we didn't end up pushing anything, remove the entry. We don't want to waste the
			// space if we've no assignments.
			if cq.is_empty() {
				la.remove(&core_idx);
			}
		});
	}

	fn pop_front_of_claim_queue(core_idx: &CoreIndex) -> Option<Assignment> {
		ClaimQueue::<T>::mutate(|cq| cq.get_mut(core_idx)?.pop_front())
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
	pub(crate) fn set_claim_queue(claim_queue: BTreeMap<CoreIndex, VecDeque<Assignment>>) {
		ClaimQueue::<T>::set(claim_queue);
	}
}
