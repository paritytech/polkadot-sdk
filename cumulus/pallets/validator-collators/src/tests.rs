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
	mock::*, relay_proof::relay_state_proof, Activity, AuthoredBlocks, Call, Collators, Config,
	CountedSlots, CurrentDropRule, DrawnCollators, DropRule, Dropped, EpochRandomness,
	EraValidatorSet, Error, Event, ExpectedEraDuration, LastActivity, MaxCollators,
	PendingRotation, ReportMargin, ReportPeriod, RotationState, SetReceivedAt, SlotCount,
	ValidatorSet,
};
use codec::{Decode, Encode};
use cumulus_pallet_parachain_system::{
	relay_state_snapshot::{Error as ProofError, ReadEntryErr},
	OnSystemEvent, RelayChainStateProof,
};
use cumulus_primitives_core::relay_chain::well_known_keys::ONE_EPOCH_AGO_RANDOMNESS;
use frame_support::{
	assert_noop, assert_ok,
	traits::{Hooks, UnfilteredDispatchable, ValidatorRegistration},
	BoundedBTreeSet,
};
use pallet_session::SessionManager;
use rand::{seq::SliceRandom, SeedableRng};
use rand_chacha::ChaCha20Rng;
use sp_consensus_aura::{Slot, AURA_ENGINE_ID};
use sp_runtime::{
	testing::UintAuthorityId,
	traits::{BadOrigin, BlakeTwo256, Hash},
	Digest, DigestItem, DispatchResult,
};
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

fn set_cap(cap: Option<u32>) {
	assert_ok!(ValidatorCollators::set_max_collators(
		RuntimeOrigin::signed(RootAccount::get()),
		cap
	));
}

fn drawn() -> Vec<u64> {
	Collators::<Test>::get()
		.map(|drawn| drawn.validators.into_inner())
		.unwrap_or_default()
}

fn returned() -> Option<Vec<u64>> {
	<ValidatorCollators as SessionManager<u64>>::new_session(Session::current_index())
}

/// The seed the pallet must use for a draw from the set of `era`.
fn expected_seed(era: EraIndex, randomness: [u8; 32]) -> [u8; 32] {
	let genesis = System::block_hash(0);
	BlakeTwo256::hash_of(&(b"validator-collators", genesis, era, randomness)).into()
}

/// The draw the pallet must make: shuffle the whole set in account order with `seed`, then keep
/// the first `cap` validators with registered keys and not in `dropped`.
fn reference_draw(seed: [u8; 32], set: &[u64], cap: usize, dropped: &[u64]) -> Vec<u64> {
	let mut order = set.to_vec();
	order.sort();
	order.shuffle(&mut ChaCha20Rng::from_seed(seed));
	order
		.into_iter()
		.filter(|v| Session::is_registered(v) && !dropped.contains(v))
		.take(cap)
		.collect()
}

#[test]
fn calls_reject_wrong_origin() {
	new_test_ext().execute_with(|| {
		// GIVEN accounts other than the configured origin of each call
		let not_set_origin = RootAccount::get();
		let not_update_origin = SetAccount::get();
		// WHEN they submit a validator set or change a setting
		// THEN every call fails with BadOrigin
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
		assert_noop!(
			ValidatorCollators::set_report_period(RuntimeOrigin::signed(not_update_origin), 3),
			BadOrigin
		);
		assert_noop!(
			ValidatorCollators::set_expected_era_duration(
				RuntimeOrigin::signed(not_update_origin),
				1
			),
			BadOrigin
		);
		assert_noop!(
			ValidatorCollators::set_report_margin(RuntimeOrigin::signed(not_update_origin), 1),
			BadOrigin
		);
		assert_noop!(
			ValidatorCollators::set_drop_rule(RuntimeOrigin::signed(not_update_origin), None),
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
	// A cap draws the validators but must not add or delay rotations.
	for cap in [None, Some(3)] {
		new_test_ext().execute_with(|| {
			// GIVEN invulnerables 1 and 2, candidate 4 and validators with keys
			initialize_to_block(1);
			register_candidate(4);
			set_keys(10);
			set_keys(11);
			set_cap(cap);
			initialize_to_block(3);
			assert_eq!(Session::current_index(), 0);
			// WHEN a set that repeats invulnerable 2 is received at block 3
			assert_ok!(receive(1, vec![11, 2, 10]));
			// THEN the session rotates at blocks 4 and 5 and then holds the deduplicated union,
			// with the validators in account order without a cap
			initialize_to_block(4);
			assert_eq!(Session::current_index(), 1, "cap {cap:?}");
			assert_eq!(PendingRotation::<Test>::get(), RotationState::AwaitingEnactment);
			initialize_to_block(5);
			assert_eq!(Session::current_index(), 2, "cap {cap:?}");
			assert_eq!(PendingRotation::<Test>::get(), RotationState::Idle);
			let validators = match cap {
				Some(_) => drawn(),
				None => vec![10, 11],
			};
			let expected = [1, 2, 4]
				.into_iter()
				.chain(validators.into_iter().filter(|v| *v != 2))
				.collect::<Vec<_>>();
			assert_eq!(Session::validators(), expected, "cap {cap:?}");
			initialize_to_block(6);
			assert_eq!(Session::current_index(), 2);
			assert_ok!(ValidatorCollators::do_try_state());
		});
	}
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
	let set = vec![10, 11, 12, 13];
	// (cap, number of validators returned by the pallet)
	let cases: [(Option<u32>, usize); 5] =
		[(Some(0), 0), (Some(1), 1), (Some(2), 2), (Some(10), 3), (None, 3)];
	for (cap, count) in cases {
		new_test_ext().execute_with(|| {
			// GIVEN invulnerables 1 and 2, candidate 4 and a set whose first validator has no keys
			initialize_to_block(1);
			register_candidate(4);
			[11, 12, 13].into_iter().for_each(set_keys);
			// WHEN the cap is set and the set is received and enacted
			set_cap(cap);
			System::assert_last_event(Event::MaxCollatorsSet { max: cap }.into());
			assert_ok!(receive(1, set.clone()));
			assert_eq!(Collators::<Test>::get(), None);
			initialize_to_block(3);
			// THEN the cap counts only validators with keys and leaves the other side untouched
			let returned = returned().unwrap();
			assert_eq!(returned.len(), count, "cap {cap:?}");
			let expected = [1, 2, 4].into_iter().chain(returned.clone()).collect::<Vec<_>>();
			assert_eq!(Session::validators(), expected, "cap {cap:?}");
			// AND under a cap the drawn list is exactly the reference draw with the seed of
			// era 1 and the cached randomness
			if let Some(cap) = cap {
				let reference = reference_draw(expected_seed(1, [7; 32]), &set, cap as usize, &[]);
				assert_eq!(returned, reference, "cap {cap:?}");
				assert_eq!(drawn(), reference);
				System::assert_has_event(
					Event::CollatorsDrawn { era: 1, validators: reference }.into(),
				);
			} else {
				assert_eq!(returned, vec![11, 12, 13]);
			}
			assert_ok!(ValidatorCollators::do_try_state());
		});
	}
	new_test_ext().execute_with(|| {
		// GIVEN invulnerables 1 and 2, candidate 4, and a set where only candidate 4 has keys
		initialize_to_block(1);
		register_candidate(4);
		// WHEN a cap of 1 is set and the set is received and enacted
		set_cap(Some(1));
		assert_ok!(receive(1, vec![4, 11, 12]));
		initialize_to_block(3);
		// THEN candidate 4 takes the only place under the cap and adds no collator
		assert_eq!(returned(), Some(vec![4]));
		assert_eq!(Session::validators(), vec![1, 2, 4]);
	});
}

#[test]
fn each_new_set_is_drawn_with_the_seed_of_its_era() {
	new_test_ext().execute_with(|| {
		// GIVEN validators 11, 12 and 13 with keys, 10 without, and a cap of 1
		initialize_to_block(1);
		[11, 12, 13].into_iter().for_each(set_keys);
		set_cap(Some(1));
		let set = vec![10, 11, 12, 13];
		for era in 1..=10 {
			// WHEN the set of `era` arrives
			assert_ok!(receive(era, set.clone()));
			// THEN try-state holds before the draw, while the drawn list belongs to the
			// previous era or is missing
			assert_ok!(ValidatorCollators::do_try_state());
			// AND the next rotation draws with the seed of that era
			initialize_to_block(System::block_number() + 1);
			assert_eq!(
				drawn(),
				reference_draw(expected_seed(era, [7; 32]), &set, 1, &[]),
				"era {era}"
			);
		}
	});
}

#[test]
fn cap_change_redraws_with_the_current_randomness_and_forces_two_rotations() {
	new_test_ext().execute_with(|| {
		// GIVEN six validators with keys and a set received at block 3 under a cap of 3
		initialize_to_block(1);
		let set = vec![10, 11, 12, 13, 14, 15];
		set.iter().copied().for_each(set_keys);
		set_cap(Some(3));
		initialize_to_block(3);
		assert_ok!(receive(1, set.clone()));
		// WHEN the first forced rotation happens at block 4
		initialize_to_block(4);
		// THEN it draws with the seed of era 1 and the value cached before the rotation
		let three = drawn();
		assert_eq!(three, reference_draw(expected_seed(1, [7; 32]), &set, 3, &[]));
		initialize_to_block(5);
		assert_eq!(returned(), Some(three.clone()));

		// WHEN the relay randomness changes and the cap is raised to 5 at block 6
		RelayEpochRandomness::set(Some([8; 32]));
		initialize_to_block(6);
		set_cap(Some(5));
		// THEN nothing is drawn before the next rotation, and two rotations follow at blocks 7
		// and 8
		assert_eq!(drawn(), three);
		assert_eq!(PendingRotation::<Test>::get(), RotationState::AwaitingQueue);
		let session = Session::current_index();
		initialize_to_block(8);
		assert_eq!(Session::current_index(), session + 2);
		// AND the rotation at block 7 drew anew from the stored set with the current randomness
		let five = reference_draw(expected_seed(1, [8; 32]), &set, 5, &[]);
		assert_eq!(drawn(), five);
		System::assert_has_event(Event::CollatorsDrawn { era: 1, validators: five.clone() }.into());
		assert_eq!(
			Session::validators(),
			[1, 2].into_iter().chain(five.clone()).collect::<Vec<_>>()
		);

		// WHEN the relay randomness changes again and the cap is lowered to 2 at block 9
		RelayEpochRandomness::set(Some([9; 32]));
		initialize_to_block(9);
		set_cap(Some(2));
		initialize_to_block(10);
		// THEN the redraw at block 10 uses the randomness cached then
		let two = reference_draw(expected_seed(1, [9; 32]), &set, 2, &[]);
		assert_eq!(drawn(), two);

		// WHEN a drawn validator purges its keys
		assert_ok!(Session::purge_keys(RuntimeOrigin::signed(two[0])));
		// THEN it is no longer returned and its place stays empty until the next draw
		assert_eq!(returned(), Some(vec![two[1]]));

		// WHEN the cap is removed
		set_cap(None);
		// THEN nothing is drawn, two rotations follow and every validator with keys is returned
		assert_eq!(Collators::<Test>::get(), None);
		assert_eq!(PendingRotation::<Test>::get(), RotationState::AwaitingQueue);
		let all_with_keys = set.iter().copied().filter(|v| *v != two[0]).collect::<Vec<_>>();
		let session = Session::current_index();
		initialize_to_block(12);
		assert_eq!(Session::current_index(), session + 2);
		assert_eq!(
			Session::validators(),
			[1, 2].into_iter().chain(all_with_keys).collect::<Vec<_>>()
		);
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

#[test]
fn validator_registering_keys_after_the_draw_waits_for_the_next_draw() {
	new_test_ext().execute_with(|| {
		// GIVEN a set of 10, 11 and 12 drawn under a cap of 3 at block 2, while 12 has no keys
		initialize_to_block(1);
		set_keys(10);
		set_keys(11);
		set_cap(Some(3));
		assert_ok!(receive(1, vec![10, 11, 12]));
		initialize_to_block(3);
		let drawn_without_12 = drawn();
		assert_eq!(drawn_without_12.len(), 2);
		assert!(!drawn_without_12.contains(&12));
		// WHEN 12 registers keys and the periodic rotations at blocks 10 and 20 happen
		set_keys(12);
		initialize_to_block(20);
		// THEN 12 is not drawn and does not collate
		assert_eq!(drawn(), drawn_without_12);
		assert!(!Session::validators().contains(&12));
		// WHEN the cap is set again, which forces two rotations and a redraw
		set_cap(Some(3));
		initialize_to_block(22);
		// THEN 12 is drawn and collates
		assert!(drawn().contains(&12));
		assert!(Session::validators().contains(&12));
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

#[test]
fn cap_without_a_stored_set_draws_nothing_and_forces_no_rotation() {
	new_test_ext().execute_with(|| {
		// GIVEN no stored set
		initialize_to_block(1);
		// WHEN a cap is set and then removed
		set_cap(Some(1));
		assert_eq!(Collators::<Test>::get(), None);
		set_cap(None);
		// THEN nothing is drawn and no rotation is pending
		assert_eq!(Collators::<Test>::get(), None);
		assert_eq!(PendingRotation::<Test>::get(), RotationState::Idle);
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

#[test]
fn capped_set_draws_with_a_zero_value_without_cache_or_with_the_key_absent() {
	new_test_ext().execute_with(|| {
		// GIVEN a runtime without the `OnSystemEvent` wiring, so no randomness is cached
		RelayEpochRandomness::set(None);
		initialize_to_block(1);
		set_keys(10);
		set_keys(11);
		assert_eq!(EpochRandomness::<Test>::get(), None);
		// WHEN a capped set arrives and the next rotation draws
		set_cap(Some(1));
		assert_ok!(receive(1, vec![10, 11]));
		initialize_to_block(2);
		// THEN it is drawn with a zero randomness value instead of being skipped
		assert_eq!(drawn(), reference_draw(expected_seed(1, [0; 32]), &[10, 11], 1, &[]));
		// AND try-state flags drawn collators without a cached randomness
		assert_eq!(
			ValidatorCollators::do_try_state(),
			Err("collators are drawn without a cached relay chain epoch randomness".into())
		);
		// WHEN the wiring processes a proof showing the key absent at block 3 and the next set is
		// drawn at block 4
		initialize_to_block(3);
		on_relay_state_proof(&relay_state_proof(
			&[(OTHER_KEY, vec![1; 40])],
			&[ONE_EPOCH_AGO_RANDOMNESS],
		));
		assert_ok!(receive(2, vec![10, 11]));
		initialize_to_block(4);
		// THEN the draw uses the cached zero value and try-state holds
		assert_eq!(drawn(), reference_draw(expected_seed(2, [0; 32]), &[10, 11], 1, &[]));
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

/// A key whose first nibble differs from the randomness key, so their trie nodes are distinct and
/// the proof of one does not carry the other.
const OTHER_KEY: &[u8] = &[0xff; 32];

fn read_randomness(proof: &RelayChainStateProof) -> Result<[u8; 32], ProofError> {
	proof.read_entry::<[u8; 32]>(ONE_EPOCH_AGO_RANDOMNESS, None)
}

fn on_relay_state_proof(proof: &RelayChainStateProof) {
	<ValidatorCollators as OnSystemEvent>::on_relay_state_proof(proof);
}

#[test]
fn relay_randomness_cache_follows_the_proof() {
	new_test_ext().execute_with(|| {
		let key = ONE_EPOCH_AGO_RANDOMNESS;
		// GIVEN a proof with a new value and a proof showing the key absent, each asserted to
		// yield its read result first
		let new_value = relay_state_proof(&[(key, [2u8; 32].encode())], &[key]);
		let absent = relay_state_proof(&[(OTHER_KEY, vec![1; 40])], &[key]);
		assert_eq!(read_randomness(&new_value).ok(), Some([2; 32]));
		assert!(matches!(
			read_randomness(&absent),
			Err(ProofError::ReadEntry(ReadEntryErr::Absent))
		));

		// WHEN the proof carries a new value THEN it is cached
		EpochRandomness::<Test>::put([1; 32]);
		on_relay_state_proof(&new_value);
		assert_eq!(EpochRandomness::<Test>::get(), Some([2; 32]));

		// WHEN the proof carries the cached value THEN the cache is not rewritten
		let storage_key = EpochRandomness::<Test>::hashed_key();
		let stored = [[2u8; 32].as_slice(), &[0]].concat();
		frame_support::storage::unhashed::put_raw(&storage_key, &stored);
		on_relay_state_proof(&new_value);
		assert_eq!(frame_support::storage::unhashed::get_raw(&storage_key), Some(stored));

		// WHEN the proof shows the key absent THEN the relay chain's default zero value is cached
		EpochRandomness::<Test>::put([2; 32]);
		on_relay_state_proof(&absent);
		assert_eq!(EpochRandomness::<Test>::get(), Some([0; 32]));
	});
}

#[test]
#[should_panic(
	expected = "Invalid relay chain epoch randomness in the relay chain state proof: Proof"
)]
fn relay_proof_without_the_randomness_trie_nodes_makes_the_block_invalid() {
	new_test_ext().execute_with(|| {
		// GIVEN a proof that holds the randomness key in its trie but not the nodes to read it
		let key = ONE_EPOCH_AGO_RANDOMNESS;
		let proof =
			relay_state_proof(&[(key, [3u8; 32].encode()), (OTHER_KEY, vec![1; 40])], &[OTHER_KEY]);
		assert!(matches!(read_randomness(&proof), Err(ProofError::ReadEntry(ReadEntryErr::Proof))));
		// WHEN the pallet processes it THEN it panics
		on_relay_state_proof(&proof);
	});
}

#[test]
#[should_panic(
	expected = "Invalid relay chain epoch randomness in the relay chain state proof: Decode"
)]
fn undecodable_relay_randomness_makes_the_block_invalid() {
	new_test_ext().execute_with(|| {
		// GIVEN a proof whose randomness value is too short to decode
		let key = ONE_EPOCH_AGO_RANDOMNESS;
		let proof = relay_state_proof(&[(key, vec![1; 31])], &[key]);
		assert!(matches!(
			read_randomness(&proof),
			Err(ProofError::ReadEntry(ReadEntryErr::Decode))
		));
		// WHEN the pallet processes it THEN it panics
		on_relay_state_proof(&proof);
	});
}

#[test]
fn set_received_while_planned_rearms_the_rotations() {
	// A cap draws the validators but must not add or delay rotations.
	for cap in [None, Some(1)] {
		new_test_ext().execute_with(|| {
			// GIVEN a set for era 1 received at block 3 and queued at block 4
			initialize_to_block(1);
			set_keys(10);
			set_keys(11);
			set_cap(cap);
			initialize_to_block(3);
			assert_ok!(receive(1, vec![10]));
			initialize_to_block(4);
			assert_eq!(PendingRotation::<Test>::get(), RotationState::AwaitingEnactment);
			// WHEN a set for era 2 arrives at block 4
			assert_ok!(receive(2, vec![11]));
			// THEN the session rotates again at blocks 5 and 6 and the era 2 set is enacted
			assert_eq!(PendingRotation::<Test>::get(), RotationState::AwaitingQueue);
			initialize_to_block(5);
			assert_eq!(Session::current_index(), 2, "cap {cap:?}");
			initialize_to_block(6);
			assert_eq!(Session::current_index(), 3, "cap {cap:?}");
			assert_eq!(PendingRotation::<Test>::get(), RotationState::Idle);
			assert_eq!(Session::validators(), vec![1, 2, 11], "cap {cap:?}");
			initialize_to_block(7);
			assert_eq!(Session::current_index(), 3);
			assert_ok!(ValidatorCollators::do_try_state());
		});
	}
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

#[test]
fn try_state_detects_drawn_collators_that_break_the_invariants() {
	let drawn = |era: EraIndex, validators: Vec<u64>| DrawnCollators {
		era,
		validators: validators.try_into().unwrap(),
	};
	// (cap, drawn collators, randomness cached), checked after the forced rotations
	let cases = [
		// drawn collators without a cap
		(None, Some(drawn(1, vec![10])), true),
		// a cap and a set without drawn collators
		(Some(2), None, true),
		// drawn collators of another era
		(Some(2), Some(drawn(0, vec![10])), true),
		// a drawn collator outside the set
		(Some(2), Some(drawn(1, vec![12])), true),
		// a duplicate
		(Some(2), Some(drawn(1, vec![10, 10])), true),
		// more than the cap
		(Some(1), Some(drawn(1, vec![10, 11])), true),
		// drawn collators without a cached randomness
		(Some(2), Some(drawn(1, vec![10])), false),
	];
	for (cap, collators, cached) in cases {
		new_test_ext().execute_with(|| {
			// GIVEN a stored set of 10 and 11 brought into force, and a consistent state
			assert_ok!(receive(1, vec![10, 11]));
			initialize_to_block(2);
			assert_ok!(ValidatorCollators::do_try_state());
			// WHEN the cap, the drawn collators and the cache break an invariant
			MaxCollators::<Test>::set(cap);
			Collators::<Test>::set(collators.clone());
			if !cached {
				EpochRandomness::<Test>::kill();
			}
			// THEN try_state fails
			assert!(ValidatorCollators::do_try_state().is_err(), "{cap:?} {collators:?} {cached}");
		});
	}
	new_test_ext().execute_with(|| {
		// GIVEN 10 and 11 with keys under a cap of 1, brought into force
		set_keys(10);
		set_keys(11);
		set_cap(Some(1));
		assert_ok!(receive(1, vec![10, 11]));
		initialize_to_block(2);
		let outside =
			if Collators::<Test>::get().unwrap().validators.contains(&10) { 11 } else { 10 };
		assert_ok!(ValidatorCollators::do_try_state());
		// WHEN a validator of the set that is not drawn has an activity record
		LastActivity::<Test>::insert(outside, Activity::Queued);
		// THEN try_state fails
		assert_eq!(
			ValidatorCollators::do_try_state(),
			Err("an activity record belongs to a validator that is not drawn".into())
		);
	});
}

fn note_blocks(author: u64, blocks: u32) {
	(0..blocks).for_each(|_| {
		<ValidatorCollators as pallet_authorship::EventHandler<_, _>>::note_author(author)
	});
}

fn counts() -> Vec<(u64, u32)> {
	let mut counts = AuthoredBlocks::<Test>::iter().collect::<Vec<_>>();
	counts.sort();
	counts
}

fn reported_entries() -> Vec<(u64, u32)> {
	let mut entries = reported().concat();
	entries.sort();
	entries
}

#[test]
fn report_sends_counts_of_stored_validators_only() {
	new_test_ext().execute_with(|| {
		// GIVEN the set of 10 and 11 is stored, and 10, 11 and invulnerable 1 authored blocks
		initialize_to_block(1);
		assert_ok!(receive(1, vec![10, 11]));
		note_blocks(10, 3);
		note_blocks(11, 1);
		note_blocks(1, 2);
		assert_eq!(AuthoredBlocks::<Test>::count(), 3);
		assert_ok!(ValidatorCollators::do_try_state());
		// AND the invariants reject an author counted with zero blocks
		frame_support::hypothetically!({
			AuthoredBlocks::<Test>::insert(12, 0);
			assert_eq!(
				ValidatorCollators::do_try_state(),
				Err("an author is counted with zero blocks".into())
			);
		});

		// WHEN the report block comes without a stored set
		// THEN nothing reaches the reporter and every count is dropped
		frame_support::hypothetically!({
			ValidatorSet::<Test>::kill();
			PendingRotation::<Test>::kill();
			initialize_to_block(5);
			assert_eq!(reported(), Vec::<Vec<(u64, u32)>>::new());
			assert_eq!(counts(), vec![]);
			System::assert_has_event(Event::BlocksReported { authors: 3, reported: 0 }.into());
		});

		// WHEN the report block comes with the stored set
		initialize_to_block(4);
		assert!(reported().is_empty());
		initialize_to_block(5);

		// THEN the reporter got the counts of the validators only, and the map is empty, so
		// a non-validator never reaches the chain that credits the counts
		assert_eq!(reported_entries(), vec![(10, 3), (11, 1)]);
		assert_eq!(counts(), vec![]);
		assert_eq!(AuthoredBlocks::<Test>::count(), 0);
		System::assert_has_event(Event::BlocksReported { authors: 3, reported: 2 }.into());
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

#[test]
fn report_is_split_into_chunks() {
	new_test_ext().execute_with(|| {
		// GIVEN at most two entries per report and three validators with counts
		MaxEntriesPerReport::set(2);
		initialize_to_block(1);
		assert_ok!(receive(1, vec![10, 11, 12]));
		note_blocks(10, 1);
		note_blocks(11, 2);
		note_blocks(12, 3);

		// WHEN the report block comes
		initialize_to_block(5);

		// THEN every validator is reported in this period, in a chunk of two and one of one
		assert_eq!(reported().iter().map(Vec::len).collect::<Vec<_>>(), vec![2, 1]);
		assert_eq!(reported_entries(), vec![(10, 1), (11, 2), (12, 3)]);
		assert_eq!(counts(), vec![]);
	});
}

#[test]
fn failed_report_keeps_the_counts_for_the_next_period() {
	new_test_ext().execute_with(|| {
		// GIVEN counts of three validators over two chunks, of invulnerable 1, and a reporter
		// that rejects the second chunk
		MaxEntriesPerReport::set(2);
		FailingReportCall::set(Some(2));
		initialize_to_block(1);
		assert_ok!(receive(1, vec![10, 11, 12]));
		note_blocks(10, 1);
		note_blocks(11, 2);
		note_blocks(12, 3);
		note_blocks(1, 1);
		let before = counts();

		// WHEN the report block comes
		initialize_to_block(5);

		// THEN the first chunk is rolled back and every count is kept, invulnerable 1 included
		assert_eq!(ReportCalls::get(), 2);
		assert!(reported().is_empty());
		assert_eq!(counts(), before);
		assert_eq!(AuthoredBlocks::<Test>::count(), 4);
		System::assert_has_event(Event::BlocksReportFailed { authors: 4 }.into());

		// WHEN more blocks are authored and the reporter accepts the next report
		note_blocks(10, 4);
		initialize_to_block(10);

		// THEN it carries the sum of both periods, so nothing is lost or reported twice
		assert_eq!(reported_entries(), vec![(10, 5), (11, 2), (12, 3)]);
		assert_eq!(counts(), vec![]);
		System::assert_has_event(Event::BlocksReported { authors: 4, reported: 3 }.into());
	});
}

fn root() -> RuntimeOrigin {
	RuntimeOrigin::signed(RootAccount::get())
}

/// Number of reports made so far.
fn reports() -> usize {
	System::events()
		.into_iter()
		.filter(|record| {
			matches!(record.event, RuntimeEvent::ValidatorCollators(Event::BlocksReported { .. }))
		})
		.count()
}

/// Run the next block at `time` milliseconds, with validator 10 as its author.
fn block_at(time: u64) {
	Now::set(time);
	note_blocks(10, 1);
	initialize_to_block(System::block_number() + 1);
}

#[test]
fn reports_once_before_the_expected_era_end() {
	new_test_ext().execute_with(|| {
		// GIVEN an expected era of 1000 ms, a margin of 100 ms, no report period in the test
		// blocks, and the set of 10 received at time 0
		initialize_to_block(1);
		assert_ok!(ValidatorCollators::set_report_period(root(), 1000));
		assert_eq!((ExpectedEraDuration::<Test>::get(), ReportMargin::<Test>::get()), (1000, 100));
		assert_ok!(receive(1, vec![10]));
		assert_eq!(SetReceivedAt::<Test>::get(), Some(0));

		// WHEN blocks run up to time 899
		block_at(500);
		block_at(899);
		// THEN nothing is reported and the arrival is kept
		assert_eq!(reports(), 0);
		assert_eq!(SetReceivedAt::<Test>::get(), Some(0));

		// WHEN a block runs at time 900
		block_at(900);
		// THEN the counts are reported once and the arrival is removed
		assert_eq!(reports(), 1);
		assert_eq!(reported_entries(), vec![(10, 3)]);
		assert_eq!(SetReceivedAt::<Test>::get(), None);
		assert_ok!(ValidatorCollators::do_try_state());

		// WHEN later blocks run past the expected era end
		block_at(950);
		block_at(1500);
		// THEN there is no second early report
		assert_eq!(reports(), 1);

		// WHEN the next set arrives
		Now::set(1600);
		assert_ok!(receive(2, vec![10]));
		// THEN its arrival is recorded and times the next early report
		assert_eq!(SetReceivedAt::<Test>::get(), Some(1600));
		block_at(2499);
		assert_eq!(reports(), 1);
		block_at(2500);
		assert_eq!(reports(), 2);
		assert_eq!(SetReceivedAt::<Test>::get(), None);
	});
}

#[test]
fn report_timing_edge_cases() {
	// The early report and a `ReportPeriod` block in the same block report once.
	new_test_ext().execute_with(|| {
		// GIVEN the default period of 5 blocks and a set received at time 0
		initialize_to_block(1);
		assert_ok!(receive(1, vec![10]));
		block_at(0);
		block_at(0);
		block_at(0);
		assert_eq!(reports(), 0);
		Now::set(900);
		note_blocks(10, 1);
		System::set_block_number(5);
		// AND the charge of block 5 at the due time if it were not a `ReportPeriod` block
		let early_only = frame_support::hypothetically!({
			ReportPeriod::<Test>::put(1000);
			ValidatorCollators::on_initialize(5)
		});
		// WHEN block 5 runs at the due time
		let both = ValidatorCollators::on_initialize(5);
		// THEN one report is made and charged, and the arrival is removed
		assert_eq!(both, early_only);
		assert_eq!(reports(), 1);
		assert_eq!(reported().len(), 1);
		assert_eq!(SetReceivedAt::<Test>::get(), None);
	});

	// A rejected early report is not retried, and the next `ReportPeriod` block carries its counts.
	new_test_ext().execute_with(|| {
		// GIVEN the default period of 5 blocks, a set received at time 0 and a reporter that
		// rejects its first call
		FailingReportCall::set(Some(1));
		initialize_to_block(1);
		assert_ok!(receive(1, vec![10]));
		// WHEN block 2 runs at the due time
		block_at(900);
		// THEN the report fails, the arrival is removed and the counts are kept
		System::assert_has_event(Event::BlocksReportFailed { authors: 1 }.into());
		assert_eq!(SetReceivedAt::<Test>::get(), None);
		assert_eq!(counts(), vec![(10, 1)]);
		// WHEN blocks 3 and 4 run
		block_at(1000);
		block_at(1100);
		// THEN the reporter is not called again
		assert_eq!(ReportCalls::get(), 1);
		// WHEN block 5, a multiple of the period, runs
		block_at(1200);
		// THEN the reporter accepts the counts of every block since the failed report
		assert_eq!(ReportCalls::get(), 2);
		assert_eq!(reported_entries(), vec![(10, 4)]);
		assert_eq!(counts(), vec![]);
		System::assert_has_event(Event::BlocksReported { authors: 1, reported: 1 }.into());
	});

	// A margin above half of the expected era reports in the middle of the expected era.
	new_test_ext().execute_with(|| {
		// GIVEN an expected era of 100 ms, a margin of 200 ms and a set received at time 0
		initialize_to_block(1);
		assert_ok!(ValidatorCollators::set_report_period(root(), 1000));
		assert_ok!(ValidatorCollators::set_expected_era_duration(root(), 100));
		assert_ok!(ValidatorCollators::set_report_margin(root(), 200));
		assert_ok!(receive(1, vec![10]));
		// WHEN blocks run at 49 and 50 ms
		// THEN the report is made at 50 ms, after the arrival
		block_at(49);
		assert_eq!(reports(), 0);
		block_at(50);
		assert_eq!(reports(), 1);
	});

	// A set that arrives before the due time of the previous one replaces its arrival.
	new_test_ext().execute_with(|| {
		// GIVEN a set received at time 0, due at 900
		initialize_to_block(1);
		assert_ok!(ValidatorCollators::set_report_period(root(), 1000));
		assert_ok!(receive(1, vec![10]));
		// WHEN the next set arrives at 500
		Now::set(500);
		assert_ok!(receive(2, vec![10]));
		// THEN the early report follows the new arrival, due at 1400
		block_at(900);
		assert_eq!(reports(), 0);
		block_at(1400);
		assert_eq!(reports(), 1);
	});

	// Without an arrival, only `ReportPeriod` blocks report.
	new_test_ext().execute_with(|| {
		// GIVEN a stored set without an arrival, as after the upgrade that adds the timing
		initialize_to_block(1);
		assert_ok!(receive(1, vec![10]));
		SetReceivedAt::<Test>::kill();
		// WHEN blocks 2 to 4 run long past any due time
		block_at(10_000);
		block_at(10_000);
		block_at(10_000);
		// THEN nothing is reported until block 5, a multiple of the period
		assert_eq!(reports(), 0);
		block_at(10_000);
		assert_eq!(reports(), 1);
		assert_eq!(reported_entries(), vec![(10, 4)]);
	});

	// A change of `ReportMargin` after the arrival moves the due time.
	new_test_ext().execute_with(|| {
		// GIVEN a set received at time 0, due at 900 with the default margin
		initialize_to_block(1);
		assert_ok!(ValidatorCollators::set_report_period(root(), 1000));
		assert_ok!(receive(1, vec![10]));
		// WHEN the margin is raised to 300
		assert_ok!(ValidatorCollators::set_report_margin(root(), 300));
		// THEN the report is made at 700
		block_at(699);
		assert_eq!(reports(), 0);
		block_at(700);
		assert_eq!(reports(), 1);
	});

	// A stored margin above `MaxReportMargin`, as after a runtime upgrade that lowers it, is
	// used up to `MaxReportMargin`.
	new_test_ext().execute_with(|| {
		// GIVEN an expected era of 2000 ms, a stored margin of 600 ms above the maximum of
		// 500 ms, and a set received at time 0
		initialize_to_block(1);
		assert_ok!(ValidatorCollators::set_report_period(root(), 1000));
		assert_ok!(ValidatorCollators::set_expected_era_duration(root(), 2000));
		ReportMargin::<Test>::put(600);
		assert_ok!(receive(1, vec![10]));
		// WHEN blocks run at 1499 and 1500 ms
		// THEN the report is made at 1500 ms, with the margin capped
		block_at(1499);
		assert_eq!(reports(), 0);
		block_at(1500);
		assert_eq!(reports(), 1);
		// AND the stored margin passes the invariants
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

#[test]
fn setters_are_bounded_and_announced() {
	new_test_ext().execute_with(|| {
		let max_margin =
			<<Test as Config>::MaxReportMargin as frame_support::traits::Get<u64>>::get();
		// GIVEN validator 10 authored a block
		initialize_to_block(1);
		assert_ok!(receive(1, vec![10]));
		note_blocks(10, 1);

		// WHEN the update origin sets a zero period, a zero margin, a margin above the maximum
		// or a zero duration
		// THEN each call fails and the stored values are unchanged
		assert_noop!(
			ValidatorCollators::set_report_period(root(), 0),
			Error::<Test>::ZeroReportPeriod
		);
		assert_noop!(
			ValidatorCollators::set_report_margin(root(), 0),
			Error::<Test>::ZeroReportMargin
		);
		assert_noop!(
			ValidatorCollators::set_report_margin(root(), max_margin + 1),
			Error::<Test>::ReportMarginTooHigh
		);
		assert_noop!(
			ValidatorCollators::set_expected_era_duration(root(), 0),
			Error::<Test>::ZeroExpectedEraDuration
		);

		// WHEN it sets a period of 3 instead of the default 5
		assert_ok!(ValidatorCollators::set_report_period(root(), 3));
		System::assert_last_event(Event::ReportPeriodSet { period: 3 }.into());
		assert_eq!(ReportPeriod::<Test>::get(), 3);
		// THEN the next report is made at block 3
		initialize_to_block(2);
		assert!(reported().is_empty());
		initialize_to_block(3);
		assert_eq!(reported_entries(), vec![(10, 1)]);

		// WHEN it sets the smallest and the largest margin, and the smallest duration
		// THEN each is stored and announced
		assert_ok!(ValidatorCollators::set_report_margin(root(), 1));
		System::assert_last_event(Event::ReportMarginSet { margin: 1 }.into());
		assert_ok!(ValidatorCollators::set_report_margin(root(), max_margin));
		System::assert_last_event(Event::ReportMarginSet { margin: max_margin }.into());
		assert_eq!(ReportMargin::<Test>::get(), max_margin);
		assert_ok!(ValidatorCollators::set_expected_era_duration(root(), 1));
		System::assert_last_event(Event::ExpectedEraDurationSet { duration: 1 }.into());
		assert_eq!(ExpectedEraDuration::<Test>::get(), 1);
		assert_ok!(ValidatorCollators::do_try_state());

		// AND the invariants reject the zero values the calls refuse
		for (duration, margin, error) in
			[(0, 1, "the expected era duration is zero"), (1, 0, "the report margin is zero")]
		{
			frame_support::hypothetically!({
				ExpectedEraDuration::<Test>::put(duration);
				ReportMargin::<Test>::put(margin);
				assert_eq!(ValidatorCollators::do_try_state(), Err(error.into()));
			});
		}
	});
}

fn set_rule(rounds: u32, slots: u64, min_collators: u32) {
	assert_ok!(ValidatorCollators::set_drop_rule(
		RuntimeOrigin::signed(RootAccount::get()),
		Some(DropRule { rounds, slots, min_collators })
	));
}

fn remove_rule() {
	assert_ok!(ValidatorCollators::set_drop_rule(RuntimeOrigin::signed(RootAccount::get()), None));
}

fn activity(who: u64) -> Option<Activity> {
	LastActivity::<Test>::get(who)
}

fn active(since: u64, authorities: u32) -> Option<Activity> {
	Some(Activity::Active { since, authorities })
}

fn dropped() -> Vec<(u64, Option<u64>)> {
	Dropped::<Test>::get().into_iter().collect()
}

/// The validators queued by the last rotation, in force from the next one.
fn queued() -> Vec<u64> {
	Session::queued_keys().into_iter().map(|(who, _)| who).collect()
}

/// The slots counted up to the current block.
fn counted() -> u64 {
	CountedSlots::<Test>::get().map_or(0, |counted| counted.count)
}

/// Produce blocks up to `n`, with each of `authors` authoring block `n - 1`.
fn run_to(n: u64, authors: &[u64]) {
	initialize_to_block(n - 1);
	authors.iter().for_each(|author| note_blocks(*author, 1));
	initialize_to_block(n);
}

// The mock counts 4 Aura authorities and puts block `n` in slot `n` unless a test sets
// `NextSlotGap`, so a rule of two rounds has a window of 8 counted slots, 8 blocks without gaps,
// shorter than the session period of 10 blocks. The count of block `n` is `n` without gaps.

#[test]
fn idle_validator_is_dropped_one_session_after_the_judgement_and_returns_after_its_slots() {
	new_test_ext().execute_with(|| {
		// GIVEN a window of 8 slots, drops of 10 slots, and 10 and 11 received at block 3 and
		// enacted at block 5
		initialize_to_block(1);
		set_keys(10);
		set_keys(11);
		set_rule(2, 10, 0);
		initialize_to_block(3);
		assert_ok!(receive(1, vec![10, 11]));
		initialize_to_block(5);
		assert_eq!(activity(11), active(5, 0));
		// AND a validator whose last block is exactly a window before the rotation at block 20
		// is dropped there, one slot later it is kept, for windows of `rounds × authorities`
		// slots
		for (rounds, authorities) in [(2, 4), (3, 4), (2, 3)] {
			let window = (rounds * authorities) as u64;
			for (block, is_dropped) in [(20 - window, true), (21 - window, false)] {
				frame_support::hypothetically!({
					set_rule(rounds, 10, 0);
					AuthorityCount::set(authorities);
					initialize_to_block(block);
					note_blocks(10, 1);
					initialize_to_block(20);
					AuthorityCount::set(4);
					assert_eq!(
						dropped().iter().any(|(v, _)| *v == 10),
						is_dropped,
						"window of {window} slots, last block {block}"
					);
				});
			}
		}
		// AND 10 authors at block 15
		initialize_to_block(15);
		note_blocks(10, 1);

		// WHEN the rotation at block 20 judges them
		initialize_to_block(20);
		// THEN 11 is dropped (20 - 5 >= 8) and 10 is not (20 - 15 < 8)
		System::assert_has_event(Event::CollatorsDropped { validators: vec![11], kept: 0 }.into());
		assert_eq!(dropped(), vec![(11, None)]);
		assert_eq!(activity(11), None);
		assert_eq!(activity(10), active(15, 4));
		// AND 11 keeps its slots in the session that starts now
		assert_eq!(Session::validators(), vec![1, 2, 10, 11]);
		frame_support::hypothetically!({
			// WHEN a set arrives at block 21, so rotations at blocks 22 and 23 start two sessions,
			// while 10 authors
			let index = Session::current_index();
			initialize_to_block(21);
			note_blocks(10, 1);
			assert_ok!(receive(2, vec![10, 11]));
			initialize_to_block(23);
			// THEN 11 loses its slots at block 22, which starts its drop of 10 slots
			assert_eq!(Session::current_index(), index + 2);
			assert_eq!(Session::validators(), vec![1, 2, 10]);
			assert_eq!(dropped(), vec![(11, Some(32))]);
		});
		// WHEN the rotation at block 30 enacts the list without 11
		run_to(30, &[10]);
		// THEN 11 loses its slots and its drop of 10 slots starts
		assert_eq!(Session::validators(), vec![1, 2, 10]);
		assert_eq!(dropped(), vec![(11, Some(40))]);
		assert!(!queued().contains(&11));

		frame_support::hypothetically!({
			// WHEN a set arrives at block 39, so forced rotations at blocks 40 and 41 follow the
			// end of its drop
			initialize_to_block(39);
			note_blocks(10, 1);
			assert_ok!(receive(2, vec![10, 11]));
			// THEN the rotation at block 40 queues it and the one at block 41 enacts it, so it
			// was no authority for 11 slots
			initialize_to_block(40);
			assert_eq!(dropped(), vec![]);
			assert_eq!(activity(11), Some(Activity::Queued));
			assert_eq!(Session::validators(), vec![1, 2, 10]);
			initialize_to_block(41);
			assert_eq!(Session::validators(), vec![1, 2, 10, 11]);
		});

		// WHEN the rotation at block 40 ends its drop
		run_to(40, &[10]);
		// THEN it is queued, a collator from block 50
		assert_eq!(dropped(), vec![]);
		assert_eq!(activity(11), Some(Activity::Queued));
		assert_eq!(queued(), vec![1, 2, 10, 11]);
		run_to(50, &[10]);
		assert_eq!(Session::validators(), vec![1, 2, 10, 11]);
		// AND the rotation that enacts it starts a fresh window instead of judging it
		assert_eq!(activity(11), active(50, 0));
		assert_eq!(dropped(), vec![]);

		// WHEN it authors nothing until the rotation at block 60 THEN it is dropped again
		frame_support::hypothetically!({
			run_to(60, &[10]);
			assert_eq!(dropped(), vec![(11, None)]);
		});
		// WHEN it authors at block 59 THEN it stays
		run_to(60, &[10, 11]);
		assert_eq!(dropped(), vec![]);
		assert_eq!(activity(11), active(59, 4));
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

/// Start the next block and run this pallet's `on_initialize` for it.
///
/// With `Some(slot)` the block carries an Aura digest for that slot. With `None` it carries no
/// Aura digest. `before_hooks` runs after the block starts and before `on_initialize`, to call
/// what the runtime runs earlier in a block, such as `note_author`.
fn block_in_slot(slot: Option<u64>, before_hooks: impl FnOnce()) {
	let n = System::block_number() + 1;
	let logs = slot
		.map(|slot| DigestItem::PreRuntime(AURA_ENGINE_ID, Slot::from(slot).encode()))
		.into_iter()
		.collect();
	System::initialize(&n, &System::block_hash(n - 1), &Digest { logs });
	before_hooks();
	ValidatorCollators::on_initialize(n);
}

#[test]
fn a_gap_up_to_the_authority_count_counts_in_full_and_a_longer_one_as_one_slot() {
	new_test_ext().execute_with(|| {
		// GIVEN block 1 in slot 1, one counted slot, 4 authorities and an active record of 10
		initialize_to_block(1);
		assert_eq!(counted(), 1);
		let slot = || CountedSlots::<Test>::get().map(|counted| *counted.slot);
		LastActivity::<Test>::insert(10, Activity::Active { since: 0, authorities: 4 });
		// WHEN the next block is 4 slots later, after 3 authorities in a row authored nothing
		block_in_slot(Some(5), || {});
		// THEN the 4 slots count
		assert_eq!((slot(), counted()), (Some(5), 5));
		// WHEN the next block is 5 slots later, so every authority missed a turn, as in a stall
		block_in_slot(Some(10), || {});
		// THEN one slot counts
		assert_eq!((slot(), counted()), (Some(10), 6));
		// WHEN the next blocks are in the same slot or have no slot
		for next in [Some(10), None] {
			block_in_slot(next, || {});
			// THEN nothing counts and the slot is kept
			assert_eq!((slot(), counted()), (Some(10), 6));
		}
		// WHEN the next block is in an earlier slot, as after a longer slot duration
		block_in_slot(Some(7), || {});
		// THEN the count keeps its value and follows the new slots from there
		assert_eq!((slot(), counted()), (Some(7), 6));
		block_in_slot(Some(9), || {});
		assert_eq!((slot(), counted()), (Some(9), 8));
		// WHEN no authority is counted
		AuthorityCount::set(0);
		block_in_slot(Some(11), || {});
		AuthorityCount::set(4);
		// THEN a gap of 2 slots counts as one slot
		assert_eq!((slot(), counted()), (Some(11), 9));
		// WHEN 10 authors a block 2 slots later and the authorship handler runs before the hooks
		block_in_slot(Some(13), || note_blocks(10, 1));
		// THEN its record holds the count of its own block, and the gap counts once
		assert_eq!(activity(10), active(11, 0));
		assert_eq!(counted(), 11);
	});
}

#[test]
fn first_advance_seeds_the_count_at_a_rotation() {
	new_test_ext().execute_with(|| {
		// GIVEN 10 with keys in a set received at block 1, and no counted slots, as in the first
		// block after the upgrade that adds them
		initialize_to_block(1);
		set_keys(10);
		assert_ok!(receive(1, vec![10]));
		CountedSlots::<Test>::kill();
		// WHEN the forced rotation at block 2, in slot 1, is the first to advance the count
		initialize_to_block(2);
		// THEN it stores the slot with a zero count and queues 10
		assert_eq!(CountedSlots::<Test>::get(), Some(SlotCount { slot: 1.into(), count: 0 }));
		assert_eq!(activity(10), Some(Activity::Queued));
		// AND the rotation at block 3, in slot 2, enacts 10 at count 1
		initialize_to_block(3);
		assert_eq!(activity(10), active(1, 0));
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

#[test]
fn silent_authorities_shorten_the_window_in_blocks_and_a_stall_counts_one_slot() {
	new_test_ext().execute_with(|| {
		// GIVEN a window of 8 slots, drops of 30 slots, and 10 and 11 enacted at block 5, count 5
		initialize_to_block(1);
		set_keys(10);
		set_keys(11);
		set_rule(2, 30, 0);
		initialize_to_block(3);
		assert_ok!(receive(1, vec![10, 11]));
		initialize_to_block(5);
		assert_eq!(activity(11), active(5, 0));

		frame_support::hypothetically!({
			// WHEN 10 authors block 15 at count 15, blocks 16 to 18 follow in the next slots, the
			// chain stalls for 5 slots before block 19, and block 20 follows in the next slot,
			// so the slots 4 and 8 after its block fall in the stall
			run_to(16, &[10]);
			initialize_to_block(18);
			NextSlotGap::set(6);
			initialize_to_block(20);
			// THEN the stall counts one slot, so the rotation at block 20 keeps 10 (20 - 15 < 8)
			// and drops 11 (20 - 5 >= 8)
			assert_eq!(counted(), 20);
			assert_eq!(dropped(), vec![(11, None)]);
			assert_eq!(activity(10), active(15, 4));
		});

		// WHEN every block from block 6 to block 10 follows a slot of a silent authority, so
		// each counts 2 slots, and 10 authors block 9
		for block in 6..=10 {
			NextSlotGap::set(2);
			initialize_to_block(block);
			if block == 9 {
				note_blocks(10, 1);
			}
		}
		// THEN the rotation at block 10, 5 blocks after the enactment but at count 15, drops 11
		// (15 - 5 >= 8) and keeps 10 (15 - 13 < 8)
		assert_eq!(counted(), 15);
		assert_eq!(dropped(), vec![(11, None)]);
		assert_eq!(activity(10), active(13, 4));

		// WHEN the rotation at block 20 enacts the list without 11, at count 25
		run_to(20, &[10]);
		// THEN its drop of 30 slots starts
		assert_eq!(counted(), 25);
		assert_eq!(dropped(), vec![(11, Some(55))]);
		assert_eq!(Session::validators(), vec![1, 2, 10]);
		// WHEN 30 slots pass in the 10 blocks to the next rotation, each after two empty slots
		for block in 21..=30 {
			NextSlotGap::set(3);
			initialize_to_block(block);
			if block == 29 {
				note_blocks(10, 1);
			}
		}
		// THEN the rotation at block 30 ends the drop and queues 11
		assert_eq!(counted(), 55);
		assert_eq!(dropped(), vec![]);
		assert_eq!(activity(11), Some(Activity::Queued));
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

#[test]
fn validator_is_judged_only_from_its_enactment() {
	new_test_ext().execute_with(|| {
		// GIVEN a window of 8 slots and the set of 12 enacted at block 5 while 12 has no keys
		initialize_to_block(1);
		set_rule(2, 10, 0);
		initialize_to_block(3);
		assert_ok!(receive(1, vec![12]));
		// WHEN 12 registers keys at block 12
		initialize_to_block(12);
		set_keys(12);
		// THEN the rotation at block 20 queues it
		initialize_to_block(20);
		assert_eq!(activity(12), Some(Activity::Queued));
		assert_eq!(queued(), vec![1, 2, 12]);

		frame_support::hypothetically!({
			// WHEN a set with 12 and 13 arrives at block 27, so the forced rotations at blocks 28
			// and 29 enact both, and the regular rotation follows at block 30
			set_keys(13);
			initialize_to_block(27);
			assert_ok!(receive(2, vec![12, 13]));
			initialize_to_block(30);
			// THEN that rotation drops nobody, since both are collators only from block 28 or 29
			assert_eq!(Session::validators(), vec![1, 2, 12, 13]);
			assert_eq!(activity(12), active(28, 4));
			assert_eq!(activity(13), active(29, 4));
			assert_eq!(dropped(), vec![]);
		});

		// WHEN the rotation at block 30 enacts it, 10 blocks after it was queued
		initialize_to_block(30);
		// THEN it is not judged and its window starts at block 30
		assert_eq!(activity(12), active(30, 0));
		assert_eq!(dropped(), vec![]);
		assert_eq!(Session::validators(), vec![1, 2, 12]);
		// WHEN it authors nothing until the rotation at block 40 THEN it is dropped
		initialize_to_block(40);
		assert_eq!(dropped(), vec![(12, None)]);
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

#[test]
fn window_follows_the_largest_list_in_force_since_the_record() {
	new_test_ext().execute_with(|| {
		// GIVEN 10 and 11 enacted at block 5 under a rule of two rounds
		initialize_to_block(1);
		set_keys(10);
		set_keys(11);
		set_rule(2, 10, 0);
		initialize_to_block(3);
		assert_ok!(receive(1, vec![10, 11]));
		initialize_to_block(5);
		assert_eq!(activity(11), active(5, 0));

		frame_support::hypothetically!({
			// WHEN no Aura authority is counted, as with broken runtime glue
			AuthorityCount::set(0);
			initialize_to_block(50);
			// THEN no rotation drops anyone and the records keep a zero count
			assert_eq!(dropped(), vec![]);
			assert_eq!(activity(11), active(5, 0));
		});

		// WHEN a list of 3 authorities ends at the rotation at block 10, a window of 6 slots
		AuthorityCount::set(3);
		initialize_to_block(10);
		assert_eq!(activity(11), active(5, 3));
		// AND a list of 20 ends at the rotation at block 20, 15 blocks after the enactment
		AuthorityCount::set(20);
		initialize_to_block(20);
		// THEN the count is raised before the judgement, so the window is 40 slots and nobody is
		// dropped
		assert_eq!(dropped(), vec![]);
		assert_eq!(activity(11), active(5, 20));

		// WHEN the lists that end later have 2 authorities and only 10 authors, at block 48
		AuthorityCount::set(2);
		initialize_to_block(40);
		// THEN 11 is not dropped within 40 slots of its enactment and keeps the larger count
		assert_eq!(dropped(), vec![]);
		assert_eq!(activity(11), active(5, 20));
		run_to(50, &[10]);
		// AND the rotation at block 50 drops 11 alone, while 10 is judged on the list of 2 since
		// its block (50 - 49 < 4)
		assert_eq!(dropped(), vec![(11, None)]);
		assert_eq!(activity(10), active(49, 2));
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

#[test]
fn floor_keeps_the_most_recently_active_idle_validators() {
	new_test_ext().execute_with(|| {
		// GIVEN 10, 11, 12 and 13 enacted at block 5 and authoring last at blocks 9, 8, 7 and 6
		initialize_to_block(1);
		[10, 11, 12, 13].into_iter().for_each(set_keys);
		initialize_to_block(3);
		assert_ok!(receive(1, vec![10, 11, 12, 13]));
		frame_support::hypothetically!({
			// WHEN none of them authors, so all four share the enactment block as their record,
			// and the rotation at block 20 applies a floor of 2
			set_rule(2, 10, 2);
			initialize_to_block(20);
			// THEN the tie is broken by account, the two lowest are dropped
			assert_eq!(dropped().into_iter().map(|(v, _)| v).collect::<Vec<_>>(), vec![10, 11]);
			assert_eq!(queued(), vec![1, 2, 12, 13]);
		});
		for (block, author) in [(6, 13), (7, 12), (8, 11), (9, 10)] {
			initialize_to_block(block);
			note_blocks(author, 1);
		}

		frame_support::hypothetically!({
			// WHEN the floor exceeds the number of validators
			set_rule(2, 10, 5);
			initialize_to_block(20);
			// THEN nobody is dropped and the event counts the idle validators kept
			assert_eq!(dropped(), vec![]);
			System::assert_has_event(
				Event::CollatorsDropped { validators: vec![], kept: 4 }.into(),
			);
		});

		// WHEN the rotation at block 20 finds all four idle under a floor of 2
		set_rule(2, 10, 2);
		initialize_to_block(20);
		// THEN the two with the oldest records are dropped and the two others are returned
		System::assert_has_event(
			Event::CollatorsDropped { validators: vec![12, 13], kept: 2 }.into(),
		);
		assert_eq!(queued(), vec![1, 2, 10, 11]);
		assert_eq!(activity(10), active(9, 4));
		assert_eq!(activity(11), active(8, 4));
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

#[test]
fn under_a_cap_only_drawn_validators_are_judged_and_the_draw_skips_dropped_ones() {
	new_test_ext().execute_with(|| {
		// GIVEN a cap of 2 over 10, 11, 12 and 13 with keys, and drops of 10 slots
		initialize_to_block(1);
		let set = vec![10, 11, 12, 13];
		set.iter().copied().for_each(set_keys);
		set_cap(Some(2));
		set_rule(2, 10, 0);
		initialize_to_block(3);
		assert_ok!(receive(1, set.clone()));
		initialize_to_block(5);
		let (a, b) = (drawn()[0], drawn()[1]);
		let outside = set.iter().copied().filter(|v| *v != a && *v != b).collect::<Vec<_>>();

		// WHEN a authors nothing and b authors before the rotation at block 20
		run_to(20, &[b]);
		// THEN only a is dropped, and the validators outside the draw have no record
		assert_eq!(dropped(), vec![(a, None)]);
		assert!(outside.iter().all(|v| activity(*v).is_none()));

		frame_support::hypothetically!({
			// WHEN no new set arrives, so the rotation at block 30 starts its drop and the one at
			// block 40 ends it
			run_to(30, &[b]);
			assert_eq!(dropped(), vec![(a, Some(40))]);
			run_to(40, &[b]);
			// THEN it is queued again in its place of the drawn list
			assert_eq!(dropped(), vec![]);
			assert_eq!(activity(a), Some(Activity::Queued));
			assert_eq!(queued(), vec![1, 2, a, b]);
		});

		frame_support::hypothetically!({
			// WHEN a set that leaves a out arrives at block 21
			initialize_to_block(21);
			assert_ok!(receive(2, set.iter().copied().filter(|v| *v != a).collect()));
			initialize_to_block(22);
			// THEN the first forced rotation removes its drop record
			assert_eq!(dropped(), vec![]);
			assert_ok!(ValidatorCollators::do_try_state());
			// WHEN a set of a and b arrives at block 25, while the drop of a would still run
			initialize_to_block(25);
			assert_ok!(receive(3, vec![a, b]));
			initialize_to_block(26);
			// THEN a is drawn and starts queued, as a validator returned for the first time
			assert!(drawn().contains(&a));
			assert_eq!(activity(a), Some(Activity::Queued));
			initialize_to_block(27);
			assert_eq!(activity(a), active(27, 0));
			assert_ok!(ValidatorCollators::do_try_state());
		});

		// WHEN a new set arrives at block 22 while a is dropped
		initialize_to_block(22);
		assert_ok!(receive(2, set.clone()));
		initialize_to_block(23);
		// THEN the draw skips a and still fills the cap
		let redrawn = drawn();
		assert_eq!(redrawn, reference_draw(expected_seed(2, [7; 32]), &set, 2, &[a]));
		assert_eq!(redrawn.len(), 2);
		assert!(!redrawn.contains(&a));
		// AND the rotation at block 23, which enacts the list without it, starts its drop
		initialize_to_block(24);
		assert_eq!(dropped(), vec![(a, Some(33))]);
		// WHEN its drop ends at the rotation at block 40
		run_to(30, &redrawn);
		run_to(40, &redrawn);
		// THEN it is not returned until the next draw
		assert_eq!(dropped(), vec![]);
		assert_eq!(activity(a), None);
		assert!(!queued().contains(&a));

		// WHEN a set of a alone arrives at block 42, so the next draw returns it
		initialize_to_block(42);
		assert_ok!(receive(3, vec![a]));
		initialize_to_block(43);
		// THEN it starts queued, and its window starts at the enactment instead of its last drop
		assert_eq!(drawn(), vec![a]);
		assert_eq!(activity(a), Some(Activity::Queued));
		initialize_to_block(44);
		assert_eq!(activity(a), active(44, 0));
		initialize_to_block(50);
		assert_eq!(dropped(), vec![]);
		// AND it is judged a full window after the enactment
		initialize_to_block(60);
		assert_eq!(dropped(), vec![(a, None)]);
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

#[test]
fn drop_rule_is_set_by_governance_validated_and_lifted() {
	new_test_ext().execute_with(|| {
		let set_drop_rule = |rule| {
			ValidatorCollators::set_drop_rule(RuntimeOrigin::signed(RootAccount::get()), rule)
		};
		// GIVEN 10 and 11 with keys
		initialize_to_block(1);
		set_keys(10);
		set_keys(11);
		// WHEN a rule with fewer than two rounds or zero slots is set THEN it is rejected
		for (rounds, slots) in [(0, 10), (1, 10), (2, 0)] {
			assert_noop!(
				set_drop_rule(Some(DropRule { rounds, slots, min_collators: 0 })),
				Error::<Test>::InvalidDropRule
			);
		}
		// WHEN a rule without a floor is set THEN it is stored
		let rule = DropRule { rounds: 2, slots: 20, min_collators: 0 };
		assert_ok!(set_drop_rule(Some(rule)));
		System::assert_last_event(Event::DropRuleSet { rule: Some(rule) }.into());
		assert_eq!(CurrentDropRule::<Test>::get(), Some(rule));
		// AND 11 is dropped at block 20 while 10 authors
		initialize_to_block(3);
		assert_ok!(receive(1, vec![10, 11]));
		run_to(20, &[10]);
		assert_eq!(dropped(), vec![(11, None)]);

		frame_support::hypothetically!({
			// WHEN the rule changes to drops of 30 slots before the drop of 11 starts
			set_rule(2, 30, 0);
			run_to(30, &[10]);
			// THEN its drop takes the 30 slots of the rule in force at the rotation that starts it
			assert_eq!(dropped(), vec![(11, Some(60))]);
			// WHEN the rule changes to drops of 5 slots while its drop counts
			set_rule(2, 5, 0);
			run_to(40, &[10]);
			// THEN its drop keeps its end
			assert_eq!(dropped(), vec![(11, Some(60))]);
		});

		frame_support::hypothetically!({
			// WHEN the rule is removed
			assert_ok!(set_drop_rule(None));
			// THEN every drop record is removed at once and 11 is queued at the next rotation
			System::assert_last_event(Event::DropRuleSet { rule: None }.into());
			assert!(!Dropped::<Test>::exists());
			initialize_to_block(30);
			assert_eq!(activity(11), Some(Activity::Queued));
		});
		frame_support::hypothetically!({
			// WHEN the rule is removed and set again in one block, as a batch that fixes a rule
			assert_ok!(set_drop_rule(None));
			assert_ok!(set_drop_rule(Some(rule)));
			// THEN 11 is returned at the next rotation
			run_to(30, &[10]);
			assert_eq!(activity(11), Some(Activity::Queued));
			assert!(queued().contains(&11));
		});

		// WHEN the rotation at block 30 starts the drop of 11 for 20 slots
		run_to(30, &[10]);
		assert_eq!(dropped(), vec![(11, Some(50))]);
		// AND the rotation at block 50 ends it and drops 10, idle since block 39
		run_to(40, &[10]);
		initialize_to_block(50);
		assert_eq!(dropped(), vec![(10, None)]);
		assert_eq!(activity(11), Some(Activity::Queued));
		// AND the rule is removed in the same block
		assert_ok!(set_drop_rule(None));
		// THEN no drop record is left
		assert!(!Dropped::<Test>::exists());
		assert_ok!(ValidatorCollators::do_try_state());
	});
}

#[test]
fn activity_entries_follow_the_returned_list() {
	new_test_ext().execute_with(|| {
		// GIVEN 10 and 11 enacted at block 5, and 12 in the set without keys
		initialize_to_block(1);
		set_keys(10);
		set_keys(11);
		initialize_to_block(3);
		assert_ok!(receive(1, vec![10, 11, 12]));
		initialize_to_block(5);
		// WHEN invulnerable 1 and 12, which is not returned, author blocks
		note_blocks(1, 1);
		note_blocks(12, 1);
		// THEN neither gets a record, while the returned validators have one
		assert_eq!(activity(1), None);
		assert_eq!(activity(12), None);
		assert_eq!(activity(10), active(5, 0));
		// WHEN 11 purges its keys and the next rotation does not return it
		assert_ok!(Session::purge_keys(RuntimeOrigin::signed(11)));
		initialize_to_block(10);
		// THEN its record is removed and 10 keeps its record
		assert_eq!(activity(11), None);
		assert_eq!(activity(10), active(5, 4));
		assert_ok!(ValidatorCollators::do_try_state());

		// WHEN 12 registers keys, is queued at block 20 and purges them before the next rotation
		set_keys(12);
		initialize_to_block(20);
		assert_eq!(activity(12), Some(Activity::Queued));
		assert_ok!(Session::purge_keys(RuntimeOrigin::signed(12)));
		initialize_to_block(30);
		// THEN its queued record is removed
		assert_eq!(activity(12), None);
		// AND when it registers keys again, it is queued again instead of active, so its window
		// starts only at its enactment
		set_keys(12);
		initialize_to_block(40);
		assert_eq!(activity(12), Some(Activity::Queued));
		assert_ok!(ValidatorCollators::do_try_state());

		// AND the invariants reject a record outside the stored set, a record newer than the
		// counted slots, a dropped validator with a record, a drop record without a rule, a drop
		// record outside the stored set and drop records that do not decode
		let records =
			|v: u64| [(v, Some(9))].into_iter().collect::<std::collections::BTreeMap<_, _>>();
		frame_support::hypothetically!({
			set_rule(2, 10, 0);
			Dropped::<Test>::put(crate::DropRecords::<Test>::try_from(records(13)).unwrap());
			assert_eq!(
				ValidatorCollators::do_try_state(),
				Err("a drop record belongs to a validator outside the stored set".into())
			);
		});
		frame_support::hypothetically!({
			set_rule(2, 10, 0);
			frame_support::storage::unhashed::put_raw(
				&Dropped::<Test>::hashed_key(),
				&(100..151u64).map(|v| (v, Some(9u64))).collect::<Vec<_>>().encode(),
			);
			assert_eq!(
				ValidatorCollators::do_try_state(),
				Err("the drop records do not decode".into())
			);
		});
		frame_support::hypothetically!({
			LastActivity::<Test>::insert(
				10,
				Activity::Active { since: counted() + 1, authorities: 4 },
			);
			assert_eq!(
				ValidatorCollators::do_try_state(),
				Err("an activity record is newer than the counted slots".into())
			);
		});
		frame_support::hypothetically!({
			LastActivity::<Test>::insert(13, Activity::Queued);
			assert_eq!(
				ValidatorCollators::do_try_state(),
				Err("an activity record belongs to a validator outside the stored set".into())
			);
		});
		frame_support::hypothetically!({
			set_rule(2, 10, 0);
			Dropped::<Test>::put(crate::DropRecords::<Test>::try_from(records(10)).unwrap());
			assert_eq!(
				ValidatorCollators::do_try_state(),
				Err("a dropped validator has an activity record".into())
			);
		});
		frame_support::hypothetically!({
			Dropped::<Test>::put(crate::DropRecords::<Test>::try_from(records(11)).unwrap());
			assert_eq!(
				ValidatorCollators::do_try_state(),
				Err("validators are dropped without a drop rule".into())
			);
		});
	});
}

#[test]
fn block_in_the_lag_session_creates_no_record() {
	new_test_ext().execute_with(|| {
		// GIVEN 11 dropped for 10 slots at the rotation at block 20 while 10 authors
		initialize_to_block(1);
		set_keys(10);
		set_keys(11);
		set_rule(2, 10, 0);
		initialize_to_block(3);
		assert_ok!(receive(1, vec![10, 11]));
		run_to(20, &[10]);
		assert_eq!(dropped(), vec![(11, None)]);
		// WHEN 11 authors at block 25, in the session it is still an authority of
		initialize_to_block(25);
		assert!(Session::validators().contains(&11));
		note_blocks(11, 1);
		// THEN the block creates no record
		assert_eq!(activity(11), None);

		// AND once returned it is queued at block `queued_at` and gets a fresh window 10 blocks
		// later, so an alive validator is not dropped again for a block it authored before
		let returns_with_a_fresh_window = |queued_at: u64| {
			run_to(queued_at, &[10]);
			assert_eq!(activity(11), Some(Activity::Queued));
			run_to(queued_at + 10, &[10]);
			assert_eq!(activity(11), active(queued_at + 10, 0));
			assert_eq!(dropped(), vec![]);
			assert_ok!(ValidatorCollators::do_try_state());
		};
		frame_support::hypothetically!({
			// WHEN the rule is removed and set again at block 25 instead of the drop ending
			remove_rule();
			set_rule(2, 10, 0);
			returns_with_a_fresh_window(30);
		});
		// WHEN its drop runs from block 30 to block 40
		returns_with_a_fresh_window(40);
	});
}
