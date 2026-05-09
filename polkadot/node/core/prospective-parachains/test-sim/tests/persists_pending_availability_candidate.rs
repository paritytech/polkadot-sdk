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

//! Faithful port of `persists_pending_availability_candidate`.

use polkadot_node_subsystem::messages::BackableCandidateRef;
use polkadot_primitives::{
	async_backing::CandidatePendingAvailability, BlockNumber, HeadData, Hash, Id as ParaId,
	DEFAULT_SCHEDULING_LOOKAHEAD,
};
use polkadot_primitives_test_helpers::make_candidate;
use polkadot_prospective_parachains_test_sim::world::{WorldExt as _, 
	get_parent_hash, PerParaData, TestLeaf, TestState, World,
};

const MAX_POV_SIZE: u32 = 1_000_000;

#[test]
fn persists_pending_availability_candidate() {
	let para_id = ParaId::from(1);
	let mut test_state = TestState::default();
	test_state.claim_queue = test_state
		.claim_queue
		.into_iter()
		.filter(|(_, paras)| matches!(paras.front(), Some(p) if p == &para_id))
		.collect();
	assert_eq!(test_state.claim_queue.len(), 1);
	let mut world = World::start(&test_state);

	let para_head = HeadData(vec![1, 2, 3]);
	let candidate_relay_parent_number: BlockNumber = 97;

	let leaf_a_hash = Hash::from_low_u64_be(2);
	let leaf_a_number: BlockNumber = candidate_relay_parent_number + (DEFAULT_SCHEDULING_LOOKAHEAD - 1);

	// candidate_relay_parent is the (lookahead-1)-th ancestor of leaf_a.
	let mut cur = leaf_a_hash;
	for _ in 0..(DEFAULT_SCHEDULING_LOOKAHEAD - 1) {
		cur = get_parent_hash(cur);
	}
	let candidate_relay_parent = cur;

	let leaf_a = TestLeaf {
		number: leaf_a_number,
		hash: leaf_a_hash,
		para_data: vec![(para_id, PerParaData::new(para_head.clone()))],
	};

	let leaf_b_hash = Hash::from_low_u64_be(1);
	let leaf_b_number = leaf_a.number + 1;

	world.activate_leaf(&leaf_a, &test_state);

	let (candidate_a, pvd_a) = make_candidate(
		candidate_relay_parent,
		candidate_relay_parent_number,
		para_id,
		para_head.clone(),
		HeadData(vec![1]),
		test_state.validation_code_hash,
	);
	let candidate_hash_a = candidate_a.hash();

	let (candidate_b, pvd_b) = make_candidate(
		leaf_b_hash,
		leaf_b_number,
		para_id,
		HeadData(vec![1]),
		HeadData(vec![2]),
		test_state.validation_code_hash,
	);
	let candidate_hash_b = candidate_b.hash();

	assert!(world.introduce_seconded_candidate(candidate_a.clone(), pvd_a.clone()));
	world.back_candidate(para_id, candidate_hash_a);

	let candidate_a_pending_av = CandidatePendingAvailability {
		candidate_hash: candidate_hash_a,
		descriptor: candidate_a.descriptor.clone(),
		commitments: candidate_a.commitments.clone(),
		relay_parent_number: candidate_relay_parent_number,
		max_pov_size: MAX_POV_SIZE,
	};
	let leaf_b = TestLeaf {
		number: leaf_b_number,
		hash: leaf_b_hash,
		para_data: vec![(
			para_id,
			PerParaData::new_with_pending(para_head, vec![candidate_a_pending_av]),
		)],
	};
	// leaf_b's parent is leaf_a so prospective inherits leaf_a's view.
	world.activate_leaf_with_parent_hash_fn(&leaf_b, &test_state, |hash| {
		if hash == leaf_b_hash {
			leaf_a_hash
		} else {
			get_parent_hash(hash)
		}
	});

	let resp = world.get_hypothetical_membership(candidate_hash_a, candidate_a, pvd_a);
	assert_eq!(resp.len(), 1);
	let (_, membership) = &resp[0];
	let mut got: Vec<Hash> = membership.iter().copied().collect();
	got.sort();
	let mut want = vec![leaf_a.hash, leaf_b.hash];
	want.sort();
	assert_eq!(got, want);

	assert!(world.introduce_seconded_candidate(candidate_b.clone(), pvd_b));
	world.back_candidate(para_id, candidate_hash_b);

	assert_eq!(
		world.get_backable_candidates(
			leaf_b.hash,
			para_id,
			1,
			vec![candidate_hash_a].into_iter().collect(),
		),
		vec![BackableCandidateRef {
			candidate_hash: candidate_hash_b,
			scheduling_parent: leaf_b_hash,
		}],
	);
}
