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

//! Tests for StakingRewardProvider implementation.

use crate::{
	mock::{new_test_ext, Balances, Dap, System, Test},
	EraPotType, Event,
};
use frame_support::{
	assert_ok,
	traits::{fungible::Inspect, Get},
};
use sp_staking::StakingRewardProvider;

#[test]
fn allocate_era_rewards_creates_pot_with_provider() {
	new_test_ext().execute_with(|| {
		// GIVEN: An era and pot account that doesn't exist yet
		System::set_block_number(1);
		let era = 1;
		let total_staked = 1000;
		let era_duration_millis = 60_000;

		let pot = Dap::era_pot_account(era, EraPotType::Staker);
		let providers_before = System::providers(&pot);
		let balance_before = Balances::balance(&pot);

		// WHEN: Allocating era rewards
		let staker_rewards = Dap::allocate_era_rewards(era, total_staked, era_duration_millis);

		// THEN: Allocation adds 2 providers (explicit inc_providers + mint_into)
		assert_eq!(System::providers(&pot), providers_before + 2);
		// Pot balance increases by staker rewards amount
		assert_eq!(Balances::balance(&pot), balance_before + staker_rewards);
		// 85 is hardcoded to mint
		assert_eq!(staker_rewards, 85);
		// has_era_pot returns true
		assert!(Dap::has_era_pot(era));
		// EraRewardsAllocated event is emitted
		System::assert_has_event(
			Event::EraRewardsAllocated { era, staker_rewards, treasury_rewards: 15 }.into(),
		);
	});
}

#[test]
fn transfer_era_reward_depletes_pot() {
	new_test_ext().execute_with(|| {
		// GIVEN: An era with allocated rewards
		System::set_block_number(1);
		let era = 1;
		let alice = 9999;

		let total_rewards = Dap::allocate_era_rewards(era, 1000, 60_000);
		System::assert_has_event(
			Event::EraRewardsAllocated { era, staker_rewards: total_rewards, treasury_rewards: 15 }
				.into(),
		);

		let pot = Dap::era_pot_account(era, EraPotType::Staker);
		let initial_pot_balance = Balances::balance(&pot);

		// WHEN: Transferring half the rewards to a beneficiary
		let transfer_amount = total_rewards / 2;
		assert_ok!(Dap::transfer_era_reward(era, &alice, transfer_amount));

		// THEN: Pot balance decreases by transfer amount
		assert_eq!(Balances::balance(&pot), initial_pot_balance - transfer_amount);

		// THEN: alice receives the transfer amount
		assert_eq!(Balances::balance(&alice), transfer_amount);
	});
}

#[test]
fn cleanup_old_era_pot_transfers_to_buffer_and_removes_provider() {
	new_test_ext().execute_with(|| {
		// GIVEN: An era with allocated rewards, partially claimed
		System::set_block_number(1);
		let era = 1;
		let alice = 9999;

		let total_rewards = Dap::allocate_era_rewards(era, 1000, 60_000);
		System::assert_has_event(
			Event::EraRewardsAllocated { era, staker_rewards: total_rewards, treasury_rewards: 15 }
				.into(),
		);

		let pot = Dap::era_pot_account(era, EraPotType::Staker);
		let buffer = Dap::buffer_account();

		let providers_before_cleanup = System::providers(&pot);
		assert_eq!(providers_before_cleanup, 2);

		let claimed = total_rewards / 2;
		assert_ok!(Dap::transfer_era_reward(era, &alice, claimed));

		let unclaimed = total_rewards - claimed;
		let buffer_before = Balances::balance(&buffer);

		// WHEN: Cleaning up old era pot
		Dap::cleanup_old_era_pot(era);

		// THEN: Unclaimed funds are transferred to buffer
		assert_eq!(Balances::balance(&buffer), buffer_before + unclaimed);
		// Cleanup removes all providers (minus 2)
		assert_eq!(System::providers(&pot), 0);
		// EraPotCleaned event is emitted with unclaimed amount
		System::assert_has_event(Event::EraPotCleaned { era, unclaimed_rewards: unclaimed }.into());
	});
}

#[test]
fn pot_account_preserved_below_ed() {
	new_test_ext().execute_with(|| {
		// GIVEN: An era with allocated rewards
		System::set_block_number(1);
		let era = 1;
		let ed = <Test as pallet_balances::Config>::ExistentialDeposit::get();
		let alice = 9999;

		let staker_rewards = Dap::allocate_era_rewards(era, 1000, 60_000);
		System::assert_has_event(
			Event::EraRewardsAllocated { era, staker_rewards, treasury_rewards: 15 }.into(),
		);

		let pot = Dap::era_pot_account(era, EraPotType::Staker);

		// WHEN: Draining pot to below ED (but not zero)
		let pot_balance = Balances::balance(&pot);
		let drain_amount = pot_balance - (ed / 2);
		assert_ok!(Dap::transfer_era_reward(era, &alice, drain_amount));

		// THEN: Pot still exists (has provider reference)
		assert!(Balances::balance(&pot) < ed);
		assert!(System::providers(&pot) > 0);
		assert!(Dap::has_era_pot(era));

		// WHEN: Claiming the remaining rewards
		let remaining_balance = Balances::balance(&pot);
		let bob = 8888;
		assert_ok!(Dap::transfer_era_reward(era, &bob, remaining_balance));

		// THEN: Bob receives the remaining amount
		assert_eq!(Balances::balance(&bob), remaining_balance);
		// Pot balance is now zero
		assert_eq!(Balances::balance(&pot), 0);
		// Pot still exists (provider keeps it alive)
		assert!(Dap::has_era_pot(era));
	});
}

#[test]
fn cleanup_zero_balance_pot() {
	new_test_ext().execute_with(|| {
		// GIVEN: An era with allocated rewards, fully claimed (zero balance)
		System::set_block_number(1);
		let era = 2;
		let alice = 9999;

		let total_rewards = Dap::allocate_era_rewards(era, 1000, 60_000);
		System::assert_has_event(
			Event::EraRewardsAllocated { era, staker_rewards: total_rewards, treasury_rewards: 15 }
				.into(),
		);

		let pot = Dap::era_pot_account(era, EraPotType::Staker);

		assert_ok!(Dap::transfer_era_reward(era, &alice, total_rewards));
		assert_eq!(Balances::balance(&pot), 0);

		// WHEN: Cleaning up pot with zero balance
		Dap::cleanup_old_era_pot(era);

		// THEN: Cleanup removes all providers and account is killed
		assert_eq!(System::providers(&pot), 0);
	});
}

#[test]
fn treasury_rewards_go_to_buffer() {
	new_test_ext().execute_with(|| {
		// GIVEN: A buffer account with initial balance
		System::set_block_number(1);
		let era = 3;
		let buffer = Dap::buffer_account();
		let buffer_before = Balances::balance(&buffer);

		// WHEN: Allocating era rewards
		let staker_rewards = Dap::allocate_era_rewards(era, 1000, 60_000);

		// THEN: Buffer receives treasury portion (15 per TestEraPayout)
		let buffer_after = Balances::balance(&buffer);
		assert_eq!(buffer_after - buffer_before, 15);
		// EraRewardsAllocated event is emitted
		System::assert_has_event(
			Event::EraRewardsAllocated { era, staker_rewards, treasury_rewards: 15 }.into(),
		);
	});
}

#[test]
fn cleanup_nonexistent_pot_is_noop() {
	new_test_ext().execute_with(|| {
		// GIVEN: An era with no allocated pot
		let era = 99;
		let pot = Dap::era_pot_account(era, EraPotType::Staker);
		let providers_before = System::providers(&pot);

		// WHEN: Attempting to clean up nonexistent pot
		Dap::cleanup_old_era_pot(era);

		// THEN: Operation is a no-op (providers unchanged, no panic)
		assert_eq!(System::providers(&pot), providers_before);
	});
}
