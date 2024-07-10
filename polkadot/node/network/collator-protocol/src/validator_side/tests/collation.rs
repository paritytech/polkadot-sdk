// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use polkadot_primitives::{CandidateHash, CollatorId, Hash, Id as ParaId};

use sc_network::PeerId;
use sp_core::sr25519;

use crate::validator_side::tests::CollationStatus;

use super::{Collations, PendingCollation, ProspectiveCandidate};

#[test]
fn cant_add_more_than_claim_queue() {
	sp_tracing::init_for_tests();

	let para_a = ParaId::from(1);
	let para_b = ParaId::from(2);
	let assignments = vec![para_a, para_b, para_a];
	let max_candidate_depth = 4;
	let claim_queue_support = true;

	let mut collations = Collations::new(&assignments, claim_queue_support);

	// first collation for `para_a` is in the limit
	assert!(!collations.is_seconded_limit_reached(max_candidate_depth, para_a));
	collations.note_seconded(para_a);
	// and `para_b` is not affected
	assert!(!collations.is_seconded_limit_reached(max_candidate_depth, para_b));

	// second collation for `para_a` is also in the limit
	assert!(!collations.is_seconded_limit_reached(max_candidate_depth, para_a));
	collations.note_seconded(para_a);

	// `para_b`` is still not affected
	assert!(!collations.is_seconded_limit_reached(max_candidate_depth, para_b));

	// third collation for `para_a`` will be above the limit
	assert!(collations.is_seconded_limit_reached(max_candidate_depth, para_a));

	// one fetch for b
	assert!(!collations.is_seconded_limit_reached(max_candidate_depth, para_b));
	collations.note_seconded(para_b);

	// and now both paras are over limit
	assert!(collations.is_seconded_limit_reached(max_candidate_depth, para_a));
	assert!(collations.is_seconded_limit_reached(max_candidate_depth, para_b));
}

#[test]
fn pending_fetches_are_counted() {
	sp_tracing::init_for_tests();

	let para_a = ParaId::from(1);
	let para_b = ParaId::from(2);
	let assignments = vec![para_a, para_b, para_a];
	let max_candidate_depth = 4;
	let claim_queue_support = true;

	let mut collations = Collations::new(&assignments, claim_queue_support);
	collations.status = CollationStatus::Fetching(para_a); //para_a is pending

	// first collation for `para_a` is in the limit
	assert!(!collations.is_seconded_limit_reached(max_candidate_depth, para_a));
	collations.note_seconded(para_a);

	// second collation for `para_a` is not in the limit due to the pending fetch
	assert!(collations.is_seconded_limit_reached(max_candidate_depth, para_a));

	// a collation for `para_b` is accepted since the pending fetch is for `para_a`
	assert!(!collations.is_seconded_limit_reached(max_candidate_depth, para_b));
}

#[test]
fn collation_fetching_respects_claim_queue() {
	sp_tracing::init_for_tests();

	let para_a = ParaId::from(1);
	let collator_id_a = CollatorId::from(sr25519::Public::from_raw([10u8; 32]));
	let peer_a = PeerId::random();

	let para_b = ParaId::from(2);
	let collator_id_b = CollatorId::from(sr25519::Public::from_raw([20u8; 32]));
	let peer_b = PeerId::random();

	let claim_queue = vec![para_a, para_b, para_a];
	let claim_queue_support = true;

	let mut collations = Collations::new(&claim_queue, claim_queue_support);

	collations.fetching_from = None;
	collations.status = CollationStatus::Waiting; //nothing pending

	let relay_parent = Hash::repeat_byte(0x01);

	let collation_a1 = (
		PendingCollation::new(
			relay_parent,
			para_a,
			&peer_a,
			Some(ProspectiveCandidate {
				candidate_hash: CandidateHash(Hash::repeat_byte(1)),
				parent_head_data_hash: Hash::repeat_byte(1),
			}),
		),
		collator_id_a.clone(),
	);

	let collation_a2 = (
		PendingCollation::new(
			relay_parent,
			para_a,
			&peer_a,
			Some(ProspectiveCandidate {
				candidate_hash: CandidateHash(Hash::repeat_byte(2)),
				parent_head_data_hash: Hash::repeat_byte(2),
			}),
		),
		collator_id_a.clone(),
	);

	let collation_b1 = (
		PendingCollation::new(
			relay_parent,
			para_b,
			&peer_b,
			Some(ProspectiveCandidate {
				candidate_hash: CandidateHash(Hash::repeat_byte(3)),
				parent_head_data_hash: Hash::repeat_byte(3),
			}),
		),
		collator_id_b.clone(),
	);

	collations.add_to_waiting_queue(collation_b1.clone());
	collations.add_to_waiting_queue(collation_a1.clone());
	collations.add_to_waiting_queue(collation_a2.clone());

	assert_eq!(
		Some(collation_a1.clone()),
		collations.get_next_collation_to_fetch(
			// doesn't matter since `fetching_from` is `None`
			&(collator_id_a.clone(), Some(CandidateHash(Hash::repeat_byte(0)))),
			&claim_queue,
		)
	);
	collations.note_seconded(collation_a1.0.para_id);

	assert_eq!(
		Some(collation_b1.clone()),
		collations.get_next_collation_to_fetch(
			// doesn't matter since `fetching_from` is `None`
			&(collator_id_a.clone(), Some(CandidateHash(Hash::repeat_byte(0)))),
			&claim_queue,
		)
	);
	collations.note_seconded(collation_b1.0.para_id);

	assert_eq!(
		Some(collation_a2.clone()),
		collations.get_next_collation_to_fetch(
			// doesn't matter since `fetching_from` is `None`
			&(collator_id_a.clone(), Some(CandidateHash(Hash::repeat_byte(0)))),
			&claim_queue,
		)
	);
	collations.note_seconded(collation_a2.0.para_id);
}

#[test]
fn collation_fetching_fallback_works() {
	sp_tracing::init_for_tests();

	let para_a = ParaId::from(1);
	let collator_id_a = CollatorId::from(sr25519::Public::from_raw([10u8; 32]));
	let peer_a = PeerId::random();

	let claim_queue = vec![para_a];
	let claim_queue_support = false;

	let mut collations = Collations::new(&claim_queue, claim_queue_support);

	collations.fetching_from = None;
	collations.status = CollationStatus::Waiting; //nothing pending

	let relay_parent = Hash::repeat_byte(0x01);

	let collation_a1 = (
		PendingCollation::new(
			relay_parent,
			para_a,
			&peer_a,
			Some(ProspectiveCandidate {
				candidate_hash: CandidateHash(Hash::repeat_byte(1)),
				parent_head_data_hash: Hash::repeat_byte(1),
			}),
		),
		collator_id_a.clone(),
	);

	let collation_a2 = (
		PendingCollation::new(
			relay_parent,
			para_a,
			&peer_a,
			Some(ProspectiveCandidate {
				candidate_hash: CandidateHash(Hash::repeat_byte(2)),
				parent_head_data_hash: Hash::repeat_byte(2),
			}),
		),
		collator_id_a.clone(),
	);

	// Collations will be fetched in the order they were added
	collations.add_to_waiting_queue(collation_a1.clone());
	collations.add_to_waiting_queue(collation_a2.clone());

	assert_eq!(
		Some(collation_a1.clone()),
		collations.get_next_collation_to_fetch(
			// doesn't matter since `fetching_from` is `None`
			&(collator_id_a.clone(), Some(CandidateHash(Hash::repeat_byte(0)))),
			&claim_queue,
		)
	);
	collations.note_seconded(collation_a1.0.para_id);

	assert_eq!(
		Some(collation_a2.clone()),
		collations.get_next_collation_to_fetch(
			// doesn't matter since `fetching_from` is `None`
			&(collator_id_a.clone(), Some(CandidateHash(Hash::repeat_byte(0)))),
			&claim_queue,
		)
	);
	collations.note_seconded(collation_a2.0.para_id);
}

#[test]
fn collation_fetching_prefer_entries_earlier_in_claim_queue() {
	sp_tracing::init_for_tests();

	let para_a = ParaId::from(1);
	let collator_id_a = CollatorId::from(sr25519::Public::from_raw([10u8; 32]));
	let peer_a = PeerId::random();

	let para_b = ParaId::from(2);
	let collator_id_b = CollatorId::from(sr25519::Public::from_raw([20u8; 32]));
	let peer_b = PeerId::random();

	let para_c = ParaId::from(3);
	let collator_id_c = CollatorId::from(sr25519::Public::from_raw([30u8; 32]));
	let peer_c = PeerId::random();

	let claim_queue = vec![para_a, para_b, para_a, para_b, para_c, para_c];
	let claim_queue_support = true;

	let mut collations = Collations::new(&claim_queue, claim_queue_support);
	collations.fetching_from = None;
	collations.status = CollationStatus::Waiting; //nothing pending

	let relay_parent = Hash::repeat_byte(0x01);

	let collation_a1 = (
		PendingCollation::new(
			relay_parent,
			para_a,
			&peer_a,
			Some(ProspectiveCandidate {
				candidate_hash: CandidateHash(Hash::repeat_byte(1)),
				parent_head_data_hash: Hash::repeat_byte(1),
			}),
		),
		collator_id_a.clone(),
	);

	let collation_a2 = (
		PendingCollation::new(
			relay_parent,
			para_a,
			&peer_a,
			Some(ProspectiveCandidate {
				candidate_hash: CandidateHash(Hash::repeat_byte(2)),
				parent_head_data_hash: Hash::repeat_byte(2),
			}),
		),
		collator_id_a.clone(),
	);

	let collation_b1 = (
		PendingCollation::new(
			relay_parent,
			para_b,
			&peer_b,
			Some(ProspectiveCandidate {
				candidate_hash: CandidateHash(Hash::repeat_byte(3)),
				parent_head_data_hash: Hash::repeat_byte(3),
			}),
		),
		collator_id_b.clone(),
	);

	let collation_b2 = (
		PendingCollation::new(
			relay_parent,
			para_b,
			&peer_b,
			Some(ProspectiveCandidate {
				candidate_hash: CandidateHash(Hash::repeat_byte(4)),
				parent_head_data_hash: Hash::repeat_byte(4),
			}),
		),
		collator_id_b.clone(),
	);

	let collation_c1 = (
		PendingCollation::new(
			relay_parent,
			para_c,
			&peer_c,
			Some(ProspectiveCandidate {
				candidate_hash: CandidateHash(Hash::repeat_byte(5)),
				parent_head_data_hash: Hash::repeat_byte(5),
			}),
		),
		collator_id_c.clone(),
	);

	let collation_c2 = (
		PendingCollation::new(
			relay_parent,
			para_c,
			&peer_c,
			Some(ProspectiveCandidate {
				candidate_hash: CandidateHash(Hash::repeat_byte(6)),
				parent_head_data_hash: Hash::repeat_byte(6),
			}),
		),
		collator_id_c.clone(),
	);

	// Despite the order here the fetches should follow the claim queue
	collations.add_to_waiting_queue(collation_c1.clone());
	collations.add_to_waiting_queue(collation_c2.clone());
	collations.add_to_waiting_queue(collation_b1.clone());
	collations.add_to_waiting_queue(collation_b2.clone());
	collations.add_to_waiting_queue(collation_a1.clone());
	collations.add_to_waiting_queue(collation_a2.clone());

	assert_eq!(
		Some(collation_a1.clone()),
		collations.get_next_collation_to_fetch(
			// doesn't matter since `fetching_from` is `None`
			&(collator_id_a.clone(), Some(CandidateHash(Hash::repeat_byte(0)))),
			&claim_queue,
		)
	);
	collations.note_seconded(collation_a1.0.para_id);

	assert_eq!(
		Some(collation_b1.clone()),
		collations.get_next_collation_to_fetch(
			// doesn't matter since `fetching_from` is `None`
			&(collator_id_a.clone(), Some(CandidateHash(Hash::repeat_byte(0)))),
			&claim_queue,
		)
	);
	collations.note_seconded(collation_b1.0.para_id);

	assert_eq!(
		Some(collation_a2.clone()),
		collations.get_next_collation_to_fetch(
			// doesn't matter since `fetching_from` is `None`
			&(collator_id_a.clone(), Some(CandidateHash(Hash::repeat_byte(0)))),
			&claim_queue,
		)
	);
	collations.note_seconded(collation_a2.0.para_id);

	assert_eq!(
		Some(collation_b2.clone()),
		collations.get_next_collation_to_fetch(
			// doesn't matter since `fetching_from` is `None`
			&(collator_id_a.clone(), Some(CandidateHash(Hash::repeat_byte(0)))),
			&claim_queue,
		)
	);
	collations.note_seconded(collation_b2.0.para_id);

	assert_eq!(
		Some(collation_c1.clone()),
		collations.get_next_collation_to_fetch(
			// doesn't matter since `fetching_from` is `None`
			&(collator_id_a.clone(), Some(CandidateHash(Hash::repeat_byte(0)))),
			&claim_queue,
		)
	);
	collations.note_seconded(collation_c1.0.para_id);

	assert_eq!(
		Some(collation_c2.clone()),
		collations.get_next_collation_to_fetch(
			// doesn't matter since `fetching_from` is `None`
			&(collator_id_a.clone(), Some(CandidateHash(Hash::repeat_byte(0)))),
			&claim_queue,
		)
	);
	collations.note_seconded(collation_c2.0.para_id);
}

#[test]
fn collation_fetching_fills_holes_in_claim_queue() {
	sp_tracing::init_for_tests();

	let para_a = ParaId::from(1);
	let collator_id_a = CollatorId::from(sr25519::Public::from_raw([10u8; 32]));
	let peer_a = PeerId::random();

	let para_b = ParaId::from(2);
	let collator_id_b = CollatorId::from(sr25519::Public::from_raw([20u8; 32]));
	let peer_b = PeerId::random();

	let para_c = ParaId::from(3);
	let collator_id_c = CollatorId::from(sr25519::Public::from_raw([30u8; 32]));
	let peer_c = PeerId::random();

	let claim_queue = vec![para_a, para_b, para_a, para_b, para_c, para_c];
	let claim_queue_support = true;

	let mut collations = Collations::new(&claim_queue, claim_queue_support);
	collations.fetching_from = None;
	collations.status = CollationStatus::Waiting; //nothing pending

	let relay_parent = Hash::repeat_byte(0x01);

	let collation_a1 = (
		PendingCollation::new(
			relay_parent,
			para_a,
			&peer_a,
			Some(ProspectiveCandidate {
				candidate_hash: CandidateHash(Hash::repeat_byte(1)),
				parent_head_data_hash: Hash::repeat_byte(1),
			}),
		),
		collator_id_a.clone(),
	);

	let collation_a2 = (
		PendingCollation::new(
			relay_parent,
			para_a,
			&peer_a,
			Some(ProspectiveCandidate {
				candidate_hash: CandidateHash(Hash::repeat_byte(2)),
				parent_head_data_hash: Hash::repeat_byte(2),
			}),
		),
		collator_id_a.clone(),
	);

	let collation_b1 = (
		PendingCollation::new(
			relay_parent,
			para_b,
			&peer_b,
			Some(ProspectiveCandidate {
				candidate_hash: CandidateHash(Hash::repeat_byte(3)),
				parent_head_data_hash: Hash::repeat_byte(3),
			}),
		),
		collator_id_b.clone(),
	);

	let collation_b2 = (
		PendingCollation::new(
			relay_parent,
			para_b,
			&peer_b,
			Some(ProspectiveCandidate {
				candidate_hash: CandidateHash(Hash::repeat_byte(4)),
				parent_head_data_hash: Hash::repeat_byte(4),
			}),
		),
		collator_id_b.clone(),
	);

	let collation_c1 = (
		PendingCollation::new(
			relay_parent,
			para_c,
			&peer_c,
			Some(ProspectiveCandidate {
				candidate_hash: CandidateHash(Hash::repeat_byte(5)),
				parent_head_data_hash: Hash::repeat_byte(5),
			}),
		),
		collator_id_c.clone(),
	);

	let collation_c2 = (
		PendingCollation::new(
			relay_parent,
			para_c,
			&peer_c,
			Some(ProspectiveCandidate {
				candidate_hash: CandidateHash(Hash::repeat_byte(6)),
				parent_head_data_hash: Hash::repeat_byte(6),
			}),
		),
		collator_id_c.clone(),
	);

	collations.add_to_waiting_queue(collation_c1.clone());
	collations.add_to_waiting_queue(collation_a1.clone());

	assert_eq!(
		Some(collation_a1.clone()),
		collations.get_next_collation_to_fetch(
			// doesn't matter since `fetching_from` is `None`
			&(collator_id_a.clone(), Some(CandidateHash(Hash::repeat_byte(0)))),
			&claim_queue,
		)
	);
	collations.note_seconded(collation_a1.0.para_id);

	// fetch c1 since there is nothing better to fetch
	assert_eq!(
		Some(collation_c1.clone()),
		collations.get_next_collation_to_fetch(
			// doesn't matter since `fetching_from` is `None`
			&(collator_id_a.clone(), Some(CandidateHash(Hash::repeat_byte(0)))),
			&claim_queue,
		)
	);
	collations.note_seconded(collation_c1.0.para_id);

	// b1 should be prioritized since there is a hole in the claim queue
	collations.add_to_waiting_queue(collation_c2.clone());
	collations.add_to_waiting_queue(collation_b1.clone());

	assert_eq!(
		Some(collation_b1.clone()),
		collations.get_next_collation_to_fetch(
			// doesn't matter since `fetching_from` is `None`
			&(collator_id_a.clone(), Some(CandidateHash(Hash::repeat_byte(0)))),
			&claim_queue,
		)
	);
	collations.note_seconded(collation_b1.0.para_id);

	assert_eq!(
		Some(collation_c2.clone()),
		collations.get_next_collation_to_fetch(
			// doesn't matter since `fetching_from` is `None`
			&(collator_id_a.clone(), Some(CandidateHash(Hash::repeat_byte(0)))),
			&claim_queue,
		)
	);
	collations.note_seconded(collation_c2.0.para_id);

	// same with a2
	collations.add_to_waiting_queue(collation_b2.clone());
	collations.add_to_waiting_queue(collation_a2.clone());

	assert_eq!(
		Some(collation_a2.clone()),
		collations.get_next_collation_to_fetch(
			// doesn't matter since `fetching_from` is `None`
			&(collator_id_a.clone(), Some(CandidateHash(Hash::repeat_byte(0)))),
			&claim_queue,
		)
	);
	collations.note_seconded(collation_a2.0.para_id);

	assert_eq!(
		Some(collation_b2.clone()),
		collations.get_next_collation_to_fetch(
			// doesn't matter since `fetching_from` is `None`
			&(collator_id_a.clone(), Some(CandidateHash(Hash::repeat_byte(0)))),
			&claim_queue,
		)
	);
	collations.note_seconded(collation_b2.0.para_id);
}

#[test]
fn collations_fetching_respects_seconded_limit() {
	sp_tracing::init_for_tests();

	let para_a = ParaId::from(1);
	let collator_id_a = CollatorId::from(sr25519::Public::from_raw([10u8; 32]));

	let para_b = ParaId::from(2);

	let claim_queue = vec![para_a, para_b, para_a];
	let claim_queue_support = true;

	let mut collations = Collations::new(&claim_queue, claim_queue_support);
	collations.fetching_from = None;
	collations.status = CollationStatus::Fetching(para_a); //para_a is pending

	collations.note_seconded(para_a);
	collations.note_seconded(para_a);

	assert_eq!(
		None,
		collations.get_next_collation_to_fetch(
			// doesn't matter since `fetching_from` is `None`
			&(collator_id_a.clone(), Some(CandidateHash(Hash::repeat_byte(0)))),
			&claim_queue,
		)
	);
}
