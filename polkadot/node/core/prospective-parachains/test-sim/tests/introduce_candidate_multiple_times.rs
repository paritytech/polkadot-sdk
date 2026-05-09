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

//! Faithful port of `introduce_candidate_multiple_times`.

use polkadot_node_subsystem::messages::{Ancestors, BackableCandidateRef};
use polkadot_primitives::{HeadData, Hash, Id as ParaId};
use polkadot_primitives_test_helpers::make_candidate;
use polkadot_prospective_parachains_test_sim::world::{WorldExt as _, PerParaData, TestLeaf, TestState, World};

#[test]
fn introduce_candidate_multiple_times() {
	let test_state = TestState::default();
	let mut world = World::start(&test_state);

	let leaf_a = TestLeaf {
		number: 100,
		hash: Hash::from_low_u64_be(1 << 20),
		para_data: vec![
			(ParaId::from(1), PerParaData::new(HeadData(vec![1, 2, 3]))),
			(ParaId::from(2), PerParaData::new(HeadData(vec![2, 3, 4]))),
		],
	};
	world.activate_leaf(&leaf_a, &test_state);

	let (candidate_a, pvd_a) = make_candidate(
		leaf_a.hash,
		leaf_a.number,
		ParaId::from(1),
		HeadData(vec![1, 2, 3]),
		HeadData(vec![1]),
		test_state.validation_code_hash,
	);
	let candidate_hash_a = candidate_a.hash();
	let response_a = vec![BackableCandidateRef {
		candidate_hash: candidate_hash_a,
		scheduling_parent: leaf_a.hash,
	}];

	assert!(world.introduce_seconded_candidate(candidate_a.clone(), pvd_a.clone()));
	world.back_candidate(ParaId::from(1), candidate_hash_a);

	assert_eq!(
		world.get_backable_candidates(leaf_a.hash, ParaId::from(1), 5, Ancestors::default()),
		response_a,
	);

	// Re-introduce the same candidate 5 more times. Each call returns true (already
	// present) and does not duplicate state.
	for _ in 0..5 {
		assert!(world.introduce_seconded_candidate(candidate_a.clone(), pvd_a.clone()));
	}

	assert_eq!(
		world.get_backable_candidates(leaf_a.hash, ParaId::from(1), 5, Ancestors::default()),
		response_a,
	);

	assert_eq!(world.base.leaves.len(), 1);
}
