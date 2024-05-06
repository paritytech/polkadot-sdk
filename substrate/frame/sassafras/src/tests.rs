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

const GENESIS_SLOT: u64 = 100;

fn h2b(hex: &str) -> Vec<u8> {
	array_bytes::hex2bytes_unchecked(hex)
}

fn b2h(bytes: &[u8]) -> String {
	array_bytes::bytes2hex("", bytes)
}

macro_rules! prefix_eq {
	($a:expr, $b:expr) => {{
		let len = $a.len().min($b.len());
		if &$a[..len] != &$b[..len] {
			panic!("left: {}, right: {}", b2h(&$a[..len]), b2h(&$b[..len]));
		}
	}};
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
	let first_slot = GENESIS_SLOT.into();

	ext.execute_with(|| {
		let genesis_randomness = Sassafras::randomness_buf();
		assert_eq!(genesis_randomness, RandomnessBuffer::default());

		// Test the values with a zero genesis block hash

		let _ = initialize_block(1, first_slot, [0x00; 32].into(), pair);

		let randomness = Sassafras::randomness_buf();
		prefix_eq!(randomness[0], h2b("f0d42f6b"));
		prefix_eq!(randomness[1], h2b("28702cc1"));
		prefix_eq!(randomness[2], h2b("a2bd8b31"));
		prefix_eq!(randomness[3], h2b("76d83666"));

		let (id1, _) = make_ticket_body(0, pair);

		// Reset what is relevant
		RandomnessBuf::<Test>::set(genesis_randomness);

		// Test the values with a non-zero genesis block hash

		let _ = initialize_block(1, first_slot, [0xff; 32].into(), pair);

		let randomness = Sassafras::randomness_buf();
		prefix_eq!(randomness[0], h2b("548534cf"));
		prefix_eq!(randomness[1], h2b("5b9cb838"));
		prefix_eq!(randomness[2], h2b("192a2a4b"));
		prefix_eq!(randomness[3], h2b("2e152bf9"));

		let (id2, _) = make_ticket_body(0, pair);

		// Ticket ids should be different when next epoch randomness is different
		assert_ne!(id1, id2);
	});
}

#[test]
fn on_first_block() {
	let (pairs, mut ext) = new_test_ext_with_pairs(4, false);
	let start_slot = GENESIS_SLOT.into();
	let start_block = 1;

	ext.execute_with(|| {
		let common_assertions = |initialized| {
			assert_eq!(Sassafras::current_slot(), start_slot);
			assert_eq!(Sassafras::current_slot_index(), 0);
			assert_eq!(PostInitCache::<Test>::exists(), initialized);
		};

		// Post-initialization status

		assert_eq!(Sassafras::randomness_buf(), RandomnessBuffer::default());

		let digest = initialize_block(start_block, start_slot, Default::default(), &pairs[0]);

		common_assertions(true);
		let post_init_randomness = Sassafras::randomness_buf();
		prefix_eq!(post_init_randomness[0], h2b("f0d42f6b"));
		prefix_eq!(post_init_randomness[1], h2b("28702cc1"));
		prefix_eq!(post_init_randomness[2], h2b("a2bd8b31"));
		prefix_eq!(post_init_randomness[3], h2b("76d83666"));

		// // Post-finalization status

		let header = finalize_block(start_block);

		common_assertions(false);
		let post_fini_randomness = Sassafras::randomness_buf();
		prefix_eq!(post_fini_randomness[0], h2b("6b117a72"));
		prefix_eq!(post_fini_randomness[1], post_init_randomness[1]);
		prefix_eq!(post_fini_randomness[2], post_init_randomness[2]);
		prefix_eq!(post_fini_randomness[3], post_init_randomness[3]);

		// Header data check

		assert_eq!(header.digest.logs.len(), 2);
		assert_eq!(header.digest.logs[0], digest.logs[0]);

		// Genesis epoch start deposits consensus
		let consensus_log = sp_consensus_sassafras::digests::ConsensusLog::NextEpochData(
			sp_consensus_sassafras::digests::NextEpochDescriptor {
				randomness: Sassafras::next_randomness(),
				authorities: Sassafras::next_authorities().into_inner(),
			},
		);
		let consensus_digest = DigestItem::Consensus(SASSAFRAS_ENGINE_ID, consensus_log.encode());
		assert_eq!(header.digest.logs[1], consensus_digest)
	})
}

#[test]
fn on_normal_block() {
	let (pairs, mut ext) = new_test_ext_with_pairs(4, false);
	let start_slot = GENESIS_SLOT.into();
	let start_block = 1;
	let end_block = start_block + 1;

	ext.execute_with(|| {
		initialize_block(start_block, start_slot, Default::default(), &pairs[0]);

		// We don't want to trigger an epoch change in this test.
		let epoch_length = Sassafras::epoch_length() as u64;
		assert!(epoch_length > end_block);

		// Progress to block 2
		let digest = progress_to_block(end_block, &pairs[0]).unwrap();

		let common_assertions = |initialized| {
			assert_eq!(Sassafras::current_slot(), start_slot + 1);
			assert_eq!(Sassafras::current_slot_index(), 1);
			assert_eq!(PostInitCache::<Test>::exists(), initialized);
		};

		// Post-initialization status

		common_assertions(true);
		let post_init_randomness = Sassafras::randomness_buf();
		prefix_eq!(post_init_randomness[0], h2b("6b117a72"));
		prefix_eq!(post_init_randomness[1], h2b("28702cc1"));
		prefix_eq!(post_init_randomness[2], h2b("a2bd8b31"));
		prefix_eq!(post_init_randomness[3], h2b("76d83666"));

		let header = finalize_block(end_block);

		// Post-finalization status

		common_assertions(false);
		let post_fini_randomness = Sassafras::randomness_buf();
		prefix_eq!(post_fini_randomness[0], h2b("3489b933"));
		prefix_eq!(post_fini_randomness[1], h2b("28702cc1"));
		prefix_eq!(post_fini_randomness[2], h2b("a2bd8b31"));
		prefix_eq!(post_fini_randomness[3], h2b("76d83666"));

		// Header data check

		assert_eq!(header.digest.logs.len(), 1);
		assert_eq!(header.digest.logs[0], digest.logs[0]);
	});
}

#[test]
fn produce_epoch_change_digest() {
	let (pairs, mut ext) = new_test_ext_with_pairs(4, false);
	let start_slot = (GENESIS_SLOT + 1).into();
	let start_block = 1;

	ext.execute_with(|| {
		initialize_block(start_block, start_slot, Default::default(), &pairs[0]);

		// We want to trigger an epoch change in this test.
		let epoch_length = Sassafras::epoch_length() as u64;
		let end_block = start_block + epoch_length - 1;
		println!("END BLOCK: {}", end_block);

		let common_assertions = |initialized| {
			assert_eq!(Sassafras::current_slot(), GENESIS_SLOT + epoch_length);
			assert_eq!(Sassafras::current_slot_index(), 0);
			assert_eq!(PostInitCache::<Test>::exists(), initialized);
		};

		let digest = progress_to_block(end_block, &pairs[0]).unwrap();

		// Post-initialization status

		common_assertions(true);

		let header = finalize_block(end_block);

		// Post-finalization status

		common_assertions(false);

		// Header data check

		assert_eq!(header.digest.logs.len(), 2);
		assert_eq!(header.digest.logs[0], digest.logs[0]);
		// Deposits consensus log on epoch change
		let consensus_log = sp_consensus_sassafras::digests::ConsensusLog::NextEpochData(
			sp_consensus_sassafras::digests::NextEpochDescriptor {
				authorities: Sassafras::next_authorities().into_inner(),
				randomness: Sassafras::next_randomness(),
			},
		);
		let consensus_digest = DigestItem::Consensus(SASSAFRAS_ENGINE_ID, consensus_log.encode());
		assert_eq!(header.digest.logs[1], consensus_digest)
	})
}

// Tests if the sorted tickets are assigned to each slot outside-in.
#[test]
fn slot_ticket_id_outside_in_fetch() {
	let genesis_slot = Slot::from(GENESIS_SLOT);
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
		EpochIndex::<Test>::set(*genesis_slot / Sassafras::epoch_length() as u64);

		// Before importing the first block the pallet always return `None`
		// This is a kind of special hardcoded case that should never happen in practice
		// as the first thing the pallet does is to initialize the genesis slot.

		assert_eq!(Sassafras::slot_ticket(0.into()), None);
		assert_eq!(Sassafras::slot_ticket(genesis_slot + 0), None);
		assert_eq!(Sassafras::slot_ticket(genesis_slot + 1), None);
		assert_eq!(Sassafras::slot_ticket(genesis_slot + 100), None);

		// Reset block number..
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
fn tickets_accumulator_consume() {
	let curr_count = 8;
	let next_count = 6;
	let accu_count = 10;

	let tickets_count = curr_count + next_count + accu_count;
	let start_block = 1;
	let start_slot = (GENESIS_SLOT + 1).into();

	let tickets: Vec<_> = (0..tickets_count)
		.map(|i| {
			(TicketId([i as u8; 32]), TicketBody { attempt_idx: 0, extra: Default::default() })
		})
		.collect();
	// Current epoch tickets
	let curr_tickets = tickets[..curr_count].to_vec();
	let next_tickets = tickets[curr_count..curr_count + next_count].to_vec();
	let accu_tickets = tickets[curr_count + next_count..].to_vec();

	let (pairs, mut ext) = new_test_ext_with_pairs(4, false);

	ext.execute_with(|| {
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

		initialize_block(start_block, start_slot, Default::default(), &pairs[0]);

		accu_tickets
			.iter()
			.for_each(|t| TicketsAccumulator::<Test>::insert(TicketKey::from(t.0), &t.1));

		// We don't want to trigger an epoch change in this test.
		let epoch_length = Sassafras::epoch_length();

		// Progress to block 2
		let end_block =
			start_block + (epoch_length - epoch_length / EPOCH_TAIL_FRACTION) as u64 - 1;
		println!("ST: {}, EN: {}, L: {}", start_block, end_block, epoch_length);
		let digest = progress_to_block(end_block, &pairs[0]).unwrap();

		let header = finalize_block(end_block);
	});
}
