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
use sp_staking::delegation::StakingDelegationSupport;

#[test]
fn create_a_delegate_with_first_delegator() {
	ExtBuilder::default().build_and_execute(|| {
		let delegate: AccountId = 200;
		let reward_account: AccountId = 201;
		let delegator: AccountId = 202;

		// set intention to accept delegation.
		fund(&delegate, 1000);
		assert_ok!(DelegatedStaking::register_as_delegate(
			RawOrigin::Signed(delegate).into(),
			reward_account
		));

		// delegate to this account
		fund(&delegator, 1000);
		assert_ok!(DelegatedStaking::delegate_funds(
			RawOrigin::Signed(delegator).into(),
			delegate,
			100
		));

		// verify
		assert!(DelegatedStaking::is_delegate(&delegate));
		assert_eq!(DelegatedStaking::stakeable_balance(&delegate), 100);
		assert_eq!(Balances::balance_on_hold(&HoldReason::Delegating.into(), &delegator), 100);
	});
}

#[test]
fn cannot_become_delegate() {
	ExtBuilder::default().build_and_execute(|| {
		// cannot set reward account same as delegate account
		assert_noop!(
			DelegatedStaking::register_as_delegate(RawOrigin::Signed(100).into(), 100),
			Error::<T>::InvalidRewardDestination
		);

		// an existing validator cannot become delegate
		assert_noop!(
			DelegatedStaking::register_as_delegate(
				RawOrigin::Signed(mock::GENESIS_VALIDATOR).into(),
				100
			),
			Error::<T>::AlreadyStaker
		);

		// an existing nominator cannot become delegate
		assert_noop!(
			DelegatedStaking::register_as_delegate(
				RawOrigin::Signed(mock::GENESIS_NOMINATOR_ONE).into(),
				100
			),
			Error::<T>::AlreadyStaker
		);
		assert_noop!(
			DelegatedStaking::register_as_delegate(
				RawOrigin::Signed(mock::GENESIS_NOMINATOR_TWO).into(),
				100
			),
			Error::<T>::AlreadyStaker
		);
	});
}

#[test]
fn create_multiple_delegators() {
	ExtBuilder::default().build_and_execute(|| {
		let delegate: AccountId = 200;
		let reward_account: AccountId = 201;

		// stakeable balance is 0 for non delegate
		fund(&delegate, 1000);
		assert!(!DelegatedStaking::is_delegate(&delegate));
		assert_eq!(DelegatedStaking::stakeable_balance(&delegate), 0);

		// set intention to accept delegation.
		assert_ok!(DelegatedStaking::register_as_delegate(
			RawOrigin::Signed(delegate).into(),
			reward_account
		));

		// create 100 delegators
		for i in 202..302 {
			fund(&i, 100 + ExistentialDeposit::get());
			assert_ok!(DelegatedStaking::delegate_funds(
				RawOrigin::Signed(i).into(),
				delegate,
				100
			));
			// Balance of 100 held on delegator account for delegating to the delegate.
			assert_eq!(Balances::balance_on_hold(&HoldReason::Delegating.into(), &i), 100);
		}

		// verify
		assert!(DelegatedStaking::is_delegate(&delegate));
		assert_eq!(DelegatedStaking::stakeable_balance(&delegate), 100 * 100);
	});
}

#[test]
fn delegate_restrictions() {
	// Similar to creating a nomination pool
	ExtBuilder::default().build_and_execute(|| {
		let delegate_one = 200;
		let delegator_one = 210;
		fund(&delegate_one, 100);
		assert_ok!(DelegatedStaking::register_as_delegate(
			RawOrigin::Signed(delegate_one).into(),
			delegate_one + 1
		));
		fund(&delegator_one, 200);
		assert_ok!(DelegatedStaking::delegate_funds(
			RawOrigin::Signed(delegator_one).into(),
			delegate_one,
			100
		));

		let delegate_two = 300;
		let delegator_two = 310;
		fund(&delegate_two, 100);
		assert_ok!(DelegatedStaking::register_as_delegate(
			RawOrigin::Signed(delegate_two).into(),
			delegate_two + 1
		));
		fund(&delegator_two, 200);
		assert_ok!(DelegatedStaking::delegate_funds(
			RawOrigin::Signed(delegator_two).into(),
			delegate_two,
			100
		));

		// delegate one tries to delegate to delegate 2
		assert_noop!(
			DelegatedStaking::delegate_funds(
				RawOrigin::Signed(delegate_one).into(),
				delegate_two,
				10
			),
			Error::<T>::InvalidDelegation
		);

		// delegate one tries to delegate to a delegator
		assert_noop!(
			DelegatedStaking::delegate_funds(
				RawOrigin::Signed(delegate_one).into(),
				delegator_one,
				10
			),
			Error::<T>::InvalidDelegation
		);
		assert_noop!(
			DelegatedStaking::delegate_funds(
				RawOrigin::Signed(delegate_one).into(),
				delegator_two,
				10
			),
			Error::<T>::InvalidDelegation
		);

		// delegator one tries to delegate to delegate 2 as well (it already delegates to delegate
		// 1)
		assert_noop!(
			DelegatedStaking::delegate_funds(
				RawOrigin::Signed(delegator_one).into(),
				delegate_two,
				10
			),
			Error::<T>::InvalidDelegation
		);
	});
}

#[test]
fn apply_pending_slash() {
	ExtBuilder::default().build_and_execute(|| todo!());
}

/// Integration tests with pallet-staking.
mod staking_integration {
	use super::*;
	use pallet_staking::RewardDestination;
	use sp_staking::Stake;

	#[test]
	fn bond() {
		ExtBuilder::default().build_and_execute(|| {
			let delegate: AccountId = 99;
			let reward_acc: AccountId = 100;
			assert_eq!(Staking::status(&delegate), Err(StakingError::<T>::NotStash.into()));

			// set intention to become a delegate
			fund(&delegate, 100);
			assert_ok!(DelegatedStaking::register_as_delegate(
				RawOrigin::Signed(delegate).into(),
				reward_acc
			));
			assert_eq!(DelegatedStaking::stakeable_balance(&delegate), 0);

			let mut delegated_balance: Balance = 0;

			// set some delegations
			for delegator in 200..250 {
				fund(&delegator, 200);
				assert_ok!(DelegatedStaking::delegate_funds(
					RawOrigin::Signed(delegator).into(),
					delegate,
					100
				));
				delegated_balance += 100;
				assert_eq!(
					Balances::balance_on_hold(&HoldReason::Delegating.into(), &delegator),
					100
				);

				assert_eq!(DelegatedStaking::stakeable_balance(&delegate), delegated_balance);

				// unbonded balance is the newly delegated 100
				assert_eq!(get_delegate(&delegate).available_to_bond(), 100);
				if get_delegate(&delegate).is_bonded() {
					assert_ok!(DelegatedStaking::bond_extra(&delegate, 100));
				} else {
					assert_ok!(DelegatedStaking::bond(&delegate, 100, &reward_acc));
				}

				// after bond, unbonded balance is 0
				assert_eq!(get_delegate(&delegate).available_to_bond(), 0);
			}

			assert_eq!(
				Staking::stake(&delegate).unwrap(),
				Stake { total: 50 * 100, active: 50 * 100 }
			)
		});
	}

	#[test]
	fn withdraw_test() {
		ExtBuilder::default().build_and_execute(|| {
			// initial era
			start_era(1);
			let delegate: AccountId = 200;
			let reward_acc: AccountId = 201;
			let delegators: Vec<AccountId> = (301..=350).collect();
			let total_staked =
				setup_delegation_stake(delegate, reward_acc, delegators.clone(), 10, 10);

			// lets go to a new era
			start_era(2);

			assert!(eq_stake(delegate, total_staked, total_staked));
			// Withdrawing without unbonding would fail.
			assert_noop!(
				DelegatedStaking::release(RawOrigin::Signed(delegate).into(), 301, 50, 0),
				Error::<T>::NotEnoughFunds
			);
			// assert_noop!(DelegatedStaking::release(RawOrigin::Signed(delegate).into(), 200, 50,
			// 0), Error::<T>::NotAllowed); active and total stake remains same
			assert!(eq_stake(delegate, total_staked, total_staked));

			// 305 wants to unbond 50 in era 2, withdrawable in era 5.
			assert_ok!(DelegatedStaking::unbond(&delegate, 50));
			// 310 wants to unbond 100 in era 3, withdrawable in era 6.
			start_era(3);
			assert_ok!(DelegatedStaking::unbond(&delegate, 100));
			// 320 wants to unbond 200 in era 4, withdrawable in era 7.
			start_era(4);
			assert_ok!(DelegatedStaking::unbond(&delegate, 200));

			// active stake is now reduced..
			let expected_active = total_staked - (50 + 100 + 200);
			assert!(eq_stake(delegate, total_staked, expected_active));

			// nothing to withdraw at era 4
			assert_noop!(
				DelegatedStaking::release(RawOrigin::Signed(delegate).into(), 305, 50, 0),
				Error::<T>::NotEnoughFunds
			);

			assert!(eq_stake(delegate, total_staked, expected_active));
			assert_eq!(get_delegate(&delegate).available_to_bond(), 0);
			// full amount is still delegated
			assert_eq!(get_delegate(&delegate).ledger.effective_balance(), total_staked);

			start_era(5);
			// at era 5, 50 tokens are withdrawable, cannot withdraw more.
			assert_noop!(
				DelegatedStaking::release(RawOrigin::Signed(delegate).into(), 305, 51, 0),
				Error::<T>::NotEnoughFunds
			);
			// less is possible
			assert_ok!(DelegatedStaking::release(RawOrigin::Signed(delegate).into(), 305, 30, 0));
			assert_ok!(DelegatedStaking::release(RawOrigin::Signed(delegate).into(), 305, 20, 0));

			// Lets go to future era where everything is unbonded. Withdrawable amount: 100 + 200
			start_era(7);
			// 305 has no more amount delegated so it cannot withdraw.
			assert_noop!(
				DelegatedStaking::release(RawOrigin::Signed(delegate).into(), 305, 5, 0),
				Error::<T>::NotDelegator
			);
			// 309 is an active delegator but has total delegation of 90, so it cannot withdraw more
			// than that.
			assert_noop!(
				DelegatedStaking::release(RawOrigin::Signed(delegate).into(), 309, 91, 0),
				Error::<T>::NotEnoughFunds
			);
			// 310 cannot withdraw more than delegated funds.
			assert_noop!(
				DelegatedStaking::release(RawOrigin::Signed(delegate).into(), 310, 101, 0),
				Error::<T>::NotEnoughFunds
			);
			// but can withdraw all its delegation amount.
			assert_ok!(DelegatedStaking::release(RawOrigin::Signed(delegate).into(), 310, 100, 0));
			// 320 can withdraw all its delegation amount.
			assert_ok!(DelegatedStaking::release(RawOrigin::Signed(delegate).into(), 320, 200, 0));

			// cannot withdraw anything more..
			assert_noop!(
				DelegatedStaking::release(RawOrigin::Signed(delegate).into(), 301, 1, 0),
				Error::<T>::NotEnoughFunds
			);
			assert_noop!(
				DelegatedStaking::release(RawOrigin::Signed(delegate).into(), 350, 1, 0),
				Error::<T>::NotEnoughFunds
			);
		});
	}

	#[test]
	fn withdraw_happens_with_unbonded_balance_first() {
		ExtBuilder::default().build_and_execute(|| {
			let delegate = 200;
			setup_delegation_stake(delegate, 201, (300..350).collect(), 100, 0);

			// verify withdraw not possible yet
			assert_noop!(
				DelegatedStaking::release(RawOrigin::Signed(delegate).into(), 300, 100, 0),
				Error::<T>::NotEnoughFunds
			);

			// add new delegation that is not staked

			// FIXME(ank4n): add scenario where staked funds are withdrawn from ledger but not
			// withdrawn and test its claimed from there first.

			// fund(&300, 1000);
			// assert_ok!(DelegatedStaking::delegate_funds(RawOrigin::Signed(300.into()), delegate,
			// 100));
			//
			// // verify unbonded balance
			// assert_eq!(get_delegate(&delegate).available_to_bond(), 100);
			//
			// // withdraw works now without unbonding
			// assert_ok!(DelegatedStaking::release(RawOrigin::Signed(delegate).into(), 300, 100,
			// 0)); assert_eq!(get_delegate(&delegate).available_to_bond(), 0);
		});
	}

	#[test]
	fn reward_destination_restrictions() {
		ExtBuilder::default().build_and_execute(|| {
			// give some funds to 200
			fund(&200, 1000);
			let balance_200 = Balances::free_balance(200);

			// `delegate` account cannot be reward destination
			assert_noop!(
				DelegatedStaking::register_as_delegate(RawOrigin::Signed(200).into(), 200),
				Error::<T>::InvalidRewardDestination
			);

			// different reward account works
			assert_ok!(DelegatedStaking::register_as_delegate(RawOrigin::Signed(200).into(), 201));
			// add some delegations to it
			fund(&300, 1000);
			assert_ok!(DelegatedStaking::delegate_funds(RawOrigin::Signed(300).into(), 200, 100));

			// if delegate calls Staking pallet directly with a different reward destination, it
			// fails.
			assert_noop!(
				Staking::bond(RuntimeOrigin::signed(200), 100, RewardDestination::Stash),
				StakingError::<T>::RewardDestinationRestricted
			);
			// non stash account different than one passed to DelegatedStaking also does not work..
			assert_noop!(
				Staking::bond(RuntimeOrigin::signed(200), 100, RewardDestination::Account(202)),
				StakingError::<T>::RewardDestinationRestricted
			);
			// passing correct reward destination works
			assert_ok!(Staking::bond(
				RuntimeOrigin::signed(200),
				100,
				RewardDestination::Account(201)
			));
			// amount is staked correctly
			assert!(eq_stake(200, 100, 100));
			assert_eq!(get_delegate(&200).available_to_bond(), 0);
			assert_eq!(get_delegate(&200).ledger.effective_balance(), 100);

			// free balance of delegate is untouched
			assert_eq!(Balances::free_balance(200), balance_200);

			// trying to change reward destination later directly via staking does not work.
			assert_noop!(
				Staking::set_payee(RuntimeOrigin::signed(200), RewardDestination::Staked),
				StakingError::<T>::RewardDestinationRestricted
			);
			assert_noop!(
				Staking::set_payee(RuntimeOrigin::signed(200), RewardDestination::Account(300)),
				StakingError::<T>::RewardDestinationRestricted
			);
		});
	}

	#[test]
	fn delegate_restrictions() {
		ExtBuilder::default().build_and_execute(|| {
			setup_delegation_stake(200, 201, (202..203).collect(), 100, 0);

			// Registering again is noop
			assert_noop!(
				DelegatedStaking::register_as_delegate(RawOrigin::Signed(200).into(), 201),
				Error::<T>::NotAllowed
			);
			// a delegator cannot become delegate
			assert_noop!(
				DelegatedStaking::register_as_delegate(RawOrigin::Signed(202).into(), 203),
				Error::<T>::NotAllowed
			);
			// existing staker cannot become a delegate
			assert_noop!(
				DelegatedStaking::register_as_delegate(
					RawOrigin::Signed(GENESIS_NOMINATOR_ONE).into(),
					201
				),
				Error::<T>::AlreadyStaker
			);
			assert_noop!(
				DelegatedStaking::register_as_delegate(
					RawOrigin::Signed(GENESIS_VALIDATOR).into(),
					201
				),
				Error::<T>::AlreadyStaker
			);
		});
	}

	#[test]
	fn block_delegations() {
		ExtBuilder::default().build_and_execute(|| {
			assert_ok!(DelegatedStaking::register_as_delegate(RawOrigin::Signed(200).into(), 201));

			// delegation works
			fund(&300, 1000);
			assert_ok!(DelegatedStaking::delegate_funds(RawOrigin::Signed(300).into(), 200, 100));

			// delegate blocks delegation
			assert_ok!(DelegatedStaking::block_delegations(RawOrigin::Signed(200).into(), true));

			// cannot delegate to it anymore
			assert_noop!(
				DelegatedStaking::delegate_funds(RawOrigin::Signed(300).into(), 200, 100),
				Error::<T>::NotAcceptingDelegations
			);

			// delegate can unblock delegation
			assert_ok!(DelegatedStaking::block_delegations(RawOrigin::Signed(200).into(), false));

			// delegation works again
			assert_ok!(DelegatedStaking::delegate_funds(RawOrigin::Signed(300).into(), 200, 100));
		});
	}

	#[test]
	fn slash_works() {
		ExtBuilder::default().build_and_execute(|| {
			setup_delegation_stake(200, 201, (210..250).collect(), 100, 0);
			start_era(1);

			// delegate is slashed
			todo!()
		});
	}

	#[test]
	fn migration_works() {
		ExtBuilder::default().build_and_execute(|| {
			// add a nominator
			fund(&200, 5000);
			let staked_amount = 4000;
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
			assert_eq!(Balances::free_balance(200), 5000);

			// to migrate, nominator needs to set an account as a proxy delegator where staked funds
			// will be moved and delegated back to this old nominator account. This should be funded
			// with at least ED.
			let proxy_delegator = DelegatedStaking::sub_account(AccountType::ProxyDelegator, 200);

			assert_ok!(DelegatedStaking::migrate_to_delegate(RawOrigin::Signed(200).into(), 201));

			// verify all went well
			let mut expected_proxy_delegated_amount = staked_amount;
			assert_eq!(
				Balances::balance_on_hold(&HoldReason::Delegating.into(), &proxy_delegator),
				expected_proxy_delegated_amount
			);
			// ED + stake amount is transferred from delegate to proxy delegator account.
			assert_eq!(
				Balances::free_balance(200),
				5000 - staked_amount - ExistentialDeposit::get()
			);
			assert_eq!(DelegatedStaking::stake(&200).unwrap(), init_stake);
			assert_eq!(get_delegate(&200).ledger.effective_balance(), 4000);
			assert_eq!(get_delegate(&200).available_to_bond(), 0);

			// now lets migrate the delegators
			let delegator_share = staked_amount / 4;
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
					Balances::balance_on_hold(&HoldReason::Delegating.into(), &delegator),
					delegator_share
				);
				expected_proxy_delegated_amount -= delegator_share;
				assert_eq!(
					Balances::balance_on_hold(&HoldReason::Delegating.into(), &proxy_delegator),
					expected_proxy_delegated_amount
				);

				// delegate stake is unchanged.
				assert_eq!(DelegatedStaking::stake(&200).unwrap(), init_stake);
				assert_eq!(get_delegate(&200).ledger.effective_balance(), 4000);
				assert_eq!(get_delegate(&200).available_to_bond(), 0);
			}

			// cannot use migrate delegator anymore
			assert_noop!(
				DelegatedStaking::migrate_delegation(RawOrigin::Signed(200).into(), 305, 1),
				Error::<T>::NotEnoughFunds
			);
		});
	}
}

mod pool_integration {
	use super::*;
	#[test]
	fn create_pool() {
		ExtBuilder::default().build_and_execute(|| {
			let creator: AccountId = 100;
			fund(&creator, 500);
			let delegate_amount = 200;

			assert_ok!(Pools::create(
				RawOrigin::Signed(creator).into(),
				delegate_amount,
				creator,
				creator,
				creator
			));
			assert_eq!(held_balance(&creator), delegate_amount);

			let pool_account = Pools::create_bonded_account(1);
			let delegate = get_delegate(&pool_account);

			assert_eq!(delegate.ledger.effective_balance(), delegate_amount);
			assert_eq!(delegate.available_to_bond(), 0);
			assert_eq!(delegate.unbonded(), 0);
		});
	}

	#[test]
	fn join_pool() {
		ExtBuilder::default().build_and_execute(|| {});
	}

	#[test]
	fn bond_extra_to_pool() {
		ExtBuilder::default().build_and_execute(|| {});
	}

	#[test]
	fn claim_pool_rewards() {
		ExtBuilder::default().build_and_execute(|| {});
	}

	#[test]
	fn unbond_delegation_from_pool() {
		ExtBuilder::default().build_and_execute(|| {});
	}

	#[test]
	fn pool_withdraw_unbonded() {
		ExtBuilder::default().build_and_execute(|| {});
	}

	#[test]
	fn delegator_withdraw_unbonded() {
		ExtBuilder::default().build_and_execute(|| {});
	}

	#[test]
	fn update_nominations() {
		ExtBuilder::default().build_and_execute(|| {});
	}

	#[test]
	fn destroy_pool() {
		ExtBuilder::default().build_and_execute(|| {});
	}

	#[test]
	fn chill_pool() {
		ExtBuilder::default().build_and_execute(|| {});
	}

	#[test]
	fn claim_commission_pool_operator() {
		ExtBuilder::default().build_and_execute(|| {});
	}

	#[test]
	fn pool_slashed() {
		ExtBuilder::default().build_and_execute(|| {});
	}
}
