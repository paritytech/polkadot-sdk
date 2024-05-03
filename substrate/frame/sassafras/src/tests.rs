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

//! Tests for Sassafras pallet.

use crate::*;
use mock::*;

use sp_consensus_sassafras::Slot;

fn h2b<const N: usize>(hex: &str) -> [u8; N] {
	array_bytes::hex2array_unchecked(hex)
}

fn b2h<const N: usize>(bytes: [u8; N]) -> String {
	array_bytes::bytes2hex("", &bytes)
}

// Fisher Yates shuffle.
//
// We don't want to implement anything secure here.
// Just a trivial shuffle for the tests.
fn shuffle<T>(vector: &mut Vec<T>, random_seed: u64) {
	let mut rng = random_seed as usize;
	for i in (1..vector.len()).rev() {
		let j = rng % (i + 1);
		vector.swap(i, j);
		rng = (rng.wrapping_mul(6364793005) + 1) as usize; // Some random number generation
	}
}

fn dummy_tickets(count: usize) -> Vec<(TicketId, TicketBody)> {
	(0..count)
		.map(|v| {
			let id = TicketId([v as u8; 32]);
			let body = TicketBody { attempt_idx: v as u32, extra: Default::default() };
			(id, body)
		})
		.collect()
}

#[test]
fn genesis_values_assumptions_check() {
	new_test_ext(3).execute_with(|| {
		assert_eq!(Sassafras::authorities().len(), 3);
	});
}

#[test]
fn deposit_tickets_failure() {
	new_test_ext(3).execute_with(|| {
		let mut tickets = dummy_tickets(15);
		shuffle(&mut tickets, 123);

		let mut candidates = tickets[..5].to_vec();
		Sassafras::deposit_tickets(&candidates).unwrap();
		assert_eq!(TicketsAccumulator::<Test>::count(), 5);

		candidates.sort_unstable_by(|a, b| b.0.cmp(&a.0));
		let stored: Vec<_> = TicketsAccumulator::<Test>::iter()
			.map(|(k, b)| (TicketId::from(k), b))
			.collect();
		assert_eq!(candidates, stored);

		TicketsAccumulator::<Test>::iter().for_each(|(key, body)| {
			println!("{:?}, {:?}", TicketId::from(key), body);
		});

		println!("-----------------------");

		Sassafras::deposit_tickets(&tickets[5..7]).unwrap();
		assert_eq!(TicketsAccumulator::<Test>::count(), 7);

		TicketsAccumulator::<Test>::iter().for_each(|(key, body)| {
			println!("{:?}, {:?}", TicketId::from(key), body);
		});

		assert!(Sassafras::deposit_tickets(&tickets[7..]).is_err());
		println!("ENTRIES: {}", TicketsAccumulator::<Test>::count());
	});
}

#[test]
fn post_genesis_randomness_initialization() {
	let (pairs, mut ext) = new_test_ext_with_pairs(1, false);
	let pair = &pairs[0];

	ext.execute_with(|| {
		let genesis_randomness = Sassafras::randomness();
		assert_eq!(genesis_randomness, RandomnessBuffer::default());

		// Test the values with a zero genesis block hash

		let _ = initialize_block(1, 123.into(), [0x00; 32].into(), pair);

		let randomness = Sassafras::randomness();
		assert_eq!(randomness[0], randomness[1]);
		println!("[RAND] {}", b2h(randomness[0]));
		assert_eq!(
			randomness[0],
			h2b("febcc7fe9539fe17ed29f525831394edfb30b301755dc9bd91584a1f065faf87")
		);
		assert_eq!(randomness[2], randomness[3]);
		assert_eq!(randomness[2], Randomness::default());

		let (id1, _) = make_ticket_body(0, pair);

		// Reset what is relevant
		RandomnessBuf::<Test>::set(genesis_randomness);

		// Test the values with a non-zero genesis block hash

		let _ = initialize_block(1, 123.into(), [0xff; 32].into(), pair);

		let randomness = Sassafras::randomness();
		assert_eq!(randomness[0], randomness[1]);
		println!("[RAND] {}", b2h(randomness[0]));
		assert_eq!(
			randomness[0],
			h2b("466bf3007f2e17bffee0b3c42c90f33d654f5ff61eff28b0cc650825960abd52")
		);
		assert_eq!(randomness[2], randomness[3]);
		assert_eq!(randomness[2], Randomness::default());

		let (id2, _) = make_ticket_body(0, pair);

		// Ticket ids should be different when next epoch randomness is different
		assert_ne!(id1, id2);
	});
}

// Tests if the sorted tickets are assigned to each slot outside-in.
#[test]
fn slot_ticket_id_outside_in_fetch() {
	let genesis_slot = Slot::from(100);
	let curr_count = 8;
	let next_count = 6;
	let tickets_count = curr_count + next_count;

	let tickets: Vec<_> = (0..tickets_count)
		.map(|i| {
			(TicketId([i as u8; 32]), TicketBody { attempt_idx: 0, extra: Default::default() })
		})
		.collect();
	// Current epoch tickets
	let curr_tickets = tickets[..curr_count].to_vec();
	let next_tickets = tickets[curr_count..].to_vec();

	new_test_ext(0).execute_with(|| {
		curr_tickets
			.iter()
			.enumerate()
			.for_each(|(i, t)| Tickets::<Test>::insert((0, i as u32), t));

		next_tickets
			.iter()
			.enumerate()
			.for_each(|(i, t)| Tickets::<Test>::insert((1, i as u32), t));

		TicketsMeta::<Test>::set(TicketsMetadata {
			tickets_count: [curr_count as u32, next_count as u32],
		});

		// Before importing the first block the pallet always return `None`
		// This is a kind of special hardcoded case that should never happen in practice
		// as the first thing the pallet does is to initialize the genesis slot.

		assert_eq!(Sassafras::slot_ticket(0.into()), None);
		assert_eq!(Sassafras::slot_ticket(genesis_slot + 0), None);
		assert_eq!(Sassafras::slot_ticket(genesis_slot + 1), None);
		assert_eq!(Sassafras::slot_ticket(genesis_slot + 100), None);

		// Initialize genesis slot..
		GenesisSlot::<Test>::set(genesis_slot);
		frame_system::Pallet::<Test>::set_block_number(One::one());

		// Try to fetch a ticket for a slot before current epoch.
		assert_eq!(Sassafras::slot_ticket(0.into()), None);

		// Current epoch tickets.
		assert_eq!(Sassafras::slot_ticket(genesis_slot + 0).unwrap(), curr_tickets[0]);
		assert_eq!(Sassafras::slot_ticket(genesis_slot + 1).unwrap(), curr_tickets[7]);
		assert_eq!(Sassafras::slot_ticket(genesis_slot + 2).unwrap(), curr_tickets[1]);
		assert_eq!(Sassafras::slot_ticket(genesis_slot + 3).unwrap(), curr_tickets[6]);
		assert_eq!(Sassafras::slot_ticket(genesis_slot + 4).unwrap(), curr_tickets[2]);
		assert_eq!(Sassafras::slot_ticket(genesis_slot + 5).unwrap(), curr_tickets[5]);
		assert_eq!(Sassafras::slot_ticket(genesis_slot + 6).unwrap(), curr_tickets[3]);
		assert_eq!(Sassafras::slot_ticket(genesis_slot + 7).unwrap(), curr_tickets[4]);
		assert!(Sassafras::slot_ticket(genesis_slot + 8).is_none());
		assert!(Sassafras::slot_ticket(genesis_slot + 9).is_none());

		// Next epoch tickets.
		assert_eq!(Sassafras::slot_ticket(genesis_slot + 10).unwrap(), next_tickets[0]);
		assert_eq!(Sassafras::slot_ticket(genesis_slot + 11).unwrap(), next_tickets[5]);
		assert_eq!(Sassafras::slot_ticket(genesis_slot + 12).unwrap(), next_tickets[1]);
		assert_eq!(Sassafras::slot_ticket(genesis_slot + 13).unwrap(), next_tickets[4]);
		assert_eq!(Sassafras::slot_ticket(genesis_slot + 14).unwrap(), next_tickets[2]);
		assert_eq!(Sassafras::slot_ticket(genesis_slot + 15).unwrap(), next_tickets[3]);
		assert!(Sassafras::slot_ticket(genesis_slot + 16).is_none());
		assert!(Sassafras::slot_ticket(genesis_slot + 17).is_none());
		assert!(Sassafras::slot_ticket(genesis_slot + 18).is_none());
		assert!(Sassafras::slot_ticket(genesis_slot + 19).is_none());

		// Try to fetch the tickets for slots beyond the next epoch.
		assert!(Sassafras::slot_ticket(genesis_slot + 20).is_none());
		assert!(Sassafras::slot_ticket(genesis_slot + 42).is_none());
	});
}

#[test]
fn on_first_block_after_genesis() {
	let (pairs, mut ext) = new_test_ext_with_pairs(4, false);

	ext.execute_with(|| {
		let start_slot = Slot::from(100);
		let start_block = 1;

		let common_assertions = || {
			assert_eq!(Sassafras::genesis_slot(), start_slot);
			assert_eq!(Sassafras::current_slot(), start_slot);
			assert_eq!(Sassafras::epoch_index(), 0);
			assert_eq!(Sassafras::current_epoch_start(), start_slot);
			assert_eq!(Sassafras::current_slot_index(), 0);
		};

		// Post-initialization status

		assert_eq!(Sassafras::randomness(), RandomnessBuffer::default());

		let digest = initialize_block(start_block, start_slot, Default::default(), &pairs[0]);

		assert!(ClaimTemporaryData::<Test>::exists());
		common_assertions();
		let post_ini_randomness = Sassafras::randomness();
		println!("[DEBUG] {}", b2h(post_ini_randomness[0]));
		assert_eq!(post_ini_randomness[0], post_ini_randomness[1]);
		assert_eq!(
			post_ini_randomness[0],
			h2b("f0d42f6b7c0d157ecbd788be44847b80a96c290c04b5dfa5d1d40c98aa0c04ed")
		);
		assert_eq!(post_ini_randomness[2], post_ini_randomness[3]);
		assert_eq!(post_ini_randomness[2], Randomness::default());

		// Post-finalization status

		let header = finalize_block(start_block);

		assert!(!ClaimTemporaryData::<Test>::exists());
		common_assertions();
		let post_fin_randomness = Sassafras::randomness();
		println!("[DEBUG] {}", b2h(post_fin_randomness[0]));
		assert_ne!(post_fin_randomness[0], post_fin_randomness[1]);
		assert_eq!(
			post_fin_randomness[0],
			h2b("30361b634c74109911e59b5b773cb428ff17e13ff8ab52d4f56636c39575a9d2"),
		);

		// Header data check

		assert_eq!(header.digest.logs.len(), 2);
		assert_eq!(header.digest.logs[0], digest.logs[0]);

		// Genesis epoch start deposits consensus
		let consensus_log = sp_consensus_sassafras::digests::ConsensusLog::NextEpochData(
			sp_consensus_sassafras::digests::NextEpochDescriptor {
				authorities: Sassafras::next_authorities().into_inner(),
				randomness: Sassafras::randomness()[1],
			},
		);
		let consensus_digest = DigestItem::Consensus(SASSAFRAS_ENGINE_ID, consensus_log.encode());
		assert_eq!(header.digest.logs[1], consensus_digest)
	})
}
