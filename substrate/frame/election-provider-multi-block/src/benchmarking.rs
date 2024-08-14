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

//! # Benchmarking for the Elections Multiblock pallet and sub-pallets.

use super::*;
use crate::Snapshot;

use frame_benchmarking::v2::*;
use frame_support::{assert_ok, traits::OnInitialize};

/// Seed to generate account IDs.
const SEED: u32 = 999;
/// Default page to use in the benchmarking.
const PAGE: u32 = 0;
/// Minimum number of voters in the data provider and per snapshot page.
const MIN_VOTERS: u32 = 10;

#[benchmarks(
    where T: ConfigCore + ConfigSigned + ConfigVerifier,
)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn create_targets_snapshot_paged(
		t: Linear<
			{ T::BenchmarkingConfig::TARGETS_PER_PAGE[0] },
			{ T::BenchmarkingConfig::TARGETS_PER_PAGE[1] },
		>,
	) -> Result<(), BenchmarkError> {
		helpers::setup_data_provider::<T>(
			T::BenchmarkingConfig::VOTERS,
			T::BenchmarkingConfig::TARGETS.max(t),
		);

		assert!(Snapshot::<T>::targets().is_none());

		#[block]
		{
			assert_ok!(PalletCore::<T>::create_targets_snapshot_inner(t));
		}

		assert!(Snapshot::<T>::targets().is_some());

		Ok(())
	}

	#[benchmark]
	fn create_voters_snapshot_paged(
		v: Linear<
			{ T::BenchmarkingConfig::VOTERS_PER_PAGE[0] },
			{ T::BenchmarkingConfig::VOTERS_PER_PAGE[1] },
		>,
	) -> Result<(), BenchmarkError> {
		helpers::setup_data_provider::<T>(
			T::BenchmarkingConfig::VOTERS.max(v),
			T::BenchmarkingConfig::TARGETS,
		);

		assert!(Snapshot::<T>::voters(PAGE).is_none());

		#[block]
		{
			assert_ok!(PalletCore::<T>::create_voters_snapshot_inner(PAGE, v));
		}

		assert!(Snapshot::<T>::voters(PAGE).is_some());

		Ok(())
	}

	#[benchmark]
	fn on_initialize_start_signed() -> Result<(), BenchmarkError> {
		assert!(PalletCore::<T>::current_phase() == Phase::Off);

		#[block]
		{
			let _ = PalletCore::<T>::start_signed_phase();
		}

		assert!(PalletCore::<T>::current_phase() == Phase::Signed);

		Ok(())
	}

	#[benchmark]
	fn on_phase_transition() -> Result<(), BenchmarkError> {
		assert!(PalletCore::<T>::current_phase() == Phase::Off);

		#[block]
		{
			let _ = PalletCore::<T>::phase_transition(Phase::Snapshot(0));
		}

		assert!(PalletCore::<T>::current_phase() == Phase::Snapshot(0));

		Ok(())
	}

	#[benchmark]
	fn on_initialize_start_export() -> Result<(), BenchmarkError> {
		#[block]
		{
			let _ = PalletCore::<T>::do_export_phase(1u32.into(), 100u32.into());
		}

		Ok(())
	}

	#[benchmark]
	fn on_initialize_do_nothing() -> Result<(), BenchmarkError> {
		assert!(PalletCore::<T>::current_phase() == Phase::Off);

		#[block]
		{
			let _ = PalletCore::<T>::on_initialize(0u32.into());
		}

		assert!(PalletCore::<T>::current_phase() == Phase::Off);

		Ok(())
	}

	impl_benchmark_test_suite!(
		PalletCore,
		crate::mock::ExtBuilder::default(),
		crate::mock::Runtime,
		exec_name = build_and_execute
	);
}

/// Helper fns to use across the benchmarking of the core pallet and its sub-pallets.
pub(crate) mod helpers {
	use super::*;
	use crate::{signed::pallet::Submissions, unsigned::miner::Miner, SolutionOf};
	use frame_election_provider_support::ElectionDataProvider;
	use frame_support::traits::tokens::Precision;
	use sp_std::vec::Vec;

	pub(crate) fn setup_funded_account<T: ConfigSigned>(
		domain: &'static str,
		id: u32,
		balance_factor: u32,
	) -> T::AccountId {
		use frame_support::traits::fungible::{Balanced, Inspect};

		let account = frame_benchmarking::account::<T::AccountId>(domain, id, SEED);
		let funds = (T::Currency::minimum_balance() + 1u32.into()) * balance_factor.into();
		// increase issuance to ensure a sane voter weight.
		let _ = T::Currency::deposit(&account, funds.into(), Precision::Exact);

		account
	}

	/// Generates and adds `v` voters and `t` targets in the data provider stores. The voters
	/// nominate `DataProvider::MaxVotesPerVoter` targets.
	pub(crate) fn setup_data_provider<T: ConfigCore + ConfigSigned>(v: u32, t: u32) {
		<T as Config>::DataProvider::clear();

		log!(info, "setup_data_provider with v: {}, t: {}", v, t,);

		// generate and add targets.
		let mut targets = (0..t)
			.map(|i| {
				let target = setup_funded_account::<T>("Target", i, 200);
				<T as Config>::DataProvider::add_target(target.clone());
				target
			})
			.collect::<Vec<_>>();

		// we should always have enough voters to fill.
		assert!(
			targets.len() >
				<<T as Config>::DataProvider as ElectionDataProvider>::MaxVotesPerVoter::get()
					as usize
		);

		targets.truncate(
			<<T as Config>::DataProvider as ElectionDataProvider>::MaxVotesPerVoter::get() as usize,
		);

		// generate and add voters with `MaxVotesPerVoter` nominations from the list of targets.
		(0..v.max(MIN_VOTERS)).for_each(|i| {
			let voter = setup_funded_account::<T>("Voter", i, 2_000);
			<<T as Config>::DataProvider as ElectionDataProvider>::add_voter(
				voter,
				1_000,
				targets.clone().try_into().unwrap(),
			);
		});
	}

	/// Generates the full paged snapshot for both targets and voters.
	pub(crate) fn setup_snapshot<T: ConfigCore>(v: u32, t: u32) -> Result<(), &'static str> {
		// set desired targets to match the size off the target page.
		<T::DataProvider as ElectionDataProvider>::set_desired_targets(t);

		log!(
            info,
            "setting up the snapshot. voters/page: {:?}, targets/page: {:?} (desired_targets: {:?})",
            v,
            t,
            <T::DataProvider as ElectionDataProvider>::desired_targets(),
        );

		let _ = PalletCore::<T>::create_targets_snapshot_inner(t)
			.map_err(|_| "error creating the target snapshot, most likely `T::TargetSnapshotPerBlock` config needs to be adjusted")?;

		for page in 0..T::Pages::get() {
			let _ = PalletCore::<T>::create_voters_snapshot_inner(page, v)
			    .map_err(|_| "error creating the voter snapshot, most likely `T::VoterSnapshotPerBlock` config needs to be adjusted")?;
		}

		Ok(())
	}

	/// Mines a full solution for the current snapshot and submits `maybe_page`. Otherwise submits
	/// all pages. `valid` defines whether the solution is valid or not.
	pub(crate) fn mine_and_submit<
		T: ConfigCore + ConfigUnsigned + ConfigSigned + ConfigVerifier,
	>(
		maybe_page: Option<u32>,
		valid: bool,
	) -> Result<T::AccountId, &'static str> {
		// ensure that the number of desired targets fits within the bound of max winners per page,
		// otherwise preemptively since the feasibilty check will fail.
		ensure!(
			<T::DataProvider as ElectionDataProvider>::desired_targets()
				.expect("desired_targets is set") <=
				<T as ConfigVerifier>::MaxWinnersPerPage::get(),
			"`MaxWinnersPerPage` must be equal or higher than desired_targets. fix the configs.",
		);

		let submitter = setup_funded_account::<T>("Submitter", 1, 1_000);

		let (mut solution, _) =
			Miner::<T, T::OffchainSolver>::mine_paged_solution(T::Pages::get(), true).map_err(
				|e| {
					log!(info, "ERR:: {:?}", e);
					"error mining solution"
				},
			)?;

		// if the submission is full and the fesibility check must fail, mess up with the solution's
		// claimed score to fail the verification (worst case scenario in terms of async solution
		// verification).
		let claimed_score = if maybe_page.is_none() && !valid {
			solution.score.sum_stake += 1_000_000;
			solution.score
		} else {
			solution.score
		};

		// set transition to phase to ensure the page mutation works.
		PalletCore::<T>::phase_transition(Phase::Signed);

		// first register submission.
		PalletSigned::<T>::do_register(&submitter, claimed_score, PalletCore::<T>::current_round())
			.map_err(|_| "error registering solution")?;

		for page in maybe_page
			.map(|p| sp_std::vec![p])
			.unwrap_or((0..T::Pages::get()).rev().collect::<Vec<_>>())
			.into_iter()
		{
			let paged_solution =
				solution.solution_pages.get(page as usize).ok_or("page out of bounds")?;

			// if it is a single page submission and submission should be invalid, make the paged
			// paged submission invalid by tweaking the current snapshot.
			if maybe_page.is_some() && !valid {
				ensure_solution_invalid::<T>(&paged_solution)?;
			}

			// process and submit only onle page.
			Submissions::<T>::try_mutate_page(
				&submitter,
				PalletCore::<T>::current_round(),
				page,
				Some(paged_solution.clone()),
			)
			.map_err(|_| "error storing page")?;
		}

		Ok(submitter)
	}

	/// ensures `solution` will be considered invalid in the feasibility check by tweaking the
	/// snapshot which was used to compute the solution and remove one of the targets from the
	/// snapshot. NOTE: we expect that the `solution` was generated based on the current snapshot
	/// state.
	fn ensure_solution_invalid<T: ConfigCore>(
		solution: &SolutionOf<T>,
	) -> Result<(), &'static str> {
		let new_count_targets = solution.unique_targets().len().saturating_sub(1);

		// remove a target from the snapshot to invalidate solution.
		let _ = PalletCore::<T>::create_targets_snapshot_inner(new_count_targets as u32)
			.map_err(|_| "error regenerating the target snapshot")?;

		Ok(())
	}
}
