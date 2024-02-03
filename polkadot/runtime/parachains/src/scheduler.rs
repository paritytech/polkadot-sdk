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
use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::BlockNumberFor;
pub use polkadot_core_primitives::v2::BlockNumber;
use primitives::{
	CoreIndex, GroupIndex, GroupRotationInfo, Id as ParaId, ScheduledCore, ValidatorIndex,
};
use sp_runtime::traits::One;
use sp_std::{
	collections::{btree_map::BTreeMap, vec_deque::VecDeque},
	prelude::*,
};

pub mod common;

use common::{Assignment, AssignmentProvider, AssignmentProviderConfig};

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
	#[pallet::getter(fn validator_groups)]
	pub(crate) type ValidatorGroups<T> = StorageValue<_, Vec<Vec<ValidatorIndex>>, ValueQuery>;

	/// One entry for each availability core. Entries are `None` if the core is not currently
	/// occupied. Can be temporarily `Some` if scheduled but not occupied.
	/// The i'th parachain belongs to the i'th core, with the remaining cores all being
	/// parathread-multiplexers.
	///
	/// Bounded by the maximum of either of these two values:
	///   * The number of parachains and parathread multiplexers
	///   * The number of validators divided by `configuration.max_validators_per_core`.
	#[pallet::storage]
	#[pallet::getter(fn availability_cores)]
	pub(crate) type AvailabilityCores<T: Config> =
		StorageValue<_, Vec<CoreOccupiedType<T>>, ValueQuery>;

	/// Representation of a core in `AvailabilityCores`.
	///
	/// This is not to be confused with `CoreState` which is an enriched variant of this and exposed
	/// to the node side. It also provides information about scheduled/upcoming assignments for
	/// example and is computed on the fly in the `availability_cores` runtime call.
	#[derive(Encode, Decode, TypeInfo, RuntimeDebug, PartialEq)]
	pub enum CoreOccupied<N> {
		/// No candidate is waiting availability on this core right now (the core is not occupied).
		Free,
		/// A para is currently waiting for availability/inclusion on this core.
		Paras(ParasEntry<N>),
	}

	/// Convenience type alias for `CoreOccupied`.
	pub type CoreOccupiedType<T> = CoreOccupied<BlockNumberFor<T>>;

	impl<N> CoreOccupied<N> {
		/// Is core free?
		pub fn is_free(&self) -> bool {
			matches!(self, Self::Free)
		}
	}

	/// Reasons a core might be freed.
	#[derive(Clone, Copy)]
	pub enum FreedReason {
		/// The core's work concluded and the parablock assigned to it is considered available.
		Concluded,
		/// The core's work timed out.
		TimedOut,
	}

	/// The block number where the session start occurred. Used to track how many group rotations
	/// have occurred.
	///
	/// Note that in the context of parachains modules the session change is signaled during
	/// the block and enacted at the end of the block (at the finalization stage, to be exact).
	/// Thus for all intents and purposes the effect of the session change is observed at the
	/// block following the session change, block number of which we save in this storage value.
	#[pallet::storage]
	#[pallet::getter(fn session_start_block)]
	pub(crate) type SessionStartBlock<T: Config> = StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	/// One entry for each availability core. The `VecDeque` represents the assignments to be
	/// scheduled on that core. The value contained here will not be valid after the end of
	/// a block. Runtime APIs should be used to determine scheduled cores/ for the upcoming block.
	#[pallet::storage]
	#[pallet::getter(fn claimqueue)]
	pub(crate) type ClaimQueue<T: Config> =
		StorageValue<_, BTreeMap<CoreIndex, VecDeque<ParasEntryType<T>>>, ValueQuery>;

	/// Assignments as tracked in the claim queue.
	#[derive(Encode, Decode, TypeInfo, RuntimeDebug, PartialEq, Clone)]
	pub struct ParasEntry<N> {
		/// The underlying [`Assignment`].
		pub assignment: Assignment,
		/// The number of times the entry has timed out in availability already.
		pub availability_timeouts: u32,
		/// The block height until this entry needs to be backed.
		///
		/// If missed the entry will be removed from the claim queue without ever having occupied
		/// the core.
		pub ttl: N,
	}

	/// Convenience type declaration for `ParasEntry`.
	pub type ParasEntryType<T> = ParasEntry<BlockNumberFor<T>>;

	impl<N> ParasEntry<N> {
		/// Create a new `ParasEntry`.
		pub fn new(assignment: Assignment, now: N) -> Self {
			ParasEntry { assignment, availability_timeouts: 0, ttl: now }
		}

		/// Return `Id` from the underlying `Assignment`.
		pub fn para_id(&self) -> ParaId {
			self.assignment.para_id()
		}
	}

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

type PositionInClaimqueue = u32;

impl<T: Config> Pallet<T> {
	/// Called by the initializer to initialize the scheduler pallet.
	pub(crate) fn initializer_initialize(_now: BlockNumberFor<T>) -> Weight {
		Weight::zero()
	}

	/// Called by the initializer to finalize the scheduler pallet.
	pub(crate) fn initializer_finalize() {}

	/// Called before the initializer notifies of a new session.
	pub(crate) fn pre_new_session() {
		Self::push_claimqueue_items_to_assignment_provider();
		Self::push_occupied_cores_to_assignment_provider();
	}

	/// Called by the initializer to note that a new session has started.
	pub(crate) fn initializer_on_new_session(
		notification: &SessionChangeNotification<BlockNumberFor<T>>,
	) {
		let SessionChangeNotification { validators, new_config, .. } = notification;
		let config = new_config;

		let n_cores = core::cmp::max(
			T::AssignmentProvider::session_core_count(),
			match config.max_validators_per_core {
				Some(x) if x != 0 => validators.len() as u32 / x,
				_ => 0,
			},
		);

		AvailabilityCores::<T>::mutate(|cores| {
			cores.resize_with(n_cores as _, || CoreOccupied::Free);
		});

		// shuffle validators into groups.
		if n_cores == 0 || validators.is_empty() {
			ValidatorGroups::<T>::set(Vec::new());
		} else {
			let group_base_size = validators.len() / n_cores as usize;
			let n_larger_groups = validators.len() % n_cores as usize;

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

		let now = <frame_system::Pallet<T>>::block_number() + One::one();
		<SessionStartBlock<T>>::set(now);
	}

	/// Free unassigned cores. Provide a list of cores that should be considered newly-freed along
	/// with the reason for them being freed. Returns a tuple of concluded and timedout paras.
	fn free_cores(
		just_freed_cores: impl IntoIterator<Item = (CoreIndex, FreedReason)>,
	) -> (BTreeMap<CoreIndex, Assignment>, BTreeMap<CoreIndex, ParasEntryType<T>>) {
		let mut timedout_paras: BTreeMap<CoreIndex, ParasEntryType<T>> = BTreeMap::new();
		let mut concluded_paras = BTreeMap::new();

		AvailabilityCores::<T>::mutate(|cores| {
			let c_len = cores.len();

			just_freed_cores
				.into_iter()
				.filter(|(freed_index, _)| (freed_index.0 as usize) < c_len)
				.for_each(|(freed_index, freed_reason)| {
					match sp_std::mem::replace(
						&mut cores[freed_index.0 as usize],
						CoreOccupied::Free,
					) {
						CoreOccupied::Free => {},
						CoreOccupied::Paras(entry) => {
							match freed_reason {
								FreedReason::Concluded => {
									concluded_paras.insert(freed_index, entry.assignment);
								},
								FreedReason::TimedOut => {
									timedout_paras.insert(freed_index, entry);
								},
							};
						},
					};
				})
		});

		(concluded_paras, timedout_paras)
	}

	/// Note that the given cores have become occupied. Update the claimqueue accordingly.
	pub(crate) fn occupied(
		now_occupied: BTreeMap<CoreIndex, ParaId>,
	) -> BTreeMap<CoreIndex, PositionInClaimqueue> {
		let mut availability_cores = AvailabilityCores::<T>::get();

		log::debug!(target: LOG_TARGET, "[occupied] now_occupied {:?}", now_occupied);

		let pos_mapping: BTreeMap<CoreIndex, PositionInClaimqueue> = now_occupied
			.iter()
			.flat_map(|(core_idx, para_id)| {
				match Self::remove_from_claimqueue(*core_idx, *para_id) {
					Err(e) => {
						log::debug!(
							target: LOG_TARGET,
							"[occupied] error on remove_from_claimqueue {}",
							e
						);
						None
					},
					Ok((pos_in_claimqueue, pe)) => {
						availability_cores[core_idx.0 as usize] = CoreOccupied::Paras(pe);

						Some((*core_idx, pos_in_claimqueue))
					},
				}
			})
			.collect();

		// Drop expired claims after processing now_occupied.
		Self::drop_expired_claims_from_claimqueue();

		AvailabilityCores::<T>::set(availability_cores);

		pos_mapping
	}

	/// Iterates through every element in all claim queues and tries to add new assignments from the
	/// `AssignmentProvider`. A claim is considered expired if it's `ttl` field is lower than the
	/// current block height.
	fn drop_expired_claims_from_claimqueue() {
		let now = <frame_system::Pallet<T>>::block_number();
		let availability_cores = AvailabilityCores::<T>::get();

		ClaimQueue::<T>::mutate(|cq| {
			for (idx, _) in (0u32..).zip(availability_cores) {
				let core_idx = CoreIndex(idx);
				if let Some(core_claimqueue) = cq.get_mut(&core_idx) {
					let mut i = 0;
					let mut num_dropped = 0;
					while i < core_claimqueue.len() {
						let maybe_dropped = if let Some(entry) = core_claimqueue.get(i) {
							if entry.ttl < now {
								core_claimqueue.remove(i)
							} else {
								None
							}
						} else {
							None
						};

						if let Some(dropped) = maybe_dropped {
							num_dropped += 1;
							T::AssignmentProvider::report_processed(dropped.assignment);
						} else {
							i += 1;
						}
					}

					for _ in 0..num_dropped {
						// For all claims dropped due to TTL, attempt to pop a new entry to
						// the back of the claimqueue.
						if let Some(assignment) =
							T::AssignmentProvider::pop_assignment_for_core(core_idx)
						{
							let AssignmentProviderConfig { ttl, .. } =
								T::AssignmentProvider::get_provider_config(core_idx);
							core_claimqueue.push_back(ParasEntry::new(assignment, now + ttl));
						}
					}
				}
			}
		});
	}

	/// Get the para (chain or thread) ID assigned to a particular core or index, if any. Core
	/// indices out of bounds will return `None`, as will indices of unassigned cores.
	pub(crate) fn core_para(core_index: CoreIndex) -> Option<ParaId> {
		let cores = AvailabilityCores::<T>::get();
		match cores.get(core_index.0 as usize) {
			None | Some(CoreOccupied::Free) => None,
			Some(CoreOccupied::Paras(entry)) => Some(entry.para_id()),
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
		let config = <configuration::Pallet<T>>::config();
		let session_start_block = <SessionStartBlock<T>>::get();

		if at < session_start_block {
			return None
		}

		let validator_groups = ValidatorGroups::<T>::get();

		if core.0 as usize >= validator_groups.len() {
			return None
		}

		let rotations_since_session_start: BlockNumberFor<T> =
			(at - session_start_block) / config.group_rotation_frequency;

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
		let config = <configuration::Pallet<T>>::config();
		let now = <frame_system::Pallet<T>>::block_number();
		let rotation_info = Self::group_rotation_info(now);

		let next_rotation = rotation_info.next_rotation_at();

		let times_out = Self::availability_timeout_check_required();

		move |pending_since| {
			let time_out_at = if times_out {
				// We are at the beginning of the rotation, here availability period is relevant.
				// Note: blocks backed in this rotation will never time out here as backed_in +
				// config.paras_availability_period will always be > now for these blocks, as
				// otherwise above condition would not be true.
				pending_since + config.paras_availability_period
			} else {
				next_rotation + config.paras_availability_period
			};

			AvailabilityTimeoutStatus { timed_out: time_out_at <= now, live_until: time_out_at }
		}
	}

	/// Is evaluation of `availability_timeout_predicate` necessary at the current block?
	///
	/// This can be used to avoid calling `availability_timeout_predicate` for each core in case
	/// this function returns false.
	pub(crate) fn availability_timeout_check_required() -> bool {
		let config = <configuration::Pallet<T>>::config();
		let now = <frame_system::Pallet<T>>::block_number() + One::one();
		let rotation_info = Self::group_rotation_info(now);

		let current_window = rotation_info.last_rotation_at() + config.paras_availability_period;
		now < current_window
	}

	/// Returns a helper for determining group rotation.
	pub(crate) fn group_rotation_info(
		now: BlockNumberFor<T>,
	) -> GroupRotationInfo<BlockNumberFor<T>> {
		let session_start_block = Self::session_start_block();
		let group_rotation_frequency =
			<configuration::Pallet<T>>::config().group_rotation_frequency;

		GroupRotationInfo { session_start_block, now, group_rotation_frequency }
	}

	/// Return the next thing that will be scheduled on this core assuming it is currently
	/// occupied and the candidate occupying it became available.
	pub(crate) fn next_up_on_available(core: CoreIndex) -> Option<ScheduledCore> {
		ClaimQueue::<T>::get()
			.get(&core)
			.and_then(|a| a.front().map(|pe| Self::paras_entry_to_scheduled_core(pe)))
	}

	fn paras_entry_to_scheduled_core(pe: &ParasEntryType<T>) -> ScheduledCore {
		ScheduledCore { para_id: pe.para_id(), collator: None }
	}

	/// Return the next thing that will be scheduled on this core assuming it is currently
	/// occupied and the candidate occupying it times out.
	pub(crate) fn next_up_on_time_out(core: CoreIndex) -> Option<ScheduledCore> {
		Self::next_up_on_available(core).or_else(|| {
			// Or, if none, the claim currently occupying the core,
			// as it would be put back on the queue after timing out if number of retries is not at
			// the maximum.
			let cores = AvailabilityCores::<T>::get();
			cores.get(core.0 as usize).and_then(|c| match c {
				CoreOccupied::Free => None,
				CoreOccupied::Paras(pe) => {
					let AssignmentProviderConfig { max_availability_timeouts, .. } =
						T::AssignmentProvider::get_provider_config(core);

					if pe.availability_timeouts < max_availability_timeouts {
						Some(Self::paras_entry_to_scheduled_core(pe))
					} else {
						None
					}
				},
			})
		})
	}

	/// Pushes occupied cores to the assignment provider.
	fn push_occupied_cores_to_assignment_provider() {
		AvailabilityCores::<T>::mutate(|cores| {
			for core in cores.iter_mut() {
				match sp_std::mem::replace(core, CoreOccupied::Free) {
					CoreOccupied::Free => continue,
					CoreOccupied::Paras(entry) => {
						Self::maybe_push_assignment(entry);
					},
				}
			}
		});
	}

	// on new session
	fn push_claimqueue_items_to_assignment_provider() {
		for (_, claim_queue) in ClaimQueue::<T>::take() {
			// Push back in reverse order so that when we pop from the provider again,
			// the entries in the claimqueue are in the same order as they are right now.
			for para_entry in claim_queue.into_iter().rev() {
				Self::maybe_push_assignment(para_entry);
			}
		}
	}

	/// Push assignments back to the provider on session change unless the paras
	/// timed out on availability before.
	fn maybe_push_assignment(pe: ParasEntryType<T>) {
		if pe.availability_timeouts == 0 {
			T::AssignmentProvider::push_back_assignment(pe.assignment);
		}
	}

	//
	//  ClaimQueue related functions
	//
	fn claimqueue_lookahead() -> u32 {
		<configuration::Pallet<T>>::config().scheduling_lookahead
	}

	/// Frees cores and fills the free claimqueue spots by popping from the `AssignmentProvider`.
	pub fn free_cores_and_fill_claimqueue(
		just_freed_cores: impl IntoIterator<Item = (CoreIndex, FreedReason)>,
		now: BlockNumberFor<T>,
	) {
		let (mut concluded_paras, mut timedout_paras) = Self::free_cores(just_freed_cores);

		// This can only happen on new sessions at which we move all assignments back to the
		// provider. Hence, there's nothing we need to do here.
		if ValidatorGroups::<T>::decode_len().map_or(true, |l| l == 0) {
			return
		}
		// If there exists a core, ensure we schedule at least one job onto it.
		let n_lookahead = Self::claimqueue_lookahead().max(1);
		let n_session_cores = T::AssignmentProvider::session_core_count();
		let cq = ClaimQueue::<T>::get();
		let ttl = <configuration::Pallet<T>>::config().on_demand_ttl;

		for core_idx in 0..n_session_cores {
			let core_idx = CoreIndex::from(core_idx);

			// add previously timedout paras back into the queue
			if let Some(mut entry) = timedout_paras.remove(&core_idx) {
				let AssignmentProviderConfig { max_availability_timeouts, .. } =
					T::AssignmentProvider::get_provider_config(core_idx);
				if entry.availability_timeouts < max_availability_timeouts {
					// Increment the timeout counter.
					entry.availability_timeouts += 1;
					// Reset the ttl so that a timed out assignment.
					entry.ttl = now + ttl;
					Self::add_to_claimqueue(core_idx, entry);
					// The claim has been added back into the claimqueue.
					// Do not pop another assignment for the core.
					continue
				} else {
					// Consider timed out assignments for on demand parachains as concluded for
					// the assignment provider
					let ret = concluded_paras.insert(core_idx, entry.assignment);
					debug_assert!(ret.is_none());
				}
			}

			if let Some(concluded_para) = concluded_paras.remove(&core_idx) {
				T::AssignmentProvider::report_processed(concluded_para);
			}
			// We consider occupied cores to be part of the claimqueue
			let n_lookahead_used = cq.get(&core_idx).map_or(0, |v| v.len() as u32) +
				if Self::is_core_occupied(core_idx) { 1 } else { 0 };
			for _ in n_lookahead_used..n_lookahead {
				if let Some(assignment) = T::AssignmentProvider::pop_assignment_for_core(core_idx) {
					Self::add_to_claimqueue(core_idx, ParasEntry::new(assignment, now + ttl));
				}
			}
		}

		debug_assert!(timedout_paras.is_empty());
		debug_assert!(concluded_paras.is_empty());
	}

	fn is_core_occupied(core_idx: CoreIndex) -> bool {
		match AvailabilityCores::<T>::get().get(core_idx.0 as usize) {
			None | Some(CoreOccupied::Free) => false,
			Some(CoreOccupied::Paras(_)) => true,
		}
	}

	fn add_to_claimqueue(core_idx: CoreIndex, pe: ParasEntryType<T>) {
		ClaimQueue::<T>::mutate(|la| {
			la.entry(core_idx).or_default().push_back(pe);
		});
	}

	/// Returns `ParasEntry` with `para_id` at `core_idx` if found.
	fn remove_from_claimqueue(
		core_idx: CoreIndex,
		para_id: ParaId,
	) -> Result<(PositionInClaimqueue, ParasEntryType<T>), &'static str> {
		ClaimQueue::<T>::mutate(|cq| {
			let core_claims = cq.get_mut(&core_idx).ok_or("core_idx not found in lookahead")?;

			let pos = core_claims
				.iter()
				.position(|pe| pe.para_id() == para_id)
				.ok_or("para id not found at core_idx lookahead")?;

			let pe = core_claims.remove(pos).ok_or("remove returned None")?;

			Ok((pos as u32, pe))
		})
	}

	/// Paras scheduled next in the claim queue.
	pub(crate) fn scheduled_paras() -> impl Iterator<Item = (CoreIndex, ParaId)> {
		let claimqueue = ClaimQueue::<T>::get();
		claimqueue
			.into_iter()
			.filter_map(|(core_idx, v)| v.front().map(|e| (core_idx, e.assignment.para_id())))
	}

	#[cfg(any(feature = "runtime-benchmarks", test))]
	pub(crate) fn assignment_provider_config(
		core_idx: CoreIndex,
	) -> AssignmentProviderConfig<BlockNumberFor<T>> {
		T::AssignmentProvider::get_provider_config(core_idx)
	}

	#[cfg(any(feature = "try-runtime", test))]
	fn claimqueue_len() -> usize {
		ClaimQueue::<T>::get().iter().map(|la_vec| la_vec.1.len()).sum()
	}

	#[cfg(all(not(feature = "runtime-benchmarks"), test))]
	pub(crate) fn claimqueue_is_empty() -> bool {
		Self::claimqueue_len() == 0
	}

	#[cfg(test)]
	pub(crate) fn set_validator_groups(validator_groups: Vec<Vec<ValidatorIndex>>) {
		ValidatorGroups::<T>::set(validator_groups);
	}

	#[cfg(test)]
	pub(crate) fn set_claimqueue(claimqueue: BTreeMap<CoreIndex, VecDeque<ParasEntryType<T>>>) {
		ClaimQueue::<T>::set(claimqueue);
	}
}
