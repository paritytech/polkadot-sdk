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

#![cfg(test)]

mod mock;

use frame_support::{
	assert_noop, assert_ok,
	traits::{fungible::InspectHold, Currency},
};
use mock::*;
use pallet_nomination_pools::{
	BondExtra, BondedPools, Error as PoolsError, Event as PoolsEvent, LastPoolId, PoolMember,
	PoolMembers, PoolState,
};
use pallet_staking::{
	CurrentEra, Error as StakingError, Event as StakingEvent, Payee, RewardDestination,
};

use pallet_delegated_staking::{Error as DelegatedStakingError, Event as DelegatedStakingEvent};

use sp_runtime::{bounded_btree_map, traits::Zero};
use sp_staking::Agent;

#[test]
fn pool_lifecycle_e2e() {
	new_test_ext().execute_with(|| {
		assert_eq!(Balances::minimum_balance(), 5);
		assert_eq!(Staking::current_era(), None);

		// create the pool, we know this has id 1.
		assert_ok!(Pools::create(RuntimeOrigin::signed(10), 50, 10, 10, 10));
		assert_eq!(LastPoolId::<Runtime>::get(), 1);

		// have the pool nominate.
		assert_ok!(Pools::nominate(RuntimeOrigin::signed(10), 1, vec![1, 2, 3]));

		assert_eq!(
			staking_events_since_last_call(),
			vec![StakingEvent::Bonded { stash: POOL1_BONDED, amount: 50 }]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![
				PoolsEvent::Created { depositor: 10, pool_id: 1 },
				PoolsEvent::Bonded { member: 10, pool_id: 1, bonded: 50, joined: true },
			]
		);

		// have two members join
		assert_ok!(Pools::join(RuntimeOrigin::signed(20), 10, 1));
		assert_ok!(Pools::join(RuntimeOrigin::signed(21), 10, 1));

		assert_eq!(
			staking_events_since_last_call(),
			vec![
				StakingEvent::Bonded { stash: POOL1_BONDED, amount: 10 },
				StakingEvent::Bonded { stash: POOL1_BONDED, amount: 10 },
			]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![
				PoolsEvent::Bonded { member: 20, pool_id: 1, bonded: 10, joined: true },
				PoolsEvent::Bonded { member: 21, pool_id: 1, bonded: 10, joined: true },
			]
		);

		// pool goes into destroying
		assert_ok!(Pools::set_state(RuntimeOrigin::signed(10), 1, PoolState::Destroying));

		// depositor cannot unbond yet.
		assert_noop!(
			Pools::unbond(RuntimeOrigin::signed(10), 10, 50),
			PoolsError::<Runtime>::MinimumBondNotMet,
		);

		// now the members want to unbond.
		assert_ok!(Pools::unbond(RuntimeOrigin::signed(20), 20, 10));
		assert_ok!(Pools::unbond(RuntimeOrigin::signed(21), 21, 10));

		assert_eq!(PoolMembers::<Runtime>::get(20).unwrap().unbonding_eras.len(), 1);
		assert_eq!(PoolMembers::<Runtime>::get(20).unwrap().points, 0);
		assert_eq!(PoolMembers::<Runtime>::get(21).unwrap().unbonding_eras.len(), 1);
		assert_eq!(PoolMembers::<Runtime>::get(21).unwrap().points, 0);

		assert_eq!(
			staking_events_since_last_call(),
			vec![
				StakingEvent::Unbonded { stash: POOL1_BONDED, amount: 10 },
				StakingEvent::Unbonded { stash: POOL1_BONDED, amount: 10 },
			]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![
				PoolsEvent::StateChanged { pool_id: 1, new_state: PoolState::Destroying },
				PoolsEvent::Unbonded { member: 20, pool_id: 1, points: 10, balance: 10, era: 3 },
				PoolsEvent::Unbonded { member: 21, pool_id: 1, points: 10, balance: 10, era: 3 },
			]
		);

		// depositor cannot still unbond
		assert_noop!(
			Pools::unbond(RuntimeOrigin::signed(10), 10, 50),
			PoolsError::<Runtime>::MinimumBondNotMet,
		);

		for e in 1..BondingDuration::get() {
			CurrentEra::<Runtime>::set(Some(e));
			assert_noop!(
				Pools::withdraw_unbonded(RuntimeOrigin::signed(20), 20, 0),
				PoolsError::<Runtime>::CannotWithdrawAny
			);
		}

		// members are now unlocked.
		CurrentEra::<Runtime>::set(Some(BondingDuration::get()));

		// depositor cannot still unbond
		assert_noop!(
			Pools::unbond(RuntimeOrigin::signed(10), 10, 50),
			PoolsError::<Runtime>::MinimumBondNotMet,
		);

		// but members can now withdraw.
		assert_ok!(Pools::withdraw_unbonded(RuntimeOrigin::signed(20), 20, 0));
		assert_ok!(Pools::withdraw_unbonded(RuntimeOrigin::signed(21), 21, 0));
		assert!(PoolMembers::<Runtime>::get(20).is_none());
		assert!(PoolMembers::<Runtime>::get(21).is_none());

		assert_eq!(
			staking_events_since_last_call(),
			vec![StakingEvent::Withdrawn { stash: POOL1_BONDED, amount: 20 },]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![
				PoolsEvent::Withdrawn { member: 20, pool_id: 1, points: 10, balance: 10 },
				PoolsEvent::MemberRemoved { pool_id: 1, member: 20 },
				PoolsEvent::Withdrawn { member: 21, pool_id: 1, points: 10, balance: 10 },
				PoolsEvent::MemberRemoved { pool_id: 1, member: 21 },
			]
		);

		// as soon as all members have left, the depositor can try to unbond, but since the
		// min-nominator intention is set, they must chill first.
		assert_noop!(
			Pools::unbond(RuntimeOrigin::signed(10), 10, 50),
			pallet_staking::Error::<Runtime>::InsufficientBond
		);

		assert_ok!(Pools::chill(RuntimeOrigin::signed(10), 1));
		assert_ok!(Pools::unbond(RuntimeOrigin::signed(10), 10, 50));

		assert_eq!(
			staking_events_since_last_call(),
			vec![
				StakingEvent::Chilled { stash: POOL1_BONDED },
				StakingEvent::Unbonded { stash: POOL1_BONDED, amount: 50 },
			]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![PoolsEvent::Unbonded { member: 10, pool_id: 1, points: 50, balance: 50, era: 6 }]
		);

		// waiting another bonding duration:
		CurrentEra::<Runtime>::set(Some(BondingDuration::get() * 2));
		assert_ok!(Pools::withdraw_unbonded(RuntimeOrigin::signed(10), 10, 1));

		// pools is fully destroyed now.
		assert_eq!(
			staking_events_since_last_call(),
			vec![StakingEvent::Withdrawn { stash: POOL1_BONDED, amount: 50 },]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![
				PoolsEvent::Withdrawn { member: 10, pool_id: 1, points: 50, balance: 50 },
				PoolsEvent::MemberRemoved { pool_id: 1, member: 10 },
				PoolsEvent::Destroyed { pool_id: 1 }
			]
		);
	})
}

#[test]
fn pool_chill_e2e() {
	new_test_ext().execute_with(|| {
		assert_eq!(Balances::minimum_balance(), 5);
		assert_eq!(Staking::current_era(), None);

		// create the pool, we know this has id 1.
		assert_ok!(Pools::create(RuntimeOrigin::signed(10), 50, 10, 10, 10));
		assert_eq!(LastPoolId::<Runtime>::get(), 1);

		// have the pool nominate.
		assert_ok!(Pools::nominate(RuntimeOrigin::signed(10), 1, vec![1, 2, 3]));

		assert_eq!(
			staking_events_since_last_call(),
			vec![StakingEvent::Bonded { stash: POOL1_BONDED, amount: 50 }]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![
				PoolsEvent::Created { depositor: 10, pool_id: 1 },
				PoolsEvent::Bonded { member: 10, pool_id: 1, bonded: 50, joined: true },
			]
		);

		// have two members join
		assert_ok!(Pools::join(RuntimeOrigin::signed(20), 10, 1));
		assert_ok!(Pools::join(RuntimeOrigin::signed(21), 10, 1));

		assert_eq!(
			staking_events_since_last_call(),
			vec![
				StakingEvent::Bonded { stash: POOL1_BONDED, amount: 10 },
				StakingEvent::Bonded { stash: POOL1_BONDED, amount: 10 },
			]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![
				PoolsEvent::Bonded { member: 20, pool_id: 1, bonded: 10, joined: true },
				PoolsEvent::Bonded { member: 21, pool_id: 1, bonded: 10, joined: true },
			]
		);

		// in case depositor does not have more than `MinNominatorBond` staked, we can end up in
		// situation where a member unbonding would cause pool balance to drop below
		// `MinNominatorBond` and hence not allowed. This can happen if the `MinNominatorBond` is
		// increased after the pool is created.
		assert_ok!(Staking::set_staking_configs(
			RuntimeOrigin::root(),
			pallet_staking::ConfigOp::Set(55), // minimum nominator bond
			pallet_staking::ConfigOp::Noop,
			pallet_staking::ConfigOp::Noop,
			pallet_staking::ConfigOp::Noop,
			pallet_staking::ConfigOp::Noop,
			pallet_staking::ConfigOp::Noop,
			pallet_staking::ConfigOp::Noop,
		));

		// members can unbond as long as total stake of the pool is above min nominator bond
		assert_ok!(Pools::unbond(RuntimeOrigin::signed(20), 20, 10),);
		assert_eq!(PoolMembers::<Runtime>::get(20).unwrap().unbonding_eras.len(), 1);
		assert_eq!(PoolMembers::<Runtime>::get(20).unwrap().points, 0);

		// this member cannot unbond since it will cause `pool stake < MinNominatorBond`
		assert_noop!(
			Pools::unbond(RuntimeOrigin::signed(21), 21, 10),
			StakingError::<Runtime>::InsufficientBond,
		);

		// members can call `chill` permissionlessly now
		assert_ok!(Pools::chill(RuntimeOrigin::signed(20), 1));

		// now another member can unbond.
		assert_ok!(Pools::unbond(RuntimeOrigin::signed(21), 21, 10));
		assert_eq!(PoolMembers::<Runtime>::get(21).unwrap().unbonding_eras.len(), 1);
		assert_eq!(PoolMembers::<Runtime>::get(21).unwrap().points, 0);

		// nominator can not resume nomination until depositor have enough stake
		assert_noop!(
			Pools::nominate(RuntimeOrigin::signed(10), 1, vec![1, 2, 3]),
			PoolsError::<Runtime>::MinimumBondNotMet,
		);

		// other members joining pool does not affect the depositor's ability to resume nomination
		assert_ok!(Pools::join(RuntimeOrigin::signed(22), 10, 1));

		assert_noop!(
			Pools::nominate(RuntimeOrigin::signed(10), 1, vec![1, 2, 3]),
			PoolsError::<Runtime>::MinimumBondNotMet,
		);

		// depositor can bond extra stake
		assert_ok!(Pools::bond_extra(RuntimeOrigin::signed(10), BondExtra::FreeBalance(10)));

		// `chill` can not be called permissionlessly anymore
		assert_noop!(
			Pools::chill(RuntimeOrigin::signed(20), 1),
			PoolsError::<Runtime>::NotNominator,
		);

		// now nominator can resume nomination
		assert_ok!(Pools::nominate(RuntimeOrigin::signed(10), 1, vec![1, 2, 3]));

		// skip to make the unbonding period end.
		CurrentEra::<Runtime>::set(Some(BondingDuration::get()));

		// members can now withdraw.
		assert_ok!(Pools::withdraw_unbonded(RuntimeOrigin::signed(20), 20, 0));
		assert_ok!(Pools::withdraw_unbonded(RuntimeOrigin::signed(21), 21, 0));

		assert_eq!(
			staking_events_since_last_call(),
			vec![
				StakingEvent::Unbonded { stash: POOL1_BONDED, amount: 10 },
				StakingEvent::Chilled { stash: POOL1_BONDED },
				StakingEvent::Unbonded { stash: POOL1_BONDED, amount: 10 },
				StakingEvent::Bonded { stash: POOL1_BONDED, amount: 10 }, // other member bonding
				StakingEvent::Bonded { stash: POOL1_BONDED, amount: 10 }, // depositor bond extra
				StakingEvent::Withdrawn { stash: POOL1_BONDED, amount: 20 },
			]
		);
	})
}

#[test]
fn pool_slash_e2e() {
	new_test_ext().execute_with(|| {
		ExistentialDeposit::set(1);
		assert_eq!(Balances::minimum_balance(), 1);
		assert_eq!(Staking::current_era(), None);

		// create the pool, we know this has id 1.
		assert_ok!(Pools::create(RuntimeOrigin::signed(10), 40, 10, 10, 10));
		assert_eq!(LastPoolId::<Runtime>::get(), 1);

		assert_eq!(
			staking_events_since_last_call(),
			vec![StakingEvent::Bonded { stash: POOL1_BONDED, amount: 40 }]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![
				PoolsEvent::Created { depositor: 10, pool_id: 1 },
				PoolsEvent::Bonded { member: 10, pool_id: 1, bonded: 40, joined: true },
			]
		);

		assert_eq!(
			Payee::<Runtime>::get(POOL1_BONDED),
			Some(RewardDestination::Account(POOL1_REWARD))
		);

		// have two members join
		assert_ok!(Pools::join(RuntimeOrigin::signed(20), 20, 1));
		assert_ok!(Pools::join(RuntimeOrigin::signed(21), 20, 1));

		assert_eq!(
			staking_events_since_last_call(),
			vec![
				StakingEvent::Bonded { stash: POOL1_BONDED, amount: 20 },
				StakingEvent::Bonded { stash: POOL1_BONDED, amount: 20 }
			]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![
				PoolsEvent::Bonded { member: 20, pool_id: 1, bonded: 20, joined: true },
				PoolsEvent::Bonded { member: 21, pool_id: 1, bonded: 20, joined: true },
			]
		);

		// now let's progress a bit.
		CurrentEra::<Runtime>::set(Some(1));

		// 20 / 80 of the total funds are unlocked, and safe from any further slash.
		assert_ok!(Pools::unbond(RuntimeOrigin::signed(10), 10, 10));
		assert_ok!(Pools::unbond(RuntimeOrigin::signed(20), 20, 10));

		assert_eq!(
			staking_events_since_last_call(),
			vec![
				StakingEvent::Unbonded { stash: POOL1_BONDED, amount: 10 },
				StakingEvent::Unbonded { stash: POOL1_BONDED, amount: 10 }
			]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![
				PoolsEvent::Unbonded { member: 10, pool_id: 1, balance: 10, points: 10, era: 4 },
				PoolsEvent::Unbonded { member: 20, pool_id: 1, balance: 10, points: 10, era: 4 }
			]
		);

		CurrentEra::<Runtime>::set(Some(2));

		// note: depositor cannot fully unbond at this point.
		// these funds will still get slashed.
		assert_ok!(Pools::unbond(RuntimeOrigin::signed(10), 10, 10));
		assert_ok!(Pools::unbond(RuntimeOrigin::signed(20), 20, 10));
		assert_ok!(Pools::unbond(RuntimeOrigin::signed(21), 21, 10));

		assert_eq!(
			staking_events_since_last_call(),
			vec![
				StakingEvent::Unbonded { stash: POOL1_BONDED, amount: 10 },
				StakingEvent::Unbonded { stash: POOL1_BONDED, amount: 10 },
				StakingEvent::Unbonded { stash: POOL1_BONDED, amount: 10 },
			]
		);

		assert_eq!(
			pool_events_since_last_call(),
			vec![
				PoolsEvent::Unbonded { member: 10, pool_id: 1, balance: 10, points: 10, era: 5 },
				PoolsEvent::Unbonded { member: 20, pool_id: 1, balance: 10, points: 10, era: 5 },
				PoolsEvent::Unbonded { member: 21, pool_id: 1, balance: 10, points: 10, era: 5 },
			]
		);

		// At this point, 20 are safe from slash, 30 are unlocking but vulnerable to slash, and and
		// another 30 are active and vulnerable to slash. Let's slash half of them.
		pallet_staking::slashing::do_slash::<Runtime>(
			&POOL1_BONDED,
			30,
			&mut Default::default(),
			&mut Default::default(),
			2, // slash era 2, affects chunks at era 5 onwards.
		);

		assert_eq!(
			staking_events_since_last_call(),
			vec![StakingEvent::Slashed { staker: POOL1_BONDED, amount: 30 }]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![
				// 30 has been slashed to 15 (15 slash)
				PoolsEvent::UnbondingPoolSlashed { pool_id: 1, era: 5, balance: 15 },
				// 30 has been slashed to 15 (15 slash)
				PoolsEvent::PoolSlashed { pool_id: 1, balance: 15 }
			]
		);

		CurrentEra::<Runtime>::set(Some(3));
		assert_ok!(Pools::unbond(RuntimeOrigin::signed(21), 21, 10));

		assert_eq!(
			PoolMembers::<Runtime>::get(21).unwrap(),
			PoolMember {
				pool_id: 1,
				points: 0,
				last_recorded_reward_counter: Zero::zero(),
				// the 10 points unlocked just now correspond to 5 points in the unbond pool.
				unbonding_eras: bounded_btree_map!(5 => 10, 6 => 5)
			}
		);
		assert_eq!(
			staking_events_since_last_call(),
			vec![StakingEvent::Unbonded { stash: POOL1_BONDED, amount: 5 }]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![PoolsEvent::Unbonded { member: 21, pool_id: 1, balance: 5, points: 5, era: 6 }]
		);

		// now we start withdrawing. we do it all at once, at era 6 where 20 and 21 are fully free.
		CurrentEra::<Runtime>::set(Some(6));
		assert_ok!(Pools::withdraw_unbonded(RuntimeOrigin::signed(20), 20, 0));
		assert_ok!(Pools::withdraw_unbonded(RuntimeOrigin::signed(21), 21, 0));

		assert_eq!(
			pool_events_since_last_call(),
			vec![
				// 20 had unbonded 10 safely, and 10 got slashed by half.
				PoolsEvent::Withdrawn { member: 20, pool_id: 1, balance: 10 + 5, points: 20 },
				PoolsEvent::MemberRemoved { pool_id: 1, member: 20 },
				// 21 unbonded all of it after the slash
				PoolsEvent::Withdrawn { member: 21, pool_id: 1, balance: 5 + 5, points: 15 },
				PoolsEvent::MemberRemoved { pool_id: 1, member: 21 }
			]
		);
		assert_eq!(
			staking_events_since_last_call(),
			// a 10 (un-slashed) + 10/2 (slashed) balance from 10 has also been unlocked
			vec![StakingEvent::Withdrawn { stash: POOL1_BONDED, amount: 15 + 10 + 15 }]
		);

		// now, finally, we can unbond the depositor further than their current limit.
		assert_ok!(Pools::set_state(RuntimeOrigin::signed(10), 1, PoolState::Destroying));
		assert_ok!(Pools::unbond(RuntimeOrigin::signed(10), 10, 20));

		assert_eq!(
			staking_events_since_last_call(),
			vec![StakingEvent::Unbonded { stash: POOL1_BONDED, amount: 10 }]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![
				PoolsEvent::StateChanged { pool_id: 1, new_state: PoolState::Destroying },
				PoolsEvent::Unbonded { member: 10, pool_id: 1, points: 10, balance: 10, era: 9 }
			]
		);

		CurrentEra::<Runtime>::set(Some(9));
		assert_eq!(
			PoolMembers::<Runtime>::get(10).unwrap(),
			PoolMember {
				pool_id: 1,
				points: 0,
				last_recorded_reward_counter: Zero::zero(),
				unbonding_eras: bounded_btree_map!(4 => 10, 5 => 10, 9 => 10)
			}
		);
		// withdraw the depositor, they should lose 12 balance in total due to slash.
		assert_ok!(Pools::withdraw_unbonded(RuntimeOrigin::signed(10), 10, 0));

		assert_eq!(
			staking_events_since_last_call(),
			vec![StakingEvent::Withdrawn { stash: POOL1_BONDED, amount: 10 }]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![
				PoolsEvent::Withdrawn { member: 10, pool_id: 1, balance: 10 + 15, points: 30 },
				PoolsEvent::MemberRemoved { pool_id: 1, member: 10 },
				PoolsEvent::Destroyed { pool_id: 1 }
			]
		);
	});
}

#[test]
fn pool_slash_proportional() {
	// a typical example where 3 pool members unbond in era 99, 100, and 101, and a slash that
	// happened in era 100 should only affect the latter two.
	new_test_ext().execute_with(|| {
		ExistentialDeposit::set(1);
		BondingDuration::set(28);
		assert_eq!(Balances::minimum_balance(), 1);
		assert_eq!(Staking::current_era(), None);

		// create the pool, we know this has id 1.
		assert_ok!(Pools::create(RuntimeOrigin::signed(10), 40, 10, 10, 10));
		assert_eq!(LastPoolId::<T>::get(), 1);

		assert_eq!(
			staking_events_since_last_call(),
			vec![StakingEvent::Bonded { stash: POOL1_BONDED, amount: 40 }]
		);
		assert_eq!(
			delegated_staking_events_since_last_call(),
			vec![DelegatedStakingEvent::Delegated {
				agent: POOL1_BONDED,
				delegator: 10,
				amount: 40
			}]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![
				PoolsEvent::Created { depositor: 10, pool_id: 1 },
				PoolsEvent::Bonded { member: 10, pool_id: 1, bonded: 40, joined: true },
			]
		);

		// have two members join
		let bond = 20;
		assert_ok!(Pools::join(RuntimeOrigin::signed(20), bond, 1));
		assert_ok!(Pools::join(RuntimeOrigin::signed(21), bond, 1));
		assert_ok!(Pools::join(RuntimeOrigin::signed(22), bond, 1));

		assert_eq!(
			staking_events_since_last_call(),
			vec![
				StakingEvent::Bonded { stash: POOL1_BONDED, amount: bond },
				StakingEvent::Bonded { stash: POOL1_BONDED, amount: bond },
				StakingEvent::Bonded { stash: POOL1_BONDED, amount: bond },
			]
		);
		assert_eq!(
			delegated_staking_events_since_last_call(),
			vec![
				DelegatedStakingEvent::Delegated {
					agent: POOL1_BONDED,
					delegator: 20,
					amount: bond
				},
				DelegatedStakingEvent::Delegated {
					agent: POOL1_BONDED,
					delegator: 21,
					amount: bond
				},
				DelegatedStakingEvent::Delegated {
					agent: POOL1_BONDED,
					delegator: 22,
					amount: bond
				}
			]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![
				PoolsEvent::Bonded { member: 20, pool_id: 1, bonded: bond, joined: true },
				PoolsEvent::Bonded { member: 21, pool_id: 1, bonded: bond, joined: true },
				PoolsEvent::Bonded { member: 22, pool_id: 1, bonded: bond, joined: true },
			]
		);

		// now let's progress a lot.
		CurrentEra::<T>::set(Some(99));

		// and unbond
		assert_ok!(Pools::unbond(RuntimeOrigin::signed(20), 20, bond));

		assert_eq!(
			staking_events_since_last_call(),
			vec![StakingEvent::Unbonded { stash: POOL1_BONDED, amount: bond },]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![PoolsEvent::Unbonded {
				member: 20,
				pool_id: 1,
				balance: bond,
				points: bond,
				era: 127
			}]
		);

		CurrentEra::<T>::set(Some(100));
		assert_ok!(Pools::unbond(RuntimeOrigin::signed(21), 21, bond));
		assert_eq!(
			staking_events_since_last_call(),
			vec![StakingEvent::Unbonded { stash: POOL1_BONDED, amount: bond },]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![PoolsEvent::Unbonded {
				member: 21,
				pool_id: 1,
				balance: bond,
				points: bond,
				era: 128
			}]
		);

		CurrentEra::<T>::set(Some(101));
		assert_ok!(Pools::unbond(RuntimeOrigin::signed(22), 22, bond));
		assert_eq!(
			staking_events_since_last_call(),
			vec![StakingEvent::Unbonded { stash: POOL1_BONDED, amount: bond },]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![PoolsEvent::Unbonded {
				member: 22,
				pool_id: 1,
				balance: bond,
				points: bond,
				era: 129
			}]
		);

		// Apply a slash that happened in era 100. This is typically applied with a delay.
		// Of the total 100, 50 is slashed.
		assert_eq!(BondedPools::<T>::get(1).unwrap().points, 40);

		// no pending slash yet.
		assert_eq!(Pools::api_pool_pending_slash(1), 0);

		pallet_staking::slashing::do_slash::<Runtime>(
			&POOL1_BONDED,
			50,
			&mut Default::default(),
			&mut Default::default(),
			100,
		);

		// Pools api returns correct slash amount.
		assert_eq!(Pools::api_pool_pending_slash(1), 50);

		assert_eq!(
			staking_events_since_last_call(),
			vec![StakingEvent::Slashed { staker: POOL1_BONDED, amount: 50 }]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![
				// This era got slashed 12.5, which rounded up to 13.
				PoolsEvent::UnbondingPoolSlashed { pool_id: 1, era: 128, balance: 7 },
				// This era got slashed 12 instead of 12.5 because an earlier chunk got 0.5 more
				// slashed, and 12 is all the remaining slash
				PoolsEvent::UnbondingPoolSlashed { pool_id: 1, era: 129, balance: 8 },
				// Bonded pool got slashed for 25, remaining 15 in it.
				PoolsEvent::PoolSlashed { pool_id: 1, balance: 15 }
			]
		);

		// 21's balance in the pool is slashed.
		assert_eq!(PoolMembers::<Runtime>::get(21).unwrap().total_balance(), 7);
		// But their actual balance is still unslashed.
		assert_eq!(Balances::total_balance_on_hold(&21), bond);
		// 21 has pending slash
		assert_eq!(Pools::api_member_pending_slash(21), bond - 7);
		// apply slash permissionlessly.
		assert_ok!(Pools::apply_slash(RuntimeOrigin::signed(10), 21));
		// member balance is slashed.
		assert_eq!(Balances::total_balance_on_hold(&21), 7);
		// 21 has no pending slash anymore
		assert_eq!(Pools::api_member_pending_slash(21), 0);

		assert_eq!(
			delegated_staking_events_since_last_call(),
			vec![DelegatedStakingEvent::Slashed {
				agent: POOL1_BONDED,
				delegator: 21,
				amount: bond - 7
			}]
		);

		// 22 balance isn't slashed yet as well.
		assert_eq!(PoolMembers::<Runtime>::get(22).unwrap().total_balance(), 8);
		assert_eq!(Balances::total_balance_on_hold(&22), bond);

		// they try to withdraw. This should slash them.
		CurrentEra::<T>::set(Some(129));
		let pre_balance = Balances::free_balance(&22);
		assert_ok!(Pools::withdraw_unbonded(RuntimeOrigin::signed(22), 22, 0));
		// all balance should be released.
		assert_eq!(Balances::total_balance_on_hold(&22), 0);
		assert_eq!(Balances::free_balance(&22), pre_balance + 8);

		assert_eq!(
			delegated_staking_events_since_last_call(),
			vec![
				DelegatedStakingEvent::Slashed {
					agent: POOL1_BONDED,
					delegator: 22,
					amount: bond - 8
				},
				DelegatedStakingEvent::Released { agent: POOL1_BONDED, delegator: 22, amount: 8 },
			]
		);
	});
}

#[test]
fn pool_slash_non_proportional_only_bonded_pool() {
	// A typical example where a pool member unbonds in era 99, and they can get away with a slash
	// that happened in era 100, as long as the pool has enough active bond to cover the slash. If
	// everything else in the slashing/staking system works, this should always be the case.
	// Nonetheless, `ledger.slash` has been written such that it will slash greedily from any chunk
	// if it runs out of chunks that it thinks should be affected by the slash.
	new_test_ext().execute_with(|| {
		ExistentialDeposit::set(1);
		BondingDuration::set(28);
		assert_eq!(Balances::minimum_balance(), 1);
		assert_eq!(Staking::current_era(), None);

		// create the pool, we know this has id 1.
		assert_ok!(Pools::create(RuntimeOrigin::signed(10), 40, 10, 10, 10));
		assert_eq!(
			staking_events_since_last_call(),
			vec![StakingEvent::Bonded { stash: POOL1_BONDED, amount: 40 }]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![
				PoolsEvent::Created { depositor: 10, pool_id: 1 },
				PoolsEvent::Bonded { member: 10, pool_id: 1, bonded: 40, joined: true },
			]
		);

		// have two members join
		let bond = 20;
		assert_ok!(Pools::join(RuntimeOrigin::signed(20), bond, 1));
		assert_eq!(
			staking_events_since_last_call(),
			vec![StakingEvent::Bonded { stash: POOL1_BONDED, amount: bond }]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![PoolsEvent::Bonded { member: 20, pool_id: 1, bonded: bond, joined: true }]
		);

		// progress and unbond.
		CurrentEra::<T>::set(Some(99));
		assert_ok!(Pools::unbond(RuntimeOrigin::signed(20), 20, bond));
		assert_eq!(
			staking_events_since_last_call(),
			vec![StakingEvent::Unbonded { stash: POOL1_BONDED, amount: bond }]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![PoolsEvent::Unbonded {
				member: 20,
				pool_id: 1,
				balance: bond,
				points: bond,
				era: 127
			}]
		);

		// slash for 30. This will be deducted only from the bonded pool.
		CurrentEra::<T>::set(Some(100));
		assert_eq!(BondedPools::<T>::get(1).unwrap().points, 40);
		pallet_staking::slashing::do_slash::<Runtime>(
			&POOL1_BONDED,
			30,
			&mut Default::default(),
			&mut Default::default(),
			100,
		);

		assert_eq!(
			staking_events_since_last_call(),
			vec![StakingEvent::Slashed { staker: POOL1_BONDED, amount: 30 }]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![PoolsEvent::PoolSlashed { pool_id: 1, balance: 10 }]
		);
	});
}

#[test]
fn pool_slash_non_proportional_bonded_pool_and_chunks() {
	// An uncommon example where even though some funds are unlocked such that they should not be
	// affected by a slash, we still slash out of them. This should not happen at all. If a
	// nomination has unbonded, from the next era onwards, their exposure will drop, so if an era
	// happens in that era, then their share of that slash should naturally be less, such that only
	// their active ledger stake is enough to compensate it.
	new_test_ext().execute_with(|| {
		ExistentialDeposit::set(1);
		BondingDuration::set(28);
		assert_eq!(Balances::minimum_balance(), 1);
		assert_eq!(Staking::current_era(), None);

		// create the pool, we know this has id 1.
		assert_ok!(Pools::create(RuntimeOrigin::signed(10), 40, 10, 10, 10));
		assert_eq!(
			staking_events_since_last_call(),
			vec![StakingEvent::Bonded { stash: POOL1_BONDED, amount: 40 }]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![
				PoolsEvent::Created { depositor: 10, pool_id: 1 },
				PoolsEvent::Bonded { member: 10, pool_id: 1, bonded: 40, joined: true },
			]
		);

		// have two members join
		let bond = 20;
		assert_ok!(Pools::join(RuntimeOrigin::signed(20), bond, 1));
		assert_eq!(
			staking_events_since_last_call(),
			vec![StakingEvent::Bonded { stash: POOL1_BONDED, amount: bond }]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![PoolsEvent::Bonded { member: 20, pool_id: 1, bonded: bond, joined: true }]
		);

		// progress and unbond.
		CurrentEra::<T>::set(Some(99));
		assert_ok!(Pools::unbond(RuntimeOrigin::signed(20), 20, bond));
		assert_eq!(
			staking_events_since_last_call(),
			vec![StakingEvent::Unbonded { stash: POOL1_BONDED, amount: bond }]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![PoolsEvent::Unbonded {
				member: 20,
				pool_id: 1,
				balance: bond,
				points: bond,
				era: 127
			}]
		);

		// slash 50. This will be deducted only from the bonded pool and one of the unbonding pools.
		CurrentEra::<T>::set(Some(100));
		assert_eq!(BondedPools::<T>::get(1).unwrap().points, 40);
		pallet_staking::slashing::do_slash::<Runtime>(
			&POOL1_BONDED,
			50,
			&mut Default::default(),
			&mut Default::default(),
			100,
		);

		assert_eq!(
			staking_events_since_last_call(),
			vec![StakingEvent::Slashed { staker: POOL1_BONDED, amount: 50 }]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![
				// out of 20, 10 was taken.
				PoolsEvent::UnbondingPoolSlashed { pool_id: 1, era: 127, balance: 10 },
				// out of 40, all was taken.
				PoolsEvent::PoolSlashed { pool_id: 1, balance: 0 }
			]
		);
	});
}
#[test]
fn pool_migration_e2e() {
	new_test_ext().execute_with(|| {
		LegacyAdapter::set(true);
		assert_eq!(Balances::minimum_balance(), 5);
		assert_eq!(Staking::current_era(), None);

		// create the pool with TransferStake strategy.
		assert_ok!(Pools::create(RuntimeOrigin::signed(10), 50, 10, 10, 10));
		assert_eq!(LastPoolId::<Runtime>::get(), 1);

		// have the pool nominate.
		assert_ok!(Pools::nominate(RuntimeOrigin::signed(10), 1, vec![1, 2, 3]));

		assert_eq!(
			staking_events_since_last_call(),
			vec![StakingEvent::Bonded { stash: POOL1_BONDED, amount: 50 }]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![
				PoolsEvent::Created { depositor: 10, pool_id: 1 },
				PoolsEvent::Bonded { member: 10, pool_id: 1, bonded: 50, joined: true },
			]
		);

		// have three members join
		let pre_20 = Balances::free_balance(20);
		assert_ok!(Pools::join(RuntimeOrigin::signed(20), 10, 1));
		let pre_21 = Balances::free_balance(21);
		assert_ok!(Pools::join(RuntimeOrigin::signed(21), 10, 1));
		let pre_22 = Balances::free_balance(22);
		assert_ok!(Pools::join(RuntimeOrigin::signed(22), 10, 1));

		// verify members balance is moved to pool.
		assert_eq!(Balances::free_balance(20), pre_20 - 10);
		assert_eq!(Balances::free_balance(21), pre_21 - 10);
		assert_eq!(Balances::free_balance(22), pre_22 - 10);

		assert_eq!(
			staking_events_since_last_call(),
			vec![
				StakingEvent::Bonded { stash: POOL1_BONDED, amount: 10 },
				StakingEvent::Bonded { stash: POOL1_BONDED, amount: 10 },
				StakingEvent::Bonded { stash: POOL1_BONDED, amount: 10 },
			]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![
				PoolsEvent::Bonded { member: 20, pool_id: 1, bonded: 10, joined: true },
				PoolsEvent::Bonded { member: 21, pool_id: 1, bonded: 10, joined: true },
				PoolsEvent::Bonded { member: 22, pool_id: 1, bonded: 10, joined: true },
			]
		);

		CurrentEra::<Runtime>::set(Some(2));
		// 20 is partially unbonding
		assert_ok!(Pools::unbond(RuntimeOrigin::signed(20), 20, 5));

		CurrentEra::<Runtime>::set(Some(3));
		// 21 is fully unbonding
		assert_ok!(Pools::unbond(RuntimeOrigin::signed(21), 21, 10));

		assert_eq!(
			staking_events_since_last_call(),
			vec![
				StakingEvent::Unbonded { stash: POOL1_BONDED, amount: 5 },
				StakingEvent::Unbonded { stash: POOL1_BONDED, amount: 10 },
			]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![
				PoolsEvent::Unbonded { member: 20, pool_id: 1, balance: 5, points: 5, era: 5 },
				PoolsEvent::Unbonded { member: 21, pool_id: 1, balance: 10, points: 10, era: 6 },
			]
		);

		// with `TransferStake`, we can't migrate.
		assert!(!Pools::api_pool_needs_delegate_migration(1));
		assert_noop!(
			Pools::migrate_pool_to_delegate_stake(RuntimeOrigin::signed(10), 1),
			PoolsError::<Runtime>::NotSupported
		);

		// we reset the adapter to `DelegateStake`.
		LegacyAdapter::set(false);

		// cannot migrate the member delegation unless pool is migrated first.
		assert!(!Pools::api_member_needs_delegate_migration(20));
		assert_noop!(
			Pools::migrate_delegation(RuntimeOrigin::signed(10), 20),
			PoolsError::<Runtime>::NotMigrated
		);

		// migrate the pool.
		assert!(Pools::api_pool_needs_delegate_migration(1));
		assert_ok!(Pools::migrate_pool_to_delegate_stake(RuntimeOrigin::signed(10), 1));

		// migrate again does not work.
		assert!(!Pools::api_pool_needs_delegate_migration(1));
		assert_noop!(
			Pools::migrate_pool_to_delegate_stake(RuntimeOrigin::signed(10), 1),
			PoolsError::<Runtime>::AlreadyMigrated
		);

		// unclaimed delegations to the pool are stored in this account.
		let proxy_delegator_1 =
			DelegatedStaking::generate_proxy_delegator(Agent(POOL1_BONDED));

		assert_eq!(
			delegated_staking_events_since_last_call(),
			vec![DelegatedStakingEvent::Delegated {
				agent: POOL1_BONDED,
				delegator: proxy_delegator_1.0,
				amount: 50 + 10 * 3
			}]
		);

		// move to era 5 when 20 can withdraw unbonded funds.
		CurrentEra::<Runtime>::set(Some(5));
		// Unbond works even without claiming delegation. Lets unbond 22.
		assert_ok!(Pools::unbond(RuntimeOrigin::signed(22), 22, 5));

		// withdraw fails for 20 before claiming delegation
		assert_noop!(
			Pools::withdraw_unbonded(RuntimeOrigin::signed(20), 20, 10),
			DelegatedStakingError::<Runtime>::NotDelegator
		);

		let pre_claim_balance_20 = Balances::total_balance(&20);
		assert_eq!(Balances::total_balance_on_hold(&20), 0);

		// migrate delegation for 20. This is permissionless and can be called by anyone.
		assert!(Pools::api_member_needs_delegate_migration(20));
		assert_ok!(Pools::migrate_delegation(RuntimeOrigin::signed(10), 20));

		// tokens moved to 20's account and held there.
		assert_eq!(Balances::total_balance(&20), pre_claim_balance_20 + 10);
		assert_eq!(Balances::total_balance_on_hold(&20), 10);

		// withdraw works now
		assert_ok!(Pools::withdraw_unbonded(RuntimeOrigin::signed(20), 20, 5));

		// balance unlocked in 20's account
		assert_eq!(Balances::total_balance_on_hold(&20), 5);
		assert_eq!(Balances::total_balance(&20), pre_claim_balance_20 + 10);

		assert_eq!(
			staking_events_since_last_call(),
			vec![
				StakingEvent::Unbonded { stash: POOL1_BONDED, amount: 5 },
				StakingEvent::Withdrawn { stash: POOL1_BONDED, amount: 5 }
			]
		);
		assert_eq!(
			pool_events_since_last_call(),
			vec![
				PoolsEvent::Unbonded { member: 22, pool_id: 1, balance: 5, points: 5, era: 8 },
				PoolsEvent::Withdrawn { member: 20, pool_id: 1, balance: 5, points: 5 },
			]
		);
		assert_eq!(
			delegated_staking_events_since_last_call(),
			vec![
				DelegatedStakingEvent::MigratedDelegation {
					agent: POOL1_BONDED,
					delegator: 20,
					amount: 10
				},
				DelegatedStakingEvent::Released { agent: POOL1_BONDED, delegator: 20, amount: 5 }
			]
		);

		// MIGRATE 21
		let pre_migrate_balance_21 = Balances::total_balance(&21);
		assert_eq!(Balances::total_balance_on_hold(&21), 0);

		// migrate delegation for 21.
		assert!(Pools::api_member_needs_delegate_migration(21));
		assert_ok!(Pools::migrate_delegation(RuntimeOrigin::signed(10), 21));

		// tokens moved to 21's account and held there.
		assert_eq!(Balances::total_balance(&21), pre_migrate_balance_21 + 10);
		assert_eq!(Balances::total_balance_on_hold(&21), 10);

		// withdraw fails since 21 only unbonds at era 6.
		assert_noop!(
			Pools::withdraw_unbonded(RuntimeOrigin::signed(21), 21, 10),
			PoolsError::<Runtime>::CannotWithdrawAny
		);

		// go to era when 21 can unbond
		CurrentEra::<Runtime>::set(Some(6));

		// withdraw works now
		assert_ok!(Pools::withdraw_unbonded(RuntimeOrigin::signed(21), 21, 10));

		// all balance unlocked in 21's account
		assert_eq!(Balances::total_balance_on_hold(&21), 0);
		assert_eq!(Balances::total_balance(&21), pre_migrate_balance_21 + 10);

		// MIGRATE 22
		let pre_migrate_balance_22 = Balances::total_balance(&22);
		assert_eq!(Balances::total_balance_on_hold(&22), 0);

		// migrate delegation for 22.
		assert!(Pools::api_member_needs_delegate_migration(22));
		assert_ok!(Pools::migrate_delegation(RuntimeOrigin::signed(10), 22));

		// cannot migrate a pool member again.
		assert!(!Pools::api_member_needs_delegate_migration(22));
		assert_noop!(
			Pools::migrate_delegation(RuntimeOrigin::signed(10), 22),
			PoolsError::<Runtime>::AlreadyMigrated
		);

		// tokens moved to 22's account and held there.
		assert_eq!(Balances::total_balance(&22), pre_migrate_balance_22 + 10);
		assert_eq!(Balances::total_balance_on_hold(&22), 10);

		// withdraw fails since 22 only unbonds at era 8.
		assert_noop!(
			Pools::withdraw_unbonded(RuntimeOrigin::signed(22), 22, 5),
			PoolsError::<Runtime>::CannotWithdrawAny
		);

		// go to era when 22 can unbond
		CurrentEra::<Runtime>::set(Some(10));

		// withdraw works now
		assert_ok!(Pools::withdraw_unbonded(RuntimeOrigin::signed(22), 22, 10));

		// balance of 5 unlocked in 22's account
		assert_eq!(Balances::total_balance_on_hold(&22), 10 - 5);

		// assert events for 21 and 22.
		assert_eq!(
			staking_events_since_last_call(),
			vec![
				StakingEvent::Withdrawn { stash: POOL1_BONDED, amount: 10 },
				StakingEvent::Withdrawn { stash: POOL1_BONDED, amount: 5 }
			]
		);

		assert_eq!(
			pool_events_since_last_call(),
			vec![
				PoolsEvent::Withdrawn { member: 21, pool_id: 1, balance: 10, points: 10 },
				// 21 was fully unbonding and removed from pool.
				PoolsEvent::MemberRemoved { member: 21, pool_id: 1 },
				PoolsEvent::Withdrawn { member: 22, pool_id: 1, balance: 5, points: 5 },
			]
		);
		assert_eq!(
			delegated_staking_events_since_last_call(),
			vec![
				DelegatedStakingEvent::MigratedDelegation {
					agent: POOL1_BONDED,
					delegator: 21,
					amount: 10
				},
				DelegatedStakingEvent::Released { agent: POOL1_BONDED, delegator: 21, amount: 10 },
				DelegatedStakingEvent::MigratedDelegation {
					agent: POOL1_BONDED,
					delegator: 22,
					amount: 10
				},
				DelegatedStakingEvent::Released { agent: POOL1_BONDED, delegator: 22, amount: 5 }
			]
		);
	})
}
