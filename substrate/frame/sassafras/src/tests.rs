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
use sp_runtime::DispatchError;

const TICKETS_FILE: &str = "src/data/tickets.bin";

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

// Fisher-Yates shuffle.
//
// We don't want to implement something secure here.
// Just a trivial pseudo-random shuffle for the tests.
fn shuffle<T>(vector: &mut Vec<T>, random_seed: u64) {
	let mut r = random_seed as usize;
	for i in (1..vector.len()).rev() {
		let j = r % (i + 1);
		vector.swap(i, j);
		r = (r.wrapping_mul(6364793005) + 1) as usize;
	}
}

fn dummy_tickets(count: u8) -> Vec<TicketBody> {
	make_ticket_bodies(count, None)
}

#[test]
fn assumptions_check() {
	let mut tickets = dummy_tickets(100);
	shuffle(&mut tickets, 123);

	new_test_ext(3).execute_with(|| {
		assert_eq!(Sassafras::authorities().len(), 3);

		// Check that entries are stored sorted (bigger first)
		tickets
			.iter()
			.for_each(|t| TicketsAccumulator::<Test>::insert(TicketKey::from(t.id), t));
		assert_eq!(TicketsAccumulator::<Test>::count(), 100);
		tickets.sort_unstable_by_key(|t| TicketKey::from(t.id));
		let accumulator: Vec<_> = TicketsAccumulator::<Test>::iter_values().collect();
		assert_eq!(tickets, accumulator);

		// Check accumulator clear
		let _ = TicketsAccumulator::<Test>::clear(u32::MAX, None);
		assert_eq!(TicketsAccumulator::<Test>::count(), 0);
	});
}

#[test]
fn deposit_tickets_works() {
	let mut tickets = dummy_tickets(15);
	shuffle(&mut tickets, 123);

	new_test_ext(1).execute_with(|| {
		// Try to append an unsorted chunk
		let mut candidates = tickets[..5].to_vec();
		let err = Sassafras::deposit_tickets(candidates).unwrap_err();
		assert!(matches!(err, Error::TicketBadOrder));
		let _ = TicketsAccumulator::<Test>::clear(u32::MAX, None);

		// Correctly append the first sorted chunk
		let mut candidates = tickets[..5].to_vec();
		candidates.sort_unstable();
		Sassafras::deposit_tickets(candidates).unwrap();
		assert_eq!(TicketsAccumulator::<Test>::count(), 5);
		// Note: internally the tickets are stored in reverse order (bigger first)
		let stored: Vec<_> = TicketsAccumulator::<Test>::iter_values().collect();
		let mut expected = tickets[..5].to_vec();
		expected.sort_unstable_by_key(|t| TicketKey::from(t.id));
		assert_eq!(expected, stored);

		// Try to append a chunk with a ticket already pushed
		let mut candidates = tickets[4..10].to_vec();
		candidates.sort_unstable();
		let err = Sassafras::deposit_tickets(candidates).unwrap_err();
		assert!(matches!(err, Error::TicketDuplicate));
		// Restore last correct state
		let _ = TicketsAccumulator::<Test>::clear(u32::MAX, None);
		let mut candidates = tickets[..5].to_vec();
		candidates.sort_unstable();
		Sassafras::deposit_tickets(candidates).unwrap();

		// Correctly push the second sorted chunk
		let mut candidates = tickets[5..10].to_vec();
		candidates.sort_unstable();
		Sassafras::deposit_tickets(candidates).unwrap();
		assert_eq!(TicketsAccumulator::<Test>::count(), 10);
		// Note: internally the tickets are stored in reverse order (bigger first)
		let mut stored: Vec<_> = TicketsAccumulator::<Test>::iter_values().collect();
		let mut expected = tickets[..10].to_vec();
		expected.sort_unstable_by_key(|t| TicketKey::from(t.id));
		assert_eq!(expected, stored);

		// Now the buffer is full, pick only the tickets that will eventually fit.
		let mut candidates = tickets[10..].to_vec();
		candidates.sort_unstable();
		let mut eligible = Vec::new();
		for candidate in candidates {
			if stored.is_empty() {
				break
			}
			let bigger = stored.remove(0);
			if bigger.id <= candidate.id {
				break
			}
			eligible.push(candidate);
		}
		candidates = eligible;

		// Correctly push the last candidates chunk
		Sassafras::deposit_tickets(candidates).unwrap();

		assert_eq!(TicketsAccumulator::<Test>::count(), 10);
		// Note: internally the tickets are stored in reverse order (bigger first)
		let mut stored: Vec<_> = TicketsAccumulator::<Test>::iter_values().collect();
		tickets.sort_unstable_by_key(|t| TicketKey::from(t.id));

		assert_eq!(tickets[5..], stored);
	});
}

#[test]
fn post_genesis_randomness_initialization() {
	let (pairs, mut ext) = new_test_ext_with_pairs(1, false);
	let pair = &pairs[0];
	let first_slot = (GENESIS_SLOT + 1).into();

	ext.execute_with(|| {
		let genesis_randomness = Sassafras::randomness_buf();
		assert_eq!(genesis_randomness, RandomnessBuffer::default());

		// Test the values with a zero genesis block hash

		let _ = initialize_block(1, first_slot, [0x00; 32].into(), pair);

		let randomness = Sassafras::randomness_buf();
		prefix_eq!(randomness[0], h2b("89eb0d6a"));
		prefix_eq!(randomness[1], h2b("4e8c71d2"));
		prefix_eq!(randomness[2], h2b("3a4c0005"));
		prefix_eq!(randomness[3], h2b("0dd43c54"));

		let ticket1 = make_ticket_body(0, pair);

		// Reset what is relevant
		RandomnessBuf::<Test>::set(genesis_randomness);

		// Test the values with a non-zero genesis block hash

		let _ = initialize_block(1, first_slot, [0xff; 32].into(), pair);

		let randomness = Sassafras::randomness_buf();
		prefix_eq!(randomness[0], h2b("e2021160"));
		prefix_eq!(randomness[1], h2b("3b0c0905"));
		prefix_eq!(randomness[2], h2b("632ac0d9"));
		prefix_eq!(randomness[3], h2b("575088c3"));

		let ticket2 = make_ticket_body(0, pair);

		// Ticket ids should be different when next epoch randomness is different
		assert_ne!(ticket1.id, ticket2.id);
	});
}

#[test]
fn on_first_block() {
	let (pairs, mut ext) = new_test_ext_with_pairs(4, false);
	let start_slot = (GENESIS_SLOT + 1).into();
	let start_block = 1;

	ext.execute_with(|| {
		let common_assertions = |initialized| {
			assert_eq!(Sassafras::current_slot(), start_slot);
			assert_eq!(Sassafras::current_slot_index(), 1);
			assert_eq!(TemporaryData::<Test>::exists(), initialized);
		};

		// Post-initialization status

		assert_eq!(Sassafras::randomness_buf(), RandomnessBuffer::default());

		let digest = initialize_block(start_block, start_slot, Default::default(), &pairs[0]);

		common_assertions(true);
		let post_init_randomness = Sassafras::randomness_buf();
		prefix_eq!(post_init_randomness[0], h2b("89eb0d6a"));
		prefix_eq!(post_init_randomness[1], h2b("4e8c71d2"));
		prefix_eq!(post_init_randomness[2], h2b("3a4c0005"));
		prefix_eq!(post_init_randomness[3], h2b("0dd43c54"));

		// // Post-finalization status

		let header = finalize_block(start_block);

		common_assertions(false);
		let post_fini_randomness = Sassafras::randomness_buf();
		prefix_eq!(post_fini_randomness[0], h2b("334d1a4c"));
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
	let start_slot = (GENESIS_SLOT + 1).into();
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
			assert_eq!(Sassafras::current_slot_index(), 2);
			assert_eq!(TemporaryData::<Test>::exists(), initialized);
		};

		// Post-initialization status

		common_assertions(true);
		let post_init_randomness = Sassafras::randomness_buf();
		prefix_eq!(post_init_randomness[0], h2b("334d1a4c"));
		prefix_eq!(post_init_randomness[1], h2b("4e8c71d2"));
		prefix_eq!(post_init_randomness[2], h2b("3a4c0005"));
		prefix_eq!(post_init_randomness[3], h2b("0dd43c54"));

		let header = finalize_block(end_block);

		// Post-finalization status

		common_assertions(false);
		let post_fini_randomness = Sassafras::randomness_buf();
		prefix_eq!(post_fini_randomness[0], h2b("277138ab"));
		prefix_eq!(post_fini_randomness[1], post_init_randomness[1]);
		prefix_eq!(post_fini_randomness[2], post_init_randomness[2]);
		prefix_eq!(post_fini_randomness[3], post_init_randomness[3]);

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

		let common_assertions = |initialized| {
			assert_eq!(Sassafras::current_slot(), GENESIS_SLOT + epoch_length);
			assert_eq!(Sassafras::current_slot_index(), 0);
			assert_eq!(TemporaryData::<Test>::exists(), initialized);
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
	let tickets = dummy_tickets(curr_count + next_count);

	// Current epoch tickets
	let curr_tickets = tickets[..curr_count as usize].to_vec();
	let next_tickets = tickets[curr_count as usize..].to_vec();

	new_test_ext(0).execute_with(|| {
		curr_tickets
			.iter()
			.enumerate()
			.for_each(|(i, t)| Tickets::<Test>::insert((0, i as u32), t));

		next_tickets
			.iter()
			.enumerate()
			.for_each(|(i, t)| Tickets::<Test>::insert((1, i as u32), t));

		TicketsCount::<Test>::set([curr_count as u32, next_count as u32]);
		CurrentSlot::<Test>::set(genesis_slot);

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
fn slot_and_epoch_helpers_works() {
	let start_block = 1;
	let start_slot = (GENESIS_SLOT + 1).into();

	let (pairs, mut ext) = new_test_ext_with_pairs(1, false);

	ext.execute_with(|| {
		let epoch_length = Sassafras::epoch_length() as u64;
		assert_eq!(epoch_length, 10);

		let check = |slot, slot_idx, epoch_slot, epoch_idx| {
			assert_eq!(Sassafras::current_slot(), Slot::from(slot));
			assert_eq!(Sassafras::current_slot_index(), slot_idx);
			assert_eq!(Sassafras::current_epoch_start(), Slot::from(epoch_slot));
			assert_eq!(Sassafras::current_epoch_index(), epoch_idx);
		};

		// Post genesis state (before first initialization of epoch N)
		check(0, 0, 0, 0);

		// Epoch N first block
		initialize_block(start_block, start_slot, Default::default(), &pairs[0]);
		check(101, 1, 100, 10);

		// Progress to epoch N last block
		let end_block = start_block + epoch_length - 2;
		progress_to_block(end_block, &pairs[0]).unwrap();
		check(109, 9, 100, 10);

		// Progres to epoch N+1 first block
		progress_to_block(end_block + 1, &pairs[0]).unwrap();
		check(110, 0, 110, 11);

		// Progress to epoch N+1 last block
		let end_block = end_block + epoch_length;
		progress_to_block(end_block, &pairs[0]).unwrap();
		check(119, 9, 110, 11);

		// Progres to epoch N+2 first block
		progress_to_block(end_block + 1, &pairs[0]).unwrap();
		check(120, 0, 120, 12);
	})
}

#[test]
fn tickets_accumulator_works() {
	let start_block = 1;
	let start_slot = (GENESIS_SLOT + 1).into();
	let e1_count = 6;
	let e2_count = 10;
	let tickets = dummy_tickets(e1_count + e2_count);
	let e1_tickets = tickets[..e1_count as usize].to_vec();
	let e2_tickets = tickets[e1_count as usize..].to_vec();

	let (pairs, mut ext) = new_test_ext_with_pairs(1, false);

	ext.execute_with(|| {
		let epoch_length = Sassafras::epoch_length() as u64;

		let epoch_idx = Sassafras::current_epoch_index();
		let epoch_tag = (epoch_idx % 2) as u8;
		let next_epoch_tag = epoch_tag ^ 1;

		initialize_block(start_block, start_slot, Default::default(), &pairs[0]);

		// Append some tickets to the accumulator
		e1_tickets
			.iter()
			.for_each(|t| TicketsAccumulator::<Test>::insert(TicketKey::from(t.id), t));

		// Progress to epoch's last block
		let end_block = start_block + epoch_length - 2;
		progress_to_block(end_block, &pairs[0]).unwrap();

		let tickets_count = TicketsCount::<Test>::get();
		assert_eq!(tickets_count[epoch_tag as usize], 0);
		assert_eq!(tickets_count[next_epoch_tag as usize], 0);

		finalize_block(end_block);

		let tickets_count = TicketsCount::<Test>::get();
		assert_eq!(tickets_count[epoch_tag as usize], 0);
		assert_eq!(tickets_count[next_epoch_tag as usize], e1_count as u32);

		// Start new epoch

		initialize_block(
			end_block + 1,
			Sassafras::current_slot() + 1,
			Default::default(),
			&pairs[0],
		);

		let next_epoch_tag = epoch_tag;
		let epoch_tag = epoch_tag ^ 1;
		let tickets_count = TicketsCount::<Test>::get();
		assert_eq!(tickets_count[epoch_tag as usize], e1_count as u32);
		assert_eq!(tickets_count[next_epoch_tag as usize], 0);

		// Append some tickets to the accumulator
		e2_tickets
			.iter()
			.for_each(|t| TicketsAccumulator::<Test>::insert(TicketKey::from(t.id), t));

		// Progress to epoch's last block
		let end_block = end_block + epoch_length;
		progress_to_block(end_block, &pairs[0]).unwrap();

		let tickets_count = TicketsCount::<Test>::get();
		assert_eq!(tickets_count[epoch_tag as usize], e1_count as u32);
		assert_eq!(tickets_count[next_epoch_tag as usize], 0);

		finalize_block(end_block);

		let tickets_count = TicketsCount::<Test>::get();
		assert_eq!(tickets_count[epoch_tag as usize], e1_count as u32);
		assert_eq!(tickets_count[next_epoch_tag as usize], e2_count as u32);

		// Start new epoch
		initialize_block(
			end_block + 1,
			Sassafras::current_slot() + 1,
			Default::default(),
			&pairs[0],
		);

		let next_epoch_tag = epoch_tag;
		let epoch_tag = epoch_tag ^ 1;
		let tickets_count = TicketsCount::<Test>::get();
		assert_eq!(tickets_count[epoch_tag as usize], e2_count as u32);
		assert_eq!(tickets_count[next_epoch_tag as usize], 0);
	});
}

#[test]
fn incremental_accumulator_drain() {
	let tickets = dummy_tickets(10);

	new_test_ext(0).execute_with(|| {
		tickets
			.iter()
			.for_each(|t| TicketsAccumulator::<Test>::insert(TicketKey::from(t.id), t));

		let accumulator: Vec<_> = TicketsAccumulator::<Test>::iter_values().collect();
		// Assess accumulator expected order (bigger id first)
		assert!(accumulator.windows(2).all(|chunk| chunk[0].id > chunk[1].id));

		let mut onchain_expected = accumulator.clone();
		onchain_expected.sort_unstable();

		Sassafras::consume_tickets_accumulator(5, 0);
		let tickets_count = TicketsCount::<Test>::get();
		assert_eq!(tickets_count[0], 5);
		assert_eq!(tickets_count[1], 0);

		accumulator.iter().rev().enumerate().skip(5).for_each(|(i, t)| {
			let t2 = Tickets::<Test>::get((0, i as u32)).unwrap();
			assert_eq!(t.id, t2.id);
		});

		Sassafras::consume_tickets_accumulator(3, 0);
		let tickets_count = TicketsCount::<Test>::get();
		assert_eq!(tickets_count[0], 8);
		assert_eq!(tickets_count[1], 0);
		accumulator.iter().rev().enumerate().skip(2).for_each(|(i, t)| {
			let t2 = Tickets::<Test>::get((0, i as u32)).unwrap();
			assert_eq!(t.id, t2.id);
		});

		Sassafras::consume_tickets_accumulator(5, 0);
		let tickets_count = TicketsCount::<Test>::get();
		assert_eq!(tickets_count[0], 10);
		assert_eq!(tickets_count[1], 0);
		accumulator.iter().rev().enumerate().for_each(|(i, t)| {
			let t2 = Tickets::<Test>::get((0, i as u32)).unwrap();
			assert_eq!(t.id, t2.id);
		});
	});
}

#[test]
fn submit_tickets_with_ring_proof_check_works() {
	use sp_core::Pair as _;
	let _ = env_logger::try_init();
	let start_block = 1;
	let start_slot = (GENESIS_SLOT + 1).into();

	let (randomness, authorities, mut candidates): (
		Randomness,
		Vec<AuthorityId>,
		Vec<TicketEnvelope>,
	) = data_read(TICKETS_FILE);

	// Also checks that duplicates are discarded

	let (pairs, mut ext) = new_test_ext_with_pairs(authorities.len(), true);
	let pair = &pairs[0];
	// Check if deserialized data has been generated for the correct set of authorities...
	assert!(authorities.iter().zip(pairs.iter()).all(|(auth, pair)| auth == &pair.public()));

	ext.execute_with(|| {
		initialize_block(start_block, start_slot, Default::default(), pair);

		// Use the same values as the pre-built tickets
		Sassafras::update_ring_verifier(&authorities);
		let mut randomness_buf = RandomnessBuf::<Test>::get();
		randomness_buf[2] = randomness;
		RandomnessBuf::<Test>::set(randomness_buf);
		NextAuthorities::<Test>::set(WeakBoundedVec::force_from(authorities, None));

		// Submit the tickets
		let candidates_per_call = 4;
		let mut chunks: Vec<_> = candidates
			.chunks(candidates_per_call)
			.map(|chunk| BoundedVec::truncate_from(chunk.to_vec()))
			.collect();
		assert_eq!(chunks.len(), 5);

		// Submit an invalid candidate
		let mut chunk = chunks[2].clone();
		chunk[0].attempt += 1;
		let e = Sassafras::submit_tickets(RuntimeOrigin::none(), chunk).unwrap_err();
		assert_eq!(e, DispatchError::from(Error::<Test>::TicketBadProof));
		assert_eq!(TicketsAccumulator::<Test>::count(), 0);

		// Start submitting from the mid valued chunks.
		Sassafras::submit_tickets(RuntimeOrigin::none(), chunks[2].clone()).unwrap();
		assert_eq!(TicketsAccumulator::<Test>::count(), 4);

		// Submit something bigger, but we have space for all the candidates.
		Sassafras::submit_tickets(RuntimeOrigin::none(), chunks[3].clone()).unwrap();
		assert_eq!(TicketsAccumulator::<Test>::count(), 8);

		// Try to submit duplicates
		let e = Sassafras::submit_tickets(RuntimeOrigin::none(), chunks[2].clone()).unwrap_err();
		assert_eq!(e, DispatchError::from(Error::<Test>::TicketDuplicate));
		assert_eq!(TicketsAccumulator::<Test>::count(), 8);

		// Submit something smaller. This is accepted (2 old tickets removed).
		Sassafras::submit_tickets(RuntimeOrigin::none(), chunks[1].clone()).unwrap();
		assert_eq!(TicketsAccumulator::<Test>::count(), 10);

		// Try to submit a chunk with bigger tickets. This is discarded
		let e = Sassafras::submit_tickets(RuntimeOrigin::none(), chunks[4].clone()).unwrap_err();
		assert_eq!(e, DispatchError::from(Error::<Test>::TicketInvalid));
		assert_eq!(TicketsAccumulator::<Test>::count(), 10);

		// Submit the smaller candidates chunks. This is accepted (4 old tickets removed).
		Sassafras::submit_tickets(RuntimeOrigin::none(), chunks[0].clone()).unwrap();
		assert_eq!(TicketsAccumulator::<Test>::count(), 10);
	})
}

fn data_read<T: Decode>(filename: &str) -> T {
	use std::{fs::File, io::Read};
	let mut file = File::open(filename).unwrap();
	let mut buf = Vec::new();
	file.read_to_end(&mut buf).unwrap();
	T::decode(&mut &buf[..]).unwrap()
}

fn data_write<T: Encode>(filename: &str, data: T) {
	use std::{fs::File, io::Write};
	let mut file = File::create(filename).unwrap();
	let buf = data.encode();
	file.write_all(&buf).unwrap();
}

#[test]
#[ignore = "test tickets generator"]
fn generate_test_tickets() {
	use super::*;
	use sp_core::crypto::Pair;

	let start_block = 1;
	let start_slot = (GENESIS_SLOT + 1).into();

	// Total number of authorities (the ring)
	let authorities_count = 10;
	let (pairs, mut ext) = new_test_ext_with_pairs(authorities_count, true);

	let authorities: Vec<_> = pairs.iter().map(|sk| sk.public()).collect();

	let mut tickets = Vec::new();
	ext.execute_with(|| {
		let config = Sassafras::protocol_config();
		assert!(authorities_count < config.max_authorities as usize);

		let tickets_count = authorities_count * config.attempts_number as usize;

		initialize_block(start_block, start_slot, Default::default(), &pairs[0]);

		println!("Constructing {} tickets", tickets_count);

		pairs.iter().take(authorities_count).enumerate().for_each(|(i, pair)| {
			let t = make_tickets(config.attempts_number, pair);
			tickets.extend(t);
			println!("{:.2}%", 100f32 * ((i + 1) as f32 / authorities_count as f32));
		});

		tickets.sort_unstable_by_key(|t| t.0);
		let envelopes: Vec<_> = tickets.into_iter().map(|t| t.1).collect();

		// Tickets were generated using `next_randomness`
		let randomness = Sassafras::next_randomness();

		data_write(TICKETS_FILE, (randomness, authorities, envelopes));
	});
}
