// Copyright (C) 2021 Parity Technologies (UK) Ltd.
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

use crate as fixed;
use crate::{mock::*, Error};
use frame_support::{assert_noop, assert_ok};
use sp_runtime::{traits::BadOrigin, BuildStorage};

#[test]
fn basic_setup_works() {
	new_test_ext().execute_with(|| {
		assert_eq!(Fixed::collators(), vec![1, 2]);
	});
}

#[test]
fn add_collator_works() {
	new_test_ext().execute_with(|| {
		initialize_to_block(1);
		assert_eq!(Fixed::collators(), vec![1, 2]);
		let new = 3;

		// function runs
		assert_ok!(Fixed::add_collator(RuntimeOrigin::signed(RootAccount::get()), new));

		System::assert_last_event(RuntimeEvent::Fixed(crate::Event::CollatorAdded {
			account_id: new,
		}));

		// same element cannot be added more than once
		assert_noop!(
			Fixed::add_collator(RuntimeOrigin::signed(RootAccount::get()), new),
			Error::<Test>::AlreadyCollator
		);

		// new element is now part of the collator list
		assert!(Fixed::collators().to_vec().contains(&new));

		// cannot add with non-root
		assert_noop!(Fixed::add_collator(RuntimeOrigin::signed(1), new), BadOrigin);

		// cannot add collator without associated validator keys
		let not_validator = 42;
		assert_noop!(
			Fixed::add_collator(RuntimeOrigin::signed(RootAccount::get()), not_validator),
			Error::<Test>::ValidatorNotRegistered
		);
	});
}

#[test]
fn collator_limit_works() {
	new_test_ext().execute_with(|| {
		assert_eq!(Fixed::collators(), vec![1, 2]);

		// MaxCollators: u32 = 20
		for i in 3..21 {
			assert_ok!(Fixed::add_collator(RuntimeOrigin::signed(RootAccount::get()), i));
		}
		assert_noop!(
			Fixed::add_collator(RuntimeOrigin::signed(RootAccount::get()), 21),
			Error::<Test>::TooManyCollators
		);
		let expected: Vec<u64> = (1..=20).collect();
		let actual: Vec<_> = Fixed::collators().iter().cloned().collect();
		assert_eq!(actual, expected);
	});
}

#[test]
fn remove_collator_empty_list() {
	new_test_ext().execute_with(|| {
		initialize_to_block(1);
		assert_eq!(Fixed::collators(), vec![1, 2]);

		assert_ok!(Fixed::remove_collator(RuntimeOrigin::signed(RootAccount::get()), 1));
		assert_ok!(Fixed::remove_collator(RuntimeOrigin::signed(RootAccount::get()), 2));

		assert!(Fixed::collators().is_empty());

		// cannot remove collator not in the list since the list is empty
		assert_noop!(
			Fixed::remove_collator(RuntimeOrigin::signed(RootAccount::get()), 2),
			Error::<Test>::NotCollator
		);
	});
}

#[test]
fn remove_collator_works() {
	new_test_ext().execute_with(|| {
		initialize_to_block(1);
		assert_eq!(Fixed::collators(), vec![1, 2]);

		assert_ok!(Fixed::add_collator(RuntimeOrigin::signed(RootAccount::get()), 4));
		assert_ok!(Fixed::add_collator(RuntimeOrigin::signed(RootAccount::get()), 3));

		assert_eq!(Fixed::collators(), vec![1, 2, 3, 4]);

		assert_ok!(Fixed::remove_collator(RuntimeOrigin::signed(RootAccount::get()), 2));

		System::assert_last_event(RuntimeEvent::Fixed(crate::Event::CollatorRemoved {
			account_id: 2,
		}));
		assert_eq!(Fixed::collators(), vec![1, 3, 4]);

		// cannot remove collator not in the list
		assert_noop!(
			Fixed::remove_collator(RuntimeOrigin::signed(RootAccount::get()), 2),
			Error::<Test>::NotCollator
		);

		// cannot remove without privilege
		assert_noop!(Fixed::remove_collator(RuntimeOrigin::signed(1), 3), BadOrigin);
	});
}

#[test]
fn add_collator_edge_cases() {
	new_test_ext().execute_with(|| {
		initialize_to_block(1);
		assert_eq!(Fixed::collators(), vec![1, 2]);

		assert_ok!(Fixed::add_collator(RuntimeOrigin::signed(RootAccount::get()), 4));
		assert_ok!(Fixed::add_collator(RuntimeOrigin::signed(RootAccount::get()), 3));

		assert_eq!(Fixed::collators(), vec![1, 2, 3, 4]);

		System::assert_last_event(RuntimeEvent::Fixed(crate::Event::CollatorAdded {
			account_id: 3,
		}));
		assert_eq!(Fixed::collators(), vec![1, 2, 3, 4]);

		// cannot remove without privilege
		assert_noop!(Fixed::add_collator(RuntimeOrigin::signed(1), 5), BadOrigin);
	});
}

#[test]
fn remove_collator_edge_cases() {
	new_test_ext().execute_with(|| {
		initialize_to_block(1);
		assert_eq!(Fixed::collators(), vec![1, 2]);

		assert_ok!(Fixed::add_collator(RuntimeOrigin::signed(RootAccount::get()), 4));
		assert_ok!(Fixed::add_collator(RuntimeOrigin::signed(RootAccount::get()), 3));

		assert_eq!(Fixed::collators(), vec![1, 2, 3, 4]);

		assert_ok!(Fixed::remove_collator(RuntimeOrigin::signed(RootAccount::get()), 2));

		System::assert_last_event(RuntimeEvent::Fixed(crate::Event::CollatorRemoved {
			account_id: 2,
		}));
		assert_eq!(Fixed::collators(), vec![1, 3, 4]);

		// cannot remove collator not in the list
		assert_noop!(
			Fixed::remove_collator(RuntimeOrigin::signed(RootAccount::get()), 2),
			Error::<Test>::NotCollator
		);

		// cannot remove without privilege
		assert_noop!(Fixed::remove_collator(RuntimeOrigin::signed(1), 3), BadOrigin);
	});
}

#[test]
fn session_management_works() {
	new_test_ext().execute_with(|| {
		initialize_to_block(1);

		assert_eq!(SessionChangeBlock::get(), 0);
		assert_eq!(SessionHandlerCollators::get(), vec![1, 2]);

		initialize_to_block(4);

		assert_eq!(SessionChangeBlock::get(), 0);
		assert_eq!(SessionHandlerCollators::get(), vec![1, 2]);

		// add a new collator
		assert_ok!(Fixed::add_collator(RuntimeOrigin::signed(RootAccount::get()), 4));

		// session won't see this.
		assert_eq!(SessionHandlerCollators::get(), vec![1, 2]);
		// but we have a new candidate.
		assert_eq!(Fixed::collators().len(), 3);

		initialize_to_block(10);
		assert_eq!(SessionChangeBlock::get(), 10);
		// pallet-session has 1 session delay; current validators are the same.
		assert_eq!(Session::validators(), vec![1, 2]);
		// queued ones are changed, and now we have 3.
		assert_eq!(Session::queued_keys().len(), 3);
		// session handlers (aura, et. al.) cannot see this yet.
		assert_eq!(SessionHandlerCollators::get(), vec![1, 2]);

		assert_ok!(Fixed::add_collator(RuntimeOrigin::signed(RootAccount::get()), 3));
		assert_eq!(Fixed::collators().len(), 4);

		initialize_to_block(20);
		assert_eq!(SessionChangeBlock::get(), 20);
		// collator 4 addition is now reflected to session handlers.
		assert_eq!(SessionHandlerCollators::get(), vec![1, 2, 4]);

		// remove collator 2, this will show up in 2 sessions.
		assert_ok!(Fixed::remove_collator(RuntimeOrigin::signed(RootAccount::get()), 2));
		assert_eq!(Fixed::collators().len(), 3);

		initialize_to_block(30);
		assert_eq!(SessionChangeBlock::get(), 30);
		// collator 3 addition is now reflected to session handlers.
		assert_eq!(SessionHandlerCollators::get(), vec![1, 2, 3, 4]);

		initialize_to_block(40);
		assert_eq!(SessionChangeBlock::get(), 40);
		// collator 2 removal is now reflected to session handlers.
		assert_eq!(SessionHandlerCollators::get(), vec![1, 3, 4]);
	});
}

#[test]
#[should_panic = "duplicate collators in genesis."]
fn cannot_set_genesis_value_twice() {
	sp_tracing::try_init_simple();
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let collators = vec![1, 1];

	let fixed_collators = fixed::GenesisConfig::<Test> { collators };
	// collator selection must be initialized before session.
	fixed_collators.assimilate_storage(&mut t).unwrap();
}
