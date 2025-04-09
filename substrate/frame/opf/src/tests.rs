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

//! # Tests for OPF pallet.

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

pub fn project_list() -> BoundedVec<u64, <Test as Config>::MaxProjects> {
	let mut batch = BoundedVec::<u64, <Test as Config>::MaxProjects>::new();
	for i in 0..3 {
		batch.try_push(101 + i).expect("Should work");
	}
	batch
}

pub fn run_to_block(n: BlockNumberFor<Test>) {
	while <Test as Config>::BlockNumberProvider::current_block_number() < n {
		if <Test as Config>::BlockNumberProvider::current_block_number() > 1 {
			AllPalletsWithSystem::on_finalize(
				<Test as Config>::BlockNumberProvider::current_block_number(),
			);
			AllPalletsWithSystem::on_initialize(
				<Test as Config>::BlockNumberProvider::current_block_number(),
			);
		}
		next_block();
	}
}

#[test]
fn project_registration_works() {
	new_test_ext().execute_with(|| {
		let batch = project_list();
		next_block();
		let origin = RawOrigin::Root.into();
		assert_ok!(Opf::register_projects_batch(origin, batch));
		let project_list = WhiteListedProjectAccounts::<Test>::get(101);
		assert!(project_list.is_some());
		// we should have 3 referendum started
		assert_eq!(<Test as Config>::Governance::referendum_count(), 3);
		// The storage infos are correct
		let round_info = VotingRounds::<Test>::get(0).unwrap();
		assert_eq!(round_info.batch_submitted, true);
		let infos = WhiteListedProjectAccounts::<Test>::get(&101).unwrap();
		let referendum_info =
			<Test as Config>::Governance::get_referendum_info(infos.index).unwrap();
		let referendum_status =
			<Test as Config>::Governance::handle_referendum_info(referendum_info).unwrap();
		assert_eq!(referendum_status, ReferendumStates::Ongoing);
	})
}

#[test]
fn cannot_register_twice_in_same_round() {
	new_test_ext().execute_with(|| {
		let batch = project_list();
		next_block();
		assert_ok!(Opf::register_projects_batch(RawOrigin::Root.into(), batch.clone()));
		let project_list = WhiteListedProjectAccounts::<Test>::get(101);
		assert!(project_list.is_some());
		// we should have 3 referendum started
		assert_eq!(<Test as Config>::Governance::referendum_count(), 3);
		// The storage infos are correct
		let round_info = VotingRounds::<Test>::get(0).unwrap();
		assert_eq!(round_info.batch_submitted, true);
		let round_index = NextVotingRoundNumber::<Test>::get();
		assert_noop!(
			Opf::register_projects_batch(RawOrigin::Root.into(), batch),
			Error::<Test>::BatchAlreadySubmitted
		);
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

		assert_ok!(Opf::register_projects_batch(RawOrigin::Root.into(), batch));
		let round = VotingRounds::<Test>::get(0).unwrap();
		
		assert!(
			round.round_ending_block > round.round_starting_block,
		);
		// Bob vote for project_101
		assert_ok!(Opf::vote(RuntimeOrigin::signed(BOB), 101, 100, true, Conviction::Locked1x));
		// Dave vote for project_102
		assert_ok!(Opf::vote(RuntimeOrigin::signed(DAVE), 102, 100, true, Conviction::Locked2x));
		//Round number is 0
		let round_number = NextVotingRoundNumber::<Test>::get().saturating_sub(1);
		assert_eq!(round_number, 0);

		//Bobs funds are locked
		/*let bob_hold = <Test as Config>::NativeBalance::total_balance_on_hold(&BOB);
		let dave_hold = <Test as Config>::NativeBalance::total_balance_on_hold(&DAVE);
		assert_eq!(bob_hold, 100);
		assert_eq!(dave_hold, 100);
		let round_number = NextVotingRoundNumber::<Test>::get().saturating_sub(1);
		assert_eq!(round_number, 0);

		let bob_vote_unlock = round_end.saturating_add(vote_validity);
		let dave_vote_unlock = bob_vote_unlock.saturating_add(vote_validity);

		let bob_vote_info = Votes::<Test>::get(101, BOB).unwrap();
		let dave_vote_info = Votes::<Test>::get(102, DAVE).unwrap();

		assert_eq!(bob_vote_info.funds_unlock_block, bob_vote_unlock);
		assert_eq!(dave_vote_info.funds_unlock_block, dave_vote_unlock);*/
	})
}

#[test]
fn rewards_calculation_works() {
	new_test_ext().execute_with(|| {
		let batch = project_list();
		
		assert_ok!(Opf::register_projects_batch(RawOrigin::Root.into(), batch));

		let time_periods = <Test as Config>::Governance::get_time_periods(1).ok().unwrap();

		let voting_period = time_periods.total_period.try_into().unwrap_or(0);
		let prepare_period = time_periods.prepare_period.try_into().unwrap_or(0);
		let decision_period = time_periods.decision_period.try_into().unwrap_or(0);
		let enactment_period = time_periods.min_enactment_period.try_into().unwrap_or(0);
		let confirm_period = time_periods.confirm_period.try_into().unwrap_or(0);
		
	
		let now = <Test as Config>::BlockNumberProvider::current_block_number();
		//round_end_block
		let round_end = now.saturating_add(voting_period);
		let decision_block = now.saturating_add(decision_period + prepare_period);
		assert_eq!(decision_block>0, true);

		// Bob nominate project_101 with an amount of 1000*BSX with a conviction x2 => equivalent to
		// 2000*BSX locked
		assert_ok!(Opf::vote(
			RawOrigin::Signed(BOB).into(),
			101,
			1000 * BSX,
			true,
			Conviction::Locked2x
		));

		// Alice nominate project_101 with an amount of 5000*BSX with conviction 1x => equivalent to
		// 5000*BSX locked
		assert_ok!(Opf::vote(
			RawOrigin::Signed(ALICE).into(),
			101,
			5000 * BSX,
			true,
			Conviction::Locked1x
		));

		// DAVE vote against project_102 with an amount of 3000*BSX with conviction 1x => equivalent
		// to 3000*BSX locked
		assert_ok!(Opf::vote(
			RawOrigin::Signed(DAVE).into(),
			102,
			3000 * BSX,
			false,
			Conviction::Locked1x
		));
		// Eve nominate project_102 with an amount of 5000*BSX with conviction 1x => equivalent to
		// 5000*BSX locked
		assert_ok!(Opf::vote(
			RawOrigin::Signed(EVE).into(),
			102,
			5000 * BSX,
			true,
			Conviction::Locked1x
		));

		let round_info = VotingRounds::<Test>::get(0).unwrap();

		let infos = WhiteListedProjectAccounts::<Test>::get(&101).unwrap();

		let scheduled = pallet_scheduler::Agenda::<Test>::get(21);
		println!("Scheduled: {:?}", scheduled);

		let referendum_info =
			<Test as Config>::Governance::get_referendum_info(infos.index).unwrap();
		println!("Referendum info: {:?}", referendum_info);
		run_to_block(round_info.round_ending_block);
		let now = <Test as Config>::BlockNumberProvider::current_block_number();
		let referendum_info =
			<Test as Config>::Governance::get_referendum_info(infos.index).unwrap();

		let referendum_status =
			<Test as Config>::Governance::handle_referendum_info(referendum_info).unwrap();
		// Referendum 101 status is Approved
			assert_eq!(referendum_status, ReferendumStates::Approved);

		// The right events are emitted
		expect_events(vec![
			RuntimeEvent::Opf(Event::VotingRoundEnded { round_number: 0 }),
		]);

		// The total equivalent positive amount voted is 12000
		// Project 101: 7000 -> ~58.3%; Project 102: 2000 -> ~16.6%
		// Distributed to project 101 -> 56%*100_000; Distributed to project 102 -> 16%*100_000
		//Opf::calculate_rewards(<Test as Config>::TemporaryRewards::get());

		let reward_101 = WhiteListedProjectAccounts::<Test>::get(101).unwrap().amount;
		let reward_102 = WhiteListedProjectAccounts::<Test>::get(102).unwrap().amount;
		assert_eq!(reward_101, 58000);
		assert_eq!(reward_102, 16000);


		// Proposal Enactment did not happened yet
		assert_eq!(Spends::<Test>::contains_key(101), false);
		

		next_block();
		// Enactment happened as expected
		/*assert_eq!(Spends::<Test>::contains_key(101), true);

		expect_events(vec![RuntimeEvent::Opf(Event::ProjectFundingAccepted {
			project_id: 102,
			amount: reward_102,
		})]);
		expect_events(vec![RuntimeEvent::Opf(Event::ProjectFundingAccepted {
			project_id: 101,
			amount: reward_101,
		})]);*/
	})
}

/*#[test]
fn vote_removal_works() {
	new_test_ext().execute_with(|| {
		let batch = project_list();
		//round_end_block
		assert_ok!(Opf::register_projects_batch(RuntimeOrigin::signed(EVE), batch));

		// Bob nominate project_102 with an amount of 1000 & conviction of 1 equivalent to
		// 1000
		assert_ok!(Opf::vote(
			RawOrigin::Signed(BOB).into(),
			101,
			1000,
			true,
			Conviction::Locked1x
		));

		// Eve nominate project_101 with an amount of 5000 & conviction 1x => equivalent to
		// 5000
		assert_ok!(Opf::vote(
			RawOrigin::Signed(EVE).into(),
			101,
			5000,
			true,
			Conviction::Locked1x
		));

		// ProjectFund is correctly updated
		let project_fund_before = ProjectFunds::<Test>::get(101);
		assert_eq!(project_fund_before.positive_funds, 6000);

		// Voter's funds are locked
		let locked_balance0 =
			<<Test as Config>::NativeBalance as fungible::hold::Inspect<u64>>::balance_on_hold(
				&HoldReason::FundsReserved.into(),
				&BOB,
			);

		assert_eq!(locked_balance0, 1000);
		assert_eq!(Votes::<Test>::get(101, BOB).is_some(), true);

		// Bob removes his vote
		assert_ok!(Opf::remove_vote(RawOrigin::Signed(BOB).into(), 101,));

		let locked_balance1 =
			<<Test as Config>::NativeBalance as fungible::hold::Inspect<u64>>::balance_on_hold(
				&HoldReason::FundsReserved.into(),
				&BOB,
			);

		// No more votes in storage and balance is unlocked
		assert_eq!(Votes::<Test>::get(101, BOB).is_some(), false);
		assert_eq!(locked_balance1, 0);

		// ProjectFund is correctly updated
		let project_fund_after = ProjectFunds::<Test>::get(101);
		assert_eq!(project_fund_after.positive_funds, 5000);
	})
}

#[test]
fn vote_overwrite_works() {
	new_test_ext().execute_with(|| {
		let batch = project_list();
		assert_ok!(Opf::register_projects_batch(RuntimeOrigin::signed(EVE), batch));
		// Bob nominate project_101 with an amount of 1000 with a conviction of 2 => amount*2
		// is the amount allocated to the project
		assert_ok!(Opf::vote(
			RawOrigin::Signed(BOB).into(),
			101,
			1000,
			true,
			Conviction::Locked2x
		));

		expect_events(vec![RuntimeEvent::Opf(Event::VoteCasted { who: BOB, project_id: 101 })]);

		// 2000 is allocated to project 101
		let mut funds = ProjectFunds::<Test>::get(101);
		assert_eq!(funds.positive_funds, 2000);

		// Bob nominate project_103 with an amount of 5000 with a conviction of 1 => amount
		// is the amount allocated to the project
		assert_ok!(Opf::vote(
			RawOrigin::Signed(BOB).into(),
			103,
			5000,
			true,
			Conviction::Locked1x
		));

		// 10000 is allocated to project 103
		funds = ProjectFunds::<Test>::get(103);
		assert_eq!(funds.positive_funds, 5000);

		// Voter's funds are locked
		let mut locked_balance0 = <<Test as Config>::NativeBalance as fungible::hold::Inspect<
			u64,
		>>::balance_on_hold(&HoldReason::FundsReserved.into(), &BOB);
		assert!(locked_balance0 > 0);
		assert_eq!(locked_balance0, 6000);

		// Bob changes amount in project_103 to 4500 with conviction 2=> 9000
		assert_ok!(Opf::vote(
			RawOrigin::Signed(BOB).into(),
			103,
			4500,
			true,
			Conviction::Locked2x
		));

		// Allocated amount to project 103 is now 13500
		funds = ProjectFunds::<Test>::get(103);
		assert_eq!(funds.positive_funds, 9000);

		// Storage was correctly updated
		let vote_info = Votes::<Test>::get(103, BOB).unwrap();

		locked_balance0 =
			<<Test as Config>::NativeBalance as fungible::hold::Inspect<u64>>::balance_on_hold(
				&HoldReason::FundsReserved.into(),
				&BOB,
			);

		assert_eq!(4500, vote_info.amount);
		assert_eq!(Conviction::Locked2x, vote_info.conviction);
		assert_eq!(locked_balance0, 5500);
	})
}

#[test]
fn voting_action_locked() {
	new_test_ext().execute_with(|| {
		let batch = project_list();
		let bob_bal0 = <Test as Config>::NativeBalance::reducible_balance(
			&BOB,
			Preservation::Preserve,
			Fortitude::Polite,
		);

		assert_ok!(Opf::register_projects_batch(RuntimeOrigin::signed(EVE), batch));

		// Bob nominate project_101 with an amount of 1000 and conviction 3 => 3000 locked
		assert_ok!(Opf::vote(
			RawOrigin::Signed(BOB).into(),
			101,
			1000,
			true,
			Conviction::Locked3x
		));

		expect_events(vec![RuntimeEvent::Opf(Event::VoteCasted { who: BOB, project_id: 101 })]);

		// Bob nominate project_103 with an amount of 5000
		assert_ok!(Opf::vote(
			RawOrigin::Signed(BOB).into(),
			103,
			5000,
			true,
			Conviction::Locked1x
		));

		// Voter's funds are locked
		let locked_balance0 =
			<<Test as Config>::NativeBalance as fungible::hold::Inspect<u64>>::balance_on_hold(
				&HoldReason::FundsReserved.into(),
				&BOB,
			);
		assert_eq!(locked_balance0, 6000);

		let dem_lock = <Test as pallet_democracy::Config>::Currency::reserved_balance(&BOB);
		assert_eq!(dem_lock, 6000);

		let bob_bal1 = <Test as Config>::NativeBalance::reducible_balance(
			&BOB,
			Preservation::Preserve,
			Fortitude::Polite,
		);

		assert_eq!(bob_bal1, bob_bal0.saturating_sub(6000));
		let round_info = VotingRounds::<Test>::get(0).unwrap();
		run_to_block(round_info.round_ending_block);

		// Bob cannot edit his vote for project 101
		assert_noop!(
			Opf::vote(
				RawOrigin::Signed(BOB).into(),
				101,
				2000,
				true,
				Conviction::Locked2x
			),
			Error::<Test>::VotingRoundOver
		);
	})
}

#[test]
fn not_enough_funds_to_vote() {
	new_test_ext().execute_with(|| {
		let batch = project_list();
		assert_ok!(Opf::register_projects_batch(RuntimeOrigin::signed(EVE), batch));
		let balance_plus = <Test as Config>::NativeBalance::reducible_balance(
			&BOB,
			Preservation::Preserve,
			Fortitude::Polite,
		) + 100;
		let balance = <Test as Config>::NativeBalance::reducible_balance(
			&BOB,
			Preservation::Preserve,
			Fortitude::Polite,
		);

		// Bob vote with wrong amount
		assert_noop!(
			Opf::vote(
				RawOrigin::Signed(BOB).into(),
				101,
				balance_plus,
				true,
				Conviction::Locked1x
			),
			Error::<Test>::NotEnoughFunds
		);

		//Bob commits 1/3rd of his balance to project 101
		let balance_minus = <Test as Config>::NativeBalance::reducible_balance(
			&BOB,
			Preservation::Preserve,
			Fortitude::Polite,
		)
		.checked_div(3)
		.unwrap();

		assert_ok!(Opf::vote(
			RawOrigin::Signed(BOB).into(),
			102,
			balance_minus,
			true,
			Conviction::Locked1x
		));

		//Bob tries to commit total_balance to project 102
		assert_noop!(
			Opf::vote(
				RawOrigin::Signed(BOB).into(),
				103,
				balance,
				true,
				Conviction::Locked1x
			),
			Error::<Test>::NotEnoughFunds
		);
	})
}

#[test]
fn spends_creation_works_but_claim_blocked_after_claim_period() {
	new_test_ext().execute_with(|| {
		let batch = project_list();
		let voting_period = <Test as Config>::VotingPeriod::get();
		let mut now = <Test as Config>::BlockNumberProvider::current_block_number();
		let amount1 = 400;
		let amount2 = 320;
		let amount3 = 280;
		//round_end_block
		let round_end = now.saturating_add(voting_period);
		assert_ok!(Opf::register_projects_batch(RuntimeOrigin::signed(EVE), batch));

		assert_ok!(Opf::vote(
			RawOrigin::Signed(ALICE).into(),
			101,
			amount1,
			true,
			Conviction::None
		));

		assert_ok!(Opf::vote(
			RawOrigin::Signed(DAVE).into(),
			102,
			amount2,
			true,
			Conviction::None
		));

		assert_ok!(Opf::vote(
			RawOrigin::Signed(EVE).into(),
			103,
			amount3,
			true,
			Conviction::Locked1x
		));

		// The Spends Storage should be empty
		assert_eq!(Spends::<Test>::count(), 0);

		run_to_block(round_end);

		// Claim does not work before proposal enactment
		assert_noop!(
			Opf::claim_reward_for(RawOrigin::Signed(EVE).into(), 102),
			Error::<Test>::InexistentSpend
		);

		next_block();
		now = <Test as Config>::BlockNumberProvider::current_block_number();
		let expire = now.saturating_add(<Test as Config>::ClaimingPeriod::get());

		let info101 = WhiteListedProjectAccounts::<Test>::get(101).unwrap();

		// Allocations including convictions:
		// project_101: 40, project_102: 32, project_103: 280
		// Rewards percentage to be distributed:
		// project_101: 11%, project_102: 9%, project_103: 79% (of 100,000)
		let spend101: types::SpendInfo<Test> = SpendInfo {
			amount: 11000,
			valid_from: now,
			whitelisted_project: info101,
			claimed: false,
			expire,
		};
		// Spend correctly created
		assert_eq!(Spends::<Test>::get(101), Some(spend101));
		let spend_101 = Spends::<Test>::get(101).unwrap();
		assert_eq!(spend_101.amount > 0, true);
		assert_eq!(spend_101.claimed, false);
		let balance_101_before = <Test as Config>::NativeBalance::balance(&101);
		// Claim works
		assert_ok!(Opf::claim_reward_for(RawOrigin::Signed(EVE).into(), 101));
		let balance_101_after = <Test as Config>::NativeBalance::balance(&101);

		assert_eq!(balance_101_before < balance_101_after, true);

		run_to_block(expire);
		assert_ok!(Opf::claim_reward_for(RawOrigin::Signed(EVE).into(), 102));

		// Claim does not work after claiming period
		expect_events(vec![
			RuntimeEvent::Opf(Event::RewardClaimed { amount: spend_101.amount, project_id: 101 }),
			RuntimeEvent::Opf(Event::ExpiredClaim { expired_when: expire, project_id: 102 }),
		]);
	})
}*/
