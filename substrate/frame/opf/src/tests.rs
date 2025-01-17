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
use frame_support::{assert_noop, assert_ok, traits::OnIdle};

pub fn next_block() {
	System::set_block_number(<Test as Config>::BlockNumberProvider::current_block_number() + 1);
	AllPalletsWithSystem::on_initialize(
		<Test as Config>::BlockNumberProvider::current_block_number(),
	);
	AllPalletsWithSystem::on_idle(
		<Test as Config>::BlockNumberProvider::current_block_number(),
		Weight::MAX,
	);
}

pub fn project_list() -> Vec<ProjectId<Test>> {
	vec![101, 102, 103]
}

pub fn run_to_block(n: BlockNumberFor<Test>) {
	while <Test as Config>::BlockNumberProvider::current_block_number() < n {
		if <Test as Config>::BlockNumberProvider::current_block_number() > 1 {
			AllPalletsWithSystem::on_finalize(
				<Test as Config>::BlockNumberProvider::current_block_number(),
			);
		}
		next_block();
	}
}

#[test]
fn first_round_creation_works() {
	new_test_ext().execute_with(|| {
		let _batch = project_list();

		// First round is created
		next_block();
		let voting_period = <Test as Config>::VotingPeriod::get();
		let now = <Test as Config>::BlockNumberProvider::current_block_number();

		let round_ending_block = now.saturating_add(voting_period.into());

		let first_round_info: VotingRoundInfo<Test> = VotingRoundInfo {
			round_number: 0,
			round_starting_block: now,
			round_ending_block,
			total_positive_votes_amount: 0,
			total_negative_votes_amount: 0,
			batch_submitted: false,
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
fn project_registration_works() {
	new_test_ext().execute_with(|| {
		let batch = project_list();
		next_block();
		let mut round_info = VotingRounds::<Test>::get(0).unwrap();
		assert_eq!(round_info.batch_submitted, false);
		assert_ok!(Opf::register_projects_batch(RuntimeOrigin::signed(EVE), batch));
		let project_list = WhiteListedProjectAccounts::<Test>::get(101);
		assert!(project_list.is_some());
		// we should have 3 referendum started
		assert_eq!(pallet_democracy::PublicProps::<Test>::get().len(), 3);
		assert_eq!(pallet_democracy::ReferendumCount::<Test>::get(), 3);
		// The storage infos are correct
		round_info = VotingRounds::<Test>::get(0).unwrap();
		assert_eq!(round_info.batch_submitted, true);
	})
}

#[test]
fn conviction_vote_works() {
	new_test_ext().execute_with(|| {
		next_block();
		let batch = project_list();
		let voting_period = <Test as Config>::VotingPeriod::get();
		let vote_validity = <Test as Config>::VoteValidityPeriod::get();
		let now = <Test as Config>::BlockNumberProvider::current_block_number();
		//round_end_block
		let round_end = now.saturating_add(voting_period);

		assert_ok!(Opf::register_projects_batch(RuntimeOrigin::signed(EVE), batch));
		// Bob vote for project_101
		assert_ok!(Opf::vote(
			RuntimeOrigin::signed(BOB),
			101,
			100,
			true,
			pallet_democracy::Conviction::Locked1x
		));
		// Dave vote for project_102
		assert_ok!(Opf::vote(
			RuntimeOrigin::signed(DAVE),
			102,
			100,
			true,
			pallet_democracy::Conviction::Locked2x
		));
		//Round number is 0
		let round_number = NextVotingRoundNumber::<Test>::get().saturating_sub(1);
		assert_eq!(round_number, 0);

		//Bobs funds are locked
		let bob_hold = <Test as Config>::NativeBalance::total_balance_on_hold(&BOB);
		let dave_hold = <Test as Config>::NativeBalance::total_balance_on_hold(&DAVE);
		assert_eq!(bob_hold, 100);
		assert_eq!(dave_hold, 100);
		let round_number = NextVotingRoundNumber::<Test>::get().saturating_sub(1);
		assert_eq!(round_number, 0);

		let bob_vote_unlock = round_end.saturating_add(vote_validity);
		let dave_vote_unlock = bob_vote_unlock.clone().saturating_add(vote_validity);

		let bob_vote_info = Votes::<Test>::get(101, BOB).unwrap();
		let dave_vote_info = Votes::<Test>::get(102, DAVE).unwrap();

		assert_eq!(bob_vote_info.funds_unlock_block, bob_vote_unlock);
		assert_eq!(dave_vote_info.funds_unlock_block, dave_vote_unlock);
	})
}

#[test]
fn rewards_calculation_works() {
	new_test_ext().execute_with(|| {
		let batch = project_list();
		let voting_period = <Test as Config>::VotingPeriod::get();
		let now = <Test as Config>::BlockNumberProvider::current_block_number();
		//round_end_block
		let round_end = now.saturating_add(voting_period);

		assert_ok!(Opf::register_projects_batch(RuntimeOrigin::signed(EVE), batch));

		// Bob nominate project_101 with an amount of 1000*BSX with a conviction x2 => equivalent to
		// 3000*BSX locked
		assert_ok!(Opf::vote(
			RawOrigin::Signed(BOB).into(),
			101,
			1000 * BSX,
			true,
			pallet_democracy::Conviction::Locked2x
		));

		// Alice nominate project_101 with an amount of 5000*BSX with conviction 1x => equivalent to
		// 10000*BSX locked
		assert_ok!(Opf::vote(
			RawOrigin::Signed(ALICE).into(),
			101,
			5000 * BSX,
			true,
			pallet_democracy::Conviction::Locked1x
		));

		// DAVE vote against project_102 with an amount of 3000*BSX with conviction 1x => equivalent
		// to 6000*BSX locked
		assert_ok!(Opf::vote(
			RawOrigin::Signed(DAVE).into(),
			102,
			3000 * BSX,
			false,
			pallet_democracy::Conviction::Locked1x
		));
		// Eve nominate project_102 with an amount of 5000*BSX with conviction 1x => equivalent to
		// 10000*BSX locked
		assert_ok!(Opf::vote(
			RawOrigin::Signed(EVE).into(),
			102,
			5000 * BSX,
			true,
			pallet_democracy::Conviction::Locked1x
		));

		let round_info = VotingRounds::<Test>::get(0).unwrap();

		run_to_block(round_end);
		let now = <Test as Config>::BlockNumberProvider::current_block_number();

		assert_eq!(now, round_info.round_ending_block);

		// The right events are emitted
		expect_events(vec![RuntimeEvent::Opf(Event::VotingRoundEnded {
			when: now,
			round_number: 0,
		})]);

		// The total equivalent amount voted is 17000 = 23000 - 6000
		// Project 101: 13000 -> ~76.5%; Project 102: 4000 -> ~23.5%
		// Distributed to project 101 -> 76%*100_000; Distributed to project 102 -> 23%*100_000
		//Opf::calculate_rewards(<Test as Config>::TemporaryRewards::get());

		
		let reward_101 = WhiteListedProjectAccounts::<Test>::get(101).unwrap().amount;
		let reward_102 = WhiteListedProjectAccounts::<Test>::get(102).unwrap().amount;
		assert_eq!(reward_101, 76000);
		assert_eq!(reward_102, 23000);

		next_block();
		next_block();
		expect_events(vec![RuntimeEvent::Opf(Event::ProjectFundingAccepted {
			project_id: 102,
			amount: reward_102,
		})]);
		expect_events(vec![RuntimeEvent::Opf(Event::ProjectFundingAccepted {
			project_id: 101,
			amount: reward_101,
		})]);

	})
}
