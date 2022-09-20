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

use crate as collator_selection;
use crate::{mock::*, CandidateInfo, Error};
use frame_support::{
	assert_noop, assert_ok,
	traits::{Currency, GenesisBuild, OnInitialize},
};
use pallet_balances::Error as BalancesError;
use sp_runtime::traits::BadOrigin;

#[test]
fn basic_setup_works() {
	new_test_ext().execute_with(|| {
		assert_eq!(CollatorSelection::desired_candidates(), 2);
		assert_eq!(CollatorSelection::candidacy_bond(), 10);

		assert!(CollatorSelection::candidates().is_empty());
		assert_eq!(CollatorSelection::invulnerables(), vec![1, 2]);
	});
}

#[test]
fn it_should_set_invulnerables() {
	new_test_ext().execute_with(|| {
		let new_set = vec![1, 2, 3, 4];
		assert_ok!(CollatorSelection::set_invulnerables(
			RuntimeOrigin::signed(RootAccount::get()),
			new_set.clone()
		));
		assert_eq!(CollatorSelection::invulnerables(), new_set);

		// cannot set with non-root.
		assert_noop!(
			CollatorSelection::set_invulnerables(RuntimeOrigin::signed(1), new_set.clone()),
			BadOrigin
		);

		// cannot set invulnerables without associated validator keys
		let invulnerables = vec![7];
		assert_noop!(
			CollatorSelection::set_invulnerables(
				RuntimeOrigin::signed(RootAccount::get()),
				invulnerables.clone()
			),
			Error::<Test>::ValidatorNotRegistered
		);
	});
}

#[test]
fn set_desired_candidates_works() {
	new_test_ext().execute_with(|| {
		// given
		assert_eq!(CollatorSelection::desired_candidates(), 2);

		// can set
		assert_ok!(CollatorSelection::set_desired_candidates(
			RuntimeOrigin::signed(RootAccount::get()),
			7
		));
		assert_eq!(CollatorSelection::desired_candidates(), 7);

		// rejects bad origin
		assert_noop!(
			CollatorSelection::set_desired_candidates(RuntimeOrigin::signed(1), 8),
			BadOrigin
		);
	});
}

#[test]
fn set_candidacy_bond() {
	new_test_ext().execute_with(|| {
		// given
		assert_eq!(CollatorSelection::candidacy_bond(), 10);

		// can set
		assert_ok!(CollatorSelection::set_candidacy_bond(
			RuntimeOrigin::signed(RootAccount::get()),
			7
		));
		assert_eq!(CollatorSelection::candidacy_bond(), 7);

		// rejects bad origin.
		assert_noop!(CollatorSelection::set_candidacy_bond(RuntimeOrigin::signed(1), 8), BadOrigin);
	});
}

#[test]
fn cannot_register_candidate_if_too_many() {
	new_test_ext().execute_with(|| {
		// reset desired candidates:
		<crate::DesiredCandidates<Test>>::put(0);

		// can't accept anyone anymore.
		assert_noop!(
			CollatorSelection::register_as_candidate(RuntimeOrigin::signed(3)),
			Error::<Test>::TooManyCandidates,
		);

		// reset desired candidates:
		<crate::DesiredCandidates<Test>>::put(1);
		assert_ok!(CollatorSelection::register_as_candidate(RuntimeOrigin::signed(4)));

		// but no more
		assert_noop!(
			CollatorSelection::register_as_candidate(RuntimeOrigin::signed(5)),
			Error::<Test>::TooManyCandidates,
		);
	})
}

#[test]
fn cannot_unregister_candidate_if_too_few() {
	new_test_ext().execute_with(|| {
		// reset desired candidates:
		<crate::DesiredCandidates<Test>>::put(1);
		assert_ok!(CollatorSelection::register_as_candidate(RuntimeOrigin::signed(4)));

		// can not remove too few
		assert_noop!(
			CollatorSelection::leave_intent(RuntimeOrigin::signed(4)),
			Error::<Test>::TooFewCandidates,
		);
	})
}

#[test]
fn cannot_register_as_candidate_if_invulnerable() {
	new_test_ext().execute_with(|| {
		assert_eq!(CollatorSelection::invulnerables(), vec![1, 2]);

		// can't 1 because it is invulnerable.
		assert_noop!(
			CollatorSelection::register_as_candidate(RuntimeOrigin::signed(1)),
			Error::<Test>::AlreadyInvulnerable,
		);
	})
}

#[test]
fn cannot_register_as_candidate_if_keys_not_registered() {
	new_test_ext().execute_with(|| {
		// can't 7 because keys not registered.
		assert_noop!(
			CollatorSelection::register_as_candidate(RuntimeOrigin::signed(7)),
			Error::<Test>::ValidatorNotRegistered
		);
	})
}

#[test]
fn cannot_register_dupe_candidate() {
	new_test_ext().execute_with(|| {
		// can add 3 as candidate
		assert_ok!(CollatorSelection::register_as_candidate(RuntimeOrigin::signed(3)));
		let addition = CandidateInfo { who: 3, deposit: 10 };
		assert_eq!(CollatorSelection::candidates(), vec![addition]);
		assert_eq!(CollatorSelection::last_authored_block(3), 10);
		assert_eq!(Balances::free_balance(3), 90);

		// but no more
		assert_noop!(
			CollatorSelection::register_as_candidate(RuntimeOrigin::signed(3)),
			Error::<Test>::AlreadyCandidate,
		);
	})
}

#[test]
fn cannot_register_as_candidate_if_poor() {
	new_test_ext().execute_with(|| {
		assert_eq!(Balances::free_balance(&3), 100);
		assert_eq!(Balances::free_balance(&33), 0);

		// works
		assert_ok!(CollatorSelection::register_as_candidate(RuntimeOrigin::signed(3)));

		// poor
		assert_noop!(
			CollatorSelection::register_as_candidate(RuntimeOrigin::signed(33)),
			BalancesError::<Test>::InsufficientBalance,
		);
	});
}

#[test]
fn register_as_candidate_works() {
	new_test_ext().execute_with(|| {
		// given
		assert_eq!(CollatorSelection::desired_candidates(), 2);
		assert_eq!(CollatorSelection::candidacy_bond(), 10);
		assert_eq!(CollatorSelection::candidates(), Vec::new());
		assert_eq!(CollatorSelection::invulnerables(), vec![1, 2]);

		// take two endowed, non-invulnerables accounts.
		assert_eq!(Balances::free_balance(&3), 100);
		assert_eq!(Balances::free_balance(&4), 100);

		assert_ok!(CollatorSelection::register_as_candidate(RuntimeOrigin::signed(3)));
		assert_ok!(CollatorSelection::register_as_candidate(RuntimeOrigin::signed(4)));

		assert_eq!(Balances::free_balance(&3), 90);
		assert_eq!(Balances::free_balance(&4), 90);

		assert_eq!(CollatorSelection::candidates().len(), 2);
	});
}

#[test]
fn leave_intent() {
	new_test_ext().execute_with(|| {
		// register a candidate.
		assert_ok!(CollatorSelection::register_as_candidate(RuntimeOrigin::signed(3)));
		assert_eq!(Balances::free_balance(3), 90);

		// register too so can leave above min candidates
		assert_ok!(CollatorSelection::register_as_candidate(RuntimeOrigin::signed(5)));
		assert_eq!(Balances::free_balance(5), 90);

		// cannot leave if not candidate.
		assert_noop!(
			CollatorSelection::leave_intent(RuntimeOrigin::signed(4)),
			Error::<Test>::NotCandidate
		);

		// bond is returned
		assert_ok!(CollatorSelection::leave_intent(RuntimeOrigin::signed(3)));
		assert_eq!(Balances::free_balance(3), 100);
		assert_eq!(CollatorSelection::last_authored_block(3), 0);
	});
}

#[test]
fn authorship_event_handler() {
	new_test_ext().execute_with(|| {
		// put 100 in the pot + 5 for ED
		Balances::make_free_balance_be(&CollatorSelection::account_id(), 105);

		// 4 is the default author.
		assert_eq!(Balances::free_balance(4), 100);
		assert_ok!(CollatorSelection::register_as_candidate(RuntimeOrigin::signed(4)));
		// triggers `note_author`
		Authorship::on_initialize(1);

		let collator = CandidateInfo { who: 4, deposit: 10 };

		assert_eq!(CollatorSelection::candidates(), vec![collator]);
		assert_eq!(CollatorSelection::last_authored_block(4), 0);

		// half of the pot goes to the collator who's the author (4 in tests).
		assert_eq!(Balances::free_balance(4), 140);
		// half + ED stays.
		assert_eq!(Balances::free_balance(CollatorSelection::account_id()), 55);
	});
}

#[test]
fn fees_edgecases() {
	new_test_ext().execute_with(|| {
		// Nothing panics, no reward when no ED in balance
		Authorship::on_initialize(1);
		// put some money into the pot at ED
		Balances::make_free_balance_be(&CollatorSelection::account_id(), 5);
		// 4 is the default author.
		assert_eq!(Balances::free_balance(4), 100);
		assert_ok!(CollatorSelection::register_as_candidate(RuntimeOrigin::signed(4)));
		// triggers `note_author`
		Authorship::on_initialize(1);

		let collator = CandidateInfo { who: 4, deposit: 10 };

		assert_eq!(CollatorSelection::candidates(), vec![collator]);
		assert_eq!(CollatorSelection::last_authored_block(4), 0);
		// Nothing received
		assert_eq!(Balances::free_balance(4), 90);
		// all fee stays
		assert_eq!(Balances::free_balance(CollatorSelection::account_id()), 5);
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
		assert_ok!(CollatorSelection::register_as_candidate(RuntimeOrigin::signed(3)));

		// session won't see this.
		assert_eq!(SessionHandlerCollators::get(), vec![1, 2]);
		// but we have a new candidate.
		assert_eq!(CollatorSelection::candidates().len(), 1);

		initialize_to_block(10);
		assert_eq!(SessionChangeBlock::get(), 10);
		// pallet-session has 1 session delay; current validators are the same.
		assert_eq!(Session::validators(), vec![1, 2]);
		// queued ones are changed, and now we have 3.
		assert_eq!(Session::queued_keys().len(), 3);
		// session handlers (aura, et. al.) cannot see this yet.
		assert_eq!(SessionHandlerCollators::get(), vec![1, 2]);

		initialize_to_block(20);
		assert_eq!(SessionChangeBlock::get(), 20);
		// changed are now reflected to session handlers.
		assert_eq!(SessionHandlerCollators::get(), vec![1, 2, 3]);
	});
}

#[test]
fn kick_mechanism() {
	new_test_ext().execute_with(|| {
		// add a new collator
		assert_ok!(CollatorSelection::register_as_candidate(RuntimeOrigin::signed(3)));
		assert_ok!(CollatorSelection::register_as_candidate(RuntimeOrigin::signed(4)));
		initialize_to_block(10);
		assert_eq!(CollatorSelection::candidates().len(), 2);
		initialize_to_block(20);
		assert_eq!(SessionChangeBlock::get(), 20);
		// 4 authored this block, gets to stay 3 was kicked
		assert_eq!(CollatorSelection::candidates().len(), 1);
		// 3 will be kicked after 1 session delay
		assert_eq!(SessionHandlerCollators::get(), vec![1, 2, 3, 4]);
		let collator = CandidateInfo { who: 4, deposit: 10 };
		assert_eq!(CollatorSelection::candidates(), vec![collator]);
		assert_eq!(CollatorSelection::last_authored_block(4), 20);
		initialize_to_block(30);
		// 3 gets kicked after 1 session delay
		assert_eq!(SessionHandlerCollators::get(), vec![1, 2, 4]);
		// kicked collator gets funds back
		assert_eq!(Balances::free_balance(3), 100);
	});
}

#[test]
fn should_not_kick_mechanism_too_few() {
	new_test_ext().execute_with(|| {
		// add a new collator
		assert_ok!(CollatorSelection::register_as_candidate(RuntimeOrigin::signed(3)));
		assert_ok!(CollatorSelection::register_as_candidate(RuntimeOrigin::signed(5)));
		initialize_to_block(10);
		assert_eq!(CollatorSelection::candidates().len(), 2);
		initialize_to_block(20);
		assert_eq!(SessionChangeBlock::get(), 20);
		// 4 authored this block, 5 gets to stay too few 3 was kicked
		assert_eq!(CollatorSelection::candidates().len(), 1);
		// 3 will be kicked after 1 session delay
		assert_eq!(SessionHandlerCollators::get(), vec![1, 2, 3, 5]);
		let collator = CandidateInfo { who: 5, deposit: 10 };
		assert_eq!(CollatorSelection::candidates(), vec![collator]);
		assert_eq!(CollatorSelection::last_authored_block(4), 20);
		initialize_to_block(30);
		// 3 gets kicked after 1 session delay
		assert_eq!(SessionHandlerCollators::get(), vec![1, 2, 5]);
		// kicked collator gets funds back
		assert_eq!(Balances::free_balance(3), 100);
	});
}

#[test]
#[should_panic = "duplicate invulnerables in genesis."]
fn cannot_set_genesis_value_twice() {
	sp_tracing::try_init_simple();
	let mut t = frame_system::GenesisConfig::default().build_storage::<Test>().unwrap();
	let invulnerables = vec![1, 1];

	let collator_selection = collator_selection::GenesisConfig::<Test> {
		desired_candidates: 2,
		candidacy_bond: 10,
		invulnerables,
	};
	// collator selection must be initialized before session.
	collator_selection.assimilate_storage(&mut t).unwrap();
}
