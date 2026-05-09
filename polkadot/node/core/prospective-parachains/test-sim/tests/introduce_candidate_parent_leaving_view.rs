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

//! Faithful port of `introduce_candidate_parent_leaving_view`.

use polkadot_node_subsystem::messages::{Ancestors, BackableCandidateRef};
use polkadot_primitives::{HeadData, Hash, Id as ParaId};
use polkadot_primitives_test_helpers::make_candidate;
use polkadot_prospective_parachains_test_sim::world::{PerParaData, TestLeaf, TestState, World};

#[test]
fn introduce_candidate_parent_leaving_view() {
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
	let leaf_b = TestLeaf {
		number: 101,
		hash: Hash::from_low_u64_be(2 << 20),
		para_data: vec![
			(ParaId::from(1), PerParaData::new(HeadData(vec![3, 4, 5]))),
			(ParaId::from(2), PerParaData::new(HeadData(vec![4, 5, 6]))),
		],
	};
	let leaf_c = TestLeaf {
		number: 102,
		hash: Hash::from_low_u64_be(3 << 20),
		para_data: vec![
			(ParaId::from(1), PerParaData::new(HeadData(vec![5, 6, 7]))),
			(ParaId::from(2), PerParaData::new(HeadData(vec![6, 7, 8]))),
		],
	};

	world.activate_leaf(&leaf_a, &test_state);
	world.activate_leaf(&leaf_b, &test_state);
	world.activate_leaf(&leaf_c, &test_state);

	let (candidate_a1, pvd_a1) = make_candidate(
		leaf_a.hash,
		leaf_a.number,
		ParaId::from(1),
		HeadData(vec![1, 2, 3]),
		HeadData(vec![1]),
		test_state.validation_code_hash,
	);
	let candidate_hash_a1 = candidate_a1.hash();

	let (candidate_a2, pvd_a2) = make_candidate(
		leaf_a.hash,
		leaf_a.number,
		ParaId::from(2),
		HeadData(vec![2, 3, 4]),
		HeadData(vec![2]),
		test_state.validation_code_hash,
	);
	let candidate_hash_a2 = candidate_a2.hash();

	let (candidate_b, pvd_b) = make_candidate(
		leaf_b.hash,
		leaf_b.number,
		ParaId::from(1),
		HeadData(vec![3, 4, 5]),
		HeadData(vec![3]),
		test_state.validation_code_hash,
	);
	let candidate_hash_b = candidate_b.hash();
	let response_b = vec![BackableCandidateRef {
		candidate_hash: candidate_hash_b,
		scheduling_parent: leaf_b.hash,
	}];

	let (candidate_c, pvd_c) = make_candidate(
		leaf_c.hash,
		leaf_c.number,
		ParaId::from(2),
		HeadData(vec![6, 7, 8]),
		HeadData(vec![4]),
		test_state.validation_code_hash,
	);
	let candidate_hash_c = candidate_c.hash();
	let response_c = vec![BackableCandidateRef {
		candidate_hash: candidate_hash_c,
		scheduling_parent: leaf_c.hash,
	}];

	assert!(world.introduce_seconded_candidate(candidate_a1.clone(), pvd_a1));
	assert!(world.introduce_seconded_candidate(candidate_a2.clone(), pvd_a2));
	assert!(world.introduce_seconded_candidate(candidate_b.clone(), pvd_b));
	assert!(world.introduce_seconded_candidate(candidate_c.clone(), pvd_c));

	world.back_candidate(ParaId::from(1), candidate_hash_a1);
	world.back_candidate(ParaId::from(2), candidate_hash_a2);
	world.back_candidate(ParaId::from(1), candidate_hash_b);
	world.back_candidate(ParaId::from(2), candidate_hash_c);

	world.deactivate_leaf(leaf_a.hash);

	// A1, A2 gone. B, C remain.
	assert!(world.get_backable_candidates(leaf_a.hash, ParaId::from(1), 5, Ancestors::default()).is_empty());
	assert!(world.get_backable_candidates(leaf_a.hash, ParaId::from(2), 5, Ancestors::default()).is_empty());
	assert_eq!(
		world.get_backable_candidates(leaf_b.hash, ParaId::from(1), 5, Ancestors::default()),
		response_b,
	);
	assert_eq!(
		world.get_backable_candidates(leaf_c.hash, ParaId::from(2), 5, Ancestors::default()),
		response_c.clone(),
	);

	world.deactivate_leaf(leaf_b.hash);

	// B gone too. C remains.
	assert!(world.get_backable_candidates(leaf_a.hash, ParaId::from(1), 5, Ancestors::default()).is_empty());
	assert!(world.get_backable_candidates(leaf_a.hash, ParaId::from(2), 5, Ancestors::default()).is_empty());
	assert!(world.get_backable_candidates(leaf_b.hash, ParaId::from(1), 5, Ancestors::default()).is_empty());
	assert_eq!(
		world.get_backable_candidates(leaf_c.hash, ParaId::from(2), 5, Ancestors::default()),
		response_c,
	);

	world.deactivate_leaf(leaf_c.hash);

	// All gone.
	assert!(world.get_backable_candidates(leaf_a.hash, ParaId::from(1), 5, Ancestors::default()).is_empty());
	assert!(world.get_backable_candidates(leaf_a.hash, ParaId::from(2), 5, Ancestors::default()).is_empty());
	assert!(world.get_backable_candidates(leaf_b.hash, ParaId::from(1), 5, Ancestors::default()).is_empty());
	assert!(world.get_backable_candidates(leaf_c.hash, ParaId::from(2), 5, Ancestors::default()).is_empty());

	assert_eq!(world.leaves.len(), 0);
}
