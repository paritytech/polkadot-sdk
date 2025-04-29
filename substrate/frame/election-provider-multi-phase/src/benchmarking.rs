// This file is part of Substrate.

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

//! Two phase election pallet benchmarking.

use core::cmp::Reverse;
use frame_benchmarking::{v2::*, BenchmarkError};
use frame_election_provider_support::{bounds::DataProviderBounds, IndexAssignment};
use frame_support::{
	assert_ok,
	traits::{Hooks, TryCollect},
	BoundedVec,
};
use frame_system::RawOrigin;
use rand::{prelude::SliceRandom, rngs::SmallRng, SeedableRng};
use sp_arithmetic::{per_things::Percent, traits::One};
use sp_runtime::InnerOf;

use crate::{unsigned::IndexAssignmentOf, *};

const SEED: u32 = 999;

/// Creates a **valid** solution with exactly the given size.
///
/// The snapshot is also created internally.
fn solution_with_size<T: Config>(
	size: SolutionOrSnapshotSize,
	active_voters_count: u32,
	desired_targets: u32,
) -> Result<RawSolution<SolutionOf<T::MinerConfig>>, &'static str> {
	ensure!(size.targets >= desired_targets, "must have enough targets");
	ensure!(
		size.targets >= (<SolutionOf<T::MinerConfig>>::LIMIT * 2) as u32,
		"must have enough targets for unique votes."
	);
	ensure!(size.voters >= active_voters_count, "must have enough voters");
	ensure!(
		(<SolutionOf<T::MinerConfig>>::LIMIT as u32) < desired_targets,
		"must have enough winners to give them votes."
	);

	let ed: VoteWeight = T::Currency::minimum_balance().saturated_into::<u64>();
	let stake: VoteWeight = ed.max(One::one()).saturating_mul(100);

	// first generates random targets.
	let targets: Vec<T::AccountId> = (0..size.targets)
		.map(|i| frame_benchmarking::account("Targets", i, SEED))
		.collect();

	let mut rng = SmallRng::seed_from_u64(SEED.into());

	// decide who are the winners.
	let winners = targets
		.as_slice()
		.choose_multiple(&mut rng, desired_targets as usize)
		.cloned()
		.collect::<Vec<_>>();

	// first generate active voters who must vote for a subset of winners.
	let active_voters = (0..active_voters_count)
		.map(|i| {
			// chose a random subset of winners.
			let winner_votes: BoundedVec<_, _> = winners
				.as_slice()
				.choose_multiple(&mut rng, <SolutionOf<T::MinerConfig>>::LIMIT)
				.cloned()
				.try_collect()
				.expect("<SolutionOf<T::MinerConfig>>::LIMIT is the correct bound; qed.");
			let voter = frame_benchmarking::account::<T::AccountId>("Voter", i, SEED);
			(voter, stake, winner_votes)
		})
		.collect::<Vec<_>>();

	// rest of the voters. They can only vote for non-winners.
	let non_winners = targets
		.iter()
		.filter(|t| !winners.contains(t))
		.cloned()
		.collect::<Vec<T::AccountId>>();
	let rest_voters = (active_voters_count..size.voters)
		.map(|i| {
			let votes: BoundedVec<_, _> = (&non_winners)
				.choose_multiple(&mut rng, <SolutionOf<T::MinerConfig>>::LIMIT)
				.cloned()
				.try_collect()
				.expect("<SolutionOf<T::MinerConfig>>::LIMIT is the correct bound; qed.");
			let voter = frame_benchmarking::account::<T::AccountId>("Voter", i, SEED);
			(voter, stake, votes)
		})
		.collect::<Vec<_>>();

	let mut all_voters = active_voters.clone();
	all_voters.extend(rest_voters);
	all_voters.shuffle(&mut rng);

	assert_eq!(active_voters.len() as u32, active_voters_count);
	assert_eq!(all_voters.len() as u32, size.voters);
	assert_eq!(winners.len() as u32, desired_targets);

	SnapshotMetadata::<T>::put(SolutionOrSnapshotSize {
		voters: all_voters.len() as u32,
		targets: targets.len() as u32,
	});
	DesiredTargets::<T>::put(desired_targets);
	Snapshot::<T>::put(RoundSnapshot { voters: all_voters.clone(), targets: targets.clone() });

	// write the snapshot to staking or whoever is the data provider, in case it is needed further
	// down the road.
	T::DataProvider::put_snapshot(all_voters.clone(), targets.clone(), Some(stake));

	let cache = helpers::generate_voter_cache::<T::MinerConfig>(&all_voters);
	let stake_of = helpers::stake_of_fn::<T::MinerConfig>(&all_voters, &cache);
	let voter_index = helpers::voter_index_fn::<T::MinerConfig>(&cache);
	let target_index = helpers::target_index_fn::<T::MinerConfig>(&targets);
	let voter_at = helpers::voter_at_fn::<T::MinerConfig>(&all_voters);
	let target_at = helpers::target_at_fn::<T::MinerConfig>(&targets);

	let assignments = active_voters
		.iter()
		.map(|(voter, _stake, votes)| {
			let percent_per_edge: InnerOf<SolutionAccuracyOf<T>> =
				(100 / votes.len()).try_into().unwrap_or_else(|_| panic!("failed to convert"));
			unsigned::Assignment::<T> {
				who: voter.clone(),
				distribution: votes
					.iter()
					.map(|t| (t.clone(), SolutionAccuracyOf::<T>::from_percent(percent_per_edge)))
					.collect::<Vec<_>>(),
			}
		})
		.collect::<Vec<_>>();

	let solution =
		<SolutionOf<T::MinerConfig>>::from_assignment(&assignments, &voter_index, &target_index)
			.unwrap();
	let score = solution.clone().score(stake_of, voter_at, target_at).unwrap();
	let round = Round::<T>::get();

	assert!(
		score.minimal_stake > 0,
		"score is zero, this probably means that the stakes are not set."
	);
	Ok(RawSolution { solution, score, round })
}

fn set_up_data_provider<T: Config>(v: u32, t: u32) {
	T::DataProvider::clear();
	log!(
		info,
		"setting up with voters = {} [degree = {}], targets = {}",
		v,
		<T::DataProvider as ElectionDataProvider>::MaxVotesPerVoter::get(),
		t
	);

	// fill targets.
	let mut targets = (0..t)
		.map(|i| {
			let target = frame_benchmarking::account::<T::AccountId>("Target", i, SEED);

			T::DataProvider::add_target(target.clone());
			target
		})
		.collect::<Vec<_>>();

	// we should always have enough voters to fill.
	assert!(
		targets.len() > <T::DataProvider as ElectionDataProvider>::MaxVotesPerVoter::get() as usize
	);
	targets.truncate(<T::DataProvider as ElectionDataProvider>::MaxVotesPerVoter::get() as usize);

	// fill voters.
	(0..v).for_each(|i| {
		let voter = frame_benchmarking::account::<T::AccountId>("Voter", i, SEED);
		let weight = T::Currency::minimum_balance().saturated_into::<u64>() * 1000;
		T::DataProvider::add_voter(voter, weight, targets.clone().try_into().unwrap());
	});
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn on_initialize_nothing() {
		assert!(CurrentPhase::<T>::get().is_off());

		#[block]
		{
			Pallet::<T>::on_initialize(1_u32.into());
		}

		assert!(CurrentPhase::<T>::get().is_off());
	}

	#[benchmark]
	fn on_initialize_open_signed() {
		assert!(Snapshot::<T>::get().is_none());
		assert!(CurrentPhase::<T>::get().is_off());

		#[block]
		{
			Pallet::<T>::phase_transition(Phase::Signed);
		}

		assert!(Snapshot::<T>::get().is_none());
		assert!(CurrentPhase::<T>::get().is_signed());
	}

	#[benchmark]
	fn on_initialize_open_unsigned() {
		assert!(Snapshot::<T>::get().is_none());
		assert!(CurrentPhase::<T>::get().is_off());

		#[block]
		{
			let now = frame_system::Pallet::<T>::block_number();
			Pallet::<T>::phase_transition(Phase::Unsigned((true, now)));
		}

		assert!(Snapshot::<T>::get().is_none());
		assert!(CurrentPhase::<T>::get().is_unsigned());
	}

	#[benchmark]
	fn finalize_signed_phase_accept_solution() {
		let receiver = account("receiver", 0, SEED);
		let initial_balance = T::Currency::minimum_balance() + 10_u32.into();
		T::Currency::make_free_balance_be(&receiver, initial_balance);
		let ready = Default::default();
		let deposit: BalanceOf<T> = 10_u32.into();

		let reward: BalanceOf<T> = T::SignedRewardBase::get();
		let call_fee: BalanceOf<T> = 30_u32.into();

		assert_ok!(T::Currency::reserve(&receiver, deposit));
		assert_eq!(T::Currency::free_balance(&receiver), T::Currency::minimum_balance());

		#[block]
		{
			Pallet::<T>::finalize_signed_phase_accept_solution(ready, &receiver, deposit, call_fee);
		}

		assert_eq!(T::Currency::free_balance(&receiver), initial_balance + reward + call_fee);
		assert_eq!(T::Currency::reserved_balance(&receiver), 0_u32.into());
	}

	#[benchmark]
	fn finalize_signed_phase_reject_solution() {
		let receiver = account("receiver", 0, SEED);
		let initial_balance = T::Currency::minimum_balance() + 10_u32.into();
		let deposit: BalanceOf<T> = 10_u32.into();
		T::Currency::make_free_balance_be(&receiver, initial_balance);
		assert_ok!(T::Currency::reserve(&receiver, deposit));

		assert_eq!(T::Currency::free_balance(&receiver), T::Currency::minimum_balance());
		assert_eq!(T::Currency::reserved_balance(&receiver), 10_u32.into());

		#[block]
		{
			Pallet::<T>::finalize_signed_phase_reject_solution(&receiver, deposit)
		}

		assert_eq!(T::Currency::free_balance(&receiver), T::Currency::minimum_balance());
		assert_eq!(T::Currency::reserved_balance(&receiver), 0_u32.into());
	}

	#[benchmark]
	fn create_snapshot_internal(
		// Number of votes in snapshot.
		v: Linear<{ T::BenchmarkingConfig::VOTERS[0] }, { T::BenchmarkingConfig::VOTERS[1] }>,
		// Number of targets in snapshot.
		t: Linear<{ T::BenchmarkingConfig::TARGETS[0] }, { T::BenchmarkingConfig::TARGETS[1] }>,
	) -> Result<(), BenchmarkError> {
		// We don't directly need the data-provider to be populated, but it is just easy to use it.
		set_up_data_provider::<T>(v, t);
		// default bounds are unbounded.
		let targets =
			T::DataProvider::electable_targets(DataProviderBounds::default(), Zero::zero())?;
		let voters = T::DataProvider::electing_voters(DataProviderBounds::default(), Zero::zero())?;

		let desired_targets = T::DataProvider::desired_targets()?;
		assert!(Snapshot::<T>::get().is_none());

		#[block]
		{
			Pallet::<T>::create_snapshot_internal(targets, voters, desired_targets)
		}

		assert!(Snapshot::<T>::get().is_some());
		assert_eq!(SnapshotMetadata::<T>::get().ok_or("metadata missing")?.voters, v);
		assert_eq!(SnapshotMetadata::<T>::get().ok_or("metadata missing")?.targets, t);

		Ok(())
	}

	// A call to `<Pallet as ElectionProvider>::elect` where we only return the queued solution.
	#[benchmark]
	fn elect_queued(
		// Number of assignments, i.e. `solution.len()`.
		// This means the active nominators, thus must be a subset of `v`.
		a: Linear<
			{ T::BenchmarkingConfig::ACTIVE_VOTERS[0] },
			{ T::BenchmarkingConfig::ACTIVE_VOTERS[1] },
		>,
		// Number of desired targets. Must be a subset of `t`.
		d: Linear<
			{ T::BenchmarkingConfig::DESIRED_TARGETS[0] },
			{ T::BenchmarkingConfig::DESIRED_TARGETS[1] },
		>,
	) -> Result<(), BenchmarkError> {
		// Number of votes in snapshot. Not dominant.
		let v = T::BenchmarkingConfig::VOTERS[1];
		// Number of targets in snapshot. Not dominant.
		let t = T::BenchmarkingConfig::TARGETS[1];

		let witness = SolutionOrSnapshotSize { voters: v, targets: t };
		let raw_solution = solution_with_size::<T>(witness, a, d)?;
		let ready_solution = Pallet::<T>::feasibility_check(raw_solution, ElectionCompute::Signed)
			.map_err(<&str>::from)?;
		CurrentPhase::<T>::put(Phase::Signed);
		// Assume a queued solution is stored, regardless of where it comes from.
		QueuedSolution::<T>::put(ready_solution);

		// These are set by the `solution_with_size` function.
		assert!(DesiredTargets::<T>::get().is_some());
		assert!(Snapshot::<T>::get().is_some());
		assert!(SnapshotMetadata::<T>::get().is_some());

		let result;

		#[block]
		{
			result = <Pallet<T> as ElectionProvider>::elect(Zero::zero());
		}

		assert!(result.is_ok());
		assert!(QueuedSolution::<T>::get().is_none());
		assert!(DesiredTargets::<T>::get().is_none());
		assert!(Snapshot::<T>::get().is_none());
		assert!(SnapshotMetadata::<T>::get().is_none());
		assert_eq!(
			CurrentPhase::<T>::get(),
			<Phase<frame_system::pallet_prelude::BlockNumberFor::<T>>>::Off
		);

		Ok(())
	}

	#[benchmark]
	fn submit() -> Result<(), BenchmarkError> {
		// The queue is full and the solution is only better than the worse.
		Pallet::<T>::create_snapshot().map_err(<&str>::from)?;
		Pallet::<T>::phase_transition(Phase::Signed);
		Round::<T>::put(1);

		let mut signed_submissions = SignedSubmissions::<T>::get();

		// Insert `max` submissions
		for i in 0..(T::SignedMaxSubmissions::get() - 1) {
			let raw_solution = RawSolution {
				score: ElectionScore {
					minimal_stake: 10_000_000u128 + (i as u128),
					..Default::default()
				},
				..Default::default()
			};
			let signed_submission = SignedSubmission {
				raw_solution,
				who: account("submitters", i, SEED),
				deposit: Default::default(),
				call_fee: Default::default(),
			};
			signed_submissions.insert(signed_submission);
		}
		signed_submissions.put();

		// This score will eject the weakest one.
		let solution = RawSolution {
			score: ElectionScore { minimal_stake: 10_000_000u128 + 1, ..Default::default() },
			..Default::default()
		};

		let caller = frame_benchmarking::whitelisted_caller();
		let deposit =
			Pallet::<T>::deposit_for(&solution, SnapshotMetadata::<T>::get().unwrap_or_default());
		T::Currency::make_free_balance_be(
			&caller,
			T::Currency::minimum_balance() * 1000u32.into() + deposit,
		);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), Box::new(solution));

		assert!(Pallet::<T>::signed_submissions().len() as u32 == T::SignedMaxSubmissions::get());

		Ok(())
	}

	#[benchmark]
	fn submit_unsigned(
		// Number of votes in snapshot.
		v: Linear<{ T::BenchmarkingConfig::VOTERS[0] }, { T::BenchmarkingConfig::VOTERS[1] }>,
		// Number of targets in snapshot.
		t: Linear<{ T::BenchmarkingConfig::TARGETS[0] }, { T::BenchmarkingConfig::TARGETS[1] }>,
		// Number of assignments, i.e. `solution.len()`.
		// This means the active nominators, thus must be a subset of `v` component.
		a: Linear<
			{ T::BenchmarkingConfig::ACTIVE_VOTERS[0] },
			{ T::BenchmarkingConfig::ACTIVE_VOTERS[1] },
		>,
		// Number of desired targets. Must be a subset of `t` component.
		d: Linear<
			{ T::BenchmarkingConfig::DESIRED_TARGETS[0] },
			{ T::BenchmarkingConfig::DESIRED_TARGETS[1] },
		>,
	) -> Result<(), BenchmarkError> {
		let witness = SolutionOrSnapshotSize { voters: v, targets: t };
		let raw_solution = solution_with_size::<T>(witness, a, d)?;

		assert!(QueuedSolution::<T>::get().is_none());
		CurrentPhase::<T>::put(Phase::Unsigned((true, 1_u32.into())));

		#[extrinsic_call]
		_(RawOrigin::None, Box::new(raw_solution), witness);

		assert!(QueuedSolution::<T>::get().is_some());

		Ok(())
	}

	// This is checking a valid solution. The worse case is indeed a valid solution.
	#[benchmark]
	fn feasibility_check(
		// Number of votes in snapshot.
		v: Linear<{ T::BenchmarkingConfig::VOTERS[0] }, { T::BenchmarkingConfig::VOTERS[1] }>,
		// Number of targets in snapshot.
		t: Linear<{ T::BenchmarkingConfig::TARGETS[0] }, { T::BenchmarkingConfig::TARGETS[1] }>,
		// Number of assignments, i.e. `solution.len()`.
		// This means the active nominators, thus must be a subset of `v` component.
		a: Linear<
			{ T::BenchmarkingConfig::ACTIVE_VOTERS[0] },
			{ T::BenchmarkingConfig::ACTIVE_VOTERS[1] },
		>,
		// Number of desired targets. Must be a subset of `t` component.
		d: Linear<
			{ T::BenchmarkingConfig::DESIRED_TARGETS[0] },
			{ T::BenchmarkingConfig::DESIRED_TARGETS[1] },
		>,
	) -> Result<(), BenchmarkError> {
		let size = SolutionOrSnapshotSize { voters: v, targets: t };
		let raw_solution = solution_with_size::<T>(size, a, d)?;

		assert_eq!(raw_solution.solution.voter_count() as u32, a);
		assert_eq!(raw_solution.solution.unique_targets().len() as u32, d);

		let result;

		#[block]
		{
			result = Pallet::<T>::feasibility_check(raw_solution, ElectionCompute::Unsigned);
		}

		assert!(result.is_ok());

		Ok(())
	}

	// NOTE: this weight is not used anywhere, but the fact that it should succeed when execution in
	// isolation is vital to ensure memory-safety. For the same reason, we don't care about the
	// components iterating, we merely check that this operation will work with the "maximum"
	// numbers.
	//
	// ONLY run this benchmark in isolation, and pass the `--extra` flag to enable it.
	//
	// NOTE: If this benchmark does not run out of memory with a given heap pages, it means that the
	// OCW process can SURELY succeed with the given configuration, but the opposite is not true.
	// This benchmark is doing more work than a raw call to `OffchainWorker_offchain_worker` runtime
	// api call, since it is also setting up some mock data, which will itself exhaust the heap to
	// some extent.
	#[benchmark(extra)]
	fn mine_solution_offchain_memory() {
		// Number of votes in snapshot. Fixed to maximum.
		let v = T::BenchmarkingConfig::MINER_MAXIMUM_VOTERS;
		// Number of targets in snapshot. Fixed to maximum.
		let t = T::BenchmarkingConfig::MAXIMUM_TARGETS;

		set_up_data_provider::<T>(v, t);
		let now = frame_system::Pallet::<T>::block_number();
		CurrentPhase::<T>::put(Phase::Unsigned((true, now)));
		Pallet::<T>::create_snapshot().unwrap();

		#[block]
		{
			// we can't really verify this as it won't write anything to state, check logs.
			Pallet::<T>::offchain_worker(now)
		}
	}

	// NOTE: this weight is not used anywhere, but the fact that it should succeed when execution in
	// isolation is vital to ensure memory-safety. For the same reason, we don't care about the
	// components iterating, we merely check that this operation will work with the "maximum"
	// numbers.
	//
	// ONLY run this benchmark in isolation, and pass the `--extra` flag to enable it.
	#[benchmark(extra)]
	fn create_snapshot_memory() -> Result<(), BenchmarkError> {
		// Number of votes in snapshot. Fixed to maximum.
		let v = T::BenchmarkingConfig::SNAPSHOT_MAXIMUM_VOTERS;
		// Number of targets in snapshot. Fixed to maximum.
		let t = T::BenchmarkingConfig::MAXIMUM_TARGETS;

		set_up_data_provider::<T>(v, t);
		assert!(Snapshot::<T>::get().is_none());

		#[block]
		{
			Pallet::<T>::create_snapshot().map_err(|_| "could not create snapshot")?;
		}

		assert!(Snapshot::<T>::get().is_some());
		assert_eq!(SnapshotMetadata::<T>::get().ok_or("snapshot missing")?.voters, v);
		assert_eq!(SnapshotMetadata::<T>::get().ok_or("snapshot missing")?.targets, t);

		Ok(())
	}

	#[benchmark(extra)]
	fn trim_assignments_length(
		// Number of votes in snapshot.
		v: Linear<{ T::BenchmarkingConfig::VOTERS[0] }, { T::BenchmarkingConfig::VOTERS[1] }>,
		// Number of targets in snapshot.
		t: Linear<{ T::BenchmarkingConfig::TARGETS[0] }, { T::BenchmarkingConfig::TARGETS[1] }>,
		// Number of assignments, i.e. `solution.len()`.
		// This means the active nominators, thus must be a subset of `v` component.
		a: Linear<
			{ T::BenchmarkingConfig::ACTIVE_VOTERS[0] },
			{ T::BenchmarkingConfig::ACTIVE_VOTERS[1] },
		>,
		// Number of desired targets. Must be a subset of `t` component.
		d: Linear<
			{ T::BenchmarkingConfig::DESIRED_TARGETS[0] },
			{ T::BenchmarkingConfig::DESIRED_TARGETS[1] },
		>,
		// Subtract this percentage from the actual encoded size.
		f: Linear<0, 95>,
	) -> Result<(), BenchmarkError> {
		// Compute a random solution, then work backwards to get the lists of voters, targets, and
		// assignments
		let witness = SolutionOrSnapshotSize { voters: v, targets: t };
		let RawSolution { solution, .. } = solution_with_size::<T>(witness, a, d)?;
		let RoundSnapshot { voters, targets } = Snapshot::<T>::get().ok_or("snapshot missing")?;
		let voter_at = helpers::voter_at_fn::<T::MinerConfig>(&voters);
		let target_at = helpers::target_at_fn::<T::MinerConfig>(&targets);
		let mut assignments = solution
			.into_assignment(voter_at, target_at)
			.expect("solution generated by `solution_with_size` must be valid.");

		// make a voter cache and some helper functions for access
		let cache = helpers::generate_voter_cache::<T::MinerConfig>(&voters);
		let voter_index = helpers::voter_index_fn::<T::MinerConfig>(&cache);
		let target_index = helpers::target_index_fn::<T::MinerConfig>(&targets);

		// sort assignments by decreasing voter stake
		assignments.sort_by_key(|unsigned::Assignment::<T> { who, .. }| {
			let stake = cache
				.get(who)
				.map(|idx| {
					let (_, stake, _) = voters[*idx];
					stake
				})
				.unwrap_or_default();
			Reverse(stake)
		});

		let mut index_assignments = assignments
			.into_iter()
			.map(|assignment| IndexAssignment::new(&assignment, &voter_index, &target_index))
			.collect::<Result<Vec<_>, _>>()
			.unwrap();

		let encoded_size_of = |assignments: &[IndexAssignmentOf<T::MinerConfig>]| {
			SolutionOf::<T::MinerConfig>::try_from(assignments)
				.map(|solution| solution.encoded_size())
		};

		let desired_size = Percent::from_percent(100 - f.saturated_into::<u8>())
			.mul_ceil(encoded_size_of(index_assignments.as_slice()).unwrap());
		log!(trace, "desired_size = {}", desired_size);

		#[block]
		{
			Miner::<T::MinerConfig>::trim_assignments_length(
				desired_size.saturated_into(),
				&mut index_assignments,
				&encoded_size_of,
			)
			.unwrap();
		}

		let solution =
			SolutionOf::<T::MinerConfig>::try_from(index_assignments.as_slice()).unwrap();
		let encoding = solution.encode();
		log!(
			trace,
			"encoded size prediction = {}",
			encoded_size_of(index_assignments.as_slice()).unwrap(),
		);
		log!(trace, "actual encoded size = {}", encoding.len());
		assert!(encoding.len() <= desired_size);

		Ok(())
	}

	impl_benchmark_test_suite! {
		Pallet,
		mock::ExtBuilder::default().build_offchainify(10).0,
		mock::Runtime,
	}
}
