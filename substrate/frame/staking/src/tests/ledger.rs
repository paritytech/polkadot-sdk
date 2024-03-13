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

//! Tests for the ledger.

use super::*;

#[test]
fn paired_account_works() {
	ExtBuilder::default().try_state(false).build_and_execute(|| {
		assert_ok!(Staking::bond(RuntimeOrigin::signed(10), 100, RewardDestination::Account(10)));

		assert_eq!(<Bonded<Test>>::get(&10), Some(10));
		assert_eq!(StakingLedger::<Test>::paired_account(StakingAccount::Controller(10)), Some(10));
		assert_eq!(StakingLedger::<Test>::paired_account(StakingAccount::Stash(10)), Some(10));

		assert_eq!(<Bonded<Test>>::get(&42), None);
		assert_eq!(StakingLedger::<Test>::paired_account(StakingAccount::Controller(42)), None);
		assert_eq!(StakingLedger::<Test>::paired_account(StakingAccount::Stash(42)), None);

		// bond manually stash with different controller. This is deprecated but the migration
		// has not been complete yet (controller: 100, stash: 200)
		assert_ok!(bond_controller_stash(100, 200));
		assert_eq!(<Bonded<Test>>::get(&200), Some(100));
		assert_eq!(
			StakingLedger::<Test>::paired_account(StakingAccount::Controller(100)),
			Some(200)
		);
		assert_eq!(StakingLedger::<Test>::paired_account(StakingAccount::Stash(200)), Some(100));
	})
}

#[test]
fn get_ledger_works() {
	ExtBuilder::default().try_state(false).build_and_execute(|| {
		// stash does not exist
		assert!(StakingLedger::<Test>::get(StakingAccount::Stash(42)).is_err());

		// bonded and paired
		assert_eq!(<Bonded<Test>>::get(&11), Some(11));

		match StakingLedger::<Test>::get(StakingAccount::Stash(11)) {
			Ok(ledger) => {
				assert_eq!(ledger.controller(), Some(11));
				assert_eq!(ledger.stash, 11);
			},
			Err(_) => panic!("staking ledger must exist"),
		};

		// bond manually stash with different controller. This is deprecated but the migration
		// has not been complete yet (controller: 100, stash: 200)
		assert_ok!(bond_controller_stash(100, 200));
		assert_eq!(<Bonded<Test>>::get(&200), Some(100));

		match StakingLedger::<Test>::get(StakingAccount::Stash(200)) {
			Ok(ledger) => {
				assert_eq!(ledger.controller(), Some(100));
				assert_eq!(ledger.stash, 200);
			},
			Err(_) => panic!("staking ledger must exist"),
		};

		match StakingLedger::<Test>::get(StakingAccount::Controller(100)) {
			Ok(ledger) => {
				assert_eq!(ledger.controller(), Some(100));
				assert_eq!(ledger.stash, 200);
			},
			Err(_) => panic!("staking ledger must exist"),
		};
	})
}

#[test]
fn bond_works() {
	ExtBuilder::default().build_and_execute(|| {
		assert!(!StakingLedger::<Test>::is_bonded(StakingAccount::Stash(42)));
		assert!(<Bonded<Test>>::get(&42).is_none());

		let mut ledger: StakingLedger<Test> = StakingLedger::default_from(42);
		let reward_dest = RewardDestination::Account(10);

		assert_ok!(ledger.clone().bond(reward_dest));
		assert!(StakingLedger::<Test>::is_bonded(StakingAccount::Stash(42)));
		assert!(<Bonded<Test>>::get(&42).is_some());
		assert_eq!(<Payee<Test>>::get(&42), Some(reward_dest));

		// cannot bond again.
		assert!(ledger.clone().bond(reward_dest).is_err());

		// once bonded, update works as expected.
		ledger.legacy_claimed_rewards = bounded_vec![1];
		assert_ok!(ledger.update());
	})
}

#[test]
fn is_bonded_works() {
	ExtBuilder::default().build_and_execute(|| {
		assert!(!StakingLedger::<Test>::is_bonded(StakingAccount::Stash(42)));
		assert!(!StakingLedger::<Test>::is_bonded(StakingAccount::Controller(42)));

		// adds entry to Bonded without Ledger pair (should not happen).
		<Bonded<Test>>::insert(42, 42);
		assert!(!StakingLedger::<Test>::is_bonded(StakingAccount::Controller(42)));

		assert_eq!(<Bonded<Test>>::get(&11), Some(11));
		assert!(StakingLedger::<Test>::is_bonded(StakingAccount::Stash(11)));
		assert!(StakingLedger::<Test>::is_bonded(StakingAccount::Controller(11)));

		<Bonded<Test>>::remove(42); // ensures try-state checks pass.
	})
}

#[test]
#[allow(deprecated)]
fn set_payee_errors_on_controller_destination() {
	ExtBuilder::default().build_and_execute(|| {
		Payee::<Test>::insert(11, RewardDestination::Staked);
		assert_noop!(
			Staking::set_payee(RuntimeOrigin::signed(11), RewardDestination::Controller),
			Error::<Test>::ControllerDeprecated
		);
		assert_eq!(Payee::<Test>::get(&11), Some(RewardDestination::Staked));
	})
}

#[test]
#[allow(deprecated)]
fn update_payee_migration_works() {
	ExtBuilder::default().build_and_execute(|| {
		// migrate a `Controller` variant to `Account` variant.
		Payee::<Test>::insert(11, RewardDestination::Controller);
		assert_eq!(Payee::<Test>::get(&11), Some(RewardDestination::Controller));
		assert_ok!(Staking::update_payee(RuntimeOrigin::signed(11), 11));
		assert_eq!(Payee::<Test>::get(&11), Some(RewardDestination::Account(11)));

		// Do not migrate a variant if not `Controller`.
		Payee::<Test>::insert(21, RewardDestination::Stash);
		assert_eq!(Payee::<Test>::get(&21), Some(RewardDestination::Stash));
		assert_noop!(
			Staking::update_payee(RuntimeOrigin::signed(11), 21),
			Error::<Test>::NotController
		);
		assert_eq!(Payee::<Test>::get(&21), Some(RewardDestination::Stash));
	})
}

#[test]
fn deprecate_controller_batch_works_full_weight() {
	ExtBuilder::default().build_and_execute(|| {
		// Given:

		let start = 1001;
		let mut controllers: Vec<_> = vec![];
		for n in start..(start + MaxControllersInDeprecationBatch::get()).into() {
			let ctlr: u64 = n.into();
			let stash: u64 = (n + 10000).into();

			Ledger::<Test>::insert(
				ctlr,
				StakingLedger {
					controller: None,
					total: (10 + ctlr).into(),
					active: (10 + ctlr).into(),
					..StakingLedger::default_from(stash)
				},
			);
			Bonded::<Test>::insert(stash, ctlr);
			Payee::<Test>::insert(stash, RewardDestination::Staked);

			controllers.push(ctlr);
		}

		// When:

		let bounded_controllers: BoundedVec<_, <Test as Config>::MaxControllersInDeprecationBatch> =
			BoundedVec::try_from(controllers).unwrap();

		// Only `AdminOrigin` can sign.
		assert_noop!(
			Staking::deprecate_controller_batch(
				RuntimeOrigin::signed(2),
				bounded_controllers.clone()
			),
			BadOrigin
		);

		let result =
			Staking::deprecate_controller_batch(RuntimeOrigin::root(), bounded_controllers);
		assert_ok!(result);
		assert_eq!(
			result.unwrap().actual_weight.unwrap(),
			<Test as Config>::WeightInfo::deprecate_controller_batch(
				<Test as Config>::MaxControllersInDeprecationBatch::get()
			)
		);

		// Then:

		for n in start..(start + MaxControllersInDeprecationBatch::get()).into() {
			let ctlr: u64 = n.into();
			let stash: u64 = (n + 10000).into();

			// Ledger no longer keyed by controller.
			assert_eq!(Ledger::<Test>::get(ctlr), None);
			// Bonded now maps to the stash.
			assert_eq!(Bonded::<Test>::get(stash), Some(stash));

			// Ledger is now keyed by stash.
			let ledger_updated = Ledger::<Test>::get(stash).unwrap();
			assert_eq!(ledger_updated.stash, stash);

			// Check `active` and `total` values match the original ledger set by controller.
			assert_eq!(ledger_updated.active, (10 + ctlr).into());
			assert_eq!(ledger_updated.total, (10 + ctlr).into());
		}
	})
}

#[test]
fn deprecate_controller_batch_works_half_weight() {
	ExtBuilder::default().build_and_execute(|| {
		// Given:

		let start = 1001;
		let mut controllers: Vec<_> = vec![];
		for n in start..(start + MaxControllersInDeprecationBatch::get()).into() {
			let ctlr: u64 = n.into();

			// Only half of entries are unique pairs.
			let stash: u64 = if n % 2 == 0 { (n + 10000).into() } else { ctlr };

			Ledger::<Test>::insert(
				ctlr,
				StakingLedger { controller: None, ..StakingLedger::default_from(stash) },
			);
			Bonded::<Test>::insert(stash, ctlr);
			Payee::<Test>::insert(stash, RewardDestination::Staked);

			controllers.push(ctlr);
		}

		// When:
		let bounded_controllers: BoundedVec<_, <Test as Config>::MaxControllersInDeprecationBatch> =
			BoundedVec::try_from(controllers.clone()).unwrap();

		let result =
			Staking::deprecate_controller_batch(RuntimeOrigin::root(), bounded_controllers);
		assert_ok!(result);
		assert_eq!(
			result.unwrap().actual_weight.unwrap(),
			<Test as Config>::WeightInfo::deprecate_controller_batch(controllers.len() as u32)
		);

		// Then:

		for n in start..(start + MaxControllersInDeprecationBatch::get()).into() {
			let unique_pair = n % 2 == 0;
			let ctlr: u64 = n.into();
			let stash: u64 = if unique_pair { (n + 10000).into() } else { ctlr };

			// Side effect of migration for unique pair.
			if unique_pair {
				assert_eq!(Ledger::<Test>::get(ctlr), None);
			}
			// Bonded maps to the stash.
			assert_eq!(Bonded::<Test>::get(stash), Some(stash));

			// Ledger is keyed by stash.
			let ledger_updated = Ledger::<Test>::get(stash).unwrap();
			assert_eq!(ledger_updated.stash, stash);
		}
	})
}

#[test]
fn deprecate_controller_batch_skips_unmigrated_controller_payees() {
	ExtBuilder::default().try_state(false).build_and_execute(|| {
		// Given:

		let stash: u64 = 1000;
		let ctlr: u64 = 1001;

		Ledger::<Test>::insert(
			ctlr,
			StakingLedger { controller: None, ..StakingLedger::default_from(stash) },
		);
		Bonded::<Test>::insert(stash, ctlr);
		#[allow(deprecated)]
		Payee::<Test>::insert(stash, RewardDestination::Controller);

		// When:

		let bounded_controllers: BoundedVec<_, <Test as Config>::MaxControllersInDeprecationBatch> =
			BoundedVec::try_from(vec![ctlr]).unwrap();

		let result =
			Staking::deprecate_controller_batch(RuntimeOrigin::root(), bounded_controllers);
		assert_ok!(result);
		assert_eq!(
			result.unwrap().actual_weight.unwrap(),
			<Test as Config>::WeightInfo::deprecate_controller_batch(1 as u32)
		);

		// Then:

		// Esure deprecation did not happen.
		assert_eq!(Ledger::<Test>::get(ctlr).is_some(), true);

		// Bonded still keyed by controller.
		assert_eq!(Bonded::<Test>::get(stash), Some(ctlr));

		// Ledger is still keyed by controller.
		let ledger_updated = Ledger::<Test>::get(ctlr).unwrap();
		assert_eq!(ledger_updated.stash, stash);
	})
}
