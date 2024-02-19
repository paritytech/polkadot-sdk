// This file is part of Substrate.

// Copyright (C) 2022 Parity Technologies (UK) Ltd.
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

//! ## NPoS miner

use crate::{
	helpers,
	types::{PageSize, Pagify, SupportsOf, VoterOf},
	unsigned::pallet::Config as UnsignedConfig,
	verifier::FeasibilityError,
	AssignmentOf, Config, PagedRawSolution, Pallet as EPM, Snapshot, SolutionAccuracyOf,
	SolutionOf,
};

use codec::Encode;
use frame_election_provider_support::{
	ElectionProvider, IndexAssignmentOf, NposSolution, NposSolver, PageIndex, Weight,
};
use frame_support::{traits::Get, BoundedVec};
use sp_npos_elections::{
	assignment_ratio_to_staked_normalized, assignment_staked_to_ratio_normalized, ElectionResult,
	ElectionScore, ExtendedBalance, Support,
};
use sp_runtime::SaturatedConversion;
use sp_std::{vec, vec::Vec};

#[derive(Debug, Eq, PartialEq)]
pub enum MinerError {
	/// An internal error in the NPoS elections crate.
	NposElections(sp_npos_elections::Error),
	/// Snapshot data was unavailable.
	SnapshotUnAvailable(SnapshotType),
	/// An error from the election solver.
	Solver,
	/// The solution generated from the miner is not feasible.
	Feasibility(FeasibilityError),
	InvalidPage,
}

impl From<sp_npos_elections::Error> for MinerError {
	fn from(e: sp_npos_elections::Error) -> Self {
		MinerError::NposElections(e)
	}
}

impl From<FeasibilityError> for MinerError {
	fn from(e: FeasibilityError) -> Self {
		MinerError::Feasibility(e)
	}
}

impl From<SnapshotType> for MinerError {
	fn from(typ: SnapshotType) -> Self {
		MinerError::SnapshotUnAvailable(typ)
	}
}

/// The type of the snapshot.
///
/// Used to express errors.
#[derive(Debug, Eq, PartialEq)]
pub enum SnapshotType {
	/// Voters at the given page missing.
	Voters(PageIndex),
	/// Targets are missing.
	Targets,
	// Desired targets are missing.
	DesiredTargets,
}

/// Reports the trimming result of a mined solution
#[derive(Debug, Clone)]
pub struct TrimmingStatus {
	weight: usize,
	length: usize,
}

impl Default for TrimmingStatus {
	fn default() -> Self {
		Self { weight: 0, length: 0 }
	}
}

pub struct Miner<T: UnsignedConfig, Solver: NposSolver>(sp_std::marker::PhantomData<(T, Solver)>);

impl<T: UnsignedConfig, S: NposSolver> Miner<T, S>
where
	S: NposSolver<AccountId = T::AccountId, Accuracy = SolutionAccuracyOf<T>>,
{
	/// Mines a paged solution.
	///
	/// This always trims the solution to match a few parameters:
	///
	/// 1. [`crate::verifier::Config::MaxBackersPerWinner`]
	/// 2. [`crate::unsigned::Config::MinerMaxLength`]
	/// 3. [`crate::unsigned::Config::MinerMaxWeight`]
	///
	/// //TODO(doc)
	pub fn mine_paged_solution(
		mut pages: PageIndex,
		do_reduce: bool,
	) -> Result<(PagedRawSolution<T>, TrimmingStatus), MinerError> {
		let desired_targets = Snapshot::<T>::desired_targets()
			.ok_or::<MinerError>(SnapshotType::DesiredTargets.into())?;

		// prepare range to fetch all pages of the target and voter snapshot.
		let paged_range = (0..EPM::<T>::msp() + 1).take(pages as usize);

		// fetch all pages of the voter snapshot and collect them in a bounded vec.
		let all_voter_pages: BoundedVec<_, T::Pages> = paged_range
			.map(|page| {
				Snapshot::<T>::voters(page)
					.ok_or(MinerError::SnapshotUnAvailable(SnapshotType::Voters(page)))
			})
			.collect::<Result<Vec<_>, _>>()?
			.try_into()
			.expect("range was constructed from the bounded vec bounds; qed.");

		// fetch all pages of the target snapshot and collect them in a bounded vec.
		let all_targets = Snapshot::<T>::targets()
			.ok_or(MinerError::SnapshotUnAvailable(SnapshotType::Targets))?;

		// flatten pages of voters and target snapshots.
		let all_voters: Vec<VoterOf<T>> =
			all_voter_pages.iter().cloned().flatten().collect::<Vec<_>>();

		// these closures generate an efficient index mapping of each tvoter -> the snaphot
		// that they are part of. this needs to be the same indexing fn in the verifier side to
		// sync when reconstructing the assingments page from a solution.
		let voters_page_fn = helpers::generate_voter_page_fn::<T>(&all_voter_pages);
		let targets_index_fn = helpers::target_index_fn::<T>(&all_targets);

		// run the election with all voters and targets.
		let ElectionResult { winners: _, assignments } =
			S::solve(desired_targets as usize, all_targets.clone().to_vec(), all_voters.clone())
				.map_err(|_| MinerError::Solver)?;

		// TODO(gpestana): reduce and trim.

		// split assignments into `T::Pages pages.
		let mut paged_assignments: BoundedVec<Vec<AssignmentOf<T>>, T::Pages> =
			BoundedVec::with_bounded_capacity(pages as usize);

		paged_assignments.bounded_resize(pages as usize, vec![]);

		// adds assignment to the correct page, based on the voter's snapshot page.
		for assignment in assignments {
			let page = voters_page_fn(&assignment.who).ok_or(MinerError::InvalidPage)?;
			let assignment_page =
				paged_assignments.get_mut(page as usize).ok_or(MinerError::InvalidPage)?;
			assignment_page.push(assignment);
		}

		// convert each page of assignments to a paged `T::Solution`.
		let solution_pages: BoundedVec<SolutionOf<T>, T::Pages> = paged_assignments
			.clone()
			.into_iter()
			.enumerate()
			.map(|(page_index, assignment_page)| {
				let page: PageIndex = page_index.saturated_into();
				let voter_snapshot_page = all_voter_pages
					.get(page as usize)
					.ok_or(MinerError::SnapshotUnAvailable(SnapshotType::Voters(page)))?;

				let voters_index_fn = {
					let cache = helpers::generate_voter_cache::<T, _>(&voter_snapshot_page);
					helpers::voter_index_fn_owned::<T>(cache)
				};

				<SolutionOf<T>>::from_assignment(
					&assignment_page,
					&voters_index_fn,
					&targets_index_fn,
				)
				.map_err(|e| MinerError::NposElections(e))
			})
			.collect::<Result<Vec<_>, _>>()?
			.try_into()
			.expect("paged_assignments is bound by `T::Pages. qed.");

		// TODO(gpestana): trim again?
		let trimming_status = Default::default();

		let round = crate::Pallet::<T>::current_round();
		let mut paged_solution =
			PagedRawSolution { solution_pages, score: Default::default(), round };

		// everytthing's ready - calculate final solution score.
		paged_solution.score = Self::compute_score(&paged_solution)?;

		Ok((paged_solution, trimming_status))
	}

	/// Mines a NPoS solution of a given page anc converts the result into a [`PagedRawSolution`],
	/// ready to be submitted on-chain.
	///
	/// Fetches the snapshot data (`voters`, `targets`, `desired_targets`) from storage for the
	/// requested page and calls into the NPoS solver `S` to calculate a solution.
	///
	/// The final solution may be reduced, based on the `reduce` bool.
	pub fn mine_and_prepare_solution_single_page(
		page: PageIndex,
		reduce: bool,
	) -> Result<(PagedRawSolution<T>, TrimmingStatus), MinerError> {
		let desired_targets = Snapshot::<T>::desired_targets()
			.ok_or::<MinerError>(SnapshotType::DesiredTargets.into())?;
		let voters =
			Snapshot::<T>::voters(page).ok_or::<MinerError>(SnapshotType::Voters(page).into())?;
		let targets = Snapshot::<T>::targets().ok_or::<MinerError>(SnapshotType::Targets.into())?;

		S::solve(desired_targets as usize, targets.to_vec(), voters.to_vec())
			.map_err(|e| MinerError::Solver)
			.and_then(|election_result| {
				Self::prepare_election_result_with_snapshot(
					election_result,
					voters,
					targets,
					desired_targets,
					page,
					reduce,
				)
			})
	}
	/// Convert a raw solution from [`sp_npos_elections::ElectionResult`] to [`RawSolution`], which
	/// is ready to be submitted to the chain.
	///
	/// May reduce the solution based on the `reduce` bool.
	pub fn prepare_election_result_with_snapshot(
		election_result: ElectionResult<T::AccountId, S::Accuracy>,
		voters: BoundedVec<VoterOf<T>, T::VoterSnapshotPerBlock>,
		targets: BoundedVec<T::AccountId, T::TargetSnapshotPerBlock>,
		desired_targets: u32,
		page: PageIndex,
		reduce: bool,
	) -> Result<(PagedRawSolution<T>, TrimmingStatus), MinerError> {
		// prepare helper closures.
		let cache = helpers::generate_voter_cache::<T, _>(&voters);
		let voter_index = helpers::voter_index_fn::<T>(&cache);
		let target_index = helpers::target_index_fn::<T>(&targets);
		let voter_at = helpers::voter_at_fn::<T>(&voters);
		let target_at = helpers::target_at_fn::<T>(&targets);
		let stake_of = helpers::stake_of_fn::<T, _>(&voters, &cache);

		// Compute the size of a solution comprised of the selected arguments.
		//
		// This function completes in `O(edges)`; it's expensive, but linear.
		let encoded_size_of = |assignments: &[IndexAssignmentOf<T::Solution>]| {
			SolutionOf::<T>::try_from(assignments).map(|s| s.encoded_size())
		};

		let ElectionResult { assignments, winners: _ } = election_result;

		let sorted_assignments = {
			let mut staked = assignment_ratio_to_staked_normalized(assignments, &stake_of)?;

			if reduce {
				// we reduce before sorting in order to ensure that the reduction process doesn't
				// accidentally change the sort order
				sp_npos_elections::reduce(&mut staked);
			}

			// Sort the assignments by reversed voter stake. This ensures that we can efficiently
			// truncate the list.
			staked.sort_by_key(
				|sp_npos_elections::StakedAssignment::<T::AccountId> { who, .. }| {
					// though staked assignments are expressed in terms of absolute stake, we'd
					// still need to iterate over all votes in order to actually compute the total
					// stake. it should be faster to look it up from the cache.
					let stake = cache
						.get(who)
						.map(|idx| {
							let (_, stake, _) = voters[*idx];
							stake
						})
						.unwrap_or_default();
					sp_std::cmp::Reverse(stake)
				},
			);

			// convert back.
			assignment_staked_to_ratio_normalized(staked)?
		};

		// Convert to `IndexAssignment`. This improves the runtime complexity of repeatedly
		// converting to `Solution`.
		let mut index_assignments = sorted_assignments
			.into_iter()
			.map(|assignment| {
				IndexAssignmentOf::<T::Solution>::new(&assignment, &voter_index, &target_index)
			})
			.collect::<Result<Vec<_>, _>>()?;

		// trim assignments list for weight and length.
		let size = PageSize { voters: voters.len() as u32, targets: targets.len() as u32 };
		let weight_trimmed = Self::trim_assignments_weight(
			desired_targets,
			size,
			T::MaxWeight::get(),
			&mut index_assignments,
		);
		let length_trimmed = Self::trim_assignments_length(
			T::MaxLength::get(),
			&mut index_assignments,
			&encoded_size_of,
		)?;

		// now make solution.
		let solution = SolutionOf::<T>::try_from(&index_assignments)?;

		// re-calc score.
		let score = solution.clone().score(stake_of, voter_at, target_at)?;
		let is_trimmed = TrimmingStatus { weight: weight_trimmed, length: length_trimmed };

		let round = EPM::<T>::current_round();
		let solution_pages: BoundedVec<T::Solution, T::Pages> =
			vec![solution].try_into().expect("fits");

		Ok((PagedRawSolution { solution_pages, score, round }, is_trimmed))
	}

	/// Take the given raw paged solution and compute its score. This will replicate what the chain
	/// would do as closely as possible, and expects all the corresponding snapshot data to be
	/// available.
	fn compute_score(paged_solution: &PagedRawSolution<T>) -> Result<ElectionScore, MinerError> {
		use sp_npos_elections::EvaluateSupport;
		use sp_std::collections::btree_map::BTreeMap;

		let all_supports = Self::check_feasibility(paged_solution, "mined")?;
		let mut total_backings: BTreeMap<T::AccountId, ExtendedBalance> = BTreeMap::new();
		all_supports.into_iter().map(|x| x.0).flatten().for_each(|(who, support)| {
			let backing = total_backings.entry(who).or_default();
			*backing = backing.saturating_add(support.total);
		});

		let all_supports = total_backings
			.into_iter()
			.map(|(who, total)| (who, Support { total, ..Default::default() }))
			.collect::<Vec<_>>();

		Ok((&all_supports).evaluate())
	}

	/// perform the feasibility check on all pages of a solution, returning `Ok(())` if all good and
	/// the corresponding error otherwise.
	pub fn check_feasibility(
		paged_solution: &PagedRawSolution<T>,
		solution_type: &str,
	) -> Result<Vec<SupportsOf<T::Verifier>>, MinerError> {
		// check every solution page for feasibility.
		paged_solution
			.solution_pages
			.pagify(T::Pages::get())
			.map(|(page_index, page_solution)| {
				<T::Verifier as crate::verifier::Verifier>::feasibility_check(
					page_solution.clone(),
					page_index as PageIndex,
				)
			})
			.collect::<Result<Vec<_>, _>>()
			.map_err(|err| {
				sublog!(
					warn,
					"unsigned::base-miner",
					"feasibility check failed for {} solution at: {:?}",
					solution_type,
					err
				);
				MinerError::from(err)
			})
	}

	/// Greedily reduce the size of the solution to fit into the block w.r.t length.
	///
	/// The length of the solution is largely a function of the number of voters. The number of
	/// winners cannot be changed. Thus, to reduce the solution size, we need to strip voters.
	///
	/// Note that this solution is already computed, and winners are elected based on the merit of
	/// the total stake in the system. Nevertheless, some of the voters may be removed here.
	///
	/// Sometimes, removing a voter can cause a validator to also be implicitly removed, if
	/// that voter was the only backer of that winner. In such cases, this solution is invalid,
	/// which will be caught prior to submission.
	///
	/// The score must be computed **after** this step. If this step reduces the score too much,
	/// then the solution must be discarded.
	pub fn trim_assignments_length(
		max_allowed_length: u32,
		assignments: &mut Vec<IndexAssignmentOf<T::Solution>>,
		encoded_size_of: impl Fn(
			&[IndexAssignmentOf<T::Solution>],
		) -> Result<usize, sp_npos_elections::Error>,
	) -> Result<usize, MinerError> {
		// Perform a binary search for the max subset of which can fit into the allowed
		// length. Having discovered that, we can truncate efficiently.
		let max_allowed_length: usize = max_allowed_length.saturated_into();
		let mut high = assignments.len();
		let mut low = 0;

		// not much we can do if assignments are already empty.
		if high == low {
			return Ok(0)
		}

		while high - low > 1 {
			let test = (high + low) / 2;
			if encoded_size_of(&assignments[..test])? <= max_allowed_length {
				low = test;
			} else {
				high = test;
			}
		}
		let maximum_allowed_voters = if low < assignments.len() &&
			encoded_size_of(&assignments[..low + 1])? <= max_allowed_length
		{
			low + 1
		} else {
			low
		};

		// ensure our post-conditions are correct
		debug_assert!(
			encoded_size_of(&assignments[..maximum_allowed_voters]).unwrap() <= max_allowed_length
		);
		debug_assert!(if maximum_allowed_voters < assignments.len() {
			encoded_size_of(&assignments[..maximum_allowed_voters + 1]).unwrap() >
				max_allowed_length
		} else {
			true
		});

		// NOTE: before this point, every access was immutable.
		// after this point, we never error.
		// check before edit.

		let remove = assignments.len().saturating_sub(maximum_allowed_voters);

		log!(
			debug,
			"from {} assignments, truncating to {} for length, removing {}",
			assignments.len(),
			maximum_allowed_voters,
			remove
		);
		assignments.truncate(maximum_allowed_voters);

		Ok(remove)
	}

	/// Greedily reduce the size of the solution to fit into the block w.r.t. weight.
	///
	/// The weight of the solution is foremost a function of the number of voters (i.e.
	/// `assignments.len()`). Aside from this, the other components of the weight are invariant. The
	/// number of winners shall not be changed (otherwise the solution is invalid) and the
	/// `ElectionSize` is merely a representation of the total number of stakers.
	///
	/// Thus, we reside to stripping away some voters from the `assignments`.
	///
	/// Note that the solution is already computed, and the winners are elected based on the merit
	/// of the entire stake in the system. Nonetheless, some of the voters will be removed further
	/// down the line.
	///
	/// Indeed, the score must be computed **after** this step. If this step reduces the score too
	/// much or remove a winner, then the solution must be discarded **after** this step.
	pub fn trim_assignments_weight(
		desired_targets: u32,
		size: PageSize,
		max_weight: Weight,
		assignments: &mut Vec<IndexAssignmentOf<T::Solution>>,
	) -> usize {
		let maximum_allowed_voters =
			Self::maximum_voter_for_weight(desired_targets, size, max_weight);
		let removing: usize =
			assignments.len().saturating_sub(maximum_allowed_voters.saturated_into());
		log!(
			debug,
			"from {} assignments, truncating to {} for weight, removing {}",
			assignments.len(),
			maximum_allowed_voters,
			removing,
		);
		assignments.truncate(maximum_allowed_voters as usize);

		removing
	}

	/// Find the maximum `len` that a solution can have in order to fit into the block weight.
	///
	/// This only returns a value between zero and `size.nominators`.
	pub fn maximum_voter_for_weight(
		desired_winners: u32,
		size: PageSize,
		max_weight: Weight,
	) -> u32 {
		if size.voters < 1 {
			return size.voters
		}

		let max_voters = size.voters.max(1);
		let mut voters = max_voters;

		// helper closures.
		let weight_with = |active_voters: u32| -> Weight {
			//T::solution_weight(size.voters, size.targets, active_voters, desired_winners) // TODO
			Weight::zero()
		};

		let next_voters = |current_weight: Weight, voters: u32, step: u32| -> Result<u32, ()> {
			if current_weight.all_lt(max_weight) {
				let next_voters = voters.checked_add(step);
				match next_voters {
					Some(voters) if voters < max_voters => Ok(voters),
					_ => Err(()),
				}
			} else if current_weight.any_gt(max_weight) {
				voters.checked_sub(step).ok_or(())
			} else {
				// If any of the constituent weights is equal to the max weight, we're at max
				Ok(voters)
			}
		};

		// First binary-search the right amount of voters
		let mut step = voters / 2;
		let mut current_weight = weight_with(voters);

		while step > 0 {
			match next_voters(current_weight, voters, step) {
				// proceed with the binary search
				Ok(next) if next != voters => {
					voters = next;
				},
				// we are out of bounds, break out of the loop.
				Err(()) => break,
				// we found the right value - early exit the function.
				Ok(next) => return next,
			}
			step /= 2;
			current_weight = weight_with(voters);
		}

		// Time to finish. We might have reduced less than expected due to rounding error. Increase
		// one last time if we have any room left, the reduce until we are sure we are below limit.
		while voters < max_voters && weight_with(voters + 1).all_lt(max_weight) {
			voters += 1;
		}
		while voters.checked_sub(1).is_some() && weight_with(voters).any_gt(max_weight) {
			voters -= 1;
		}

		let final_decision = voters.min(size.voters);
		debug_assert!(
			weight_with(final_decision).all_lte(max_weight),
			"weight_with({}) <= {}",
			final_decision,
			max_weight,
		);
		final_decision
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{mock::*, Phase, Snapshot};
	use frame_election_provider_support::SequentialPhragmen;
	use frame_support::BoundedVec;
	use sp_npos_elections::ElectionResult;
	use sp_runtime::Perbill;

	mod indice_encode_decode_solution {
		use super::*;

		#[test]
		fn snapshot_idx_based_works() {
			ExtBuilder::default().build_and_execute(|| {
				roll_to_phase(Phase::Signed);

				let mut all_voter_pages = vec![];
				let mut all_target_pages = vec![];
				let desired_targets = Snapshot::<T>::desired_targets().unwrap();

				for page in (0..Pages::get()).rev() {
					all_voter_pages.push(Snapshot::<T>::voters(page).unwrap());
					all_target_pages.push(Snapshot::<T>::targets().unwrap());
				}
			})
		}

		#[test]
		fn snapshot_stake_sorted_works() {}
	}
}
