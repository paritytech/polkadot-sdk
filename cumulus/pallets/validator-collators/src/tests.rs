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

use crate::{
	mock::*, Call, Config, EraValidatorSet, Error, Event, MaxCollators, PendingRotation,
	RotationState, ValidatorSet,
};
use codec::{Decode, Encode};
use frame_support::{assert_noop, assert_ok, traits::UnfilteredDispatchable, BoundedBTreeSet};
use pallet_session::SessionManager;
use sp_runtime::{testing::UintAuthorityId, traits::BadOrigin, DispatchResult};
use sp_staking::EraIndex;

fn set_keys(who: u64) {
	let mut keys = MockSessionKeys { aura: UintAuthorityId(who) };
	let proof = keys.create_ownership_proof(&who.encode()).unwrap().encode();
	assert_ok!(Session::set_keys(RuntimeOrigin::signed(who), keys, proof));
}

fn bounded(validators: Vec<u64>) -> BoundedBTreeSet<u64, <Test as Config>::MaxValidators> {
	validators
		.into_iter()
		.collect::<std::collections::BTreeSet<_>>()
		.try_into()
		.unwrap()
}

fn receive(era: EraIndex, validators: Vec<u64>) -> DispatchResult {
	ValidatorCollators::set_validators(
		RuntimeOrigin::signed(SetAccount::get()),
		era,
		bounded(validators),
	)
}

fn register_candidate(who: u64) {
	assert_ok!(CollatorSelection::register_as_candidate(RuntimeOrigin::signed(who)));
}

#[test]
fn calls_reject_wrong_origin() {
	new_test_ext().execute_with(|| {
		// GIVEN accounts other than the configured origin of each call
		let not_set_origin = RootAccount::get();
		let not_update_origin = SetAccount::get();
		// WHEN they submit a validator set or a cap
		// THEN both calls fail with BadOrigin
		assert_noop!(
			ValidatorCollators::set_validators(
				RuntimeOrigin::signed(not_set_origin),
				1,
				bounded(vec![10])
			),
			BadOrigin
		);
		assert_noop!(
			ValidatorCollators::set_max_collators(
				RuntimeOrigin::signed(not_update_origin),
				Some(1)
			),
			BadOrigin
		);
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

#[test]
fn set_is_checked_against_the_stored_era_and_max_validators() {
	new_test_ext().execute_with(|| {
		// GIVEN a stored set for era 5
		initialize_to_block(1);
		assert_ok!(receive(5, vec![10, 11]));
		System::assert_last_event(Event::ValidatorSetReceived { era: 5, count: 2 }.into());
		// WHEN a set for the same or an older era arrives
		// THEN it fails with StaleEra and storage is unchanged
		assert_noop!(receive(5, vec![12]), Error::<Test>::StaleEra);
		assert_noop!(receive(4, vec![12]), Error::<Test>::StaleEra);
		assert_eq!(
			ValidatorSet::<Test>::get(),
			Some(EraValidatorSet { era: 5, validators: bounded(vec![10, 11]) })
		);
		assert_eq!(PendingRotation::<Test>::get(), RotationState::AwaitingQueue);
		// WHEN a newer set has one account more than MaxValidators
		// THEN it fails with TooManyValidators and storage is unchanged
		assert_noop!(
			ValidatorCollators::receive_validator_set(6, 100..151),
			Error::<Test>::TooManyValidators
		);
		// WHEN a newer list has MaxValidators accounts plus one listed twice
		// THEN it fits once merged and is stored with MaxValidators accounts
		assert_ok!(ValidatorCollators::receive_validator_set(6, (100..150).chain([100])));
		System::assert_last_event(Event::ValidatorSetReceived { era: 6, count: 50 }.into());
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

#[test]
fn call_with_more_than_max_validators_does_not_decode() {
	new_test_ext().execute_with(|| {
		// GIVEN encoded set_validators calls with MaxValidators and one more accounts
		let encoded = |n: u64| (0u8, 1 as EraIndex, (100..100 + n).collect::<Vec<u64>>()).encode();
		// WHEN they are decoded
		// THEN only the call within the bound decodes
		assert!(Call::<Test>::decode(&mut &encoded(51)[..]).is_err());
		assert!(matches!(
			Call::<Test>::decode(&mut &encoded(50)[..]),
			Ok(Call::set_validators { era: 1, .. })
		));
	});
}

#[test]
fn call_listing_an_account_twice_stores_it_once() {
	new_test_ext().execute_with(|| {
		// GIVEN a set_validators call encoded with validator 10 listed twice
		let encoded = (0u8, 1 as EraIndex, vec![11u64, 10, 10]).encode();
		// WHEN it is decoded and dispatched
		let call = Call::<Test>::decode(&mut &encoded[..]).unwrap();
		assert_ok!(call.dispatch_bypass_filter(RuntimeOrigin::signed(SetAccount::get())));
		// THEN the set holds each account once
		assert_eq!(
			ValidatorSet::<Test>::get().map(|set| set.validators),
			Some(bounded(vec![10, 11]))
		);
	});
}

#[test]
fn received_set_is_enacted_after_two_forced_rotations() {
	new_test_ext().execute_with(|| {
		// GIVEN invulnerables 1 and 2, candidate 4 and validators with keys
		initialize_to_block(1);
		register_candidate(4);
		set_keys(10);
		set_keys(11);
		initialize_to_block(3);
		assert_eq!(Session::current_index(), 0);
		// WHEN a set that repeats invulnerable 2 is received at block 3
		assert_ok!(receive(1, vec![11, 2, 10]));
		// THEN the session rotates at blocks 4 and 5 and then holds the deduplicated union, with
		// the validators in account order
		initialize_to_block(4);
		assert_eq!(Session::current_index(), 1);
		assert_eq!(PendingRotation::<Test>::get(), RotationState::AwaitingEnactment);
		initialize_to_block(5);
		assert_eq!(Session::current_index(), 2);
		assert_eq!(PendingRotation::<Test>::get(), RotationState::Idle);
		assert_eq!(Session::validators(), vec![1, 2, 4, 10, 11]);
		initialize_to_block(6);
		assert_eq!(Session::current_index(), 2);
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

#[test]
fn received_validator_without_keys_is_not_a_collator() {
	new_test_ext().execute_with(|| {
		// GIVEN validator 10 with keys and validator 11 without
		initialize_to_block(1);
		set_keys(10);
		// WHEN a set with both is received and enacted
		assert_ok!(receive(1, vec![10, 11]));
		initialize_to_block(3);
		// THEN only validator 10 is returned by the pallet and is a session validator
		assert_eq!(Session::validators(), vec![1, 2, 10]);
		assert_eq!(
			<ValidatorCollators as SessionManager<u64>>::new_session(Session::current_index()),
			Some(vec![10])
		);
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

#[test]
fn set_and_key_changes_reach_the_session_validators_at_the_next_rotations() {
	new_test_ext().execute_with(|| {
		// GIVEN the era 1 set of 10, 11 and 12 is enacted while only 10 and 11 have keys
		initialize_to_block(1);
		set_keys(10);
		set_keys(11);
		assert_ok!(receive(1, vec![10, 11, 12]));
		initialize_to_block(3);
		assert_eq!(Session::validators(), vec![1, 2, 10, 11]);
		// WHEN the era 2 set drops 10 and is enacted
		assert_ok!(receive(2, vec![11, 12]));
		initialize_to_block(5);
		// THEN 10 is no longer a session validator
		assert_eq!(Session::validators(), vec![1, 2, 11]);
		// WHEN 12 registers keys and 11 purges its keys after the forced rotations
		set_keys(12);
		assert_ok!(Session::purge_keys(RuntimeOrigin::signed(11)));
		// THEN the next periodic rotation queues 12 without 11 and the one after enacts it
		initialize_to_block(10);
		let queued = Session::queued_keys().into_iter().map(|(who, _)| who).collect::<Vec<_>>();
		assert_eq!(queued, vec![1, 2, 12]);
		assert_eq!(Session::validators(), vec![1, 2, 11]);
		initialize_to_block(20);
		assert_eq!(Session::validators(), vec![1, 2, 12]);
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

#[test]
fn without_pending_set_sessions_rotate_only_periodically() {
	new_test_ext().execute_with(|| {
		// GIVEN no received validator set
		assert_eq!(ValidatorSet::<Test>::get(), None);
		for block in 1..=35u64 {
			// WHEN blocks are produced
			initialize_to_block(block);
			// THEN the session index grows only at multiples of the period
			assert_eq!(Session::current_index() as u64, block / Period::get());
		}
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

#[test]
fn periodic_rotation_continues_after_forced_rotations() {
	new_test_ext().execute_with(|| {
		// GIVEN a set received at block 3 and enacted at block 5
		initialize_to_block(3);
		assert_ok!(receive(1, vec![10]));
		initialize_to_block(5);
		assert_eq!(Session::current_index(), 2);
		// WHEN blocks are produced up to the next period boundary
		initialize_to_block(9);
		assert_eq!(Session::current_index(), 2);
		initialize_to_block(10);
		// THEN the periodic rotation still happens
		assert_eq!(Session::current_index(), 3);
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

#[test]
fn cap_limits_only_the_validator_side_after_the_key_filter() {
	// (cap, validators returned by the pallet)
	let cases: [(Option<u32>, Vec<u64>); 5] = [
		(Some(0), vec![]),
		(Some(1), vec![11]),
		(Some(2), vec![11, 12]),
		(Some(10), vec![11, 12, 13]),
		(None, vec![11, 12, 13]),
	];
	for (cap, returned) in cases {
		new_test_ext().execute_with(|| {
			// GIVEN invulnerables 1 and 2, candidate 4 and a set whose first validator has no keys
			initialize_to_block(1);
			register_candidate(4);
			[11, 12, 13].into_iter().for_each(set_keys);
			// WHEN the cap is set and the set is received and enacted
			assert_ok!(ValidatorCollators::set_max_collators(
				RuntimeOrigin::signed(RootAccount::get()),
				cap
			));
			System::assert_last_event(Event::MaxCollatorsSet { max: cap }.into());
			assert_ok!(receive(1, vec![10, 11, 12, 13]));
			initialize_to_block(3);
			// THEN the cap counts only validators with keys and leaves the other side untouched
			assert_eq!(MaxCollators::<Test>::get(), cap);
			assert_eq!(
				<ValidatorCollators as SessionManager<u64>>::new_session(Session::current_index()),
				Some(returned.clone()),
				"cap {cap:?}"
			);
			let expected = [1, 2, 4].into_iter().chain(returned).collect::<Vec<_>>();
			assert_eq!(Session::validators(), expected, "cap {cap:?}");
			assert_ok!(ValidatorCollators::do_try_state());
		});
	}
	new_test_ext().execute_with(|| {
		// GIVEN invulnerables 1 and 2, candidate 4, and a set whose first validator is candidate 4
		initialize_to_block(1);
		register_candidate(4);
		[11, 12].into_iter().for_each(set_keys);
		// WHEN a cap of 1 is set and the set is received and enacted
		assert_ok!(ValidatorCollators::set_max_collators(
			RuntimeOrigin::signed(RootAccount::get()),
			Some(1)
		));
		assert_ok!(receive(1, vec![4, 11, 12]));
		initialize_to_block(3);
		// THEN candidate 4 takes the only place under the cap and adds no collator
		assert_eq!(
			<ValidatorCollators as SessionManager<u64>>::new_session(Session::current_index()),
			Some(vec![4])
		);
		assert_eq!(Session::validators(), vec![1, 2, 4]);
	});
}

#[test]
fn set_received_while_planned_rearms_the_rotations() {
	new_test_ext().execute_with(|| {
		// GIVEN a set for era 1 received at block 3 and queued at block 4
		initialize_to_block(1);
		set_keys(10);
		set_keys(11);
		initialize_to_block(3);
		assert_ok!(receive(1, vec![10]));
		initialize_to_block(4);
		assert_eq!(PendingRotation::<Test>::get(), RotationState::AwaitingEnactment);
		// WHEN a set for era 2 arrives at block 4
		assert_ok!(receive(2, vec![11]));
		// THEN the session rotates again at blocks 5 and 6 and the era 2 set is enacted
		assert_eq!(PendingRotation::<Test>::get(), RotationState::AwaitingQueue);
		initialize_to_block(5);
		assert_eq!(Session::current_index(), 2);
		initialize_to_block(6);
		assert_eq!(Session::current_index(), 3);
		assert_eq!(PendingRotation::<Test>::get(), RotationState::Idle);
		assert_eq!(Session::validators(), vec![1, 2, 11]);
		initialize_to_block(7);
		assert_eq!(Session::current_index(), 3);
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

#[test]
fn empty_set_is_accepted_and_contributes_no_collators() {
	new_test_ext().execute_with(|| {
		// GIVEN no stored validator set
		assert_eq!(ValidatorSet::<Test>::get(), None);
		// WHEN an empty set is received
		assert_ok!(receive(1, vec![]));
		// THEN the pallet returns an empty list rather than None
		assert_eq!(<ValidatorCollators as SessionManager<u64>>::new_session(1), Some(vec![]));
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

#[test]
fn try_state_detects_a_stored_set_that_does_not_decode() {
	new_test_ext().execute_with(|| {
		// GIVEN a stored set with more validators than MaxValidators
		initialize_to_block(1);
		frame_support::storage::unhashed::put_raw(
			&ValidatorSet::<Test>::hashed_key(),
			&(1u32, (0..51u64).collect::<Vec<_>>()).encode(),
		);
		// WHEN the invariants are checked and a session is planned
		// THEN try_state fails, and the pallet returns no validators and emits an event
		assert!(ValidatorCollators::do_try_state().is_err());
		assert_eq!(<ValidatorCollators as SessionManager<u64>>::new_session(1), None);
		System::assert_last_event(Event::StoredSetUndecodable.into());
	});
}
