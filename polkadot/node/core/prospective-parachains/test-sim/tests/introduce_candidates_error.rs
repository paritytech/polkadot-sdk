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

//! Faithful port of the in-crate `introduce_candidates_error` test from
//! `polkadot/node/core/prospective-parachains/src/tests.rs`.
//!
//! Differences from the original:
//! - Original uses `leaf_a.hash = Default::default()` (zero); ChainModel uses
//!   `Hash::zero()` as the genesis-parent-marker, so the port uses `Hash::from_low_u64_be(1
//!   << 20)`.
//! - Original asserts `view.active_leaves.len() == 1` (private state); the port relies on
//!   the `GetBackableCandidates` reply being correct, which requires the leaf to be active.

use polkadot_node_subsystem::messages::{Ancestors, BackableCandidateRef};
use polkadot_primitives::{CoreIndex, HeadData, Hash, Id as ParaId, DEFAULT_SCHEDULING_LOOKAHEAD};
use polkadot_primitives_test_helpers::make_candidate;
use polkadot_prospective_parachains_test_sim::world::{PerParaData, TestLeaf, TestState, World};

#[test]
fn introduce_candidates_error() {
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

	// Candidate A: directly buildable from `[1,2,3]` (the leaf's required_parent).
	let (candidate_a, pvd_a) = make_candidate(
		leaf_a.hash,
		leaf_a.number,
		ParaId::from(1),
		HeadData(vec![1, 2, 3]),
		HeadData(vec![1]),
		test_state.validation_code_hash,
	);
	// Candidate B: child of A.
	let (candidate_b, pvd_b) = make_candidate(
		leaf_a.hash,
		leaf_a.number,
		ParaId::from(1),
		HeadData(vec![1]),
		HeadData(vec![1; 20480]),
		test_state.validation_code_hash,
	);
	// Candidate C: oversized head data, fails the constraint check.
	let (candidate_c, pvd_c) = make_candidate(
		leaf_a.hash,
		leaf_a.number,
		ParaId::from(1),
		HeadData(vec![1; 20480]),
		HeadData(vec![0; 20485]),
		test_state.validation_code_hash,
	);

	// Hypothetical membership: A directly addable, B potential. Both report leaf_a.hash.
	for (candidate, pvd) in [(candidate_a.clone(), pvd_a.clone()), (candidate_b.clone(), pvd_b.clone())] {
		let hash = candidate.hash();
		let resp = world.get_hypothetical_membership(hash, candidate, pvd);
		assert_eq!(resp.len(), 1);
		let (_, membership) = &resp[0];
		assert_eq!(membership.iter().copied().collect::<Vec<_>>(), vec![leaf_a.hash]);
	}

	// Hypothetical membership of C: empty (fails constraint check).
	{
		let resp =
			world.get_hypothetical_membership(candidate_c.hash(), candidate_c.clone(), pvd_c.clone());
		assert_eq!(resp.len(), 1);
		let (_, membership) = &resp[0];
		assert!(membership.is_empty());
	}

	// Introduce A and B successfully.
	assert!(world.introduce_seconded_candidate(candidate_a.clone(), pvd_a.clone()));
	assert!(world.introduce_seconded_candidate(candidate_b.clone(), pvd_b.clone()));
	// Introduce C: rejected.
	assert!(!world.introduce_seconded_candidate(candidate_c.clone(), pvd_c.clone()));

	world.back_candidate(ParaId::from(1), candidate_a.hash());
	world.back_candidate(ParaId::from(1), candidate_b.hash());
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

	assert_eq!(world.leaves.len(), 1);
}
