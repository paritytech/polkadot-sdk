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

use frame_support::{assert_noop, assert_ok};
use sp_staking::{StakeBalanceType, StakeBalanceProvider};
use super::*;
use crate::{mock::*, Event};

#[test]
fn create_a_delegatee_with_first_delegator() {
	// Similar to creating a nomination pool
	ExtBuilder::default().build_and_execute(|| {
		let delegatee: AccountId = 200;
		fund(delegatee, 1000);
		let reward_account: AccountId = 201;
		let delegator: AccountId = 202;
		fund(delegator, 1000);

		// set intention to accept delegation.
		assert_ok!(DelegatedStaking::accept_delegations(&delegatee, &reward_account));

		// delegate to this account
		assert_ok!(DelegatedStaking::delegate(&delegator, &delegatee, 100));

		// verify
		assert_eq!(DelegatedStaking::stake_type(&delegatee), StakeBalanceType::Delegated);
		assert_eq!(DelegatedStaking::stakeable_balance(&delegatee), 100);

	});
}

#[test]
fn cannot_become_delegatee() {
	ExtBuilder::default().build_and_execute(|| {
		// cannot set reward account same as delegatee account
		assert_noop!(DelegatedStaking::accept_delegations(&100, &100), Error::<T>::InvalidRewardDestination);

		// an existing validator cannot become delegatee
		assert_noop!(DelegatedStaking::accept_delegations(&mock::GENESIS_VALIDATOR, &100), Error::<T>::AlreadyStaker);

		// an existing nominator cannot become delegatee
		assert_noop!(DelegatedStaking::accept_delegations(&mock::GENESIS_NOMINATOR_ONE, &100), Error::<T>::AlreadyStaker);
		assert_noop!(DelegatedStaking::accept_delegations(&mock::GENESIS_NOMINATOR_TWO, &100), Error::<T>::AlreadyStaker);
	});
}

#[test]
fn add_delegation_to_existing_delegator() {
	ExtBuilder::default().build_and_execute(|| assert!(true));
}

#[test]
fn create_multiple_delegators() {
	// Similar to creating a nomination pool
	ExtBuilder::default().build_and_execute(|| assert!(true));
}

#[test]
fn withdraw_delegation() {
	// Similar to creating a nomination pool
	ExtBuilder::default().build_and_execute(|| assert!(true));
}

#[test]
fn apply_pending_slash() {
	// Similar to creating a nomination pool
	ExtBuilder::default().build_and_execute(|| assert!(true));
}

#[test]
fn distribute_rewards() {
	// Similar to creating a nomination pool
	ExtBuilder::default().build_and_execute(|| assert!(true));
}

#[test]
fn migrate_to_delegator() {
	// Similar to creating a nomination pool
	ExtBuilder::default().build_and_execute(|| assert!(true));
}
