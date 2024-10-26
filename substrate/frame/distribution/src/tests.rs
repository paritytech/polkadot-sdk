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

//! Tests for Distribution pallet.

pub use super::*;
use crate::mock::*;
use frame_support::{assert_noop, assert_ok};

pub fn next_block() {
	System::set_block_number(<Test as Config>::BlockNumberProvider::current_block_number() + 1);
	AllPalletsWithSystem::on_initialize(
		<Test as Config>::BlockNumberProvider::current_block_number(),
	);
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

pub fn create_project(project_id: AccountId, amount: u128) {
	let submission_block = <Test as Config>::BlockNumberProvider::current_block_number();
	let project: types::ProjectInfo<Test> = ProjectInfo { project_id, submission_block, amount };
	Projects::<Test>::mutate(|value| {
		let mut val = value.clone();
		let _ = val.try_push(project);
		*value = val;
	});
}

#[test]
fn spends_creation_works() {
	new_test_ext().execute_with(|| {
		// Add 3 projects
		let amount1 = 1_000_000 * BSX;
		let amount2 = 1_200_000 * BSX;
		let amount3 = 2_000_000 * BSX;
		create_project(ALICE, amount1);
		create_project(BOB, amount2);
		create_project(DAVE, amount3);

		// The Spends Storage should be empty
		assert_eq!(Spends::<Test>::count(), 0);

		// Move to epoch block => Warning: We set the system block at 1 in mock.rs, so now =
		// Epoch_Block + 1
		let now = <Test as Config>::BlockNumberProvider::current_block_number()
			.saturating_add(<Test as Config>::EpochDurationBlocks::get().into());
		run_to_block(now);

		// We should have 3 Spends
		assert!(Spends::<Test>::count() == 3);

		// The 3 Spends are known
		let alice_spend: types::SpendInfo<Test> = SpendInfo {
			amount: amount1,
			valid_from: now,
			whitelisted_project: Some(ALICE),
			claimed: false,
		};

		let bob_spend: types::SpendInfo<Test> = SpendInfo {
			amount: amount2,
			valid_from: now,
			whitelisted_project: Some(BOB),
			claimed: false,
		};

		let dave_spend: types::SpendInfo<Test> = SpendInfo {
			amount: amount3,
			valid_from: now,
			whitelisted_project: Some(DAVE),
			claimed: false,
		};

		// List of Spends actually created & stored
		let list0: Vec<_> = Spends::<Test>::iter_keys().collect();
		let list: Vec<_> = list0.into_iter().map(|x| Spends::<Test>::get(x)).collect();

		expect_events(vec![
			RuntimeEvent::Distribution(Event::SpendCreated {
				when: now.saturating_sub(1),
				amount: list[0].clone().unwrap().amount,
				project_id: list[0].clone().unwrap().whitelisted_project.unwrap(),
			}),
			RuntimeEvent::Distribution(Event::SpendCreated {
				when: now.saturating_sub(1),
				amount: list[1].clone().unwrap().amount,
				project_id: list[1].clone().unwrap().whitelisted_project.unwrap(),
			}),
			RuntimeEvent::Distribution(Event::SpendCreated {
				when: now.saturating_sub(1),
				amount: list[2].clone().unwrap().amount,
				project_id: list[2].clone().unwrap().whitelisted_project.unwrap(),
			}),
		]);

		assert!(list.contains(&Some(alice_spend)));
		assert!(list.contains(&Some(bob_spend)));
		assert!(list.contains(&Some(dave_spend)));
	})
}

#[test]
fn funds_are_locked() {
	new_test_ext().execute_with(|| {
		// Add 3 projects
		let amount1 = 1_000_000 * BSX;
		let amount2 = 1_200_000 * BSX;
		let amount3 = 2_000_000 * BSX;
		create_project(ALICE, amount1);
		create_project(BOB, amount2);
		create_project(DAVE, amount3);

		// The Spends Storage should be empty
		assert_eq!(Spends::<Test>::count(), 0);

		// Move to epoch block => Warning: We set the system block at 1 in mock.rs, so now =
		// Epoch_Block + 1
		let now = <Test as Config>::BlockNumberProvider::current_block_number()
			.saturating_add(<Test as Config>::EpochDurationBlocks::get().into());
		run_to_block(now);

		let total_on_hold = amount1.saturating_add(amount2).saturating_add(amount3);
		let pot_account = Distribution::pot_account();
		let hold =
			<<Test as Config>::NativeBalance as fungible::hold::Inspect<u64>>::balance_on_hold(
				&HoldReason::FundsReserved.into(),
				&pot_account,
			);
		assert_eq!(total_on_hold, hold);
	})
}

#[test]
fn not_enough_funds_in_pot() {
	new_test_ext().execute_with(|| {
		// Add 3 projects
		let amount1 = 50_000_000 * BSX;
		let amount2 = 60_200_000 * BSX;
		let amount3 = 70_000_000 * BSX;
		create_project(ALICE, amount1);
		create_project(BOB, amount2);
		create_project(DAVE, amount3);

		let total = amount1.saturating_add(amount2.saturating_add(amount3));
		assert_noop!(Distribution::pot_check(total), Error::<Test>::InsufficientPotReserves);
	})
}

#[test]
fn funds_claim_works() {
	new_test_ext().execute_with(|| {
		// Add 3 projects
		let amount1 = 1_000_000 * BSX;
		let amount2 = 1_200_000 * BSX;
		let amount3 = 2_000_000 * BSX;
		create_project(ALICE, amount1);
		create_project(BOB, amount2);
		create_project(DAVE, amount3);

		// The Spends Storage should be empty
		assert_eq!(Spends::<Test>::count(), 0);

		assert_eq!(Projects::<Test>::get().len(), 3);

		// Move to epoch block => Warning: We set the system block at 1 in mock.rs, so now =
		// Epoch_Block + 1
		let mut now = <Test as Config>::BlockNumberProvider::current_block_number()
			.saturating_add(<Test as Config>::EpochDurationBlocks::get().into());
		run_to_block(now);

		let project = Spends::<Test>::get(ALICE).unwrap();
		let project_id = project.whitelisted_project.unwrap();
		let balance_0 =
			<<Test as Config>::NativeBalance as fungible::Inspect<u64>>::balance(&project_id);
		now = now.saturating_add(project.valid_from);
		run_to_block(now);

		// Spend is in storage
		assert!(Spends::<Test>::get(ALICE).is_some());

		assert_ok!(Distribution::claim_reward_for(RawOrigin::Signed(EVE).into(), project_id,));
		let balance_1 =
			<<Test as Config>::NativeBalance as fungible::Inspect<u64>>::balance(&project_id);

		assert!(balance_1 > balance_0);
		assert_eq!(Projects::<Test>::get().len(), 0);
		// Spend has been removed from storage
		assert!(!Spends::<Test>::get(0).is_some());
	})
}

#[test]
fn funds_claim_fails_before_claim_period() {
	new_test_ext().execute_with(|| {
		// Add 3 projects
		let amount1 = 1_000_000 * BSX;
		let amount2 = 1_200_000 * BSX;
		let amount3 = 2_000_000 * BSX;
		create_project(ALICE, amount1);
		create_project(BOB, amount2);
		create_project(DAVE, amount3);

		// The Spends Storage should be empty
		assert_eq!(Spends::<Test>::count(), 0);

		// Move to epoch block => Warning: We set the system block at 1 in mock.rs, so now =
		// Epoch_Block + 1
		let now = <Test as Config>::BlockNumberProvider::current_block_number()
			.saturating_add(<Test as Config>::EpochDurationBlocks::get().into());
		run_to_block(now);

		let project = Spends::<Test>::get(ALICE).unwrap();
		let project_id = project.whitelisted_project.unwrap();

		assert_noop!(
			Distribution::claim_reward_for(RawOrigin::Signed(EVE).into(), project_id),
			Error::<Test>::NotClaimingPeriod
		);
	})
}
