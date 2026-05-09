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

//! Faithful port of `introduce_v3_candidate_with_older_relay_parent`.

use polkadot_node_subsystem::messages::{Ancestors, BackableCandidateRef};
use polkadot_primitives::{
	BlockNumber, HeadData, Hash, Id as ParaId, DEFAULT_SCHEDULING_LOOKAHEAD,
};
use polkadot_primitives_test_helpers::make_candidate_v3;
use polkadot_prospective_parachains_test_sim::world::{PerParaData, TestLeaf, TestState, World};
use std::collections::HashSet;

const LEAF_NUMBER: BlockNumber = 100;
const OLDER_RELAY_PARENT_NUMBER: BlockNumber =
	LEAF_NUMBER - 4 * DEFAULT_SCHEDULING_LOOKAHEAD;

#[test]
fn introduce_v3_candidate_with_older_relay_parent() {
	let para_id = ParaId::from(1);
	let mut test_state = TestState::default();
	// Allow relay parents back to the older block via constraints' min_relay_parent_number.
	test_state.min_relay_parent_number_override = Some(OLDER_RELAY_PARENT_NUMBER);
	let mut world = World::start(&test_state);

	let leaf_a = TestLeaf {
		number: LEAF_NUMBER,
		hash: Hash::from_low_u64_be(1 << 20),
		para_data: vec![
			(para_id, PerParaData::new(HeadData(vec![1, 2, 3]))),
			(ParaId::from(2), PerParaData::new(HeadData(vec![2, 3, 4]))),
		],
	};
	world.activate_leaf(&leaf_a, &test_state);

	// Older relay parent: register it in the chain so prospective's
	// AncestorRelayParentInfo / SessionIndexForChild lookups resolve.
	let older_relay_parent = Hash::from_low_u64_be(9999);
	{
		let mut chain = world.chain.lock();
		chain.register_block_with_session(
			older_relay_parent,
			Hash::zero(),
			OLDER_RELAY_PARENT_NUMBER,
			Some(test_state.session_index),
		);
	}

	let (candidate_a, pvd_a) = make_candidate_v3(
		older_relay_parent,
		OLDER_RELAY_PARENT_NUMBER,
		leaf_a.hash,
		para_id,
		HeadData(vec![1, 2, 3]),
		HeadData(vec![1]),
		test_state.validation_code_hash,
	);
	let candidate_hash_a = candidate_a.hash();

	assert_eq!(candidate_a.descriptor.relay_parent(), older_relay_parent);
	assert_eq!(candidate_a.descriptor.scheduling_parent(), leaf_a.hash);

	assert!(world.introduce_seconded_candidate(candidate_a.clone(), pvd_a.clone()));
	world.back_candidate(para_id, candidate_hash_a);

	assert_eq!(
		world.get_backable_candidates(leaf_a.hash, para_id, 1, Ancestors::default()),
		vec![BackableCandidateRef {
			candidate_hash: candidate_hash_a,
			scheduling_parent: leaf_a.hash,
		}],
	);

	let resp = world.get_hypothetical_membership(candidate_hash_a, candidate_a, pvd_a);
	assert_eq!(resp.len(), 1);
	let (_, membership) = &resp[0];
	assert_eq!(
		membership.iter().copied().collect::<HashSet<_>>(),
		[leaf_a.hash].into_iter().collect::<HashSet<_>>(),
	);

	assert_eq!(world.leaves.len(), 1);
}
