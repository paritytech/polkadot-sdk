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

//! OPF pallet benchmarking.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use alloc::vec;
use frame_benchmarking::v2::*;
use frame_support::{
	assert_ok,
	traits::{
		fungible::{Inspect, Mutate},
		EnsureOrigin, Get, OnPoll,
	},
	weights::WeightMeter,
	BoundedVec,
};
use frame_system::RawOrigin;
use pallet_conviction_voting::{AccountVote, Conviction, Vote};

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn register_project() {
		let info = ProjectInfo {
			owner: account("the_owner", 0, 0),
			fund_dest: account("fund_dest", 0, 0),
			name: BoundedVec::truncate_from(vec![1u8; 4]),
			description: BoundedVec::truncate_from(vec![2u8; 4]),
		};
		let admin = T::AdminOrigin::try_successful_origin().unwrap();

		#[extrinsic_call]
		_(admin, info.clone());

		// Assert the project is stored at index 0
		let stored = crate::Projects::<T>::get(0).unwrap();
		assert_eq!(stored.owner, info.owner);
		assert_eq!(stored.fund_dest, info.fund_dest);
		assert_eq!(stored.name, info.name);
		assert_eq!(stored.description, info.description);
	}

	#[benchmark]
	fn manage_project_info() {
		let info = ProjectInfo {
			owner: account("the_owner", 0, 0),
			fund_dest: account("fund_dest", 0, 0),
			name: BoundedVec::truncate_from(vec![1u8; 4]),
			description: BoundedVec::truncate_from(vec![2u8; 4]),
		};
		let admin = T::AdminOrigin::try_successful_origin().unwrap();
		Pallet::<T>::register_project(admin, info.clone()).unwrap();
		let index = 0u32;

		// Prepare new info (change all fields except owner)
		let new_info = ProjectInfo {
			owner: account("the_owner", 0, 0),
			fund_dest: account("new_dest", 0, 0),
			name: BoundedVec::truncate_from(vec![9u8; 4]),
			description: BoundedVec::truncate_from(vec![8u8; 4]),
		};

		#[extrinsic_call]
		_(RawOrigin::Signed(info.owner.clone()), index, new_info.clone());

		// Assert the project info is updated
		let stored = crate::Projects::<T>::get(index).unwrap();
		assert_eq!(stored.fund_dest, new_info.fund_dest);
		assert_eq!(stored.name, new_info.name);
		assert_eq!(stored.description, new_info.description);
	}

	#[benchmark]
	fn unregister_project() {
		let info = ProjectInfo {
			owner: account("the_owner", 0, 0),
			fund_dest: account("fund_dest", 0, 0),
			name: BoundedVec::truncate_from(vec![1u8; 4]),
			description: BoundedVec::truncate_from(vec![2u8; 4]),
		};
		let admin = T::AdminOrigin::try_successful_origin().unwrap();
		Pallet::<T>::register_project(admin.clone(), info.clone()).unwrap();
		let index = 0u32;

		#[extrinsic_call]
		_(admin, index);

		// Assert the project is removed
		assert!(crate::Projects::<T>::get(index).is_none());
	}

	#[benchmark]
	fn remove_automatic_forwarding() {
		let info = ProjectInfo {
			owner: account("the_owner", 0, 0),
			fund_dest: account("fund_dest", 0, 0),
			name: BoundedVec::truncate_from(vec![1u8; 4]),
			description: BoundedVec::truncate_from(vec![2u8; 4]),
		};
		let admin = T::AdminOrigin::try_successful_origin().unwrap();
		Pallet::<T>::register_project(admin.clone(), info.clone()).unwrap();
		let index = 0u32;

		// Simulate a vote so VotesToForward entry exists
		crate::VotesToForward::<T>::insert(
			index,
			&info.owner,
			crate::VoteInSession {
				round: 1,
				vote: AccountVote::Standard {
					vote: Vote { aye: true, conviction: Conviction::Locked1x },
					balance: 1u32.into(),
				},
			},
		);
		assert!(crate::VotesToForward::<T>::contains_key(index, &info.owner));

		#[extrinsic_call]
		_(RawOrigin::Signed(info.owner.clone()), index);

		// Assert the entry is removed
		assert!(!crate::VotesToForward::<T>::contains_key(index, &info.owner));
	}

	#[benchmark]
	fn on_poll_base() {
		// Setup: register a project and start a round, but do not end it and do not add votes
		let info = ProjectInfo {
			owner: account("the_owner", 0, 0),
			fund_dest: account("fund_dest", 0, 0),
			name: BoundedVec::truncate_from(vec![1u8; 4]),
			description: BoundedVec::truncate_from(vec![2u8; 4]),
		};
		let admin = T::AdminOrigin::try_successful_origin().unwrap();
		Pallet::<T>::register_project(admin, info.clone()).unwrap();
		Pallet::<T>::on_poll(1u32.into(), &mut WeightMeter::new());

		// No votes, round not ended
		#[block]
		{
			Pallet::<T>::on_poll(2u32.into(), &mut WeightMeter::new());
		}
		// Optionally: assert round is still ongoing
		assert!(crate::Round::<T>::get().is_some());
	}

	#[benchmark]
	fn on_poll_end_round() {
		let max = T::MaxProjects::get();
		let admin = T::AdminOrigin::try_successful_origin().unwrap();
		// Register MaxProjects projects
		for i in 0..max {
			let info = ProjectInfo {
				owner: account("owner", i, 0),
				fund_dest: account("fund_dest", i, 0),
				name: BoundedVec::truncate_from(vec![1u8; 4]),
				description: BoundedVec::truncate_from(vec![2u8; 4]),
			};
			Pallet::<T>::register_project(admin.clone(), info).unwrap();
		}
		// Start the round by calling on_poll at block 1
		Pallet::<T>::on_poll(1u32.into(), &mut WeightMeter::new());
		// Each project gets a positive vote
		for i in 0..max {
			let voter = account("voter", i, 0);
			// Mint funds to the voter to avoid insufficient funds
			let m = T::Fungible::minimum_balance() * 100u32.into();
			T::Fungible::mint_into(&voter, m).unwrap();
			let poll = PollIndex::new(0, i);
			let vote = AccountVote::Standard {
				vote: Vote { aye: true, conviction: Conviction::Locked1x },
				balance: 1u32.into(),
			};
			assert_ok!(pallet_conviction_voting::Pallet::<T, T::ConvictionVotingInstance>::vote(
				RawOrigin::Signed(voter).into(),
				poll,
				vote,
			));
		}
		// End the round (trigger reward distribution and all hooks)
		#[block]
		{
			Pallet::<T>::on_poll_end_round();
		}
		// Optionally: assert round is ended
		assert!(crate::Round::<T>::get().is_none());
	}

	#[benchmark]
	fn on_poll_new_round() {
		let max = T::MaxProjects::get();
		let admin = T::AdminOrigin::try_successful_origin().unwrap();
		// Register MaxProjects projects
		for i in 0..max {
			let info = ProjectInfo {
				owner: account("owner", i, 0),
				fund_dest: account("fund_dest", i, 0),
				name: BoundedVec::truncate_from(vec![1u8; 4]),
				description: BoundedVec::truncate_from(vec![2u8; 4]),
			};
			Pallet::<T>::register_project(admin.clone(), info).unwrap();
		}
		// Call on_poll_new_round to start a new round
		#[block]
		{
			Pallet::<T>::on_poll_new_round();
		}
		// Assert round is started
		assert!(crate::Round::<T>::get().is_some());
	}

	#[benchmark]
	fn on_poll_on_idle_forward_votes(n: Linear<1, 10000>) {
		let admin = T::AdminOrigin::try_successful_origin().unwrap();
		// Register a single project
		let info = ProjectInfo {
			owner: account("owner", 0, 0),
			fund_dest: account("fund_dest", 0, 0),
			name: BoundedVec::truncate_from(vec![1u8; 4]),
			description: BoundedVec::truncate_from(vec![2u8; 4]),
		};
		Pallet::<T>::register_project(admin, info).unwrap();

		// Start the 2nd round
		let round_duration: u32 = T::RoundDuration::get()
			.try_into()
			.unwrap_or_else(|_| panic!("Round duration too large for u32"));
		for i in 0..round_duration + 2 {
			frame_system::Pallet::<T>::set_block_number(i.into());
			Pallet::<T>::on_poll(i.into(), &mut WeightMeter::new());
		}
		assert_eq!(NextRoundIndex::<T>::get(), 2);

		// Insert n votes to forward for project 0
		for i in 0..n {
			let voter = account("voter", i, 0);
			let m = T::Fungible::minimum_balance() * 100u32.into();
			T::Fungible::mint_into(&voter, m).unwrap();
			let vote = AccountVote::Standard {
				vote: Vote { aye: true, conviction: Conviction::Locked1x },
				balance: 1u32.into(),
			};
			crate::VotesToForward::<T>::insert(0, &voter, crate::VoteInSession { round: 0, vote });
		}
		VotesForwardingState::<T>::put(VotesForwardingInfo {
			forwarding: ForwardingProcess::Start,
			reset_round: None,
		});

		// Forward the votes
		#[block]
		{
			Pallet::<T>::on_poll_on_idle_forward_votes(&mut WeightMeter::new());
		}

		// Assert the votes are forwarded
		let PollInfo::Ongoing(tally, _) = Polls::<T>::get(1, 0).unwrap() else {
			unreachable!("poll for round 1, project 0 should be ongoing");
		};
		assert_eq!(tally.support, n.into());
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::ExtBuilder::build(), crate::mock::Test);
}
