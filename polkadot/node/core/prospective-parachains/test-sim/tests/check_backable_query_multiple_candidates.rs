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

//! Faithful port of `check_backable_query_multiple_candidates`.

use polkadot_node_subsystem::messages::{Ancestors, BackableCandidateRef};
use polkadot_primitives::{
	CandidateHash, CoreIndex, HeadData, Hash, Id as ParaId, DEFAULT_SCHEDULING_LOOKAHEAD,
};
use polkadot_primitives_test_helpers::make_candidate;
use polkadot_prospective_parachains_test_sim::{
	make_and_back_candidate,
	world::{PerParaData, TestLeaf, TestState, World},
};

#[test]
fn check_backable_query_multiple_candidates() {
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
	let candidate_hash_a = candidate_a.hash();
	assert!(world.introduce_seconded_candidate(candidate_a.clone(), pvd_a));
	world.back_candidate(ParaId::from(1), candidate_hash_a);

	let (candidate_b, candidate_hash_b) =
		make_and_back_candidate!(test_state, world, leaf_a, &candidate_a, 2);
	let (candidate_c, candidate_hash_c) =
		make_and_back_candidate!(test_state, world, leaf_a, &candidate_b, 3);
	let (_candidate_d, candidate_hash_d) =
		make_and_back_candidate!(test_state, world, leaf_a, &candidate_c, 4);

	// Para 2 is empty.
	assert!(world.get_backable_candidates(leaf_a.hash, ParaId::from(2), 1, Ancestors::new()).is_empty());
	assert!(world.get_backable_candidates(leaf_a.hash, ParaId::from(2), 5, Ancestors::new()).is_empty());
	assert!(world
		.get_backable_candidates(
			leaf_a.hash,
			ParaId::from(2),
			1,
			vec![candidate_hash_a].into_iter().collect(),
		)
		.is_empty());

	// Empty ancestors, count 1: returns A only.
	assert_eq!(
		world.get_backable_candidates(leaf_a.hash, ParaId::from(1), 1, Ancestors::new()),
		vec![BackableCandidateRef {
			candidate_hash: candidate_hash_a,
			scheduling_parent: leaf_a.hash,
		}],
	);
	for count in 4..10 {
		assert_eq!(
			world.get_backable_candidates(leaf_a.hash, ParaId::from(1), count, Ancestors::new()),
			vec![
				BackableCandidateRef {
					candidate_hash: candidate_hash_a,
					scheduling_parent: leaf_a.hash,
				},
				BackableCandidateRef {
					candidate_hash: candidate_hash_b,
					scheduling_parent: leaf_a.hash,
				},
				BackableCandidateRef {
					candidate_hash: candidate_hash_c,
					scheduling_parent: leaf_a.hash,
				},
				BackableCandidateRef {
					candidate_hash: candidate_hash_d,
					scheduling_parent: leaf_a.hash,
				},
			],
		);
	}

	// Ancestors=[A], count 1 → B; count 2 → B,C.
	assert_eq!(
		world.get_backable_candidates(
			leaf_a.hash,
			ParaId::from(1),
			1,
			vec![candidate_hash_a].into_iter().collect(),
		),
		vec![BackableCandidateRef {
			candidate_hash: candidate_hash_b,
			scheduling_parent: leaf_a.hash,
		}],
	);
	assert_eq!(
		world.get_backable_candidates(
			leaf_a.hash,
			ParaId::from(1),
			2,
			vec![candidate_hash_a].into_iter().collect(),
		),
		vec![
			BackableCandidateRef {
				candidate_hash: candidate_hash_b,
				scheduling_parent: leaf_a.hash,
			},
			BackableCandidateRef {
				candidate_hash: candidate_hash_c,
				scheduling_parent: leaf_a.hash,
			},
		],
	);
	for count in 3..10 {
		assert_eq!(
			world.get_backable_candidates(
				leaf_a.hash,
				ParaId::from(1),
				count,
				vec![candidate_hash_a].into_iter().collect(),
			),
			vec![
				BackableCandidateRef {
					candidate_hash: candidate_hash_b,
					scheduling_parent: leaf_a.hash,
				},
				BackableCandidateRef {
					candidate_hash: candidate_hash_c,
					scheduling_parent: leaf_a.hash,
				},
				BackableCandidateRef {
					candidate_hash: candidate_hash_d,
					scheduling_parent: leaf_a.hash,
				},
			],
		);
	}

	// Ancestors=[A,B,C], count 1 → D. Ancestors=[A,B], count 1 → C.
	assert_eq!(
		world.get_backable_candidates(
			leaf_a.hash,
			ParaId::from(1),
			1,
			vec![candidate_hash_a, candidate_hash_b, candidate_hash_c]
				.into_iter()
				.collect(),
		),
		vec![BackableCandidateRef {
			candidate_hash: candidate_hash_d,
			scheduling_parent: leaf_a.hash,
		}],
	);
	assert_eq!(
		world.get_backable_candidates(
			leaf_a.hash,
			ParaId::from(1),
			1,
			vec![candidate_hash_a, candidate_hash_b].into_iter().collect(),
		),
		vec![BackableCandidateRef {
			candidate_hash: candidate_hash_c,
			scheduling_parent: leaf_a.hash,
		}],
	);
	for count in 3..10 {
		assert_eq!(
			world.get_backable_candidates(
				leaf_a.hash,
				ParaId::from(1),
				count,
				vec![candidate_hash_a, candidate_hash_b].into_iter().collect(),
			),
			vec![
				BackableCandidateRef {
					candidate_hash: candidate_hash_c,
					scheduling_parent: leaf_a.hash,
				},
				BackableCandidateRef {
					candidate_hash: candidate_hash_d,
					scheduling_parent: leaf_a.hash,
				},
			],
		);
	}

	// All four ancestors → empty.
	for count in 1..4 {
		assert!(world
			.get_backable_candidates(
				leaf_a.hash,
				ParaId::from(1),
				count,
				vec![candidate_hash_a, candidate_hash_b, candidate_hash_c, candidate_hash_d]
					.into_iter()
					.collect(),
			)
			.is_empty());
	}

	// Wrong paths.
	assert_eq!(
		world.get_backable_candidates(
			leaf_a.hash,
			ParaId::from(1),
			1,
			vec![candidate_hash_b].into_iter().collect(),
		),
		vec![BackableCandidateRef {
			candidate_hash: candidate_hash_a,
			scheduling_parent: leaf_a.hash,
		}],
	);
	assert_eq!(
		world.get_backable_candidates(
			leaf_a.hash,
			ParaId::from(1),
			3,
			vec![candidate_hash_b, candidate_hash_c].into_iter().collect(),
		),
		vec![
			BackableCandidateRef {
				candidate_hash: candidate_hash_a,
				scheduling_parent: leaf_a.hash,
			},
			BackableCandidateRef {
				candidate_hash: candidate_hash_b,
				scheduling_parent: leaf_a.hash,
			},
			BackableCandidateRef {
				candidate_hash: candidate_hash_c,
				scheduling_parent: leaf_a.hash,
			},
		],
	);
	assert_eq!(
		world.get_backable_candidates(
			leaf_a.hash,
			ParaId::from(1),
			2,
			vec![candidate_hash_a, candidate_hash_c, candidate_hash_d]
				.into_iter()
				.collect(),
		),
		vec![
			BackableCandidateRef {
				candidate_hash: candidate_hash_b,
				scheduling_parent: leaf_a.hash,
			},
			BackableCandidateRef {
				candidate_hash: candidate_hash_c,
				scheduling_parent: leaf_a.hash,
			},
		],
	);

	// Non-existent ancestor candidate.
	assert_eq!(
		world.get_backable_candidates(
			leaf_a.hash,
			ParaId::from(1),
			2,
			vec![candidate_hash_a, CandidateHash(Hash::from_low_u64_be(100))]
				.into_iter()
				.collect(),
		),
		vec![
			BackableCandidateRef {
				candidate_hash: candidate_hash_b,
				scheduling_parent: leaf_a.hash,
			},
			BackableCandidateRef {
				candidate_hash: candidate_hash_c,
				scheduling_parent: leaf_a.hash,
			},
		],
	);

	// count=0 always empty.
	assert!(world.get_backable_candidates(leaf_a.hash, ParaId::from(1), 0, Ancestors::new()).is_empty());
	assert!(world
		.get_backable_candidates(leaf_a.hash, ParaId::from(1), 0, vec![candidate_hash_a].into_iter().collect())
		.is_empty());
	assert!(world
		.get_backable_candidates(
			leaf_a.hash,
			ParaId::from(1),
			0,
			vec![candidate_hash_a, candidate_hash_b].into_iter().collect(),
		)
		.is_empty());

	assert_eq!(world.leaves.len(), 1);
}
