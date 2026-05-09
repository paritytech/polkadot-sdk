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

//! Faithful port of `unconnected_candidates_become_connected`.
//!
//! Introduces A, C, D first (B missing → only A is "connected" / backable). Then introduce
//! B and verify the full chain is returned.

use polkadot_node_subsystem::messages::{Ancestors, BackableCandidateRef};
use polkadot_primitives::{CoreIndex, HeadData, Hash, Id as ParaId, DEFAULT_SCHEDULING_LOOKAHEAD};
use polkadot_primitives_test_helpers::make_candidate;
use polkadot_prospective_parachains_test_sim::world::{WorldExt as _, PerParaData, TestLeaf, TestState, World};

#[test]
fn unconnected_candidates_become_connected() {
	let mut test_state = TestState::default();
	for i in 2..=4 {
		test_state.claim_queue.insert(
			CoreIndex(i),
			std::iter::repeat(ParaId::from(1))
				.take(DEFAULT_SCHEDULING_LOOKAHEAD as _)
				.collect(),
		);
	}
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
	let (candidate_b, pvd_b) = make_candidate(
		leaf_a.hash,
		leaf_a.number,
		ParaId::from(1),
		HeadData(vec![1]),
		HeadData(vec![2]),
		test_state.validation_code_hash,
	);
	let (candidate_c, pvd_c) = make_candidate(
		leaf_a.hash,
		leaf_a.number,
		ParaId::from(1),
		HeadData(vec![2]),
		HeadData(vec![3]),
		test_state.validation_code_hash,
	);
	let (candidate_d, pvd_d) = make_candidate(
		leaf_a.hash,
		leaf_a.number,
		ParaId::from(1),
		HeadData(vec![3]),
		HeadData(vec![4]),
		test_state.validation_code_hash,
	);

	assert!(world.introduce_seconded_candidate(candidate_a.clone(), pvd_a));
	assert!(world.introduce_seconded_candidate(candidate_c.clone(), pvd_c));
	assert!(world.introduce_seconded_candidate(candidate_d.clone(), pvd_d));

	world.back_candidate(ParaId::from(1), candidate_a.hash());
	world.back_candidate(ParaId::from(1), candidate_c.hash());
	world.back_candidate(ParaId::from(1), candidate_d.hash());

	// Without B, only A is connected to the trunk.
	assert_eq!(
		world.get_backable_candidates(leaf_a.hash, ParaId::from(1), 5, Ancestors::default()),
		vec![BackableCandidateRef {
			candidate_hash: candidate_a.hash(),
			scheduling_parent: leaf_a.hash,
		}],
	);

	// Introduce B + back. Now A → B → C → D.
	assert!(world.introduce_seconded_candidate(candidate_b.clone(), pvd_b));
	world.back_candidate(ParaId::from(1), candidate_b.hash());

	assert_eq!(
		world.get_backable_candidates(leaf_a.hash, ParaId::from(1), 5, Ancestors::default()),
		vec![
			BackableCandidateRef {
				candidate_hash: candidate_a.hash(),
				scheduling_parent: leaf_a.hash,
			},
			BackableCandidateRef {
				candidate_hash: candidate_b.hash(),
				scheduling_parent: leaf_a.hash,
			},
			BackableCandidateRef {
				candidate_hash: candidate_c.hash(),
				scheduling_parent: leaf_a.hash,
			},
			BackableCandidateRef {
				candidate_hash: candidate_d.hash(),
				scheduling_parent: leaf_a.hash,
			},
		],
	);

	assert_eq!(world.base.leaves.len(), 1);
}
