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

use sp_core::parameter_types;
use sp_runtime::bounded_vec;

use crate::{Encode, VoterOf};
use frame_election_provider_support::{
	bounds::{CountBound, SizeBound},
	data_provider, DataProviderBounds, ElectionDataProvider, PageIndex, VoterOf as VoterOfProvider,
};

use pallet_staking::election_size_tracker::StaticTracker;

use super::{AccountId, BlockNumber, MaxVotesPerVoter, T};

frame_support::parameter_types! {
	pub static Targets: Vec<AccountId> = vec![10, 20, 30, 40, 50, 60, 70, 80];
	pub static Voters: Vec<VoterOf<T>> = vec![
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
		(50, 50, bounded_vec![50]),
		(60, 60, bounded_vec![60]),
		(70, 70, bounded_vec![70]),
		(80, 80, bounded_vec![80]),
	];
	pub static EpochLength: u64 = 30;
	pub static DesiredTargets: u32 = 2;

	pub static LastIteratedTargetIndex: Option<usize> = None;
	pub static LastIteratedVoterIndex: Option<usize> = None;
}

pub struct MockStaking;
impl ElectionDataProvider for MockStaking {
	type AccountId = AccountId;
	type BlockNumber = BlockNumber;
	type MaxVotesPerVoter = MaxVotesPerVoter;

	fn electable_targets(
		bounds: DataProviderBounds,
		remaining: PageIndex,
	) -> data_provider::Result<Vec<Self::AccountId>> {
		let mut targets = Targets::get();

		// drop previously processed targets.
		if let Some(last_index) = LastIteratedTargetIndex::get() {
			targets = targets.iter().skip(last_index).cloned().collect::<Vec<_>>();
		}

		// take as many targets as requested.
		if let Some(max_len) = bounds.count {
			targets.truncate(max_len.0 as usize);
		}

		assert!(!bounds.exhausted(None, CountBound(targets.len() as u32).into(),));

		// update the last iterater target index accordingly.
		if remaining > 0 {
			let last = targets.last().cloned().unwrap();
			LastIteratedTargetIndex::set(Some(
				Targets::get().iter().position(|v| v == &last).map(|i| i + 1).unwrap(),
			));
		} else {
			LastIteratedTargetIndex::set(None);
		}

		Ok(targets)
	}

	fn electing_voters(
		bounds: DataProviderBounds,
		remaining: PageIndex,
	) -> data_provider::Result<Vec<VoterOfProvider<Self>>> {
		// drop previously processed voters.
		let to_process = if let Some(last_index) = LastIteratedVoterIndex::get() {
			Voters::get().iter().skip(last_index).cloned().collect::<Vec<_>>()
		} else {
			Voters::get()
		};

		let final_predicted_len = bounds
			.count
			.unwrap_or((to_process.len() as u32).into())
			.min((to_process.len() as u32).into())
			.0;

		let mut voters_size_tracker: StaticTracker<MockStaking> = StaticTracker::default();
		let mut voters = vec![];

		let mut to_process = to_process.iter();

		while (voters.len() as u32) < final_predicted_len {
			let voter = match to_process.next() {
				// if voter exhausts the bounds, return.
				Some(voter) => {
					if voters_size_tracker.try_register_voter(&voter, &bounds).is_err() {
						break
					}
					voter
				},
				None => break,
			};

			voters.push(voter.clone());
		}

		// update the last iterated voter index accordingly.
		if remaining > 0 {
			let last = voters.last().cloned().unwrap();
			LastIteratedVoterIndex::set(Some(
				Voters::get().iter().position(|v| v == &last).map(|i| i + 1).unwrap(),
			));
		} else {
			LastIteratedVoterIndex::set(None);
		}

		Ok(voters)
	}

	fn desired_targets() -> data_provider::Result<u32> {
		Ok(DesiredTargets::get())
	}

	fn next_election_prediction(now: Self::BlockNumber) -> Self::BlockNumber {
		now + EpochLength::get() - now % EpochLength::get()
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::mock::{ExtBuilder, Pages};

	use codec::{Decode, Encode};

	#[test]
	fn multi_page_targets() {
		ExtBuilder::default().build_and_execute(|| {
			// no bounds.
			let targets =
				<MockStaking as ElectionDataProvider>::electable_targets(Default::default(), 0);
			assert_eq!(targets.unwrap().len(), 8);
			assert_eq!(LastIteratedTargetIndex::get(), None);

			// 2 targets per page.
			let bounds: DataProviderBounds =
				DataProviderBounds { count: Some(2.into()), size: None };

			let mut all_targets = vec![];
			for page in (0..(Pages::get())).rev() {
				let mut targets =
					<MockStaking as ElectionDataProvider>::electable_targets(bounds, page).unwrap();
				assert_eq!(targets.len(), bounds.count.unwrap().0 as usize);

				all_targets.append(&mut targets);
			}

			assert_eq!(all_targets, vec![10, 20, 30, 40, 50, 60]);
			assert_eq!(LastIteratedTargetIndex::get(), None);
		})
	}

	#[test]
	fn multi_page_voters() {
		ExtBuilder::default().build_and_execute(|| {
			// no bounds.
			let voters =
				<MockStaking as ElectionDataProvider>::electing_voters(Default::default(), 0);
			assert_eq!(voters.unwrap().len(), 16);
			assert_eq!(LastIteratedVoterIndex::get(), None);

			// 2 voters per page.
			let bounds: DataProviderBounds =
				DataProviderBounds { count: Some(2.into()), size: None };

			let mut all_voters = vec![];
			for page in (0..(Pages::get())).rev() {
				let mut voters =
					<MockStaking as ElectionDataProvider>::electing_voters(bounds, page).unwrap();

				assert_eq!(voters.len(), bounds.count.unwrap().0 as usize);

				all_voters.append(&mut voters);
			}

			let mut expected_voters = Voters::get();
			expected_voters.truncate(6);

			assert_eq!(all_voters, expected_voters);
			assert_eq!(LastIteratedVoterIndex::get(), None);

			// bound based on the *encoded size* of the voters, per page.
			let bounds: DataProviderBounds =
				DataProviderBounds { count: None, size: Some(100.into()) };

			let mut all_voters = vec![];
			for page in (0..(Pages::get())).rev() {
				let mut voters =
					<MockStaking as ElectionDataProvider>::electing_voters(bounds, page).unwrap();

				all_voters.append(&mut voters);
			}

			let mut expected_voters = Voters::get();
			expected_voters.truncate(all_voters.len());

			assert_eq!(all_voters, expected_voters);
			assert_eq!(LastIteratedVoterIndex::get(), None);
		})
	}
}
