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

use super::*;

#[test]
fn change_controller_works() {
	ExtBuilder::default().build_and_execute(|| {
		let (stash, controller) = testing_utils::create_unique_stash_controller::<Test>(
			0,
			100,
			RewardDestination::Staked,
			false,
		)
		.unwrap();

		// ensure `stash` and `controller` are bonded as stash controller pair.
		assert_eq!(Staking::bonded(&stash), Some(controller));

		// `controller` can control `stash` who is initially a validator.
		assert_ok!(Staking::chill(RuntimeOrigin::signed(controller)));

		// sets controller back to `stash`.
		assert_ok!(Staking::set_controller(RuntimeOrigin::signed(stash)));
		assert_eq!(Staking::bonded(&stash), Some(stash));

		// fetch the ledger from storage and check if the controller is correct.
		let ledger = Staking::ledger(StakingAccount::Stash(stash)).unwrap();
		assert_eq!(ledger.controller(), Some(stash));

		// same if we fetch the ledger by controller.
		let ledger = Staking::ledger(StakingAccount::Controller(stash)).unwrap();
		assert_eq!(ledger.controller, Some(stash));
		assert_eq!(ledger.controller(), Some(stash));

		// the raw storage ledger's controller is always `None`. however, we can still fetch the
		// correct controller with `ledger.controller()`.
		let raw_ledger = <Ledger<Test>>::get(&stash).unwrap();
		assert_eq!(raw_ledger.controller, None);

		// `controller` is no longer in control. `stash` is now controller.
		assert_noop!(
			Staking::validate(RuntimeOrigin::signed(controller), ValidatorPrefs::default()),
			Error::<Test>::NotController,
		);
		assert_ok!(Staking::validate(RuntimeOrigin::signed(stash), ValidatorPrefs::default()));
	})
}

#[test]
fn change_controller_already_paired_once_stash() {
	ExtBuilder::default().build_and_execute(|| {
		// 11 and 11 are bonded as controller and stash respectively.
		assert_eq!(Staking::bonded(&11), Some(11));

		// 11 is initially a validator.
		assert_ok!(Staking::chill(RuntimeOrigin::signed(11)));

		// Controller cannot change once matching with stash.
		assert_noop!(
			Staking::set_controller(RuntimeOrigin::signed(11)),
			Error::<Test>::AlreadyPaired
		);
		assert_eq!(Staking::bonded(&11), Some(11));

		// 10 is no longer in control.
		assert_noop!(
			Staking::validate(RuntimeOrigin::signed(10), ValidatorPrefs::default()),
			Error::<Test>::NotController,
		);
		assert_ok!(Staking::validate(RuntimeOrigin::signed(11), ValidatorPrefs::default()));
	})
}
