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

use super::{AccountId, MaxVotesPerVoter, Runtime};
use crate::VoterOf;
use frame_election_provider_support::{
	data_provider, DataProviderBounds, ElectionDataProvider, PageIndex, VoteWeight,
};
use frame_support::pallet_prelude::*;
use sp_core::bounded_vec;
use sp_std::prelude::*;

pub type T = Runtime;

frame_support::parameter_types! {
	pub static Targets: Vec<AccountId> = vec![10, 20, 30, 40];
	pub static Voters: Vec<VoterOf<Runtime>> = vec![
		// page 2:
		(1, 10, bounded_vec![10, 20]),
		(2, 10, bounded_vec![30, 40]),
		(3, 10, bounded_vec![40]),
		(4, 10, bounded_vec![10, 20, 40]),
		// page 1:
		(5, 10, bounded_vec![10, 30, 40]),
		(6, 10, bounded_vec![20, 30, 40]),
		(7, 10, bounded_vec![20, 30]),
		(8, 10, bounded_vec![10]),
		// page 0: (self-votes)
		(10, 10, bounded_vec![10]),
		(20, 20, bounded_vec![20]),
		(30, 30, bounded_vec![30]),
		(40, 40, bounded_vec![40]),
	];
	pub static DesiredTargets: u32 = 2;
	pub static EpochLength: u64 = 30;

	pub static LastIteratedVoterIndex: Option<usize> = None;
}

pub struct MockStaking;
impl ElectionDataProvider for MockStaking {
	type AccountId = AccountId;
	type BlockNumber = u64;
	type MaxVotesPerVoter = MaxVotesPerVoter;

	fn electable_targets(
		bounds: DataProviderBounds,
		remaining: PageIndex,
	) -> data_provider::Result<Vec<AccountId>> {
		let targets = Targets::get();

		if remaining != 0 {
			crate::log!(
				warn,
				"requesting targets for non-zero page, we will return the same page in any case"
			);
		}
		if bounds.slice_exhausted(&targets) {
			return Err("Targets too big")
		}

		Ok(targets)
	}

	fn electing_voters(
		bounds: DataProviderBounds,
		remaining: PageIndex,
	) -> data_provider::Result<
		Vec<(AccountId, VoteWeight, BoundedVec<AccountId, Self::MaxVotesPerVoter>)>,
	> {
		let mut voters = Voters::get();

		// jump to the first non-iterated, if this is a follow up.
		if let Some(index) = LastIteratedVoterIndex::get() {
			voters = voters.iter().skip(index).cloned().collect::<Vec<_>>();
		}

		// take as many as you can.
		if let Some(max_len) = bounds.count.map(|c| c.0 as usize) {
			voters.truncate(max_len)
		}

		if voters.is_empty() {
			return Ok(vec![])
		}

		if remaining > 0 {
			let last = voters.last().cloned().unwrap();
			LastIteratedVoterIndex::set(Some(
				Voters::get().iter().position(|v| v == &last).map(|i| i + 1).unwrap(),
			));
		} else {
			LastIteratedVoterIndex::set(None)
		}

		Ok(voters)
	}

	fn desired_targets() -> data_provider::Result<u32> {
		Ok(DesiredTargets::get())
	}

	fn next_election_prediction(_: u64) -> u64 {
		unreachable!("not used in this pallet")
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn put_snapshot(
		voters: Vec<(AccountId, VoteWeight, BoundedVec<AccountId, MaxVotesPerVoter>)>,
		targets: Vec<AccountId>,
		_target_stake: Option<VoteWeight>,
	) {
		Targets::set(targets);
		Voters::set(voters);
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn clear() {
		Targets::set(vec![]);
		Voters::set(vec![]);
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn fetch_page(page: PageIndex) {
		use frame_election_provider_support::ElectionProvider;
		super::MultiBlock::elect(page).unwrap();
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn add_voter(
		voter: AccountId,
		weight: VoteWeight,
		targets: BoundedVec<AccountId, MaxVotesPerVoter>,
	) {
		let mut current = Voters::get();
		current.push((voter, weight, targets));
		Voters::set(current);
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn add_target(target: AccountId) {
		use super::ExistentialDeposit;

		let mut current = Targets::get();
		current.push(target);
		Targets::set(current);

		// to be on-par with staking, we add a self vote as well. the stake is really not that
		// important.
		let mut current = Voters::get();
		current.push((target, ExistentialDeposit::get() as u64, vec![target].try_into().unwrap()));
		Voters::set(current);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{bound_by_count, ExtBuilder};

	#[test]
	fn targets() {
		ExtBuilder::full().build_and_execute(|| {
			assert_eq!(Targets::get().len(), 4);

			// any non-zero page returns page zero.
			assert_eq!(MockStaking::electable_targets(bound_by_count(None), 2).unwrap().len(), 4);
			assert_eq!(MockStaking::electable_targets(bound_by_count(None), 1).unwrap().len(), 4);

			// 0 is also fine.
			assert_eq!(MockStaking::electable_targets(bound_by_count(None), 0).unwrap().len(), 4);

			// fetch less targets is error, because targets cannot be sorted (both by MockStaking,
			// and the real staking).
			assert!(MockStaking::electable_targets(bound_by_count(Some(2)), 0).is_err());

			// more targets is fine.
			assert!(MockStaking::electable_targets(bound_by_count(Some(4)), 0).is_ok());
			assert!(MockStaking::electable_targets(bound_by_count(Some(5)), 0).is_ok());
		});
	}

	#[test]
	fn multi_page_votes() {
		ExtBuilder::full().build_and_execute(|| {
			assert_eq!(MockStaking::electing_voters(bound_by_count(None), 0).unwrap().len(), 12);
			assert!(LastIteratedVoterIndex::get().is_none());

			assert_eq!(
				MockStaking::electing_voters(bound_by_count(Some(4)), 0)
					.unwrap()
					.into_iter()
					.map(|(x, _, _)| x)
					.collect::<Vec<_>>(),
				vec![1, 2, 3, 4],
			);
			assert!(LastIteratedVoterIndex::get().is_none());

			assert_eq!(
				MockStaking::electing_voters(bound_by_count(Some(4)), 2)
					.unwrap()
					.into_iter()
					.map(|(x, _, _)| x)
					.collect::<Vec<_>>(),
				vec![1, 2, 3, 4],
			);
			assert_eq!(LastIteratedVoterIndex::get().unwrap(), 4);

			assert_eq!(
				MockStaking::electing_voters(bound_by_count(Some(4)), 1)
					.unwrap()
					.into_iter()
					.map(|(x, _, _)| x)
					.collect::<Vec<_>>(),
				vec![5, 6, 7, 8],
			);
			assert_eq!(LastIteratedVoterIndex::get().unwrap(), 8);

			assert_eq!(
				MockStaking::electing_voters(bound_by_count(Some(4)), 0)
					.unwrap()
					.into_iter()
					.map(|(x, _, _)| x)
					.collect::<Vec<_>>(),
				vec![10, 20, 30, 40],
			);
			assert!(LastIteratedVoterIndex::get().is_none());
		})
	}
}
