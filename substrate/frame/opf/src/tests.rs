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

//! Tests for OPF pallet.

pub use super::*;
use crate::mock::*;
use frame_support::assert_ok;
use frame_support::traits::OnIdle;

pub fn next_block() {
	System::set_block_number(
		<Test as pallet_distribution::Config>::BlockNumberProvider::current_block_number() + 1,
	);
	AllPalletsWithSystem::on_initialize(
		<Test as pallet_distribution::Config>::BlockNumberProvider::current_block_number(),
	);
	AllPalletsWithSystem::on_idle(
		<Test as pallet_distribution::Config>::BlockNumberProvider::current_block_number(),
		Weight::MAX,
	);
}

pub fn create_project_list() {
	const MAX_NUMBER: u64 = <Test as Config>::MaxWhitelistedProjects::get() as u64;
	let mut bounded_vec = BoundedVec::<u64, <Test as Config>::MaxWhitelistedProjects>::new();
	for i in 0..MAX_NUMBER {
		let _ = bounded_vec.try_push(i + 100);
	}
	WhiteListedProjectAccounts::<Test>::mutate(|value| {
		*value = bounded_vec;
	});
}

#[test]
fn first_round_creation_works() {
	new_test_ext().execute_with(|| {
		// Creating whitelisted projects list succeeds
		create_project_list();
		let project_list = WhiteListedProjectAccounts::<Test>::get();
		let max_number: u64 = <Test as Config>::MaxWhitelistedProjects::get() as u64;
		assert_eq!(project_list.len(), max_number as usize);

		// First round is created
		next_block();
		let voting_period = <Test as Config>::VotingPeriod::get();
		let voting_lock_period = <Test as Config>::VoteLockingPeriod::get();
		let now =
			<Test as pallet_distribution::Config>::BlockNumberProvider::current_block_number();

		let round_ending_block = now.saturating_add(voting_period.into());
		let voting_locked_block = round_ending_block.saturating_sub(voting_lock_period.into());

		let first_round_info: VotingRoundInfo<Test> = VotingRoundInfo {
			round_number: 0,
			round_starting_block: now,
			voting_locked_block,
			round_ending_block,
		};

		// The righ event was emitted
		expect_events(vec![RuntimeEvent::Opf(Event::VotingRoundStarted {
			when: now,
			round_number: 0,
		})]);

		// The storage infos are correct
		let round_info = VotingRounds::<Test>::get(0).unwrap();
		assert_eq!(first_round_info, round_info);
	})
}

#[test]
fn voting_action_works() {
	new_test_ext().execute_with(|| {
		create_project_list();
		next_block();

		// Bob nominate project_102 with an amount of 1000*BSX
		assert_ok!(Opf::vote(RawOrigin::Signed(BOB).into(), 102, 1000 * BSX, true,));

		// expected event is emitted
		let voting_period = <Test as Config>::VotingPeriod::get();
		let voting_lock_period = <Test as Config>::VoteLockingPeriod::get();
		let now =
			<Test as pallet_distribution::Config>::BlockNumberProvider::current_block_number();
		let round_ending_block = now.saturating_add(voting_period.into());
		let voting_locked_block = round_ending_block.saturating_sub(voting_lock_period.into());

		let first_round_info: VotingRoundInfo<Test> = VotingRoundInfo {
			round_number: 0,
			round_starting_block: now,
			voting_locked_block,
			round_ending_block,
		};

		expect_events(vec![RuntimeEvent::Opf(Event::VoteCasted {
			who: BOB,
			when: now,
			project_id: 102,
		})]);

		// The storage infos are correct
		let first_vote_info: VoteInfo<Test> =
			VoteInfo { amount: 1000 * BSX, round: first_round_info, is_fund: true };
		let vote_info = Votes::<Test>::get(102, BOB).unwrap();
		assert_eq!(first_vote_info, vote_info);

		// Voter's funds are locked
		let locked_balance =
			<<Test as pallet_distribution::Config>::NativeBalance as fungible::hold::Inspect<
				u64,
			>>::balance_on_hold(&pallet_distribution::HoldReason::FundsReserved.into(), &BOB);
		assert!(locked_balance > Zero::zero());
	})
}

#[test]
fn rewards_calculation_works() {
	new_test_ext().execute_with(|| {
		create_project_list();
		next_block();

		// Bob nominate project_101 with an amount of 1000*BSX
		assert_ok!(Opf::vote(RawOrigin::Signed(BOB).into(), 101, 1000 * BSX, true,));

		// Alice nominate project_101 with an amount of 5000*BSX
		assert_ok!(Opf::vote(RawOrigin::Signed(ALICE).into(), 101, 5000 * BSX, true,));

		// DAVE vote against project_102 with an amount of 3000*BSX
		assert_ok!(Opf::vote(RawOrigin::Signed(DAVE).into(), 102, 3000 * BSX, false,));
		// Eve nominate project_102 with an amount of 50000*BSX
		assert_ok!(Opf::vote(RawOrigin::Signed(BOB).into(), 102, 5000 * BSX, true,));

		let round_info = VotingRounds::<Test>::get(0).unwrap();
		let mut now =
			<Test as pallet_distribution::Config>::BlockNumberProvider::current_block_number();

		while now != round_info.voting_locked_block {
			next_block();
			now =
				<Test as pallet_distribution::Config>::BlockNumberProvider::current_block_number();
		}
		assert_eq!(now, round_info.voting_locked_block);

		// The right events are emitted
		expect_events(vec![RuntimeEvent::Opf(Event::VoteActionLocked {
			when: now,
			round_number: 0,
		})]);

		// The total amount locked through votes is 8000
		// Project 101: 6000 -> ~11.3%; Project 102: 2000 -> ~88.6%
		// Distributed to project 101 -> 75%*100_000; Distributed to project 102 -> 25%*100_000

		assert_eq!(pallet_distribution::Projects::<Test>::get().len() == 2, true);
		let rewards = pallet_distribution::Projects::<Test>::get();
		assert_eq!(rewards[0].project_account, 101);
		assert_eq!(rewards[1].project_account, 102);
		assert_eq!(rewards[0].amount > rewards[1].amount, true);
		assert_eq!(rewards[0].amount, 75000);
		assert_eq!(rewards[1].amount, 25000);
	})
}

#[test]
fn vote_removal_works() {
	new_test_ext().execute_with(|| {
		create_project_list();
		next_block();

		// Bob nominate project_102 with an amount of 1000
		assert_ok!(Opf::vote(RawOrigin::Signed(BOB).into(), 101, 1000, true));

		// Voter's funds are locked
		let locked_balance0 =
			<<Test as pallet_distribution::Config>::NativeBalance as fungible::hold::Inspect<
				u64,
			>>::balance_on_hold(&pallet_distribution::HoldReason::FundsReserved.into(), &BOB);
		// Vote is in storage and balance is locked
		assert!(locked_balance0 > Zero::zero());
		assert_eq!(Votes::<Test>::get(101, BOB).is_some(), true);

		// Bob removes his vote
		assert_ok!(Opf::remove_vote(RawOrigin::Signed(BOB).into(), 101,));

		let locked_balance1 =
			<<Test as pallet_distribution::Config>::NativeBalance as fungible::hold::Inspect<
				u64,
			>>::balance_on_hold(&pallet_distribution::HoldReason::FundsReserved.into(), &BOB);

		// No more votes in storage and balance is unlocked
		assert_eq!(Votes::<Test>::get(101, BOB).is_some(), false);
		assert_eq!(locked_balance1, Zero::zero());
	})
}

#[test]
fn vote_move_works() {
	new_test_ext().execute_with(|| {
		create_project_list();
		next_block();

		let now =
			<Test as pallet_distribution::Config>::BlockNumberProvider::current_block_number();

		// Bob nominate project_102 with an amount of 1000
		assert_ok!(Opf::vote(RawOrigin::Signed(BOB).into(), 101, 1000, true));

		expect_events(vec![RuntimeEvent::Opf(Event::VoteCasted {
			who: BOB,
			when: now,
			project_id: 101,
		})]);

		// Bob nominate project_103 with an amount of 5000
		assert_ok!(Opf::vote(RawOrigin::Signed(BOB).into(), 103, 5000, true));

		// Voter's funds are locked
		let locked_balance0 =
			<<Test as pallet_distribution::Config>::NativeBalance as fungible::hold::Inspect<
				u64,
			>>::balance_on_hold(&pallet_distribution::HoldReason::FundsReserved.into(), &BOB);
		assert!(locked_balance0 > Zero::zero());

		// Bob changes amount in project_103 to 4500
		assert_ok!(Opf::vote(RawOrigin::Signed(BOB).into(), 103, 4500, true));

		// Storage was correctly updated
		let vote_info = Votes::<Test>::get(103, BOB).unwrap();
		assert_eq!(4500, vote_info.amount);
	})
}
