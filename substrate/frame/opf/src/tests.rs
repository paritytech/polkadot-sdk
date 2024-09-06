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

pub fn run_to_block(n: BlockNumberFor<Test>) {
	while <Test as pallet_distribution::Config>::BlockNumberProvider::current_block_number() < n {
		if <Test as pallet_distribution::Config>::BlockNumberProvider::current_block_number() > 1 {
			AllPalletsWithSystem::on_finalize(
				<Test as pallet_distribution::Config>::BlockNumberProvider::current_block_number(),
			);
		}
		next_block();
	}
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
			total_positive_votes_amount: 0,
			total_negative_votes_amount: 0,
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
		assert_ok!(Opf::vote(
			RawOrigin::Signed(BOB).into(),
			102,
			1000 * BSX,
			true,
			Conviction::Locked1x
		));

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
			total_positive_votes_amount: 1000 * 2 * BSX,
			total_negative_votes_amount: 0,
		};

		expect_events(vec![RuntimeEvent::Opf(Event::VoteCasted {
			who: BOB,
			when: now,
			project_id: 102,
		})]);

		let funds_unlock_block = round_ending_block.saturating_add(voting_lock_period.into());
		// The storage infos are correct
		let first_vote_info: VoteInfo<Test> = VoteInfo {
			amount: 1000 * BSX,
			round: first_round_info,
			is_fund: true,
			conviction: Conviction::Locked1x,
			funds_unlock_block,
		};
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

		// Bob nominate project_101 with an amount of 1000*BSX with a conviction x2 => equivalent to
		// 3000*BSX locked
		assert_ok!(Opf::vote(
			RawOrigin::Signed(BOB).into(),
			101,
			1000 * BSX,
			true,
			Conviction::Locked2x
		));
		let mut p1 = ProjectFunds::<Test>::get(101);
		println!("the reward is: {:?}", p1);

		// Alice nominate project_101 with an amount of 5000*BSX with conviction 1x => equivalent to
		// 10000*BSX locked
		assert_ok!(Opf::vote(
			RawOrigin::Signed(ALICE).into(),
			101,
			5000 * BSX,
			true,
			Conviction::Locked1x
		));
		p1 = ProjectFunds::<Test>::get(101);
		println!("the reward is: {:?}", p1);

		// DAVE vote against project_102 with an amount of 3000*BSX with conviction 1x => equivalent
		// to 6000*BSX locked
		assert_ok!(Opf::vote(
			RawOrigin::Signed(DAVE).into(),
			102,
			3000 * BSX,
			false,
			Conviction::Locked1x
		));
		// Eve nominate project_102 with an amount of 5000*BSX with conviction 1x => equivalent to
		// 10000*BSX locked
		assert_ok!(Opf::vote(
			RawOrigin::Signed(EVE).into(),
			102,
			5000 * BSX,
			true,
			Conviction::Locked1x
		));

		let round_info = VotingRounds::<Test>::get(0).unwrap();

		run_to_block(round_info.voting_locked_block);
		let mut now =
			<Test as pallet_distribution::Config>::BlockNumberProvider::current_block_number();

		assert_eq!(now, round_info.voting_locked_block);

		// The right events are emitted
		expect_events(vec![RuntimeEvent::Opf(Event::VoteActionLocked {
			when: now,
			round_number: 0,
		})]);

		// The total equivalent amount voted is 17000
		// Project 101: 13000 -> ~76.5%; Project 102: 4000 -> ~23.5%
		// Distributed to project 101 -> 44%*100_000; Distributed to project 102 -> 55%*100_000

		assert_eq!(pallet_distribution::Projects::<Test>::get().len() == 2, true);
		let rewards = pallet_distribution::Projects::<Test>::get();
		assert_eq!(rewards[0].project_id, 101);
		assert_eq!(rewards[1].project_id, 102);
		//assert_eq!(rewards[0].amount > rewards[1].amount, true);
		//assert_eq!(rewards[0].amount, 76000);
		assert_eq!(rewards[1].amount, 23000);

		// New round is properly started
		run_to_block(round_info.round_ending_block);
		now = round_info.round_ending_block;
		expect_events(vec![RuntimeEvent::Opf(Event::VotingRoundEnded {
			when: now,
			round_number: 0,
		})]);
		let new_round_number = VotingRoundNumber::<Test>::get() - 1;
		assert_eq!(new_round_number, 1);
		let next_round = VotingRounds::<Test>::get(1);
		assert_eq!(next_round.is_some(), true);

		now = now.saturating_add(<Test as Config>::VoteLockingPeriod::get().into());
		// Unlock funds
		run_to_block(now);
		assert_ok!(Opf::unlock_funds(RawOrigin::Signed(ALICE).into(), 101));
	})
}

#[test]
fn vote_removal_works() {
	new_test_ext().execute_with(|| {
		create_project_list();
		next_block();

		// Bob nominate project_102 with an amount of 1000  equivalent to
		// 2000 locked
		assert_ok!(Opf::vote(RawOrigin::Signed(BOB).into(), 101, 1000, true, Conviction::Locked1x));

		// Eve nominate project_101 with an amount of 5000 with conviction 1x => equivalent to
		// 10000 locked
		assert_ok!(Opf::vote(RawOrigin::Signed(EVE).into(), 101, 5000, true, Conviction::Locked1x));

		// ProjectFund is correctly updated
		let project_fund_before = ProjectFunds::<Test>::get(101);
		assert_eq!(project_fund_before[0], 12000);

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

		// ProjectFund is correctly updated
		let project_fund_after = ProjectFunds::<Test>::get(101);
		assert_eq!(project_fund_after[0], 10000);
	})
}

#[test]
fn not_enough_funds_to_vote() {
	new_test_ext().execute_with(|| {
		create_project_list();
		next_block();
		let balance_plus = <<Test as pallet_distribution::Config>
			::NativeBalance as fungible::Inspect<u64>>::balance(&BOB)+100;

		// Bob vote with wrong amount
		assert_noop!(
			Opf::vote(RawOrigin::Signed(BOB).into(), 101, balance_plus, true, Conviction::Locked1x),
			Error::<Test>::NotEnoughFunds
		);
	})
}

#[test]
fn voting_action_locked() {
	new_test_ext().execute_with(|| {
		create_project_list();
		next_block();

		let now =
			<Test as pallet_distribution::Config>::BlockNumberProvider::current_block_number();

		// Bob nominate project_101 with an amount of 1000 and conviction 3 => 3000 locked
		assert_ok!(Opf::vote(RawOrigin::Signed(BOB).into(), 101, 1000, true, Conviction::Locked3x));

		expect_events(vec![RuntimeEvent::Opf(Event::VoteCasted {
			who: BOB,
			when: now,
			project_id: 101,
		})]);

		// Bob nominate project_103 with an amount of 5000
		assert_ok!(Opf::vote(RawOrigin::Signed(BOB).into(), 103, 5000, true, Conviction::Locked1x));

		// Voter's funds are locked
		let locked_balance0 =
			<<Test as pallet_distribution::Config>::NativeBalance as fungible::hold::Inspect<
				u64,
			>>::balance_on_hold(&pallet_distribution::HoldReason::FundsReserved.into(), &BOB);
		assert!(locked_balance0 > Zero::zero());

		let round_info = VotingRounds::<Test>::get(0).unwrap();
		run_to_block(round_info.voting_locked_block);

		// Bob cannot edit his vote for project 101
		assert_noop!(
			Opf::vote(RawOrigin::Signed(BOB).into(), 101, 2000, true, Conviction::Locked2x),
			Error::<Test>::VotePeriodClosed
		);
	})
}

#[test]
fn vote_move_works() {
	new_test_ext().execute_with(|| {
		create_project_list();
		next_block();

		let now =
			<Test as pallet_distribution::Config>::BlockNumberProvider::current_block_number();

		// Bob nominate project_101 with an amount of 1000 with a conviction of 2 => amount+amount*2 is the amount allocated to the project
		assert_ok!(Opf::vote(RawOrigin::Signed(BOB).into(), 101, 1000, true, Conviction::Locked2x));

		expect_events(vec![RuntimeEvent::Opf(Event::VoteCasted {
			who: BOB,
			when: now,
			project_id: 101,
		})]);

		// 3000 is allocated to project 101
		let mut funds = ProjectFunds::<Test>::get(101);
		assert_eq!(funds[0],3000);

		// Bob nominate project_103 with an amount of 5000 with a conviction of 1 => amount+amount*1 is the amount allocated to the project
		assert_ok!(Opf::vote(RawOrigin::Signed(BOB).into(), 103, 5000, true, Conviction::Locked1x));

		// 10000 is allocated to project 103
		funds = ProjectFunds::<Test>::get(103);
		assert_eq!(funds[0],10000);

		// Voter's funds are locked
		let mut locked_balance0 =
			<<Test as pallet_distribution::Config>::NativeBalance as fungible::hold::Inspect<
				u64,
			>>::balance_on_hold(&pallet_distribution::HoldReason::FundsReserved.into(), &BOB);
		assert!(locked_balance0 > Zero::zero());
		assert_eq!(locked_balance0, 6000);
		println!("locked: {:?}", locked_balance0);

		// Bob changes amount in project_103 to 4500
		assert_ok!(Opf::vote(RawOrigin::Signed(BOB).into(), 103, 4500, true, Conviction::Locked2x));

		// Allocated amount to project 103 is now 13500 
		funds = ProjectFunds::<Test>::get(103);
		assert_eq!(funds[0],13500);

		// Storage was correctly updated
		let vote_info = Votes::<Test>::get(103, BOB).unwrap();

		locked_balance0 =
			<<Test as pallet_distribution::Config>::NativeBalance as fungible::hold::Inspect<
				u64,
			>>::balance_on_hold(&pallet_distribution::HoldReason::FundsReserved.into(), &BOB);

		assert_eq!(4500, vote_info.amount);
		assert_eq!(Conviction::Locked2x, vote_info.conviction);
		assert_eq!(locked_balance0, 5500);
	})
}
