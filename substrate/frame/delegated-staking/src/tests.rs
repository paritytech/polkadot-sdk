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

//! Tests for pallet-delegated-staking.

use super::*;
use crate::mock::*;
use frame_support::{assert_noop, assert_ok, traits::fungible::InspectHold};
use pallet_staking::Error as StakingError;
use sp_staking::DelegationInterface;

#[test]
fn create_an_agent_with_first_delegator() {
	ExtBuilder::default().build_and_execute(|| {
		let agent: AccountId = 200;
		let reward_account: AccountId = 201;
		let delegator: AccountId = 202;

		// set intention to accept delegation.
		fund(&agent, 1000);
		assert_ok!(DelegatedStaking::register_agent(
			RawOrigin::Signed(agent).into(),
			reward_account
		));

		// delegate to this account
		fund(&delegator, 1000);
		assert_ok!(DelegatedStaking::delegate_to_agent(
			RawOrigin::Signed(delegator).into(),
			agent,
			100
		));

		// verify
		assert!(DelegatedStaking::is_agent(&agent));
		assert_eq!(DelegatedStaking::stakeable_balance(&agent), 100);
		assert_eq!(
			Balances::balance_on_hold(&HoldReason::StakingDelegation.into(), &delegator),
			100
		);
		assert_eq!(DelegatedStaking::held_balance_of(&delegator), 100);
	});
}

#[test]
fn cannot_become_agent() {
	ExtBuilder::default().build_and_execute(|| {
		// cannot set reward account same as agent account
		assert_noop!(
			DelegatedStaking::register_agent(RawOrigin::Signed(100).into(), 100),
			Error::<T>::InvalidRewardDestination
		);

		// an existing validator cannot become agent
		assert_noop!(
			DelegatedStaking::register_agent(
				RawOrigin::Signed(mock::GENESIS_VALIDATOR).into(),
				100
			),
			Error::<T>::AlreadyStaking
		);

		// an existing direct staker to `CoreStaking` cannot become an agent.
		assert_noop!(
			DelegatedStaking::register_agent(
				RawOrigin::Signed(mock::GENESIS_NOMINATOR_ONE).into(),
				100
			),
			Error::<T>::AlreadyStaking
		);
		assert_noop!(
			DelegatedStaking::register_agent(
				RawOrigin::Signed(mock::GENESIS_NOMINATOR_TWO).into(),
				100
			),
			Error::<T>::AlreadyStaking
		);
	});
}

#[test]
fn create_multiple_delegators() {
	ExtBuilder::default().build_and_execute(|| {
		let agent: AccountId = 200;
		let reward_account: AccountId = 201;

		// stakeable balance is 0 for non agent
		fund(&agent, 1000);
		assert!(!DelegatedStaking::is_agent(&agent));
		assert_eq!(DelegatedStaking::stakeable_balance(&agent), 0);

		// set intention to accept delegation.
		assert_ok!(DelegatedStaking::register_agent(
			RawOrigin::Signed(agent).into(),
			reward_account
		));

		// create 100 delegators
		for i in 202..302 {
			fund(&i, 100 + ExistentialDeposit::get());
			assert_ok!(DelegatedStaking::delegate_to_agent(
				RawOrigin::Signed(i).into(),
				agent,
				100
			));
			// Balance of 100 held on delegator account for delegating to the agent.
			assert_eq!(Balances::balance_on_hold(&HoldReason::StakingDelegation.into(), &i), 100);
		}

		// verify
		assert!(DelegatedStaking::is_agent(&agent));
		assert_eq!(DelegatedStaking::stakeable_balance(&agent), 100 * 100);
	});
}

#[test]
fn agent_restrictions() {
	// Similar to creating a nomination pool
	ExtBuilder::default().build_and_execute(|| {
		let agent_one = 200;
		let delegator_one = 210;
		fund(&agent_one, 100);
		assert_ok!(DelegatedStaking::register_agent(
			RawOrigin::Signed(agent_one).into(),
			agent_one + 1
		));
		fund(&delegator_one, 200);
		assert_ok!(DelegatedStaking::delegate_to_agent(
			RawOrigin::Signed(delegator_one).into(),
			agent_one,
			100
		));

		let agent_two = 300;
		let delegator_two = 310;
		fund(&agent_two, 100);
		assert_ok!(DelegatedStaking::register_agent(
			RawOrigin::Signed(agent_two).into(),
			agent_two + 1
		));
		fund(&delegator_two, 200);
		assert_ok!(DelegatedStaking::delegate_to_agent(
			RawOrigin::Signed(delegator_two).into(),
			agent_two,
			100
		));

		// agent one tries to delegate to agent 2
		assert_noop!(
			DelegatedStaking::delegate_to_agent(RawOrigin::Signed(agent_one).into(), agent_two, 10),
			Error::<T>::InvalidDelegation
		);

		// agent one tries to delegate to a delegator
		assert_noop!(
			DelegatedStaking::delegate_to_agent(
				RawOrigin::Signed(agent_one).into(),
				delegator_one,
				10
			),
			Error::<T>::InvalidDelegation
		);
		assert_noop!(
			DelegatedStaking::delegate_to_agent(
				RawOrigin::Signed(agent_one).into(),
				delegator_two,
				10
			),
			Error::<T>::InvalidDelegation
		);

		// delegator one tries to delegate to agent 2 as well (it already delegates to agent
		// 1)
		assert_noop!(
			DelegatedStaking::delegate_to_agent(
				RawOrigin::Signed(delegator_one).into(),
				agent_two,
				10
			),
			Error::<T>::InvalidDelegation
		);

		// cannot delegate to non agents.
		let non_agent = 201;
		// give it some funds
		fund(&non_agent, 200);
		assert_noop!(
			DelegatedStaking::delegate_to_agent(
				RawOrigin::Signed(delegator_one).into(),
				non_agent,
				10
			),
			Error::<T>::InvalidDelegation
		);

		// cannot delegate to a delegator
		assert_noop!(
			DelegatedStaking::delegate_to_agent(
				RawOrigin::Signed(delegator_one).into(),
				delegator_two,
				10
			),
			Error::<T>::InvalidDelegation
		);

		// delegator cannot delegate to self
		assert_noop!(
			DelegatedStaking::delegate_to_agent(
				RawOrigin::Signed(delegator_one).into(),
				delegator_one,
				10
			),
			Error::<T>::InvalidDelegation
		);

		// agent cannot delegate to self
		assert_noop!(
			DelegatedStaking::delegate_to_agent(RawOrigin::Signed(agent_one).into(), agent_one, 10),
			Error::<T>::InvalidDelegation
		);
	});
}

#[test]
fn apply_pending_slash() {
	ExtBuilder::default().build_and_execute(|| {
		start_era(1);
		let agent: AccountId = 200;
		let reward_acc: AccountId = 201;
		let delegators: Vec<AccountId> = (301..=350).collect();
		let reporter: AccountId = 400;

		let total_staked = setup_delegation_stake(agent, reward_acc, delegators.clone(), 10, 10);

		start_era(4);
		// slash half of the stake
		pallet_staking::slashing::do_slash::<T>(
			&agent,
			total_staked / 2,
			&mut Default::default(),
			&mut Default::default(),
			3,
		);

		// agent cannot slash an account that is not its delegator.
		setup_delegation_stake(210, 211, (351..=352).collect(), 100, 0);
		assert_noop!(
			<DelegatedStaking as DelegationInterface>::delegator_slash(&agent, &351, 1, Some(400)),
			Error::<T>::NotAgent
		);
		// or a non delegator account
		fund(&353, 100);
		assert_noop!(
			<DelegatedStaking as DelegationInterface>::delegator_slash(&agent, &353, 1, Some(400)),
			Error::<T>::NotDelegator
		);

		// ensure bookkept pending slash is correct.
		assert_eq!(get_agent(&agent).ledger.pending_slash, total_staked / 2);
		let mut old_reporter_balance = Balances::free_balance(reporter);

		// lets apply the pending slash on delegators.
		for i in delegators {
			// balance before slash
			let initial_pending_slash = get_agent(&agent).ledger.pending_slash;
			assert!(initial_pending_slash > 0);
			let unslashed_balance = DelegatedStaking::held_balance_of(&i);
			let slash = unslashed_balance / 2;
			// slash half of delegator's delegation.
			assert_ok!(<DelegatedStaking as DelegationInterface>::delegator_slash(
				&agent,
				&i,
				slash,
				Some(400)
			));

			// balance after slash.
			assert_eq!(DelegatedStaking::held_balance_of(&i), unslashed_balance - slash);
			// pending slash is reduced by the amount slashed.
			assert_eq!(get_agent(&agent).ledger.pending_slash, initial_pending_slash - slash);
			// reporter get 10% of the slash amount.
			assert_eq!(
				Balances::free_balance(reporter) - old_reporter_balance,
				<Staking as StakingInterface>::slash_reward_fraction() * slash,
			);
			// update old balance
			old_reporter_balance = Balances::free_balance(reporter);
		}

		// nothing to slash anymore
		assert_eq!(get_agent(&agent).ledger.pending_slash, 0);

		// cannot slash anymore
		assert_noop!(
			<DelegatedStaking as DelegationInterface>::delegator_slash(&agent, &350, 1, None),
			Error::<T>::NothingToSlash
		);
	});
}

/// Integration tests with pallet-staking.
mod staking_integration {
	use super::*;
	use pallet_staking::RewardDestination;
	use sp_staking::Stake;

	#[test]
	fn bond() {
		ExtBuilder::default().build_and_execute(|| {
			let agent: AccountId = 99;
			let reward_acc: AccountId = 100;
			assert_eq!(Staking::status(&agent), Err(StakingError::<T>::NotStash.into()));

			// set intention to become an agent
			fund(&agent, 100);
			assert_ok!(DelegatedStaking::register_agent(
				RawOrigin::Signed(agent).into(),
				reward_acc
			));
			assert_eq!(DelegatedStaking::stakeable_balance(&agent), 0);

			let mut delegated_balance: Balance = 0;

			// set some delegations
			for delegator in 200..250 {
				fund(&delegator, 200);
				assert_ok!(DelegatedStaking::delegate_to_agent(
					RawOrigin::Signed(delegator).into(),
					agent,
					100
				));
				delegated_balance += 100;
				assert_eq!(
					Balances::balance_on_hold(&HoldReason::StakingDelegation.into(), &delegator),
					100
				);
				assert_eq!(DelegatedStaking::delegator_balance(&delegator), 100);

				let agent_obj = get_agent(&agent);
				assert_eq!(agent_obj.ledger.stakeable_balance(), delegated_balance);
				assert_eq!(agent_obj.available_to_bond(), 0);
				assert_eq!(agent_obj.bonded_stake(), delegated_balance);
			}

			assert_eq!(Staking::stake(&agent).unwrap(), Stake { total: 50 * 100, active: 50 * 100 })
		});
	}

	#[test]
	fn withdraw_test() {
		ExtBuilder::default().build_and_execute(|| {
			// initial era
			start_era(1);
			let agent: AccountId = 200;
			let reward_acc: AccountId = 201;
			let delegators: Vec<AccountId> = (301..=350).collect();
			let total_staked =
				setup_delegation_stake(agent, reward_acc, delegators.clone(), 10, 10);

			// lets go to a new era
			start_era(2);

			assert!(eq_stake(agent, total_staked, total_staked));
			// Withdrawing without unbonding would fail.
			assert_noop!(
				DelegatedStaking::release_delegation(RawOrigin::Signed(agent).into(), 301, 50, 0),
				Error::<T>::NotEnoughFunds
			);

			// 305 wants to unbond 50 in era 2, withdrawable in era 5.
			assert_ok!(Staking::unbond(RawOrigin::Signed(agent).into(), 50));

			// 310 wants to unbond 100 in era 3, withdrawable in era 6.
			start_era(3);
			assert_ok!(Staking::unbond(RawOrigin::Signed(agent).into(), 100));

			// 320 wants to unbond 200 in era 4, withdrawable in era 7.
			start_era(4);
			assert_ok!(Staking::unbond(RawOrigin::Signed(agent).into(), 200));

			// active stake is now reduced..
			let expected_active = total_staked - (50 + 100 + 200);
			assert!(eq_stake(agent, total_staked, expected_active));

			// nothing to withdraw at era 4
			assert_noop!(
				DelegatedStaking::release_delegation(RawOrigin::Signed(agent).into(), 305, 50, 0),
				Error::<T>::NotEnoughFunds
			);

			assert_eq!(get_agent(&agent).available_to_bond(), 0);
			// full amount is still delegated
			assert_eq!(get_agent(&agent).ledger.effective_balance(), total_staked);

			start_era(5);
			// at era 5, 50 tokens are withdrawable, cannot withdraw more.
			assert_noop!(
				DelegatedStaking::release_delegation(RawOrigin::Signed(agent).into(), 305, 51, 0),
				Error::<T>::NotEnoughFunds
			);
			// less is possible
			assert_ok!(DelegatedStaking::release_delegation(
				RawOrigin::Signed(agent).into(),
				305,
				30,
				0
			));
			assert_ok!(DelegatedStaking::release_delegation(
				RawOrigin::Signed(agent).into(),
				305,
				20,
				0
			));

			// Lets go to future era where everything is unbonded. Withdrawable amount: 100 + 200
			start_era(7);
			// 305 has no more amount delegated so it cannot withdraw.
			assert_noop!(
				DelegatedStaking::release_delegation(RawOrigin::Signed(agent).into(), 305, 5, 0),
				Error::<T>::NotDelegator
			);
			// 309 is an active delegator but has total delegation of 90, so it cannot withdraw more
			// than that.
			assert_noop!(
				DelegatedStaking::release_delegation(RawOrigin::Signed(agent).into(), 309, 91, 0),
				Error::<T>::NotEnoughFunds
			);
			// 310 cannot withdraw more than delegated funds.
			assert_noop!(
				DelegatedStaking::release_delegation(RawOrigin::Signed(agent).into(), 310, 101, 0),
				Error::<T>::NotEnoughFunds
			);
			// but can withdraw all its delegation amount.
			assert_ok!(DelegatedStaking::release_delegation(
				RawOrigin::Signed(agent).into(),
				310,
				100,
				0
			));
			// 320 can withdraw all its delegation amount.
			assert_ok!(DelegatedStaking::release_delegation(
				RawOrigin::Signed(agent).into(),
				320,
				200,
				0
			));

			// cannot withdraw anything more..
			assert_noop!(
				DelegatedStaking::release_delegation(RawOrigin::Signed(agent).into(), 301, 1, 0),
				Error::<T>::NotEnoughFunds
			);
			assert_noop!(
				DelegatedStaking::release_delegation(RawOrigin::Signed(agent).into(), 350, 1, 0),
				Error::<T>::NotEnoughFunds
			);
		});
	}

	#[test]
	fn withdraw_happens_with_unbonded_balance_first() {
		ExtBuilder::default().build_and_execute(|| {
			start_era(1);
			let agent = 200;
			setup_delegation_stake(agent, 201, (300..350).collect(), 100, 0);

			// verify withdraw not possible yet
			assert_noop!(
				DelegatedStaking::release_delegation(RawOrigin::Signed(agent).into(), 300, 100, 0),
				Error::<T>::NotEnoughFunds
			);

			// fill up unlocking chunks in core staking.
			// 10 is the max chunks
			for i in 2..=11 {
				start_era(i);
				assert_ok!(Staking::unbond(RawOrigin::Signed(agent).into(), 10));
				// no withdrawals from core staking yet.
				assert_eq!(get_agent(&agent).ledger.unclaimed_withdrawals, 0);
			}

			// another unbond would trigger withdrawal
			start_era(12);
			assert_ok!(Staking::unbond(RawOrigin::Signed(agent).into(), 10));

			// 8 previous unbonds would be withdrawn as they were already unlocked. Unlocking period
			// is 3 eras.
			assert_eq!(get_agent(&agent).ledger.unclaimed_withdrawals, 8 * 10);

			// release some delegation now.
			assert_ok!(DelegatedStaking::release_delegation(
				RawOrigin::Signed(agent).into(),
				300,
				40,
				0
			));
			assert_eq!(get_agent(&agent).ledger.unclaimed_withdrawals, 80 - 40);

			// cannot release more than available
			assert_noop!(
				DelegatedStaking::release_delegation(RawOrigin::Signed(agent).into(), 300, 50, 0),
				Error::<T>::NotEnoughFunds
			);
			assert_ok!(DelegatedStaking::release_delegation(
				RawOrigin::Signed(agent).into(),
				300,
				40,
				0
			));

			assert_eq!(DelegatedStaking::held_balance_of(&300), 100 - 80);
		});
	}

	#[test]
	fn reward_destination_restrictions() {
		ExtBuilder::default().build_and_execute(|| {
			// give some funds to 200
			fund(&200, 1000);
			let balance_200 = Balances::free_balance(200);

			// `Agent` account cannot be reward destination
			assert_noop!(
				DelegatedStaking::register_agent(RawOrigin::Signed(200).into(), 200),
				Error::<T>::InvalidRewardDestination
			);

			// different reward account works
			assert_ok!(DelegatedStaking::register_agent(RawOrigin::Signed(200).into(), 201));
			// add some delegations to it
			fund(&300, 1000);
			assert_ok!(DelegatedStaking::delegate_to_agent(
				RawOrigin::Signed(300).into(),
				200,
				100
			));

			// update_payee to self fails.
			assert_noop!(
				<Staking as StakingInterface>::update_payee(&200, &200),
				StakingError::<T>::RewardDestinationRestricted
			);

			// passing correct reward destination works
			assert_ok!(<Staking as StakingInterface>::update_payee(&200, &201));

			// amount is staked correctly
			assert!(eq_stake(200, 100, 100));
			assert_eq!(get_agent(&200).available_to_bond(), 0);
			assert_eq!(get_agent(&200).ledger.effective_balance(), 100);

			// free balance of delegate is untouched
			assert_eq!(Balances::free_balance(200), balance_200);
		});
	}

	#[test]
	fn agent_restrictions() {
		ExtBuilder::default().build_and_execute(|| {
			setup_delegation_stake(200, 201, (202..203).collect(), 100, 0);

			// Registering again is noop
			assert_noop!(
				DelegatedStaking::register_agent(RawOrigin::Signed(200).into(), 201),
				Error::<T>::NotAllowed
			);
			// a delegator cannot become delegate
			assert_noop!(
				DelegatedStaking::register_agent(RawOrigin::Signed(202).into(), 203),
				Error::<T>::NotAllowed
			);
			// existing staker cannot become a delegate
			assert_noop!(
				DelegatedStaking::register_agent(
					RawOrigin::Signed(GENESIS_NOMINATOR_ONE).into(),
					201
				),
				Error::<T>::AlreadyStaking
			);
			assert_noop!(
				DelegatedStaking::register_agent(RawOrigin::Signed(GENESIS_VALIDATOR).into(), 201),
				Error::<T>::AlreadyStaking
			);
		});
	}

	#[test]
	fn migration_works() {
		ExtBuilder::default().build_and_execute(|| {
			// add a nominator
			let staked_amount = 4000;
			let agent_amount = 5000;
			fund(&200, agent_amount);

			assert_ok!(Staking::bond(
				RuntimeOrigin::signed(200),
				staked_amount,
				RewardDestination::Account(201)
			));
			assert_ok!(Staking::nominate(RuntimeOrigin::signed(200), vec![GENESIS_VALIDATOR],));
			let init_stake = Staking::stake(&200).unwrap();

			// scenario: 200 is a pool account, and the stake comes from its 4 delegators (300..304)
			// in equal parts. lets try to migrate this nominator into delegate based stake.

			// all balance currently is in 200
			assert_eq!(Balances::free_balance(200), agent_amount);

			// to migrate, nominator needs to set an account as a proxy delegator where staked funds
			// will be moved and delegated back to this old nominator account. This should be funded
			// with at least ED.
			let proxy_delegator = DelegatedStaking::sub_account(AccountType::ProxyDelegator, 200);

			assert_ok!(DelegatedStaking::migrate_to_agent(RawOrigin::Signed(200).into(), 201));

			// verify all went well
			let mut expected_proxy_delegated_amount = agent_amount;
			assert_eq!(
				Balances::balance_on_hold(&HoldReason::StakingDelegation.into(), &proxy_delegator),
				expected_proxy_delegated_amount
			);
			// stake amount is transferred from delegate to proxy delegator account.
			assert_eq!(Balances::free_balance(200), 0);
			assert_eq!(Staking::stake(&200).unwrap(), init_stake);
			assert_eq!(get_agent(&200).ledger.effective_balance(), agent_amount);
			assert_eq!(get_agent(&200).available_to_bond(), 0);
			assert_eq!(get_agent(&200).ledger.unclaimed_withdrawals, agent_amount - staked_amount);

			// now lets migrate the delegators
			let delegator_share = agent_amount / 4;
			for delegator in 300..304 {
				assert_eq!(Balances::free_balance(delegator), 0);
				// fund them with ED
				fund(&delegator, ExistentialDeposit::get());
				// migrate 1/4th amount into each delegator
				assert_ok!(DelegatedStaking::migrate_delegation(
					RawOrigin::Signed(200).into(),
					delegator,
					delegator_share
				));
				assert_eq!(
					Balances::balance_on_hold(&HoldReason::StakingDelegation.into(), &delegator),
					delegator_share
				);
				expected_proxy_delegated_amount -= delegator_share;
				assert_eq!(
					Balances::balance_on_hold(
						&HoldReason::StakingDelegation.into(),
						&proxy_delegator
					),
					expected_proxy_delegated_amount
				);

				// delegate stake is unchanged.
				assert_eq!(Staking::stake(&200).unwrap(), init_stake);
				assert_eq!(get_agent(&200).ledger.effective_balance(), agent_amount);
				assert_eq!(get_agent(&200).available_to_bond(), 0);
				assert_eq!(
					get_agent(&200).ledger.unclaimed_withdrawals,
					agent_amount - staked_amount
				);
			}

			// cannot use migrate delegator anymore
			assert_noop!(
				DelegatedStaking::migrate_delegation(RawOrigin::Signed(200).into(), 305, 1),
				Error::<T>::NotEnoughFunds
			);
		});
	}
}
