use polkadot_node_subsystem_util::runtime::ProspectiveParachainsMode;
use polkadot_primitives::{CandidateHash, CollatorId, Hash, Id as ParaId};

use sc_network::PeerId;
use sp_core::sr25519;

use super::{Collations, PendingCollation, ProspectiveCandidate};

#[test]
fn cant_add_more_than_claim_queue() {
	sp_tracing::init_for_tests();

	let para_a = ParaId::from(1);
	let para_b = ParaId::from(2);
	let assignments = vec![para_a, para_b, para_a];
	let relay_parent_mode =
		ProspectiveParachainsMode::Enabled { max_candidate_depth: 4, allowed_ancestry_len: 3 };
	let claim_queue_support = true;

	let mut collations = Collations::new(&assignments, claim_queue_support);

	// first collation for `para_a` is in the limit
	assert!(!collations.is_collations_limit_reached(relay_parent_mode, para_a, 0,));
	collations.note_fetched(para_a);
	// and `para_b` is not affected
	assert!(!collations.is_collations_limit_reached(relay_parent_mode, para_b, 0));

	// second collation for `para_a` is also in the limit
	assert!(!collations.is_collations_limit_reached(relay_parent_mode, para_a, 0));
	collations.note_fetched(para_a);

	// `para_b`` is still not affected
	assert!(!collations.is_collations_limit_reached(relay_parent_mode, para_b, 0));

	// third collation for `para_a`` will be above the limit
	assert!(collations.is_collations_limit_reached(relay_parent_mode, para_a, 0));

	// one fetch for b
	assert!(!collations.is_collations_limit_reached(relay_parent_mode, para_b, 0));
	collations.note_fetched(para_b);

	// and now both paras are over limit
	assert!(collations.is_collations_limit_reached(relay_parent_mode, para_a, 0));
	assert!(collations.is_collations_limit_reached(relay_parent_mode, para_b, 0));
}

#[test]
fn pending_fetches_are_counted() {
	sp_tracing::init_for_tests();

	let para_a = ParaId::from(1);
	let collator_id_a = CollatorId::from(sr25519::Public::from_raw([10u8; 32]));
	let para_b = ParaId::from(2);
	let assignments = vec![para_a, para_b, para_a];
	let relay_parent_mode =
		ProspectiveParachainsMode::Enabled { max_candidate_depth: 4, allowed_ancestry_len: 3 };
	let claim_queue_support = true;

	let mut collations = Collations::new(&assignments, claim_queue_support);
	collations.fetching_from = Some((collator_id_a, None));

	// first collation for `para_a` is in the limit
	assert!(!collations.is_collations_limit_reached(relay_parent_mode, para_a, 1));
	collations.note_fetched(para_a);

	// second collation for `para_a`` is not in the limit due to the pending fetch
	assert!(collations.is_collations_limit_reached(relay_parent_mode, para_a, 1));
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
	let relay_parent_mode =
		ProspectiveParachainsMode::Enabled { max_candidate_depth: 4, allowed_ancestry_len: 3 };
	let claim_queue_support = true;

	let mut collations = Collations::new(&claim_queue, claim_queue_support);

	collations.fetching_from = None;

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

	collations.add_to_waiting_queue(collation_a1.clone());
	collations.add_to_waiting_queue(collation_a2.clone());
	collations.add_to_waiting_queue(collation_b1.clone());

	assert_eq!(
		Some(collation_a1.clone()),
		collations.get_next_collation_to_fetch(
			// doesn't matter since `fetching_from` is `None`
			&(collator_id_a.clone(), Some(CandidateHash(Hash::repeat_byte(0)))),
			relay_parent_mode.clone(),
			&claim_queue,
		)
	);
	collations.note_fetched(collation_a1.0.para_id);

	assert_eq!(
		Some(collation_b1.clone()),
		collations.get_next_collation_to_fetch(
			// doesn't matter since `fetching_from` is `None`
			&(collator_id_a.clone(), Some(CandidateHash(Hash::repeat_byte(0)))),
			relay_parent_mode.clone(),
			&claim_queue,
		)
	);
	collations.note_fetched(collation_b1.0.para_id);

	assert_eq!(
		Some(collation_a2.clone()),
		collations.get_next_collation_to_fetch(
			// doesn't matter since `fetching_from` is `None`
			&(collator_id_a.clone(), Some(CandidateHash(Hash::repeat_byte(0)))),
			relay_parent_mode.clone(),
			&claim_queue,
		)
	);
	collations.note_fetched(collation_a2.0.para_id);
}

#[test]
fn collation_fetching_fallback_works() {
	sp_tracing::init_for_tests();

	let para_a = ParaId::from(1);
	let collator_id_a = CollatorId::from(sr25519::Public::from_raw([10u8; 32]));
	let peer_a = PeerId::random();

	let claim_queue = vec![para_a];
	let relay_parent_mode =
		ProspectiveParachainsMode::Enabled { max_candidate_depth: 4, allowed_ancestry_len: 3 };
	let claim_queue_support = false;

	let mut collations = Collations::new(&claim_queue, claim_queue_support);

	collations.fetching_from = None;

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
			relay_parent_mode.clone(),
			&claim_queue,
		)
	);
	collations.note_fetched(collation_a1.0.para_id);

	assert_eq!(
		Some(collation_a2.clone()),
		collations.get_next_collation_to_fetch(
			// doesn't matter since `fetching_from` is `None`
			&(collator_id_a.clone(), Some(CandidateHash(Hash::repeat_byte(0)))),
			relay_parent_mode.clone(),
			&claim_queue,
		)
	);
	collations.note_fetched(collation_a2.0.para_id);
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
	let relay_parent_mode =
		ProspectiveParachainsMode::Enabled { max_candidate_depth: 6, allowed_ancestry_len: 5 };
	let claim_queue_support = true;

	let mut collations = Collations::new(&claim_queue, claim_queue_support);
	collations.fetching_from = None;

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

	// Despite the order here the fetches should be a1, b1, c1, a2, b2, c2
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
			relay_parent_mode.clone(),
			&claim_queue,
		)
	);
	collations.note_fetched(collation_a1.0.para_id);

	assert_eq!(
		Some(collation_b1.clone()),
		collations.get_next_collation_to_fetch(
			// doesn't matter since `fetching_from` is `None`
			&(collator_id_a.clone(), Some(CandidateHash(Hash::repeat_byte(0)))),
			relay_parent_mode.clone(),
			&claim_queue,
		)
	);
	collations.note_fetched(collation_b1.0.para_id);

	assert_eq!(
		Some(collation_c1.clone()),
		collations.get_next_collation_to_fetch(
			// doesn't matter since `fetching_from` is `None`
			&(collator_id_a.clone(), Some(CandidateHash(Hash::repeat_byte(0)))),
			relay_parent_mode.clone(),
			&claim_queue,
		)
	);
	collations.note_fetched(collation_c1.0.para_id);

	assert_eq!(
		Some(collation_a2.clone()),
		collations.get_next_collation_to_fetch(
			// doesn't matter since `fetching_from` is `None`
			&(collator_id_a.clone(), Some(CandidateHash(Hash::repeat_byte(0)))),
			relay_parent_mode.clone(),
			&claim_queue,
		)
	);
	collations.note_fetched(collation_a2.0.para_id);

	assert_eq!(
		Some(collation_b2.clone()),
		collations.get_next_collation_to_fetch(
			// doesn't matter since `fetching_from` is `None`
			&(collator_id_a.clone(), Some(CandidateHash(Hash::repeat_byte(0)))),
			relay_parent_mode.clone(),
			&claim_queue,
		)
	);
	collations.note_fetched(collation_b2.0.para_id);

	assert_eq!(
		Some(collation_c2.clone()),
		collations.get_next_collation_to_fetch(
			// doesn't matter since `fetching_from` is `None`
			&(collator_id_a.clone(), Some(CandidateHash(Hash::repeat_byte(0)))),
			relay_parent_mode.clone(),
			&claim_queue,
		)
	);
	collations.note_fetched(collation_c2.0.para_id);
}
