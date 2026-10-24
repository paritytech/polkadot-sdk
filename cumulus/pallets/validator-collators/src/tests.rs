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
	mock::*, relay_proof::relay_state_proof, weights::WeightInfo, AuthoredBlocks, Call, Collators,
	Config, DrawnCollators, EpochRandomness, EraValidatorSet, Error, Event, ExpectedEraDuration,
	MaxCollators, PendingRotation, ReportMargin, ReportPeriod, RotationState, SetReceivedAt,
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
	weights::{Weight, WeightMeter},
	BoundedBTreeSet,
};
use pallet_session::SessionManager;
use rand::{seq::SliceRandom, SeedableRng};
use rand_chacha::ChaCha20Rng;
use sp_runtime::{
	testing::UintAuthorityId,
	traits::{BadOrigin, BlakeTwo256, Hash},
	DispatchResult,
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
/// the first `cap` validators with registered keys.
fn reference_draw(seed: [u8; 32], set: &[u64], cap: usize) -> Vec<u64> {
	let mut order = set.to_vec();
	order.sort();
	order.shuffle(&mut ChaCha20Rng::from_seed(seed));
	order.into_iter().filter(Session::is_registered).take(cap).collect()
}

#[test]
fn calls_reject_wrong_origin() {
	build_and_execute(|| {
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
	});
}

#[test]
fn set_is_checked_against_the_stored_era_and_max_validators() {
	build_and_execute(|| {
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
	});
}

#[test]
fn call_with_more_than_max_validators_does_not_decode() {
	build_and_execute(|| {
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
	build_and_execute(|| {
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
		build_and_execute(|| {
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
		});
	}
}

#[test]
fn received_validator_without_keys_is_not_a_collator() {
	build_and_execute(|| {
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
	});
}

#[test]
fn set_and_key_changes_reach_the_session_validators_at_the_next_rotations() {
	build_and_execute(|| {
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
	});
}

#[test]
fn without_pending_set_sessions_rotate_only_periodically() {
	build_and_execute(|| {
		// GIVEN no received validator set
		assert_eq!(ValidatorSet::<Test>::get(), None);
		for block in 1..=35u64 {
			// WHEN blocks are produced
			initialize_to_block(block);
			// THEN the session index grows only at multiples of the period
			assert_eq!(Session::current_index() as u64, block / Period::get());
		}
	});
}

#[test]
fn periodic_rotation_continues_after_forced_rotations() {
	build_and_execute(|| {
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
	});
}

#[test]
fn cap_limits_only_the_validator_side_after_the_key_filter() {
	let set = vec![10, 11, 12, 13];
	// (cap, number of validators returned by the pallet)
	let cases: [(Option<u32>, usize); 5] =
		[(Some(0), 0), (Some(1), 1), (Some(2), 2), (Some(10), 3), (None, 3)];
	for (cap, count) in cases {
		build_and_execute(|| {
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
				let reference = reference_draw(expected_seed(1, [7; 32]), &set, cap as usize);
				assert_eq!(returned, reference, "cap {cap:?}");
				assert_eq!(drawn(), reference);
				System::assert_has_event(
					Event::CollatorsDrawn { era: 1, validators: reference }.into(),
				);
			} else {
				assert_eq!(returned, vec![11, 12, 13]);
			}
		});
	}
	build_and_execute(|| {
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
	build_and_execute(|| {
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
			assert_eq!(drawn(), reference_draw(expected_seed(era, [7; 32]), &set, 1), "era {era}");
		}
	});
}

#[test]
fn cap_change_redraws_with_the_current_randomness_and_forces_two_rotations() {
	build_and_execute(|| {
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
		assert_eq!(three, reference_draw(expected_seed(1, [7; 32]), &set, 3));
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
		let five = reference_draw(expected_seed(1, [8; 32]), &set, 5);
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
		let two = reference_draw(expected_seed(1, [9; 32]), &set, 2);
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
	});
}

#[test]
fn validator_registering_keys_after_the_draw_waits_for_the_next_draw() {
	build_and_execute(|| {
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
	});
}

#[test]
fn cap_without_a_stored_set_draws_nothing_and_forces_no_rotation() {
	build_and_execute(|| {
		// GIVEN no stored set
		initialize_to_block(1);
		// WHEN a cap is set and then removed
		set_cap(Some(1));
		assert_eq!(Collators::<Test>::get(), None);
		set_cap(None);
		// THEN nothing is drawn and no rotation is pending
		assert_eq!(Collators::<Test>::get(), None);
		assert_eq!(PendingRotation::<Test>::get(), RotationState::Idle);
	});
}

#[test]
fn capped_set_draws_with_a_zero_value_without_cache_or_with_the_key_absent() {
	build_and_execute(|| {
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
		assert_eq!(drawn(), reference_draw(expected_seed(1, [0; 32]), &[10, 11], 1));
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
		assert_eq!(drawn(), reference_draw(expected_seed(2, [0; 32]), &[10, 11], 1));
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
	build_and_execute(|| {
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
	build_and_execute(|| {
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
	build_and_execute(|| {
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
		build_and_execute(|| {
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
		});
	}
}

#[test]
fn empty_set_is_accepted_and_contributes_no_collators() {
	build_and_execute(|| {
		// GIVEN no stored validator set
		assert_eq!(ValidatorSet::<Test>::get(), None);
		// WHEN an empty set is received
		assert_ok!(receive(1, vec![]));
		// THEN the pallet returns an empty list rather than None
		assert_eq!(<ValidatorCollators as SessionManager<u64>>::new_session(1), Some(vec![]));
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
	build_and_execute(|| {
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
	});
}

#[test]
fn report_of_the_whole_set_is_one_call() {
	build_and_execute(|| {
		// GIVEN a set of `MaxValidators` validators, each of which authored a block
		let max = <<Test as Config>::MaxValidators as frame_support::traits::Get<u32>>::get();
		let validators = (100..100 + max as u64).collect::<Vec<_>>();
		initialize_to_block(1);
		assert_ok!(receive(1, validators.clone()));
		validators.iter().for_each(|&v| note_blocks(v, 1));

		// WHEN the report block comes
		initialize_to_block(5);

		// THEN the reporter gets every count in one call
		assert_eq!(ReportCalls::get(), 1);
		assert_eq!(reported_entries(), validators.iter().map(|&v| (v, 1)).collect::<Vec<_>>());

		// AND the counts are dropped
		assert_eq!(counts(), vec![]);
	});
}

#[test]
fn failed_report_keeps_the_counts_of_the_stored_set_only() {
	build_and_execute(|| {
		// GIVEN counts of three validators and of invulnerable 1, and a reporter that rejects its
		// first call after recording it
		FailingReportCall::set(Some(1));
		initialize_to_block(1);
		assert_ok!(receive(1, vec![10, 11, 12]));
		note_blocks(10, 1);
		note_blocks(11, 2);
		note_blocks(12, 3);
		// non-validator who authored a block.
		note_blocks(1, 1);

		// WHEN the report block comes
		initialize_to_block(5);

		// THEN the rejected report is rolled back, the counts of the validators are kept and the
		// count of invulnerable 1 is dropped
		assert_eq!(ReportCalls::get(), 1);
		assert!(reported().is_empty());
		assert_eq!(counts(), vec![(10, 1), (11, 2), (12, 3)]);
		System::assert_has_event(Event::BlocksReportFailed { kept: 3, dropped: 1 }.into());

		// WHEN more blocks are authored and the reporter accepts the next report
		note_blocks(10, 4);
		note_blocks(1, 2);
		initialize_to_block(10);

		// THEN it carries the sum of both periods, so nothing is lost or reported twice
		assert_eq!(reported_entries(), vec![(10, 5), (11, 2), (12, 3)]);
		assert_eq!(counts(), vec![]);
		System::assert_has_event(Event::BlocksReported { authors: 4, reported: 3 }.into());
	});
}

#[test]
fn report_handles_at_most_max_entries_per_report_entries() {
	build_and_execute(|| {
		// GIVEN four counted validators, and a `MaxEntriesPerReport` of two
		initialize_to_block(1);
		assert_ok!(receive(1, vec![10, 11, 12, 13]));
		[10, 11, 12, 13].into_iter().for_each(|v| note_blocks(v, 1));
		MaxEntriesPerReport::set(2);

		// WHEN the counts are reported
		ValidatorCollators::report_blocks(2);

		// THEN two of them are reported and removed, and the other two wait for the next report
		assert_eq!(ReportCalls::get(), 1);
		assert_eq!(reported_entries().len(), 2);
		assert_eq!(AuthoredBlocks::<Test>::count(), 2);
		let mut all = reported_entries();
		all.extend(counts());
		all.sort();
		assert_eq!(all, vec![(10, 1), (11, 1), (12, 1), (13, 1)]);
	});
}

#[test]
fn counts_of_a_validator_that_left_the_set_are_dropped_at_the_next_report() {
	build_and_execute(|| {
		// GIVEN the set of era 1 with 10 and 11, both of which authored blocks since the last
		// report
		initialize_to_block(1);
		assert_ok!(receive(1, vec![10, 11]));
		note_blocks(10, 2);
		note_blocks(11, 3);

		// WHEN the set of era 2 without 10 arrives before the next report
		assert_ok!(receive(2, vec![11, 12]));
		initialize_to_block(5);

		// THEN the report filters against the set of era 2: 11 is reported with the blocks it
		// authored under era 1, and the blocks of 10 are dropped
		assert_eq!(reported_entries(), vec![(11, 3)]);
		assert_eq!(counts(), vec![]);
		System::assert_has_event(Event::BlocksReported { authors: 2, reported: 1 }.into());
	});
}

#[test]
fn report_with_an_undecodable_stored_set_drops_every_count() {
	// The stored set breaks the invariants, so the test does not check them at its end.
	new_test_ext().execute_with(|| {
		// GIVEN 10 authored blocks and the stored set no longer decodes
		initialize_to_block(1);
		assert_ok!(receive(1, vec![10]));
		note_blocks(10, 2);
		frame_support::storage::unhashed::put_raw(
			&ValidatorSet::<Test>::hashed_key(),
			&(1u32, (0..51u64).collect::<Vec<_>>()).encode(),
		);

		// WHEN the counts are reported
		ValidatorCollators::report_blocks(AuthoredBlocks::<Test>::count());

		// THEN nothing reaches the reporter and every count is dropped, as without a stored set
		assert!(reported().is_empty());
		assert_eq!(counts(), vec![]);
		System::assert_last_event(Event::BlocksReported { authors: 1, reported: 0 }.into());
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
	build_and_execute(|| {
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
	build_and_execute(|| {
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
		let poll = || {
			let mut meter = WeightMeter::new();
			ValidatorCollators::on_poll(5, &mut meter);
			meter.consumed()
		};
		// AND the charge of block 5 at the due time if it were not a `ReportPeriod` block
		let early_only = frame_support::hypothetically!({
			ReportPeriod::<Test>::put(1000);
			poll()
		});
		// WHEN block 5 runs at the due time
		let both = poll();
		// THEN one report is made and charged, and the arrival is removed
		assert_eq!(both, early_only);
		assert_eq!(reports(), 1);
		assert_eq!(reported().len(), 1);
		assert_eq!(SetReceivedAt::<Test>::get(), None);
	});

	// A rejected early report is not retried, and the next `ReportPeriod` block carries its counts.
	build_and_execute(|| {
		// GIVEN the default period of 5 blocks, a set received at time 0 and a reporter that
		// rejects its first call
		FailingReportCall::set(Some(1));
		initialize_to_block(1);
		assert_ok!(receive(1, vec![10]));
		// WHEN block 2 runs at the due time
		block_at(900);
		// THEN the report fails, the arrival is removed and the counts are kept
		System::assert_has_event(Event::BlocksReportFailed { kept: 1, dropped: 0 }.into());
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
	build_and_execute(|| {
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
	build_and_execute(|| {
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
	build_and_execute(|| {
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
	build_and_execute(|| {
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
	build_and_execute(|| {
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
	});
}

#[test]
fn report_that_does_not_fit_the_meter_is_postponed() {
	build_and_execute(|| {
		// GIVEN 10 authored a block, and the early report of its set, received at time 0, is due
		// at time 900
		initialize_to_block(1);
		assert_ok!(ValidatorCollators::set_report_period(root(), 1000));
		assert_ok!(receive(1, vec![10]));
		note_blocks(10, 1);
		Now::set(900);
		let checks = <Test as Config>::WeightInfo::on_poll();
		let full = checks.saturating_add(ValidatorCollators::report_weight(1));

		// WHEN block 2 has no room for the checks
		System::set_block_number(2);
		let mut meter = WeightMeter::with_limit(Weight::zero());
		ValidatorCollators::on_poll(2, &mut meter);
		// THEN the reporter is not called, nothing is charged and the early report stays due
		assert_eq!(ReportCalls::get(), 0);
		assert_eq!(meter.consumed(), Weight::zero());
		assert_eq!(SetReceivedAt::<Test>::get(), Some(0));

		// WHEN block 3 has room for the checks only
		System::set_block_number(3);
		let mut meter = WeightMeter::with_limit(checks);
		ValidatorCollators::on_poll(3, &mut meter);
		// THEN the reporter is not called, only the checks are charged, and the counts are kept
		// for the early report, which stays due
		assert_eq!(ReportCalls::get(), 0);
		assert_eq!(meter.consumed(), checks);
		assert_eq!(counts(), vec![(10, 1)]);
		assert_eq!(SetReceivedAt::<Test>::get(), Some(0));

		// WHEN block 4 has room for the report
		System::set_block_number(4);
		let mut meter = WeightMeter::with_limit(full);
		ValidatorCollators::on_poll(4, &mut meter);
		// THEN the reporter gets the counts once, the report is charged, and the arrival is
		// removed
		assert_eq!(ReportCalls::get(), 1);
		assert_eq!(reported_entries(), vec![(10, 1)]);
		assert_eq!(meter.consumed(), full);
		assert_eq!(SetReceivedAt::<Test>::get(), None);
	});
}

#[test]
fn setters_are_bounded_and_announced() {
	build_and_execute(|| {
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
