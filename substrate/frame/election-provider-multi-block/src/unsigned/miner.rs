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

//! # NPoS miner

use crate::{
	helpers,
	types::{PageSize, Pagify},
	unsigned::{pallet::Config as UnsignedConfig, Call},
	verifier::FeasibilityError,
	AssignmentOf, MinerSupportsOf, MinerVoterOf, Pallet as EPM, Snapshot,
};

use frame_election_provider_support::{
	ElectionDataProvider, IndexAssignmentOf, NposSolution, NposSolver, PageIndex,
	TryIntoBoundedSupports, Weight,
};
use frame_support::{ensure, traits::Get, BoundedVec};
use scale_info::TypeInfo;
use sp_npos_elections::{ElectionResult, ElectionScore, ExtendedBalance, Support};
use sp_runtime::{offchain::storage::StorageValueRef, SaturatedConversion};
use sp_std::{prelude::ToOwned, vec, vec::Vec};

pub type TargetSnaphsotOf<T> =
	BoundedVec<<T as Config>::AccountId, <T as Config>::TargetSnapshotPerBlock>;
pub type VoterSnapshotPagedOf<T> = BoundedVec<
	BoundedVec<MinerVoterOf<T>, <T as Config>::VoterSnapshotPerBlock>,
	<T as Config>::Pages,
>;

#[derive(Debug, Eq, PartialEq, Clone)]
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
	SubmissionFailed,
	NotEnoughTargets,
	DataProvider,
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
#[derive(Debug, Eq, PartialEq, Clone)]
pub enum SnapshotType {
	/// Voters at the given page missing.
	Voters(PageIndex),
	/// Targets are missing.
	Targets,
	// Desired targets are missing.
	DesiredTargets,
}

/// Reports the trimming result of a mined solution
#[derive(Debug, Clone, PartialEq)]
pub struct TrimmingStatus {
	weight: usize,
	length: usize,
}

impl Default for TrimmingStatus {
	fn default() -> Self {
		Self { weight: 0, length: 0 }
	}
}

use crate::PagedRawSolution;
use codec::{EncodeLike, MaxEncodedLen};

pub trait Config {
	type AccountId: Ord + Clone + codec::Codec + core::fmt::Debug;

	type Solution: codec::Codec
		+ sp_std::fmt::Debug
		+ Default
		+ PartialEq
		+ Eq
		+ Clone
		+ Sized
		+ Ord
		+ NposSolution
		+ TypeInfo
		+ EncodeLike
		+ MaxEncodedLen;

	type Solver: NposSolver<
		AccountId = Self::AccountId,
		Accuracy = <Self::Solution as NposSolution>::Accuracy,
	>;

	type Pages: Get<u32>;

	type MaxVotesPerVoter: Get<u32>;
	type MaxWinnersPerPage: Get<u32>;
	type MaxBackersPerWinner: Get<u32>;

	type VoterSnapshotPerBlock: Get<u32>;
	type TargetSnapshotPerBlock: Get<u32>;

	type MaxWeight: Get<Weight>;
	type MaxLength: Get<u32>;
}

pub struct Miner<T: Config>(sp_std::marker::PhantomData<T>);

impl<T: Config> Miner<T> {
	pub fn mine_paged_solution_with_snapshot(
		all_voter_pages: &BoundedVec<
			BoundedVec<MinerVoterOf<T>, T::VoterSnapshotPerBlock>,
			T::Pages,
		>,
		all_targets: &BoundedVec<T::AccountId, T::TargetSnapshotPerBlock>,
		pages: PageIndex,
		round: u32,
		desired_targets: u32,
		do_reduce: bool,
	) -> Result<(PagedRawSolution<T>, TrimmingStatus), MinerError> {
		// useless to proceed if the solution will not be feasible.
		ensure!(all_targets.len() >= desired_targets as usize, MinerError::NotEnoughTargets);

		// flatten pages of voters and target snapshots.
		let all_voters: Vec<MinerVoterOf<T>> =
			all_voter_pages.iter().cloned().flatten().collect::<Vec<_>>();

		// these closures generate an efficient index mapping of each tvoter -> the snaphot
		// that they are part of. this needs to be the same indexing fn in the verifier side to
		// sync when reconstructing the assingments page from a solution.
		//let binding_targets = all_targets.clone();
		let voters_page_fn = helpers::generate_voter_page_fn::<T>(&all_voter_pages);
		let targets_index_fn = helpers::target_index_fn::<T>(&all_targets);

		// run the election with all voters and targets.
		let ElectionResult { winners: _, assignments } = <T::Solver as NposSolver>::solve(
			desired_targets as usize,
			all_targets.clone().to_vec(),
			all_voters.clone(),
		)
		.map_err(|_| MinerError::Solver)?;

		if do_reduce {
			// TODO(gpestana): reduce and trim.
		}
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
		let solution_pages: BoundedVec<<T as Config>::Solution, T::Pages> = paged_assignments
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

				<<T as Config>::Solution>::from_assignment(
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

		let mut paged_solution =
			PagedRawSolution { solution_pages, score: Default::default(), round };

		// everytthing's ready - calculate final solution score.
		paged_solution.score =
			Self::compute_score(all_voter_pages, all_targets, &paged_solution, desired_targets)?;

		Ok((paged_solution, trimming_status))
	}

	/// Take the given raw paged solution and compute its score. This will replicate what the chain
	/// would do as closely as possible, and expects all the corresponding snapshot data to be
	/// available.
	fn compute_score(
		voters: &VoterSnapshotPagedOf<T>,
		targets: &TargetSnaphsotOf<T>,
		paged_solution: &PagedRawSolution<T>,
		desired_targets: u32,
	) -> Result<ElectionScore, MinerError> {
		use sp_npos_elections::EvaluateSupport;
		use sp_std::collections::btree_map::BTreeMap;

		let all_supports =
			Self::feasibility_check(voters, targets, paged_solution, desired_targets)?;
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

	// Checks the feasibility of a paged solution and calculates the score associated with the
	// page.
	pub fn compute_partial_score(
		voters: &VoterSnapshotPagedOf<T>,
		targets: &TargetSnaphsotOf<T>,
		solution: &<T as Config>::Solution,
		desired_targets: u32,
		page: PageIndex,
	) -> Result<ElectionScore, MinerError> {
		let supports = Self::feasibility_check_partial(
			voters,
			targets,
			solution.clone(),
			desired_targets,
			page,
		)?;
		let score = sp_npos_elections::evaluate_support(
			supports.clone().into_iter().map(|(_, backings)| backings),
		);

		Ok(score)
	}

	/// Perform the feasibility check on all pages of a solution, one by one, and returns the
	/// supports of the full solution.
	pub fn feasibility_check(
		voters: &VoterSnapshotPagedOf<T>,
		targets: &TargetSnaphsotOf<T>,
		paged_solution: &PagedRawSolution<T>,
		desired_targets: u32,
	) -> Result<Vec<MinerSupportsOf<T>>, MinerError> {
		// check every solution page for feasibility.
		paged_solution
			.solution_pages
			.pagify(T::Pages::get())
			.map(|(page_index, page_solution)| {
				Self::feasibility_check_partial(
					voters,
					targets,
					page_solution.clone(),
					desired_targets,
					page_index as PageIndex,
				)
			})
			.collect::<Result<Vec<_>, _>>()
			.map_err(|err| MinerError::from(err))
	}

	/// Performs the feasibility check of a single page, returns the supports of the partial
	/// feasibility check.
	pub fn feasibility_check_partial(
		voters: &VoterSnapshotPagedOf<T>,
		targets: &TargetSnaphsotOf<T>,
		partial_solution: <T as Config>::Solution,
		desired_targets: u32,
		page: PageIndex,
	) -> Result<MinerSupportsOf<T>, FeasibilityError> {
		let voters_page: BoundedVec<MinerVoterOf<T>, <T as Config>::VoterSnapshotPerBlock> = voters
			.get(page as usize)
			.ok_or(FeasibilityError::Incomplete)
			.map(|v| v.to_owned())?;

		let voter_cache = helpers::generate_voter_cache::<T, _>(&voters_page);
		let voter_at = helpers::voter_at_fn::<T>(&voters_page);
		let target_at = helpers::target_at_fn::<T>(targets);
		let voter_index = helpers::voter_index_fn_usize::<T>(&voter_cache);

		// Then convert solution -> assignment. This will fail if any of the indices are
		// gibberish.
		let assignments = partial_solution
			.into_assignment(voter_at, target_at)
			.map_err::<FeasibilityError, _>(Into::into)?;

		// Ensure that assignments are all correct.
		let _ = assignments
			.iter()
			.map(|ref assignment| {
				// Check that assignment.who is actually a voter (defensive-only). NOTE: while
				// using the index map from `voter_index` is better than a blind linear search,
				// this *still* has room for optimization. Note that we had the index when we
				// did `solution -> assignment` and we lost it. Ideal is to keep the index
				// around.

				// Defensive-only: must exist in the snapshot.
				let snapshot_index =
					voter_index(&assignment.who).ok_or(FeasibilityError::InvalidVoter)?;
				// Defensive-only: index comes from the snapshot, must exist.
				let (_voter, _stake, targets) =
					voters_page.get(snapshot_index).ok_or(FeasibilityError::InvalidVoter)?;
				debug_assert!(*_voter == assignment.who);

				// Check that all of the targets are valid based on the snapshot.
				if assignment.distribution.iter().any(|(t, _)| !targets.contains(t)) {
					return Err(FeasibilityError::InvalidVote)
				}
				Ok(())
			})
			.collect::<Result<(), FeasibilityError>>()?;

		// ----- Start building support. First, we need one more closure.
		let stake_of = helpers::stake_of_fn::<T, _>(&voters_page, &voter_cache);

		// This might fail if the normalization fails. Very unlikely. See `integrity_test`.
		let staked_assignments =
			sp_npos_elections::assignment_ratio_to_staked_normalized(assignments, stake_of)
				.map_err::<FeasibilityError, _>(Into::into)?;

		let supports = sp_npos_elections::to_supports(&staked_assignments);

		// Check the maximum number of backers per winner. If this is a single-page solution, this
		// is enough to check `MaxBackersPerWinner`. Else, this is just a heuristic, and needs to be
		// checked again at the end (via `QueuedSolutionBackings`).
		ensure!(
			supports
				.iter()
				.all(|(_, s)| (s.voters.len() as u32) <= T::MaxBackersPerWinner::get()),
			FeasibilityError::TooManyBackings
		);

		// supports per page must not be higher than the desired targets, otherwise final solution
		// will also be higher than desired_targets.
		ensure!((supports.len() as u32) <= desired_targets, FeasibilityError::WrongWinnerCount);

		// almost-defensive-only: `MaxBackersPerWinner` is already checked. A sane value of
		// `MaxWinnersPerPage` should be more than any possible value of `desired_targets()`, which
		// is ALSO checked, so this conversion can almost never fail.
		let bounded_supports = supports
			.try_into_bounded_supports()
			.map_err(|_| FeasibilityError::WrongWinnerCount)?;

		Ok(bounded_supports)
	}

	/// Greedily reduce the size of the solution to fit into the block w.r.t length.
	///
	/// The length of the solution is largely a function of the number of voters. The number of
	/// winners cannot be changed Thus, to reduce the solution size, we need to strip voters.
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
		//debug_assert!(
		//	encoded_size_of(&assignments[..maximum_allowed_voters]).unwrap() <= max_allowed_length
		//);
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
		assignments.truncate(maximum_allowed_voters as usize);

		removing
	}

	/// Find the maximum `len` that a solution can have in order to fit into the block weight.
	///
	/// This only returns a value between zero and `size.nominators`.
	pub fn maximum_voter_for_weight(
		_desired_winners: u32,
		size: PageSize,
		max_weight: Weight,
	) -> u32 {
		if size.voters < 1 {
			return size.voters
		}

		let max_voters = size.voters.max(1);
		let mut voters = max_voters;

		// helper closures.
		let weight_with = |_active_voters: u32| -> Weight {
			Weight::zero() // TODO
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

/// Errors associated with the off-chain worker miner.
#[derive(
	frame_support::DebugNoBound, frame_support::EqNoBound, frame_support::PartialEqNoBound,
)]
pub enum OffchainMinerError {
	Miner(MinerError),
	PoolSubmissionFailed,
	NotUnsignedPhase,
	StorageError,
	PageOutOfBounds,
	Snapshots,
}

impl From<MinerError> for OffchainMinerError {
	fn from(e: MinerError) -> Self {
		OffchainMinerError::Miner(e)
	}
}

/// A miner used in the context of the offchain worker for unsigned submissions.
pub(crate) struct OffchainWorkerMiner<T: UnsignedConfig>(sp_std::marker::PhantomData<T>);

impl<T: UnsignedConfig> OffchainWorkerMiner<T> {
	/// The off-chain storage lock to work with unsigned submissions.
	pub(crate) const OFFCHAIN_LOCK: &'static [u8] = b"parity/multi-block-unsigned-election/lock";

	/// The off-chain storage ID prefix for each of the solution's pages. Each page will be
	/// prefixed by this ID, followed by the page index. The full page ID for a given index can be
	/// generated by [`Self::page_cache_id`].
	pub(crate) const OFFCHAIN_CACHED_SOLUTION: &'static [u8] =
		b"parity/multi-block-unsigned-election/solution";

	/// The off-chain storage ID for the solution's full score.
	pub(crate) const OFFCHAIN_CACHED_SCORE: &'static [u8] =
		b"parity/multi-block-unsigned-election/score";

	/// Mine a solution.
	///
	/// Mines a new solution with [`crate::Pallet::Pages`] pages and computes the partial score
	/// of the page with `page` index.
	#[allow(dead_code)]
	pub fn mine(
		page: PageIndex,
	) -> Result<
		(ElectionScore, ElectionScore, <T::MinerConfig as Config>::Solution),
		OffchainMinerError,
	> {
		let reduce = true;

		let (all_voter_pages, all_targets) = Self::fetch_snapshots()?;
		let round = crate::Pallet::<T>::current_round();
		let desired_targets =
			<<T as crate::Config>::DataProvider as ElectionDataProvider>::desired_targets()
				.map_err(|_| MinerError::DataProvider)?;

		let (solution, _trimming_status) =
			Miner::<T::MinerConfig>::mine_paged_solution_with_snapshot(
				&all_voter_pages,
				&all_targets,
				T::Pages::get(),
				round,
				desired_targets,
				reduce,
			)?;

		let partial_solution = solution
			.solution_pages
			.get(page as usize)
			.ok_or(OffchainMinerError::PageOutOfBounds)?;

		let partial_score = Miner::<T::MinerConfig>::compute_partial_score(
			&all_voter_pages,
			&all_targets,
			&partial_solution,
			desired_targets,
			page,
		)?;

		Ok((solution.score, partial_score, partial_solution.clone()))
	}

	pub(crate) fn fetch_snapshots() -> Result<
		(VoterSnapshotPagedOf<T::MinerConfig>, TargetSnaphsotOf<T::MinerConfig>),
		OffchainMinerError,
	> {
		// prepare range to fetch all pages of the target and voter snapshot.
		let paged_range = 0..EPM::<T>::msp() + 1;

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

		Ok((all_voter_pages, all_targets))
	}

	/// Fetches from the local storage or mines a new solution.
	///
	/// Calculates and returns the partial score of paged solution of the given `page` index.
	pub fn fetch_or_mine(
		page: PageIndex,
	) -> Result<
		(ElectionScore, ElectionScore, <T::MinerConfig as Config>::Solution),
		OffchainMinerError,
	> {
		let cache_id = Self::paged_cache_id(page)?;
		let score_storage = StorageValueRef::persistent(&Self::OFFCHAIN_CACHED_SCORE);
		let maybe_storage = StorageValueRef::persistent(&cache_id);

		let (full_score, paged_solution, partial_score) =
			if let Ok(Some((solution_page, partial_score))) =
				maybe_storage.get::<(<T::MinerConfig as Config>::Solution, ElectionScore)>()
			{
				sublog!(debug, "unsigned::ocw-miner", "offchain restoring a solution from cache.");

				let full_score = score_storage
					.get()
					.map_err(|_| OffchainMinerError::StorageError)?
					.ok_or(OffchainMinerError::StorageError)?;

				(full_score, solution_page, partial_score)
			} else {
				// no solution cached, compute it first.
				sublog!(debug, "unsigned::ocw-miner", "offchain miner computing a new solution.");

				// fetch snapshots.
				let (all_voter_pages, all_targets) = Self::fetch_snapshots()?;
				let round = crate::Pallet::<T>::current_round();
				let desired_targets =
					<<T as crate::Config>::DataProvider as ElectionDataProvider>::desired_targets()
						.map_err(|_| MinerError::DataProvider)?;

				let reduce = false; // TODO

				let (solution, _trimming_status) =
					Miner::<T::MinerConfig>::mine_paged_solution_with_snapshot(
						&all_voter_pages,
						&all_targets,
						T::Pages::get(),
						round,
						desired_targets,
						reduce,
					)?;

				// caches the solution score.
				score_storage
					.mutate::<_, (), _>(|_| Ok(solution.score.clone()))
					.map_err(|_| OffchainMinerError::StorageError)?;

				let mut solution_page = Default::default();
				let mut partial_score_r: ElectionScore = Default::default();

				// caches each of the individual pages and their partial score under its own key.
				for (idx, paged_solution) in solution.solution_pages.into_iter().enumerate() {
					let partial_score = Miner::<T::MinerConfig>::compute_partial_score(
						&all_voter_pages,
						&all_targets,
						&paged_solution,
						desired_targets,
						idx as u32,
					)?;

					let cache_id = Self::paged_cache_id(idx as PageIndex)?;
					let storage = StorageValueRef::persistent(&cache_id);
					storage
						.mutate::<_, (), _>(|_| Ok((paged_solution.clone(), partial_score)))
						.map_err(|_| OffchainMinerError::StorageError)?;

					// save to return the requested paged solution and partial score.
					if idx as PageIndex == page {
						solution_page = paged_solution;
						partial_score_r = partial_score;
					}
				}
				(solution.score, solution_page, partial_score_r)
			};

		Ok((full_score, partial_score, paged_solution))
	}

	/// Clears all local storage items related to the unsigned off-chain miner.
	pub(crate) fn clear_cache() {
		let mut score_storage = StorageValueRef::persistent(&Self::OFFCHAIN_CACHED_SCORE);
		score_storage.clear();

		for idx in (0..<T::MinerConfig as Config>::Pages::get()).into_iter() {
			let cache_id = Self::paged_cache_id(idx as PageIndex)
				.expect("page index was calculated based on the msp.");
			let mut page_storage = StorageValueRef::persistent(&cache_id);

			page_storage.clear();
		}

		sublog!(debug, "unsigned", "offchain miner cache cleared.");
	}

	/// Generate the page cache ID based on the `page` index and the
	/// [`Self::OFFCHAIN_CACHED_SOLUTION`] prefix.
	fn paged_cache_id(page: PageIndex) -> Result<Vec<u8>, OffchainMinerError> {
		let mut id = Self::OFFCHAIN_CACHED_SOLUTION.to_vec();
		id.push(page.try_into().map_err(|_| OffchainMinerError::PageOutOfBounds)?);
		Ok(id)
	}

	/// Submits a paged solution through the [`Call::submit_page_unsigned`] callable as an
	/// inherent.
	pub(crate) fn submit_paged_call(
		page: PageIndex,
		solution: <T::MinerConfig as Config>::Solution,
		partial_score: ElectionScore,
		claimed_full_score: ElectionScore,
	) -> Result<(), OffchainMinerError> {
		sublog!(
			debug,
			"unsigned::ocw-miner",
			"miner submitting a solution as an unsigned transaction, page: {:?}",
			page,
		);

		let call = Call::submit_page_unsigned { page, solution, partial_score, claimed_full_score };
		frame_system::offchain::SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(
			call.into(),
		)
		.map(|_| {
			sublog!(
				debug,
				"unsigned::ocw-miner",
				"miner submitted a solution as an unsigned transaction, page {:?}",
				page
			);
		})
		.map_err(|_| OffchainMinerError::PoolSubmissionFailed)
	}
}
