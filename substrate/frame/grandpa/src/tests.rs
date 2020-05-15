// This file is part of Substrate.

// Copyright (C) 2017-2020 Parity Technologies (UK) Ltd.
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

//! Tests for the module.

#![cfg(test)]

use super::*;
use crate::mock::*;
use codec::{Decode, Encode};
use fg_primitives::ScheduledChange;
use frame_support::{
	assert_err, assert_ok,
	traits::{Currency, OnFinalize},
};
use frame_system::{EventRecord, Phase};
use sp_core::H256;
use sp_keyring::Ed25519Keyring;
use sp_runtime::{testing::Digest, traits::Header};

#[test]
fn authorities_change_logged() {
	new_test_ext(vec![(1, 1), (2, 1), (3, 1)]).execute_with(|| {
		initialize_block(1, Default::default());
		Grandpa::schedule_change(to_authorities(vec![(4, 1), (5, 1), (6, 1)]), 0, None).unwrap();

		System::note_finished_extrinsics();
		Grandpa::on_finalize(1);

		let header = System::finalize();
		assert_eq!(header.digest, Digest {
			logs: vec![
				grandpa_log(ConsensusLog::ScheduledChange(
					ScheduledChange { delay: 0, next_authorities: to_authorities(vec![(4, 1), (5, 1), (6, 1)]) }
				)),
			],
		});

		assert_eq!(System::events(), vec![
			EventRecord {
				phase: Phase::Finalization,
				event: Event::NewAuthorities(to_authorities(vec![(4, 1), (5, 1), (6, 1)])).into(),
				topics: vec![],
			},
		]);
	});
}

#[test]
fn authorities_change_logged_after_delay() {
	new_test_ext(vec![(1, 1), (2, 1), (3, 1)]).execute_with(|| {
		initialize_block(1, Default::default());
		Grandpa::schedule_change(to_authorities(vec![(4, 1), (5, 1), (6, 1)]), 1, None).unwrap();
		Grandpa::on_finalize(1);
		let header = System::finalize();
		assert_eq!(header.digest, Digest {
			logs: vec![
				grandpa_log(ConsensusLog::ScheduledChange(
					ScheduledChange { delay: 1, next_authorities: to_authorities(vec![(4, 1), (5, 1), (6, 1)]) }
				)),
			],
		});

		// no change at this height.
		assert_eq!(System::events(), vec![]);

		initialize_block(2, header.hash());
		System::note_finished_extrinsics();
		Grandpa::on_finalize(2);

		let _header = System::finalize();
		assert_eq!(System::events(), vec![
			EventRecord {
				phase: Phase::Finalization,
				event: Event::NewAuthorities(to_authorities(vec![(4, 1), (5, 1), (6, 1)])).into(),
				topics: vec![],
			},
		]);
	});
}

#[test]
fn cannot_schedule_change_when_one_pending() {
	new_test_ext(vec![(1, 1), (2, 1), (3, 1)]).execute_with(|| {
		initialize_block(1, Default::default());
		Grandpa::schedule_change(to_authorities(vec![(4, 1), (5, 1), (6, 1)]), 1, None).unwrap();
		assert!(<PendingChange<Test>>::exists());
		assert!(Grandpa::schedule_change(to_authorities(vec![(5, 1)]), 1, None).is_err());

		Grandpa::on_finalize(1);
		let header = System::finalize();

		initialize_block(2, header.hash());
		assert!(<PendingChange<Test>>::exists());
		assert!(Grandpa::schedule_change(to_authorities(vec![(5, 1)]), 1, None).is_err());

		Grandpa::on_finalize(2);
		let header = System::finalize();

		initialize_block(3, header.hash());
		assert!(!<PendingChange<Test>>::exists());
		assert!(Grandpa::schedule_change(to_authorities(vec![(5, 1)]), 1, None).is_ok());

		Grandpa::on_finalize(3);
		let _header = System::finalize();
	});
}

#[test]
fn new_decodes_from_old() {
	let old = OldStoredPendingChange {
		scheduled_at: 5u32,
		delay: 100u32,
		next_authorities: to_authorities(vec![(1, 5), (2, 10), (3, 2)]),
	};

	let encoded = old.encode();
	let new = StoredPendingChange::<u32>::decode(&mut &encoded[..]).unwrap();
	assert!(new.forced.is_none());
	assert_eq!(new.scheduled_at, old.scheduled_at);
	assert_eq!(new.delay, old.delay);
	assert_eq!(new.next_authorities, old.next_authorities);
}

#[test]
fn dispatch_forced_change() {
	new_test_ext(vec![(1, 1), (2, 1), (3, 1)]).execute_with(|| {
		initialize_block(1, Default::default());
		Grandpa::schedule_change(
			to_authorities(vec![(4, 1), (5, 1), (6, 1)]),
			5,
			Some(0),
		).unwrap();

		assert!(<PendingChange<Test>>::exists());
		assert!(Grandpa::schedule_change(to_authorities(vec![(5, 1)]), 1, Some(0)).is_err());

		Grandpa::on_finalize(1);
		let mut header = System::finalize();

		for i in 2..7 {
			initialize_block(i, header.hash());
			assert!(<PendingChange<Test>>::get().unwrap().forced.is_some());
			assert_eq!(Grandpa::next_forced(), Some(11));
			assert!(Grandpa::schedule_change(to_authorities(vec![(5, 1)]), 1, None).is_err());
			assert!(Grandpa::schedule_change(to_authorities(vec![(5, 1)]), 1, Some(0)).is_err());

			Grandpa::on_finalize(i);
			header = System::finalize();
		}

		// change has been applied at the end of block 6.
		// add a normal change.
		{
			initialize_block(7, header.hash());
			assert!(!<PendingChange<Test>>::exists());
			assert_eq!(Grandpa::grandpa_authorities(), to_authorities(vec![(4, 1), (5, 1), (6, 1)]));
			assert!(Grandpa::schedule_change(to_authorities(vec![(5, 1)]), 1, None).is_ok());
			Grandpa::on_finalize(7);
			header = System::finalize();
		}

		// run the normal change.
		{
			initialize_block(8, header.hash());
			assert!(<PendingChange<Test>>::exists());
			assert_eq!(Grandpa::grandpa_authorities(), to_authorities(vec![(4, 1), (5, 1), (6, 1)]));
			assert!(Grandpa::schedule_change(to_authorities(vec![(5, 1)]), 1, None).is_err());
			Grandpa::on_finalize(8);
			header = System::finalize();
		}

		// normal change applied. but we can't apply a new forced change for some
		// time.
		for i in 9..11 {
			initialize_block(i, header.hash());
			assert!(!<PendingChange<Test>>::exists());
			assert_eq!(Grandpa::grandpa_authorities(), to_authorities(vec![(5, 1)]));
			assert_eq!(Grandpa::next_forced(), Some(11));
			assert!(Grandpa::schedule_change(to_authorities(vec![(5, 1), (6, 1)]), 5, Some(0)).is_err());
			Grandpa::on_finalize(i);
			header = System::finalize();
		}

		{
			initialize_block(11, header.hash());
			assert!(!<PendingChange<Test>>::exists());
			assert!(Grandpa::schedule_change(to_authorities(vec![(5, 1), (6, 1), (7, 1)]), 5, Some(0)).is_ok());
			assert_eq!(Grandpa::next_forced(), Some(21));
			Grandpa::on_finalize(11);
			header = System::finalize();
		}
		let _ = header;
	});
}

#[test]
fn schedule_pause_only_when_live() {
	new_test_ext(vec![(1, 1), (2, 1), (3, 1)]).execute_with(|| {
		// we schedule a pause at block 1 with delay of 1
		initialize_block(1, Default::default());
		Grandpa::schedule_pause(1).unwrap();

		// we've switched to the pending pause state
		assert_eq!(
			Grandpa::state(),
			StoredState::PendingPause {
				scheduled_at: 1u64,
				delay: 1,
			},
		);

		Grandpa::on_finalize(1);
		let _ = System::finalize();

		initialize_block(2, Default::default());

		// signaling a pause now should fail
		assert!(Grandpa::schedule_pause(1).is_err());

		Grandpa::on_finalize(2);
		let _ = System::finalize();

		// after finalizing block 2 the set should have switched to paused state
		assert_eq!(
			Grandpa::state(),
			StoredState::Paused,
		);
	});
}

#[test]
fn schedule_resume_only_when_paused() {
	new_test_ext(vec![(1, 1), (2, 1), (3, 1)]).execute_with(|| {
		initialize_block(1, Default::default());

		// the set is currently live, resuming it is an error
		assert!(Grandpa::schedule_resume(1).is_err());

		assert_eq!(
			Grandpa::state(),
			StoredState::Live,
		);

		// we schedule a pause to be applied instantly
		Grandpa::schedule_pause(0).unwrap();
		Grandpa::on_finalize(1);
		let _ = System::finalize();

		assert_eq!(
			Grandpa::state(),
			StoredState::Paused,
		);

		// we schedule the set to go back live in 2 blocks
		initialize_block(2, Default::default());
		Grandpa::schedule_resume(2).unwrap();
		Grandpa::on_finalize(2);
		let _ = System::finalize();

		initialize_block(3, Default::default());
		Grandpa::on_finalize(3);
		let _ = System::finalize();

		initialize_block(4, Default::default());
		Grandpa::on_finalize(4);
		let _ = System::finalize();

		// it should be live at block 4
		assert_eq!(
			Grandpa::state(),
			StoredState::Live,
		);
	});
}

#[test]
fn time_slot_have_sane_ord() {
	// Ensure that `Ord` implementation is sane.
	const FIXTURE: &[GrandpaTimeSlot] = &[
		GrandpaTimeSlot {
			set_id: 0,
			round: 0,
		},
		GrandpaTimeSlot {
			set_id: 0,
			round: 1,
		},
		GrandpaTimeSlot {
			set_id: 1,
			round: 0,
		},
		GrandpaTimeSlot {
			set_id: 1,
			round: 1,
		},
		GrandpaTimeSlot {
			set_id: 1,
			round: 2,
		}
	];
	assert!(FIXTURE.windows(2).all(|f| f[0] < f[1]));
}

fn test_authorities() -> AuthorityList {
	let authorities = vec![
		Ed25519Keyring::Alice,
		Ed25519Keyring::Bob,
		Ed25519Keyring::Charlie,
	];

	authorities
		.into_iter()
		.map(|id| (id.public().into(), 1u64))
		.collect()
}

#[test]
fn report_equivocation_current_set_works() {
	let authorities = test_authorities();

	new_test_ext_raw_authorities(authorities).execute_with(|| {
		assert_eq!(Staking::current_era(), Some(0));
		assert_eq!(Session::current_index(), 0);

		start_era(1);

		let authorities = Grandpa::grandpa_authorities();

		// make sure that all authorities have the same balance
		for i in 0..authorities.len() {
			assert_eq!(Balances::total_balance(&(i as u64)), 10_000_000);
			assert_eq!(Staking::slashable_balance_of(&(i as u64)), 10_000);

			assert_eq!(
				Staking::eras_stakers(1, i as u64),
				pallet_staking::Exposure {
					total: 10_000,
					own: 10_000,
					others: vec![],
				},
			);
		}

		let equivocation_authority_index = 0;
		let equivocation_key = &authorities[equivocation_authority_index].0;
		let equivocation_keyring = extract_keyring(equivocation_key);

		let set_id = Grandpa::current_set_id();

		// generate an equivocation proof, with two votes in the same round for
		// different block hashes signed by the same key
		let equivocation_proof = generate_equivocation_proof(
			set_id,
			(1, H256::random(), 10, &equivocation_keyring),
			(1, H256::random(), 10, &equivocation_keyring),
		);

		// create the key ownership proof
		let key_owner_proof =
			Historical::prove((sp_finality_grandpa::KEY_TYPE, &equivocation_key)).unwrap();

		// report the equivocation and the tx should be dispatched successfully
		let inner = report_equivocation(equivocation_proof, key_owner_proof).unwrap();
		assert_ok!(Grandpa::dispatch(inner, Origin::signed(1)));

		start_era(2);

		// check that the balance of 0-th validator is slashed 100%.
		assert_eq!(Balances::total_balance(&0), 10_000_000 - 10_000);
		assert_eq!(Staking::slashable_balance_of(&0), 0);

		assert_eq!(
			Staking::eras_stakers(2, 0),
			pallet_staking::Exposure {
				total: 0,
				own: 0,
				others: vec![],
			},
		);

		// check that the balances of all other validators are left intact.
		for i in 1..authorities.len() {
			assert_eq!(Balances::total_balance(&(i as u64)), 10_000_000);
			assert_eq!(Staking::slashable_balance_of(&(i as u64)), 10_000);

			assert_eq!(
				Staking::eras_stakers(2, i as u64),
				pallet_staking::Exposure {
					total: 10_000,
					own: 10_000,
					others: vec![],
				},
			);
		}
	});
}

#[test]
fn report_equivocation_old_set_works() {
	let authorities = test_authorities();

	new_test_ext_raw_authorities(authorities).execute_with(|| {
		start_era(1);

		let authorities = Grandpa::grandpa_authorities();

		let equivocation_authority_index = 0;
		let equivocation_key = &authorities[equivocation_authority_index].0;

		// create the key ownership proof in the "old" set
		let key_owner_proof =
			Historical::prove((sp_finality_grandpa::KEY_TYPE, &equivocation_key)).unwrap();

		start_era(2);

		// make sure that all authorities have the same balance
		for i in 0..authorities.len() {
			assert_eq!(Balances::total_balance(&(i as u64)), 10_000_000);
			assert_eq!(Staking::slashable_balance_of(&(i as u64)), 10_000);

			assert_eq!(
				Staking::eras_stakers(2, i as u64),
				pallet_staking::Exposure {
					total: 10_000,
					own: 10_000,
					others: vec![],
				},
			);
		}

		let equivocation_keyring = extract_keyring(equivocation_key);

		let set_id = Grandpa::current_set_id();

		// generate an equivocation proof for the old set,
		let equivocation_proof = generate_equivocation_proof(
			set_id - 1,
			(1, H256::random(), 10, &equivocation_keyring),
			(1, H256::random(), 10, &equivocation_keyring),
		);

		// report the equivocation using the key ownership proof generated on
		// the old set, the tx should be dispatched successfully
		let inner = report_equivocation(equivocation_proof, key_owner_proof).unwrap();
		assert_ok!(Grandpa::dispatch(inner, Origin::signed(1)));

		start_era(3);

		// check that the balance of 0-th validator is slashed 100%.
		assert_eq!(Balances::total_balance(&0), 10_000_000 - 10_000);
		assert_eq!(Staking::slashable_balance_of(&0), 0);

		assert_eq!(
			Staking::eras_stakers(3, 0),
			pallet_staking::Exposure {
				total: 0,
				own: 0,
				others: vec![],
			},
		);

		// check that the balances of all other validators are left intact.
		for i in 1..authorities.len() {
			assert_eq!(Balances::total_balance(&(i as u64)), 10_000_000);
			assert_eq!(Staking::slashable_balance_of(&(i as u64)), 10_000);

			assert_eq!(
				Staking::eras_stakers(3, i as u64),
				pallet_staking::Exposure {
					total: 10_000,
					own: 10_000,
					others: vec![],
				},
			);
		}
	});
}

#[test]
fn report_equivocation_invalid_set_id() {
	let authorities = test_authorities();

	new_test_ext_raw_authorities(authorities).execute_with(|| {
		start_era(1);

		let authorities = Grandpa::grandpa_authorities();

		let equivocation_authority_index = 0;
		let equivocation_key = &authorities[equivocation_authority_index].0;
		let equivocation_keyring = extract_keyring(equivocation_key);

		let key_owner_proof =
			Historical::prove((sp_finality_grandpa::KEY_TYPE, &equivocation_key)).unwrap();

		let set_id = Grandpa::current_set_id();

		// generate an equivocation for a future set
		let equivocation_proof = generate_equivocation_proof(
			set_id + 1,
			(1, H256::random(), 10, &equivocation_keyring),
			(1, H256::random(), 10, &equivocation_keyring),
		);

		// it should be filtered by the signed extension validation
		assert_err!(
			report_equivocation(equivocation_proof, key_owner_proof),
			equivocation::ReportEquivocationValidityError::InvalidSetId,
		);
	});
}

#[test]
fn report_equivocation_invalid_session() {
	let authorities = test_authorities();

	new_test_ext_raw_authorities(authorities).execute_with(|| {
		start_era(1);

		let authorities = Grandpa::grandpa_authorities();

		let equivocation_authority_index = 0;
		let equivocation_key = &authorities[equivocation_authority_index].0;
		let equivocation_keyring = extract_keyring(equivocation_key);

		// generate a key ownership proof at set id = 1
		let key_owner_proof =
			Historical::prove((sp_finality_grandpa::KEY_TYPE, &equivocation_key)).unwrap();

		start_era(2);

		let set_id = Grandpa::current_set_id();

		// generate an equivocation proof at set id = 2
		let equivocation_proof = generate_equivocation_proof(
			set_id,
			(1, H256::random(), 10, &equivocation_keyring),
			(1, H256::random(), 10, &equivocation_keyring),
		);

		// report an equivocation for the current set using an key ownership
		// proof from the previous set, the session should be invalid.
		assert_err!(
			report_equivocation(equivocation_proof, key_owner_proof),
			equivocation::ReportEquivocationValidityError::InvalidSession,
		);
	});
}

#[test]
fn report_equivocation_invalid_key_owner_proof() {
	let authorities = test_authorities();

	new_test_ext_raw_authorities(authorities).execute_with(|| {
		start_era(1);
		let authorities = Grandpa::grandpa_authorities();

		let invalid_owner_authority_index = 1;
		let invalid_owner_key = &authorities[invalid_owner_authority_index].0;

		// generate a key ownership proof for the authority at index 1
		let invalid_key_owner_proof =
			Historical::prove((sp_finality_grandpa::KEY_TYPE, &invalid_owner_key)).unwrap();

		let equivocation_authority_index = 0;
		let equivocation_key = &authorities[equivocation_authority_index].0;
		let equivocation_keyring = extract_keyring(equivocation_key);

		let set_id = Grandpa::current_set_id();

		// generate an equivocation proof for the authority at index 0
		let equivocation_proof = generate_equivocation_proof(
			set_id,
			(1, H256::random(), 10, &equivocation_keyring),
			(1, H256::random(), 10, &equivocation_keyring),
		);

		// we need to start a new era otherwise the key ownership proof won't be
		// checked since the authorities are part of the current session
		start_era(2);

		// report an equivocation for the current set using a key ownership
		// proof for a different key than the one in the equivocation proof.
		assert_err!(
			report_equivocation(equivocation_proof, invalid_key_owner_proof),
			equivocation::ReportEquivocationValidityError::InvalidKeyOwnershipProof,
		);
	});
}

#[test]
fn report_equivocation_invalid_equivocation_proof() {
	let authorities = test_authorities();

	new_test_ext_raw_authorities(authorities).execute_with(|| {
		start_era(1);

		let authorities = Grandpa::grandpa_authorities();

		let equivocation_authority_index = 0;
		let equivocation_key = &authorities[equivocation_authority_index].0;
		let equivocation_keyring = extract_keyring(equivocation_key);

		// generate a key ownership proof at set id = 1
		let key_owner_proof =
			Historical::prove((sp_finality_grandpa::KEY_TYPE, &equivocation_key)).unwrap();

		let set_id = Grandpa::current_set_id();

		let assert_invalid_equivocation_proof = |equivocation_proof| {
			assert_err!(
				report_equivocation(equivocation_proof, key_owner_proof.clone()),
				equivocation::ReportEquivocationValidityError::InvalidEquivocationProof,
			);
		};

		start_era(2);

		// both votes target the same block number and hash,
		// there is no equivocation.
		assert_invalid_equivocation_proof(generate_equivocation_proof(
			set_id,
			(1, H256::zero(), 10, &equivocation_keyring),
			(1, H256::zero(), 10, &equivocation_keyring),
		));

		// votes targetting different rounds, there is no equivocation.
		assert_invalid_equivocation_proof(generate_equivocation_proof(
			set_id,
			(1, H256::random(), 10, &equivocation_keyring),
			(2, H256::random(), 10, &equivocation_keyring),
		));

		// votes signed with different authority keys
		assert_invalid_equivocation_proof(generate_equivocation_proof(
			set_id,
			(1, H256::random(), 10, &equivocation_keyring),
			(1, H256::random(), 10, &Ed25519Keyring::Charlie),
		));

		// votes signed with a key that isn't part of the authority set
		assert_invalid_equivocation_proof(generate_equivocation_proof(
			set_id,
			(1, H256::random(), 10, &equivocation_keyring),
			(1, H256::random(), 10, &Ed25519Keyring::Dave),
		));
	});
}
