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

//! What `GetBackableCandidates` returns under various fragment-chain shapes,
//! ancestor sets, counts, and view shifts (parent leaving view).

use crate::common::world::{
	default_world_config, PerParaData, TestLeaf, World, WorldExt as _,
};
use polkadot_node_subsystem::messages::{Ancestors, BackableCandidateRef};
use polkadot_primitives::{
	CandidateHash, CoreIndex, HeadData,
	Hash, Id as ParaId, MutateDescriptorV2,
	DEFAULT_SCHEDULING_LOOKAHEAD,
};
use polkadot_primitives_test_helpers::make_candidate;

#[test]
fn check_backable_query_single_candidate() {
	let mut config = default_world_config();
	config.claim_queue.insert(
		CoreIndex(2),
		std::iter::repeat(ParaId::from(1))
			.take(DEFAULT_SCHEDULING_LOOKAHEAD as _)
			.collect(),
	);
	let mut world = World::start(config);

	let leaf_a = TestLeaf {
		number: 100,
		hash: Hash::from_low_u64_be(1 << 20),
		para_data: vec![
			(ParaId::from(1), PerParaData::new(HeadData(vec![1, 2, 3]))),
			(ParaId::from(2), PerParaData::new(HeadData(vec![2, 3, 4]))),
		],
	};
	world.activate_leaf(&leaf_a);

	let (candidate_a, pvd_a) = make_candidate(
		leaf_a.hash,
		leaf_a.number,
		ParaId::from(1),
		HeadData(vec![1, 2, 3]),
		HeadData(vec![1]),
		world.validation_code_hash(),
	);
	let candidate_hash_a = candidate_a.hash();

	let (mut candidate_b, pvd_b) = make_candidate(
		leaf_a.hash,
		leaf_a.number,
		ParaId::from(1),
		HeadData(vec![1]),
		HeadData(vec![2]),
		world.validation_code_hash(),
	);
	candidate_b.descriptor.set_para_head(Hash::from_low_u64_le(1000));
	let candidate_hash_b = candidate_b.hash();

	assert!(world.introduce_seconded_candidate(candidate_a.clone(), pvd_a));
	assert!(world.introduce_seconded_candidate(candidate_b.clone(), pvd_b));

	// Without backed candidates: nothing is backable.
	assert!(world
		.get_backable_candidates(leaf_a.hash, ParaId::from(1), 1, vec![candidate_hash_a].into_iter().collect())
		.is_empty());
	assert!(world
		.get_backable_candidates(leaf_a.hash, ParaId::from(1), 0, vec![candidate_hash_a].into_iter().collect())
		.is_empty());
	assert!(world
		.get_backable_candidates(leaf_a.hash, ParaId::from(1), 0, Ancestors::new())
		.is_empty());

	world.back_candidate(ParaId::from(1), candidate_hash_a);
	world.back_candidate(ParaId::from(1), candidate_hash_b);

	// Backing an unknown candidate is ignored.
	world.back_candidate(ParaId::from(1), CandidateHash(Hash::random()));

	// Other para gets nothing.
	assert!(world.get_backable_candidates(leaf_a.hash, ParaId::from(2), 1, Ancestors::new()).is_empty());
	assert!(world
		.get_backable_candidates(
			leaf_a.hash,
			ParaId::from(2),
			1,
			vec![candidate_hash_a].into_iter().collect(),
		)
		.is_empty());

	// Single backable candidate via empty ancestors → A.
	assert_eq!(
		world.get_backable_candidates(leaf_a.hash, ParaId::from(1), 1, Ancestors::new()),
		vec![BackableCandidateRef {
			candidate_hash: candidate_hash_a,
			scheduling_parent: leaf_a.hash,
		}],
	);
	// With ancestors=[A], the next backable is B.
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
	// "Wrong path" — ancestors=[B] is not on the connected chain so the subsystem returns
	// the chain root A.
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

	assert_eq!(world.base.leaves.len(), 1);
}

#[test]
fn check_backable_query_multiple_candidates() {
	let mut config = default_world_config();
	for i in 2..=4 {
		config.claim_queue.insert(
			CoreIndex(i),
			std::iter::repeat(ParaId::from(1))
				.take(DEFAULT_SCHEDULING_LOOKAHEAD as _)
				.collect(),
		);
	}
	let mut world = World::start(config);

	let leaf_a = TestLeaf {
		number: 100,
		hash: Hash::from_low_u64_be(1 << 20),
		para_data: vec![
			(ParaId::from(1), PerParaData::new(HeadData(vec![1, 2, 3]))),
			(ParaId::from(2), PerParaData::new(HeadData(vec![2, 3, 4]))),
		],
	};
	world.activate_leaf(&leaf_a);

	let (candidate_a, pvd_a) = make_candidate(
		leaf_a.hash,
		leaf_a.number,
		ParaId::from(1),
		HeadData(vec![1, 2, 3]),
		HeadData(vec![1]),
		world.validation_code_hash(),
	);
	let candidate_hash_a = candidate_a.hash();
	assert!(world.introduce_seconded_candidate(candidate_a.clone(), pvd_a));
	world.back_candidate(ParaId::from(1), candidate_hash_a);

	let (candidate_b, candidate_hash_b) =
		world.make_and_back_candidate(&leaf_a, &candidate_a, 2);
	let (candidate_c, candidate_hash_c) =
		world.make_and_back_candidate(&leaf_a, &candidate_b, 3);
	let (_candidate_d, candidate_hash_d) =
		world.make_and_back_candidate(&leaf_a, &candidate_c, 4);

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

	assert_eq!(world.base.leaves.len(), 1);
}

#[test]
fn fragment_chain_chain_length_is_bounded() {
	let mut config = default_world_config();
	config.claim_queue.insert(
		CoreIndex(2),
		std::iter::repeat(ParaId::from(1))
			.take(DEFAULT_SCHEDULING_LOOKAHEAD as _)
			.collect(),
	);
	let mut world = World::start(config);

	let leaf_a = TestLeaf {
		number: 100,
		hash: Hash::from_low_u64_be(1 << 20),
		para_data: vec![
			(ParaId::from(1), PerParaData::new(HeadData(vec![1, 2, 3]))),
			(ParaId::from(2), PerParaData::new(HeadData(vec![2, 3, 4]))),
		],
	};
	world.activate_leaf(&leaf_a);

	// A, B, C form a chain.
	let (candidate_a, pvd_a) = make_candidate(
		leaf_a.hash,
		leaf_a.number,
		ParaId::from(1),
		HeadData(vec![1, 2, 3]),
		HeadData(vec![1]),
		world.validation_code_hash(),
	);
	let (candidate_b, pvd_b) = make_candidate(
		leaf_a.hash,
		leaf_a.number,
		ParaId::from(1),
		HeadData(vec![1]),
		HeadData(vec![2]),
		world.validation_code_hash(),
	);
	let (candidate_c, pvd_c) = make_candidate(
		leaf_a.hash,
		leaf_a.number,
		ParaId::from(1),
		HeadData(vec![2]),
		HeadData(vec![3]),
		world.validation_code_hash(),
	);

	assert!(world.introduce_seconded_candidate(candidate_a.clone(), pvd_a));
	assert!(world.introduce_seconded_candidate(candidate_b.clone(), pvd_b));

	world.back_candidate(ParaId::from(1), candidate_a.hash());
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
		],
	);

	// C is introduced (lands in unconnected storage; max chain depth is 2). Backing it
	// drops it.
	assert!(world.introduce_seconded_candidate(candidate_c.clone(), pvd_c));
	world.back_candidate(ParaId::from(1), candidate_c.hash());

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
		],
	);

	assert_eq!(world.base.leaves.len(), 1);
}

#[test]
fn unconnected_candidates_become_connected() {
	let mut config = default_world_config();
	for i in 2..=4 {
		config.claim_queue.insert(
			CoreIndex(i),
			std::iter::repeat(ParaId::from(1))
				.take(DEFAULT_SCHEDULING_LOOKAHEAD as _)
				.collect(),
		);
	}
	let mut world = World::start(config);

	let leaf_a = TestLeaf {
		number: 100,
		hash: Hash::from_low_u64_be(1 << 20),
		para_data: vec![
			(ParaId::from(1), PerParaData::new(HeadData(vec![1, 2, 3]))),
			(ParaId::from(2), PerParaData::new(HeadData(vec![2, 3, 4]))),
		],
	};
	world.activate_leaf(&leaf_a);

	let (candidate_a, pvd_a) = make_candidate(
		leaf_a.hash,
		leaf_a.number,
		ParaId::from(1),
		HeadData(vec![1, 2, 3]),
		HeadData(vec![1]),
		world.validation_code_hash(),
	);
	let (candidate_b, pvd_b) = make_candidate(
		leaf_a.hash,
		leaf_a.number,
		ParaId::from(1),
		HeadData(vec![1]),
		HeadData(vec![2]),
		world.validation_code_hash(),
	);
	let (candidate_c, pvd_c) = make_candidate(
		leaf_a.hash,
		leaf_a.number,
		ParaId::from(1),
		HeadData(vec![2]),
		HeadData(vec![3]),
		world.validation_code_hash(),
	);
	let (candidate_d, pvd_d) = make_candidate(
		leaf_a.hash,
		leaf_a.number,
		ParaId::from(1),
		HeadData(vec![3]),
		HeadData(vec![4]),
		world.validation_code_hash(),
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

#[test]
fn introduce_candidate_parent_leaving_view() {
	let config = default_world_config();
	let mut world = World::start(config);

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

	world.activate_leaf(&leaf_a);
	world.activate_leaf(&leaf_b);
	world.activate_leaf(&leaf_c);

	let (candidate_a1, pvd_a1) = make_candidate(
		leaf_a.hash,
		leaf_a.number,
		ParaId::from(1),
		HeadData(vec![1, 2, 3]),
		HeadData(vec![1]),
		world.validation_code_hash(),
	);
	let candidate_hash_a1 = candidate_a1.hash();

	let (candidate_a2, pvd_a2) = make_candidate(
		leaf_a.hash,
		leaf_a.number,
		ParaId::from(2),
		HeadData(vec![2, 3, 4]),
		HeadData(vec![2]),
		world.validation_code_hash(),
	);
	let candidate_hash_a2 = candidate_a2.hash();

	let (candidate_b, pvd_b) = make_candidate(
		leaf_b.hash,
		leaf_b.number,
		ParaId::from(1),
		HeadData(vec![3, 4, 5]),
		HeadData(vec![3]),
		world.validation_code_hash(),
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
		world.validation_code_hash(),
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

	assert_eq!(world.base.leaves.len(), 0);
}
