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

//! Faithful port of `check_hypothetical_membership_query`.

use polkadot_primitives::{HeadData, Hash, Id as ParaId};
use polkadot_primitives_test_helpers::make_candidate;
use polkadot_prospective_parachains_test_sim::world::{
	get_parent_hash, PerParaData, TestLeaf, TestState, World,
};
use std::collections::HashSet;

#[test]
fn check_hypothetical_membership_query() {
	let test_state = TestState::default();
	let mut world = World::start(&test_state);

	let leaf_b_hash = Hash::from_low_u64_be(1 << 20);
	let leaf_b = TestLeaf {
		number: 101,
		hash: leaf_b_hash,
		para_data: vec![
			(ParaId::from(1), PerParaData::new(HeadData(vec![1, 2, 3]))),
			(ParaId::from(2), PerParaData::new(HeadData(vec![2, 3, 4]))),
		],
	};
	let leaf_a = TestLeaf {
		number: 100,
		hash: get_parent_hash(leaf_b_hash),
		para_data: vec![
			(ParaId::from(1), PerParaData::new(HeadData(vec![1, 2, 3]))),
			(ParaId::from(2), PerParaData::new(HeadData(vec![2, 3, 4]))),
		],
	};

	world.activate_leaf(&leaf_a, &test_state);
	world.activate_leaf(&leaf_b, &test_state);

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

	let assert_membership = |world: &mut World,
	                         candidate: polkadot_primitives::CommittedCandidateReceiptV2,
	                         pvd: polkadot_primitives::PersistedValidationData,
	                         expected: Vec<Hash>| {
		let hash = candidate.hash();
		let resp = world.get_hypothetical_membership(hash, candidate, pvd);
		assert_eq!(resp.len(), 1);
		let (_, membership) = &resp[0];
		assert_eq!(
			membership.iter().copied().collect::<HashSet<_>>(),
			expected.into_iter().collect::<HashSet<_>>(),
		);
	};

	// Before adding any candidate, A is directly addable; B and C are potential.
	for (candidate, pvd) in [
		(candidate_a.clone(), pvd_a.clone()),
		(candidate_b.clone(), pvd_b.clone()),
		(candidate_c.clone(), pvd_c.clone()),
	] {
		assert_membership(&mut world, candidate, pvd, vec![leaf_a.hash, leaf_b.hash]);
	}

	// Introduce A; all three remain visible (unconnected so far).
	assert!(world.introduce_seconded_candidate(candidate_a.clone(), pvd_a.clone()));
	for (candidate, pvd) in [
		(candidate_a.clone(), pvd_a.clone()),
		(candidate_b.clone(), pvd_b.clone()),
		(candidate_c.clone(), pvd_c.clone()),
	] {
		assert_membership(&mut world, candidate, pvd, vec![leaf_a.hash, leaf_b.hash]);
	}

	// Back A; chain root anchors here. All three remain.
	world.back_candidate(ParaId::from(1), candidate_a.hash());
	for (candidate, pvd) in [
		(candidate_a.clone(), pvd_a.clone()),
		(candidate_b.clone(), pvd_b.clone()),
		(candidate_c.clone(), pvd_c.clone()),
	] {
		assert_membership(&mut world, candidate, pvd, vec![leaf_a.hash, leaf_b.hash]);
	}

	// Candidate D has invalid relay parent → reject.
	let (candidate_d, pvd_d) = make_candidate(
		Hash::from_low_u64_be(200),
		leaf_a.number,
		ParaId::from(1),
		HeadData(vec![1]),
		HeadData(vec![2]),
		test_state.validation_code_hash,
	);
	assert!(!world.introduce_seconded_candidate(candidate_d, pvd_d));

	// Candidate E has invalid head data → reject.
	let (candidate_e, pvd_e) = make_candidate(
		leaf_a.hash,
		leaf_a.number,
		ParaId::from(1),
		HeadData(vec![2]),
		HeadData(vec![0; 20481]),
		test_state.validation_code_hash,
	);
	assert!(!world.introduce_seconded_candidate(candidate_e, pvd_e));

	// Add B + back. Membership unchanged for the three legit candidates.
	assert!(world.introduce_seconded_candidate(candidate_b.clone(), pvd_b.clone()));
	world.back_candidate(ParaId::from(1), candidate_b.hash());

	for (candidate, pvd) in [
		(candidate_a.clone(), pvd_a.clone()),
		(candidate_b.clone(), pvd_b.clone()),
		(candidate_c.clone(), pvd_c.clone()),
	] {
		assert_membership(&mut world, candidate, pvd, vec![leaf_a.hash, leaf_b.hash]);
	}

	assert_eq!(world.leaves.len(), 2);
}
