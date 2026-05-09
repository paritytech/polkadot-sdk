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

//! Faithful port of `check_pvd_query`.

use polkadot_primitives::{HeadData, Hash, Id as ParaId};
use polkadot_primitives_test_helpers::make_candidate;
use polkadot_prospective_parachains_test_sim::world::{WorldExt as _, PerParaData, TestLeaf, TestState, World};

#[test]
fn check_pvd_query() {
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
	let (candidate_e, pvd_e) = make_candidate(
		leaf_a.hash,
		leaf_a.number,
		ParaId::from(1),
		HeadData(vec![5]),
		HeadData(vec![6]),
		test_state.validation_code_hash,
	);

	// PVD of A before adding (parent_head matches the leaf's required_parent).
	assert_eq!(
		world.get_pvd(ParaId::from(1), leaf_a.hash, HeadData(vec![1, 2, 3]), test_state.session_index),
		Some(pvd_a.clone()),
	);

	assert!(world.introduce_seconded_candidate(candidate_a.clone(), pvd_a.clone()));
	world.back_candidate(ParaId::from(1), candidate_a.hash());

	// PVD of A after adding.
	assert_eq!(
		world.get_pvd(ParaId::from(1), leaf_a.hash, HeadData(vec![1, 2, 3]), test_state.session_index),
		Some(pvd_a.clone()),
	);

	// PVD of B before adding (parent is A's head_data).
	assert_eq!(
		world.get_pvd(ParaId::from(1), leaf_a.hash, HeadData(vec![1]), test_state.session_index),
		Some(pvd_b.clone()),
	);
	assert!(world.introduce_seconded_candidate(candidate_b, pvd_b.clone()));
	assert_eq!(
		world.get_pvd(ParaId::from(1), leaf_a.hash, HeadData(vec![1]), test_state.session_index),
		Some(pvd_b.clone()),
	);

	// PVD of C before adding.
	assert_eq!(
		world.get_pvd(ParaId::from(1), leaf_a.hash, HeadData(vec![2]), test_state.session_index),
		Some(pvd_c.clone()),
	);
	assert!(world.introduce_seconded_candidate(candidate_c, pvd_c.clone()));
	assert_eq!(
		world.get_pvd(ParaId::from(1), leaf_a.hash, HeadData(vec![2]), test_state.session_index),
		Some(pvd_c),
	);

	// E's parent isn't known yet.
	assert_eq!(
		world.get_pvd(ParaId::from(1), leaf_a.hash, HeadData(vec![5]), test_state.session_index),
		None,
	);

	// Add E and re-query.
	assert!(world.introduce_seconded_candidate(candidate_e, pvd_e.clone()));
	assert_eq!(
		world.get_pvd(ParaId::from(1), leaf_a.hash, HeadData(vec![5]), test_state.session_index),
		Some(pvd_e),
	);

	assert_eq!(world.base.leaves.len(), 1);
}
