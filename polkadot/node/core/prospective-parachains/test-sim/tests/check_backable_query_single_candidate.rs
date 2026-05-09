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

//! Faithful port of `check_backable_query_single_candidate`.

use polkadot_node_subsystem::messages::{Ancestors, BackableCandidateRef};
use polkadot_primitives::{
	CandidateHash, CoreIndex, HeadData, Hash, Id as ParaId, MutateDescriptorV2,
	DEFAULT_SCHEDULING_LOOKAHEAD,
};
use polkadot_primitives_test_helpers::make_candidate;
use polkadot_prospective_parachains_test_sim::world::{PerParaData, TestLeaf, TestState, World};

#[test]
fn check_backable_query_single_candidate() {
	let mut test_state = TestState::default();
	test_state.claim_queue.insert(
		CoreIndex(2),
		std::iter::repeat(ParaId::from(1))
			.take(DEFAULT_SCHEDULING_LOOKAHEAD as _)
			.collect(),
	);
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

	let (mut candidate_b, pvd_b) = make_candidate(
		leaf_a.hash,
		leaf_a.number,
		ParaId::from(1),
		HeadData(vec![1]),
		HeadData(vec![2]),
		test_state.validation_code_hash,
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

	assert_eq!(world.leaves.len(), 1);
}
