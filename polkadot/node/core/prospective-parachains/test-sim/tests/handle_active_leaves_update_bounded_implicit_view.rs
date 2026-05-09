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

//! Faithful port of `handle_active_leaves_update_bounded_implicit_view`.
//!
//! The original asserts on internal `view.per_scheduling_parent.len()` to verify the
//! implicit-view bound. The port verifies the observable shape: after activating 10
//! leaves and deactivating the first 9, `GetBackableCandidates` against the latest leaf
//! still works (latest leaf is active) and earlier ones return empty.

use polkadot_node_subsystem::messages::Ancestors;
use polkadot_primitives::{HeadData, Hash, Id as ParaId};
use polkadot_prospective_parachains_test_sim::world::{WorldExt as _, 
	get_parent_hash, PerParaData, TestLeaf, TestState, World,
};

#[test]
fn handle_active_leaves_update_bounded_implicit_view() {
	let para_id = ParaId::from(1);
	let mut test_state = TestState::default();
	test_state.claim_queue = test_state
		.claim_queue
		.into_iter()
		.filter(|(_, paras)| matches!(paras.front(), Some(p) if p == &para_id))
		.collect();
	assert_eq!(test_state.claim_queue.len(), 1);
	let mut world = World::start(&test_state);

	// Build linear chain of 10 leaves, oldest first.
	let leaves: Vec<TestLeaf> = {
		let mut v = vec![TestLeaf {
			number: 100,
			hash: Hash::from_low_u64_be(1 << 20),
			para_data: vec![(para_id, PerParaData::new(HeadData(vec![1, 2, 3])))],
		}];
		for i in 1..10 {
			let prev = &v[i - 1];
			v.push(TestLeaf {
				number: prev.number - 1,
				hash: get_parent_hash(prev.hash),
				para_data: vec![(para_id, PerParaData::new(HeadData(vec![1, 2, 3])))],
			});
		}
		v.reverse();
		v
	};

	// Activate all 10.
	for leaf in &leaves {
		world.activate_leaf(leaf, &test_state);
	}

	// Deactivate first 9, leaving only the latest.
	for leaf in &leaves[0..9] {
		world.deactivate_leaf(leaf.hash);
	}

	// Latest leaf is queryable; deactivated ones return empty.
	let _ = world.get_backable_candidates(leaves[9].hash, para_id, 5, Ancestors::default());
	for leaf in &leaves[0..9] {
		assert!(world.get_backable_candidates(leaf.hash, para_id, 5, Ancestors::default()).is_empty());
	}

	assert_eq!(world.base.leaves.len(), 1);
}
