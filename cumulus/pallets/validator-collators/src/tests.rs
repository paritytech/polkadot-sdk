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
	mock::*, Call, Config, EraValidatorSet, Error, Event, MaxCollators, OutgoingAnnouncements,
	PendingRotation, RotationState, UnionSessionManager, ValidatorSet,
};
use codec::{Decode, Encode};
use frame_support::{assert_err, assert_noop, assert_ok, parameter_types, BoundedVec};
use pallet_session::SessionManager;
use sp_runtime::{testing::UintAuthorityId, traits::BadOrigin, DispatchResult};
use sp_staking::{EraIndex, SessionIndex};

fn set_keys(who: u64) {
	let mut keys = MockSessionKeys { aura: UintAuthorityId(who) };
	let proof = keys.create_ownership_proof(&who.encode()).unwrap().encode();
	assert_ok!(Session::set_keys(RuntimeOrigin::signed(who), keys, proof));
}

fn bounded(validators: Vec<u64>) -> BoundedVec<u64, <Test as Config>::MaxValidators> {
	validators.try_into().unwrap()
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
fn set_with_era_not_newer_than_stored_is_rejected() {
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
			Some(EraValidatorSet { era: 5, validators: vec![10, 11].try_into().unwrap() })
		);
		assert_eq!(PendingRotation::<Test>::get(), RotationState::ToPlan);
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
fn set_of_exactly_max_validators_is_accepted() {
	new_test_ext().execute_with(|| {
		// GIVEN a set of exactly MaxValidators accounts
		let full = (100..150).collect::<Vec<u64>>();
		// WHEN it is submitted
		// THEN it is stored
		assert_ok!(receive(1, full));
		assert_eq!(ValidatorSet::<Test>::get().map(|set| set.validators.len()), Some(50));
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

#[test]
fn set_with_duplicate_accounts_is_rejected() {
	new_test_ext().execute_with(|| {
		// GIVEN a set that lists validator 10 twice
		let duplicated = vec![10, 10, 11];
		// WHEN it is submitted
		// THEN it fails with DuplicateValidator and nothing is stored
		assert_noop!(receive(1, duplicated), Error::<Test>::DuplicateValidator);
		assert_eq!(ValidatorSet::<Test>::get(), None);
		assert_ok!(ValidatorCollators::do_try_state());
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
		// THEN the session rotates at blocks 4 and 5 and then holds the deduplicated union
		initialize_to_block(4);
		assert_eq!(Session::current_index(), 1);
		assert_eq!(PendingRotation::<Test>::get(), RotationState::Planned);
		initialize_to_block(5);
		assert_eq!(Session::current_index(), 2);
		assert_eq!(PendingRotation::<Test>::get(), RotationState::Idle);
		assert_eq!(Session::validators(), vec![1, 2, 4, 11, 10]);
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
fn cap_truncates_only_the_validator_side() {
	new_test_ext().execute_with(|| {
		// GIVEN invulnerables 1 and 2, candidate 4 and five validators with keys
		initialize_to_block(1);
		register_candidate(4);
		(10..15).for_each(set_keys);
		// WHEN the cap is set to 2 and the set is received and enacted
		assert_ok!(ValidatorCollators::set_max_collators(
			RuntimeOrigin::signed(RootAccount::get()),
			Some(2)
		));
		System::assert_last_event(Event::MaxCollatorsSet { max: Some(2) }.into());
		assert_ok!(receive(1, (10..15).collect()));
		initialize_to_block(3);
		// THEN the first two validators join all invulnerables and candidates
		assert_eq!(MaxCollators::<Test>::get(), Some(2));
		assert_eq!(Session::validators(), vec![1, 2, 4, 10, 11]);
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

#[test]
fn keys_are_checked_before_the_cap_is_applied() {
	new_test_ext().execute_with(|| {
		// GIVEN a cap of 1 and a set whose first validator has no keys
		initialize_to_block(1);
		set_keys(11);
		assert_ok!(ValidatorCollators::set_max_collators(
			RuntimeOrigin::signed(RootAccount::get()),
			Some(1)
		));
		// WHEN the set is received and enacted
		assert_ok!(receive(1, vec![10, 11]));
		initialize_to_block(3);
		// THEN the validator with keys fills the cap
		assert_eq!(
			<ValidatorCollators as SessionManager<u64>>::new_session(Session::current_index()),
			Some(vec![11])
		);
		assert_eq!(Session::validators(), vec![1, 2, 11]);
		assert_ok!(ValidatorCollators::do_try_state());
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
		assert_eq!(PendingRotation::<Test>::get(), RotationState::Planned);
		// WHEN a set for era 2 arrives at block 4
		assert_ok!(receive(2, vec![11]));
		// THEN the session rotates again at blocks 5 and 6 and the era 2 set is enacted
		assert_eq!(PendingRotation::<Test>::get(), RotationState::ToPlan);
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
fn zero_cap_leaves_only_the_collator_selection_side() {
	new_test_ext().execute_with(|| {
		// GIVEN invulnerables 1 and 2, candidate 4 and a validator with keys
		initialize_to_block(1);
		register_candidate(4);
		set_keys(10);
		// WHEN the cap is set to 0 and the set is received and enacted
		assert_ok!(ValidatorCollators::set_max_collators(
			RuntimeOrigin::signed(RootAccount::get()),
			Some(0)
		));
		assert_ok!(receive(1, vec![10]));
		initialize_to_block(3);
		// THEN no validator is added and invulnerables and candidates remain
		assert_eq!(
			<ValidatorCollators as SessionManager<u64>>::new_session(Session::current_index()),
			Some(vec![])
		);
		assert_eq!(Session::validators(), vec![1, 2, 4]);
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

#[test]
fn cap_larger_than_the_set_returns_every_validator_with_keys() {
	new_test_ext().execute_with(|| {
		// GIVEN a cap of 10 and a set of three validators of which two have keys
		set_keys(10);
		set_keys(12);
		assert_ok!(ValidatorCollators::set_max_collators(
			RuntimeOrigin::signed(RootAccount::get()),
			Some(10)
		));
		// WHEN the set is received
		assert_ok!(receive(1, vec![10, 11, 12]));
		// THEN the pallet returns both validators with keys
		assert_eq!(<ValidatorCollators as SessionManager<u64>>::new_session(1), Some(vec![10, 12]));
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
		frame_support::storage::unhashed::put_raw(
			&ValidatorSet::<Test>::hashed_key(),
			&(1u32, (0..51u64).collect::<Vec<_>>()).encode(),
		);
		// WHEN the invariants are checked and a session is planned
		// THEN try_state fails and the pallet returns no validators
		assert!(ValidatorCollators::do_try_state().is_err());
		assert_eq!(<ValidatorCollators as SessionManager<u64>>::new_session(1), None);
	});
}

fn announce(era: EraIndex, validators: Vec<u64>) -> DispatchResult {
	ValidatorCollators::announce(era, &validators)
}

fn announcement_events() -> Vec<Event<Test>> {
	System::events()
		.into_iter()
		.filter_map(|record| match record.event {
			RuntimeEvent::ValidatorCollators(
				event @ (Event::AnnouncementSent { .. } |
				Event::AnnouncementFailed { .. } |
				Event::AnnouncementDropped { .. } |
				Event::AnnouncementRejected { .. }),
			) => Some(event),
			_ => None,
		})
		.collect()
}

fn outgoing() -> Vec<(u32, u32)> {
	let mut outgoing = OutgoingAnnouncements::<Test>::iter().collect::<Vec<_>>();
	outgoing.sort();
	outgoing
}

#[test]
fn announce_stores_the_set_and_sends_it_to_every_destination_in_the_next_block() {
	new_test_ext().execute_with(|| {
		// GIVEN two destinations that accept the set
		initialize_to_block(1);
		// WHEN a set is announced
		assert_ok!(announce(1, vec![10, 11]));
		assert_eq!(ValidatorSet::<Test>::get().map(|set| set.era), Some(1));
		assert_eq!(outgoing(), vec![(1, 2), (2, 2)]);
		assert!(Sent::get().is_empty());
		initialize_to_block(2);
		// THEN the next block sends it to both destinations and empties the queue
		assert_eq!(Sent::get(), vec![(1, 1, vec![10, 11]), (2, 1, vec![10, 11])]);
		assert_eq!(outgoing(), vec![]);
		assert_eq!(
			announcement_events(),
			vec![
				Event::AnnouncementSent { destination: 1, era: 1 },
				Event::AnnouncementSent { destination: 2, era: 1 },
			]
		);
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

#[test]
fn failed_send_is_retried_up_to_the_limit_and_then_dropped() {
	new_test_ext().execute_with(|| {
		// GIVEN destination 2 rejects every send and two retries are allowed
		initialize_to_block(1);
		FailingDestinations::set(vec![2]);
		// WHEN a set is announced and three blocks pass
		assert_ok!(announce(1, vec![10]));
		initialize_to_block(4);
		// THEN destination 2 fails three times, is dropped and destination 1 is sent once
		assert_eq!(Sent::get(), vec![(1, 1, vec![10])]);
		assert_eq!(outgoing(), vec![]);
		assert_eq!(
			announcement_events(),
			vec![
				Event::AnnouncementSent { destination: 1, era: 1 },
				Event::AnnouncementFailed { destination: 2, era: 1, retries_left: 1 },
				Event::AnnouncementFailed { destination: 2, era: 1, retries_left: 0 },
				Event::AnnouncementDropped { destination: 2, era: 1 },
			]
		);
		initialize_to_block(5);
		assert_eq!(Sent::get().len(), 1);
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

#[test]
fn failed_send_succeeds_on_a_later_retry() {
	new_test_ext().execute_with(|| {
		// GIVEN destination 2 rejects the first send of an announced set
		initialize_to_block(1);
		FailingDestinations::set(vec![2]);
		assert_ok!(announce(1, vec![10]));
		initialize_to_block(2);
		assert_eq!(outgoing(), vec![(2, 1)]);
		// WHEN destination 2 accepts again
		FailingDestinations::set(vec![]);
		initialize_to_block(3);
		// THEN the retry delivers the set and the queue is empty
		assert_eq!(Sent::get(), vec![(1, 1, vec![10]), (2, 1, vec![10])]);
		assert_eq!(outgoing(), vec![]);
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

#[test]
fn newer_era_replaces_a_queued_announcement() {
	new_test_ext().execute_with(|| {
		// GIVEN the era 1 set is still queued after a failed send to both destinations
		initialize_to_block(1);
		FailingDestinations::set(vec![1, 2]);
		assert_ok!(announce(1, vec![10]));
		initialize_to_block(2);
		assert_eq!(outgoing(), vec![(1, 1), (2, 1)]);
		// WHEN the era 2 set is announced and the destinations accept again
		assert_ok!(announce(2, vec![11]));
		FailingDestinations::set(vec![]);
		// THEN retries are reset and only the era 2 set is delivered
		assert_eq!(outgoing(), vec![(1, 2), (2, 2)]);
		initialize_to_block(3);
		assert_eq!(Sent::get(), vec![(1, 2, vec![11]), (2, 2, vec![11])]);
		assert_eq!(outgoing(), vec![]);
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

#[test]
fn rejected_announcement_queues_nothing() {
	new_test_ext().execute_with(|| {
		// GIVEN the era 1 set was announced and delivered
		initialize_to_block(1);
		assert_ok!(announce(1, vec![10]));
		initialize_to_block(2);
		// WHEN the same era is announced again
		// THEN it fails with StaleEra, is reported and nothing is queued or sent
		assert_err!(announce(1, vec![11]), Error::<Test>::StaleEra);
		System::assert_last_event(
			Event::AnnouncementRejected { era: 1, error: Error::<Test>::StaleEra.into() }.into(),
		);
		initialize_to_block(3);
		assert_eq!(Sent::get().len(), 2);
		assert_eq!(outgoing(), vec![]);
		assert_eq!(ValidatorSet::<Test>::get().map(|set| set.validators.to_vec()), Some(vec![10]));
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

#[test]
fn announcement_larger_than_max_validators_is_rejected_and_reported() {
	new_test_ext().execute_with(|| {
		// GIVEN a set of one account more than MaxValidators
		initialize_to_block(1);
		let too_many = (100..151).collect::<Vec<u64>>();
		// WHEN it is announced
		// THEN it fails with TooManyValidators, is reported and nothing is stored or queued
		assert_err!(announce(1, too_many), Error::<Test>::TooManyValidators);
		System::assert_last_event(
			Event::AnnouncementRejected { era: 1, error: Error::<Test>::TooManyValidators.into() }
				.into(),
		);
		assert_eq!(ValidatorSet::<Test>::get(), None);
		assert_eq!(outgoing(), vec![]);
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

#[test]
fn try_state_rejects_queued_announcements_outside_the_configuration() {
	new_test_ext().execute_with(|| {
		// GIVEN a stored set
		assert_ok!(announce(1, vec![10]));
		// WHEN an unknown destination or too many retries are queued
		// THEN try_state fails
		OutgoingAnnouncements::<Test>::insert(3, 0);
		assert!(ValidatorCollators::do_try_state().is_err());
		OutgoingAnnouncements::<Test>::remove(3);
		OutgoingAnnouncements::<Test>::insert(1, 3);
		assert!(ValidatorCollators::do_try_state().is_err());
		OutgoingAnnouncements::<Test>::insert(1, 2);
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

#[test]
fn announced_set_is_enacted_locally_like_a_received_one() {
	new_test_ext().execute_with(|| {
		// GIVEN validator 10 with keys
		initialize_to_block(1);
		set_keys(10);
		// WHEN its set is announced
		assert_ok!(announce(1, vec![10]));
		initialize_to_block(3);
		// THEN it becomes a session validator after the two forced rotations
		assert_eq!(Session::current_index(), 2);
		assert_eq!(Session::validators(), vec![1, 2, 10]);
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

parameter_types! {
	pub static LeftSet: Option<Vec<u64>> = None;
	pub static RightSet: Option<Vec<u64>> = None;
	pub static LeftGenesisSet: Option<Vec<u64>> = None;
	pub static RightGenesisSet: Option<Vec<u64>> = None;
	pub static Calls: Vec<(&'static str, &'static str, SessionIndex)> = Vec::new();
}

struct Left;
impl SessionManager<u64> for Left {
	fn new_session(_: SessionIndex) -> Option<Vec<u64>> {
		LeftSet::get()
	}
	fn new_session_genesis(_: SessionIndex) -> Option<Vec<u64>> {
		LeftGenesisSet::get()
	}
	fn start_session(index: SessionIndex) {
		Calls::mutate(|calls| calls.push(("left", "start", index)));
	}
	fn end_session(index: SessionIndex) {
		Calls::mutate(|calls| calls.push(("left", "end", index)));
	}
}

struct Right;
impl SessionManager<u64> for Right {
	fn new_session(_: SessionIndex) -> Option<Vec<u64>> {
		RightSet::get()
	}
	fn new_session_genesis(_: SessionIndex) -> Option<Vec<u64>> {
		RightGenesisSet::get()
	}
	fn start_session(index: SessionIndex) {
		Calls::mutate(|calls| calls.push(("right", "start", index)));
	}
	fn end_session(index: SessionIndex) {
		Calls::mutate(|calls| calls.push(("right", "end", index)));
	}
}

type Union = UnionSessionManager<Left, Right>;

#[test]
fn union_forwards_start_and_end_session_to_both() {
	// GIVEN a union of two managers
	Calls::set(Vec::new());
	// WHEN a session starts and ends
	<Union as SessionManager<u64>>::start_session(3);
	<Union as SessionManager<u64>>::end_session(3);
	// THEN both managers are called
	assert_eq!(
		Calls::get(),
		vec![("left", "start", 3), ("right", "start", 3), ("left", "end", 3), ("right", "end", 3)]
	);
}

#[test]
fn union_merges_new_session_results() {
	let merged = |left, right| {
		LeftSet::set(left);
		RightSet::set(right);
		<Union as SessionManager<u64>>::new_session(1)
	};
	// GIVEN two managers returning every combination of None and overlapping sets
	// WHEN the union plans a new session
	// THEN it is None only if both are None, else left then right without duplicates
	assert_eq!(merged(None, None), None);
	assert_eq!(merged(Some(vec![1, 2]), None), Some(vec![1, 2]));
	assert_eq!(merged(None, Some(vec![3])), Some(vec![3]));
	assert_eq!(merged(Some(vec![]), None), Some(vec![]));
	assert_eq!(merged(Some(vec![2, 1]), Some(vec![3, 1, 4, 3])), Some(vec![2, 1, 3, 4]));
}

#[test]
fn union_uses_genesis_functions_at_genesis() {
	// GIVEN managers whose genesis sets differ from their regular sets
	LeftSet::set(Some(vec![1]));
	RightSet::set(Some(vec![2]));
	LeftGenesisSet::set(Some(vec![5, 6]));
	RightGenesisSet::set(Some(vec![6, 7]));
	// WHEN the union plans the genesis session
	// THEN it merges the genesis sets
	assert_eq!(<Union as SessionManager<u64>>::new_session_genesis(0), Some(vec![5, 6, 7]));
}
