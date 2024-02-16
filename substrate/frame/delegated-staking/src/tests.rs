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
fn create_a_delegatee_with_first_delegator() {
	ExtBuilder::default().build_and_execute(|| {
		let delegatee: AccountId = 200;
		let reward_account: AccountId = 201;
		let delegator: AccountId = 202;

		// set intention to accept delegation.
		fund(&delegatee, 1000);
		assert_ok!(DelegatedStaking::accept_delegations(&delegatee, &reward_account));

		// delegate to this account
		fund(&delegator, 1000);
		assert_ok!(DelegatedStaking::delegate(&delegator, &delegatee, 100));

		// verify
		assert!(DelegatedStaking::is_delegatee(&delegatee));
		assert_eq!(DelegatedStaking::stakeable_balance(&delegatee), 100);
		assert_eq!(Balances::balance_on_hold(&HoldReason::Delegating.into(), &delegator), 100);
	});
}

#[test]
fn cannot_become_delegatee() {
	ExtBuilder::default().build_and_execute(|| {
		// cannot set reward account same as delegatee account
		assert_noop!(
			DelegatedStaking::accept_delegations(&100, &100),
			Error::<T>::InvalidRewardDestination
		);

		// an existing validator cannot become delegatee
		assert_noop!(
			DelegatedStaking::accept_delegations(&mock::GENESIS_VALIDATOR, &100),
			Error::<T>::AlreadyStaker
		);

		// an existing nominator cannot become delegatee
		assert_noop!(
			DelegatedStaking::accept_delegations(&mock::GENESIS_NOMINATOR_ONE, &100),
			Error::<T>::AlreadyStaker
		);
		assert_noop!(
			DelegatedStaking::accept_delegations(&mock::GENESIS_NOMINATOR_TWO, &100),
			Error::<T>::AlreadyStaker
		);
	});
}

#[test]
fn create_multiple_delegators() {
	ExtBuilder::default().build_and_execute(|| {
		let delegatee: AccountId = 200;
		let reward_account: AccountId = 201;

		// stakeable balance is 0 for non delegatee
		fund(&delegatee, 1000);
		assert!(!DelegatedStaking::is_delegatee(&delegatee));
		assert_eq!(DelegatedStaking::stakeable_balance(&delegatee), 0);

		// set intention to accept delegation.
		assert_ok!(DelegatedStaking::accept_delegations(&delegatee, &reward_account));

		// create 100 delegators
		for i in 202..302 {
			fund(&i, 100 + ExistentialDeposit::get());
			assert_ok!(DelegatedStaking::delegate(&i, &delegatee, 100));
			// Balance of 100 held on delegator account for delegating to the delegatee.
			assert_eq!(Balances::balance_on_hold(&HoldReason::Delegating.into(), &i), 100);
		}

		// verify
		assert!(DelegatedStaking::is_delegatee(&delegatee));
		assert_eq!(DelegatedStaking::stakeable_balance(&delegatee), 100 * 100);
	});
}

#[test]
fn delegate_restrictions() {
	// Similar to creating a nomination pool
	ExtBuilder::default().build_and_execute(|| {
		let delegatee_one = 200;
		let delegator_one = 210;
		fund(&delegatee_one, 100);
		assert_ok!(DelegatedStaking::accept_delegations(&delegatee_one, &(delegatee_one + 1)));
		fund(&delegator_one, 200);
		assert_ok!(DelegatedStaking::delegate(&delegator_one, &delegatee_one, 100));

		let delegatee_two = 300;
		let delegator_two = 310;
		fund(&delegatee_two, 100);
		assert_ok!(DelegatedStaking::accept_delegations(&delegatee_two, &(delegatee_two + 1)));
		fund(&delegator_two, 200);
		assert_ok!(DelegatedStaking::delegate(&delegator_two, &delegatee_two, 100));

		// delegatee one tries to delegate to delegatee 2
		assert_noop!(
			DelegatedStaking::delegate(&delegatee_one, &delegatee_two, 10),
			Error::<T>::InvalidDelegation
		);

		// delegatee one tries to delegate to a delegator
		assert_noop!(
			DelegatedStaking::delegate(&delegatee_one, &delegator_one, 10),
			Error::<T>::NotDelegatee
		);
		assert_noop!(
			DelegatedStaking::delegate(&delegatee_one, &delegator_two, 10),
			Error::<T>::NotDelegatee
		);

		// delegator one tries to delegate to delegatee 2 as well (it already delegates to delegatee
		// 1)
		assert_noop!(
			DelegatedStaking::delegate(&delegator_one, &delegatee_two, 10),
			Error::<T>::InvalidDelegation
		);
	});
}

#[test]
fn apply_pending_slash() {
	ExtBuilder::default().build_and_execute(|| todo!());
}

/// Integration tests with pallet-staking.
mod integration {
	use super::*;
	use pallet_staking::RewardDestination;
	use sp_staking::Stake;

	#[test]
	fn bond() {
		ExtBuilder::default().build_and_execute(|| {
			let delegatee: AccountId = 99;
			let reward_acc: AccountId = 100;
			assert_eq!(Staking::status(&delegatee), Err(StakingError::<T>::NotStash.into()));

			// set intention to become a delegatee
			fund(&delegatee, 100);
			assert_ok!(DelegatedStaking::accept_delegations(&delegatee, &reward_acc));
			assert_eq!(DelegatedStaking::stakeable_balance(&delegatee), 0);

			let mut delegated_balance: Balance = 0;
			// set some delegations
			for delegator in 200..250 {
				fund(&delegator, 200);
				assert_ok!(DelegatedStaking::delegate(&delegator, &delegatee, 100));
				delegated_balance += 100;
				assert_eq!(
					Balances::balance_on_hold(&HoldReason::Delegating.into(), &delegator),
					100
				);

				assert_eq!(DelegatedStaking::stakeable_balance(&delegatee), delegated_balance);

				// unbonded balance is the newly delegated 100
				assert_eq!(DelegatedStaking::unbonded_balance(&delegatee), 100);
				assert_ok!(DelegatedStaking::bond_all(&delegatee));
				// after bond, unbonded balance is 0
				assert_eq!(DelegatedStaking::unbonded_balance(&delegatee), 0);
			}

			assert_eq!(
				Staking::stake(&delegatee).unwrap(),
				Stake { total: 50 * 100, active: 50 * 100 }
			)
		});
	}

	#[test]
	fn withdraw_test() {
		ExtBuilder::default().build_and_execute(|| {
			// initial era
			start_era(1);
			let delegatee: AccountId = 200;
			let reward_acc: AccountId = 201;
			let delegators: Vec<AccountId> = (301..=350).collect();
			let total_staked =
				setup_delegation_stake(delegatee, reward_acc, delegators.clone(), 10, 10);

			// lets go to a new era
			start_era(2);

			assert!(eq_stake(delegatee, total_staked, total_staked));
			// Withdrawing without unbonding would fail.
			assert_noop!(
				DelegatedStaking::withdraw(&delegatee, &300, 50, 0),
				Error::<T>::WithdrawFailed
			);
			// assert_noop!(DelegatedStaking::withdraw(&delegatee, &200, 50, 0),
			// Error::<T>::NotAllowed); active and total stake remains same
			assert!(eq_stake(delegatee, total_staked, total_staked));

			// 305 wants to unbond 50 in era 2, withdrawable in era 5.
			assert_ok!(DelegatedStaking::unbond(&delegatee, 50));
			// 310 wants to unbond 100 in era 3, withdrawable in era 6.
			start_era(3);
			assert_ok!(DelegatedStaking::unbond(&delegatee, 100));
			// 320 wants to unbond 200 in era 4, withdrawable in era 7.
			start_era(4);
			assert_ok!(DelegatedStaking::unbond(&delegatee, 200));

			// active stake is now reduced..
			let expected_active = total_staked - (50 + 100 + 200);
			assert!(eq_stake(delegatee, total_staked, expected_active));

			// nothing to withdraw at era 4
			assert_noop!(
				DelegatedStaking::withdraw(&delegatee, &305, 50, 0),
				Error::<T>::WithdrawFailed
			);

			assert!(eq_stake(delegatee, total_staked, expected_active));
			assert_eq!(DelegatedStaking::unbonded_balance(&delegatee), 0);
			// full amount is still delegated
			assert_eq!(DelegatedStaking::delegated_balance(&delegatee), total_staked);

			start_era(5);
			// at era 5, 50 tokens are withdrawable, cannot withdraw more.
			assert_noop!(
				DelegatedStaking::withdraw(&delegatee, &305, 51, 0),
				Error::<T>::WithdrawFailed
			);
			// less is possible
			assert_ok!(DelegatedStaking::withdraw(&delegatee, &305, 30, 0));
			assert_ok!(DelegatedStaking::withdraw(&delegatee, &305, 20, 0));

			// Lets go to future era where everything is unbonded. Withdrawable amount: 100 + 200
			start_era(7);
			// 305 has no more amount delegated so it cannot withdraw.
			assert_noop!(
				DelegatedStaking::withdraw(&delegatee, &305, 5, 0),
				Error::<T>::NotDelegator
			);
			// 309 is an active delegator but has total delegation of 90, so it cannot withdraw more
			// than that.
			assert_noop!(
				DelegatedStaking::withdraw(&delegatee, &309, 91, 0),
				Error::<T>::NotEnoughFunds
			);
			// 310 cannot withdraw more than delegated funds.
			assert_noop!(
				DelegatedStaking::withdraw(&delegatee, &310, 101, 0),
				Error::<T>::NotEnoughFunds
			);
			// but can withdraw all its delegation amount.
			assert_ok!(DelegatedStaking::withdraw(&delegatee, &310, 100, 0));
			// 320 can withdraw all its delegation amount.
			assert_ok!(DelegatedStaking::withdraw(&delegatee, &320, 200, 0));

			// cannot withdraw anything more..
			assert_noop!(
				DelegatedStaking::withdraw(&delegatee, &301, 1, 0),
				Error::<T>::WithdrawFailed
			);
			assert_noop!(
				DelegatedStaking::withdraw(&delegatee, &350, 1, 0),
				Error::<T>::WithdrawFailed
			);
		});
	}

	#[test]
	fn withdraw_happens_with_unbonded_balance_first() {
		ExtBuilder::default().build_and_execute(|| {
			let delegatee = 200;
			setup_delegation_stake(delegatee, 201, (300..350).collect(), 100, 0);

			// verify withdraw not possible yet
			assert_noop!(
				DelegatedStaking::withdraw(&delegatee, &300, 100, 0),
				Error::<T>::WithdrawFailed
			);

			// add new delegation that is not staked
			fund(&300, 1000);
			assert_ok!(DelegatedStaking::delegate(&300, &delegatee, 100));

			// verify unbonded balance
			assert_eq!(DelegatedStaking::unbonded_balance(&delegatee), 100);

			// withdraw works now without unbonding
			assert_ok!(DelegatedStaking::withdraw(&delegatee, &300, 100, 0));
			assert_eq!(DelegatedStaking::unbonded_balance(&delegatee), 0);
		});
	}

	#[test]
	fn reward_destination_restrictions() {
		ExtBuilder::default().build_and_execute(|| {
			// give some funds to 200
			fund(&200, 1000);
			let balance_200 = Balances::free_balance(200);

			// delegatee cannot be reward destination
			assert_noop!(
				DelegatedStaking::accept_delegations(&200, &200),
				Error::<T>::InvalidRewardDestination
			);

			// different reward account works
			assert_ok!(DelegatedStaking::accept_delegations(&200, &201));
			// add some delegations to it
			fund(&300, 1000);
			assert_ok!(DelegatedStaking::delegate(&300, &200, 100));

			// if delegatee calls Staking pallet directly with a different reward destination, it
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
			assert_eq!(DelegatedStaking::unbonded_balance(&200), 0);
			assert_eq!(DelegatedStaking::delegated_balance(&200), 100);

			// free balance of delegatee is untouched
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
	fn delegatee_restrictions() {
		ExtBuilder::default().build_and_execute(|| {
			setup_delegation_stake(200, 201, (202..203).collect(), 100, 0);

			// Registering again is noop
			assert_noop!(DelegatedStaking::accept_delegations(&200, &201), Error::<T>::NotAllowed);
			// a delegator cannot become delegatee
			assert_noop!(DelegatedStaking::accept_delegations(&202, &203), Error::<T>::NotAllowed);
			// existing staker cannot become a delegatee
			assert_noop!(
				DelegatedStaking::accept_delegations(&GENESIS_NOMINATOR_ONE, &201),
				Error::<T>::AlreadyStaker
			);
			assert_noop!(
				DelegatedStaking::accept_delegations(&GENESIS_VALIDATOR, &201),
				Error::<T>::AlreadyStaker
			);
		});
	}

	#[test]
	fn block_delegations() {
		ExtBuilder::default().build_and_execute(|| {
			assert_ok!(DelegatedStaking::accept_delegations(&200, &201));

			// delegation works
			fund(&300, 1000);
			assert_ok!(DelegatedStaking::delegate(&300, &200, 100));

			// delegatee blocks delegation
			assert_ok!(DelegatedStaking::block_delegations(&200));

			// cannot delegate to it anymore
			assert_noop!(
				DelegatedStaking::delegate(&300, &200, 100),
				Error::<T>::DelegationsBlocked
			);

			// delegatee can unblock delegation
			assert_ok!(DelegatedStaking::unblock_delegations(&200));

			// delegation works again
			assert_ok!(DelegatedStaking::delegate(&300, &200, 100));
		});
	}

	#[test]
	fn slash_works() {
		ExtBuilder::default().build_and_execute(|| {
			setup_delegation_stake(200, 201, (210..250).collect(), 100, 0);
			start_era(1);

			// delegatee is slashed
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
			// in equal parts. lets try to migrate this nominator into delegatee based stake.

			// all balance currently is in 200
			assert_eq!(Balances::free_balance(200), 5000);

			// to migrate, nominator needs to set an account as a proxy delegator where staked funds
			// will be moved and delegated back to this old nominator account. This should be funded
			// with at least ED.
			let proxy_delegator = 202;
			fund(&proxy_delegator, ExistentialDeposit::get());

			assert_ok!(DelegatedStaking::migrate_accept_delegations(&200, &proxy_delegator, &201));
			assert!(DelegatedStaking::is_migrating(&200));

			// verify all went well
			let mut expected_proxy_delegated_amount = staked_amount;
			assert_eq!(
				Balances::balance_on_hold(&HoldReason::Delegating.into(), &proxy_delegator),
				expected_proxy_delegated_amount
			);
			assert_eq!(Balances::free_balance(200), 5000 - staked_amount);
			assert_eq!(DelegatedStaking::stake(&200).unwrap(), init_stake);
			assert_eq!(DelegatedStaking::delegated_balance(&200), 4000);
			assert_eq!(DelegatedStaking::unbonded_balance(&200), 0);

			// now lets migrate the delegators
			let delegator_share = staked_amount / 4;
			for delegator in 300..304 {
				assert_eq!(Balances::free_balance(delegator), 0);
				// fund them with ED
				fund(&delegator, ExistentialDeposit::get());
				// migrate 1/4th amount into each delegator
				assert_ok!(DelegatedStaking::migrate_delegator(&200, &delegator, delegator_share));
				assert_eq!(
					Balances::balance_on_hold(&HoldReason::Delegating.into(), &delegator),
					delegator_share
				);
				expected_proxy_delegated_amount -= delegator_share;
				assert_eq!(
					Balances::balance_on_hold(&HoldReason::Delegating.into(), &proxy_delegator),
					expected_proxy_delegated_amount
				);

				// delegatee stake is unchanged.
				assert_eq!(DelegatedStaking::stake(&200).unwrap(), init_stake);
				assert_eq!(DelegatedStaking::delegated_balance(&200), 4000);
				assert_eq!(DelegatedStaking::unbonded_balance(&200), 0);
			}

			assert!(!DelegatedStaking::is_migrating(&200));

			// cannot use migrate delegator anymore
			assert_noop!(
				DelegatedStaking::migrate_delegator(&200, &305, 1),
				Error::<T>::NotMigrating
			);
		});
	}
}
