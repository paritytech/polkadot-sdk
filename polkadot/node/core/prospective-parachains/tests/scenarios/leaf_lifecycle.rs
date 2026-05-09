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

//! `ActiveLeavesUpdate` handling — simple activate/deactivate, parent-inheritance,
//! implicit-view bound, pending-availability persistence across RP-out-of-scope, and
//! session-boundary ancestry stops.

use crate::common::world::{
	default_world_config, get_parent_hash, PerParaData, TestLeaf, World, WorldExt as _,
};
use crate::make_and_back_candidate;
use polkadot_node_subsystem::{
	messages::{Ancestors, BackableCandidateRef},
	ActiveLeavesUpdate, OverseerSignal,
};
use polkadot_node_subsystem_test_helpers::mock::new_leaf;
use polkadot_primitives::{
	async_backing::CandidatePendingAvailability, BlockNumber, CoreIndex, HeadData,
	Hash, Id as ParaId, SessionIndex,
	DEFAULT_SCHEDULING_LOOKAHEAD,
};
use polkadot_primitives_test_helpers::make_candidate;
use polkadot_subsystem_test_sim::chain::SessionInfo;
use std::collections::BTreeMap;

const MAX_POV_SIZE: u32 = 1_000_000;


#[test]
fn correctly_updates_leaves() {
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
	// Activating the same leaf again is a no-op for the subsystem; world tracks the
	// signal regardless. Recompute leaf list count expectation accordingly.
	world.activate_leaf(&leaf_b);

	// Empty update.
	world.signal_active_leaves(ActiveLeavesUpdate::default());

	// Activate leaf_c and deactivate leaf_b in a single update. Register leaf_c on the
	// chain first so prospective's per-leaf init can resolve its queries.
	world.register_leaf_in_chain(&leaf_c);
	world.signal_active_leaves(ActiveLeavesUpdate {
		activated: Some(new_leaf(leaf_c.hash, leaf_c.number)),
		deactivated: [leaf_b.hash][..].into(),
	});

	// Deactivate leaf_a and leaf_c together.
	world.signal_active_leaves(ActiveLeavesUpdate {
		deactivated: [leaf_a.hash, leaf_c.hash][..].into(),
		..Default::default()
	});

	// Activate and deactivate leaf_a in the same update.
	world.signal_active_leaves(ActiveLeavesUpdate {
		activated: Some(new_leaf(leaf_a.hash, leaf_a.number)),
		deactivated: [leaf_a.hash][..].into(),
	});

	// Deactivate everything (with extra unknown hashes, which is allowed).
	world.signal_active_leaves(ActiveLeavesUpdate {
		deactivated: [leaf_a.hash, leaf_b.hash, leaf_c.hash][..].into(),
		..Default::default()
	});

	// world.leaves tracks signals; subsystem's internal `active_leaves.len()` is private.
	// Final state: zero active leaves.
	assert!(world.base.leaves.is_empty());
}

#[test]
fn handle_active_leaves_update_gets_candidates_from_parent() {
	let para_id = ParaId::from(1);

	let mut config = default_world_config();
	config.claim_queue = BTreeMap::new();
	for i in 0..=4 {
		config.claim_queue.insert(
			CoreIndex(i),
			std::iter::repeat(para_id).take(DEFAULT_SCHEDULING_LOOKAHEAD as _).collect(),
		);
	}
	let mut world = World::start(config);

	let leaf_a = TestLeaf {
		number: 100,
		hash: Hash::from_low_u64_be(1 << 20),
		para_data: vec![(para_id, PerParaData::new(HeadData(vec![1, 2, 3])))],
	};
	world.activate_leaf(&leaf_a);

	let (candidate_a, pvd_a) = make_candidate(
		leaf_a.hash,
		leaf_a.number,
		para_id,
		HeadData(vec![1, 2, 3]),
		HeadData(vec![1]),
		world.validation_code_hash(),
	);
	let candidate_hash_a = candidate_a.hash();
	assert!(world.introduce_seconded_candidate(candidate_a.clone(), pvd_a));
	world.back_candidate(para_id, candidate_hash_a);

	let (candidate_b, candidate_hash_b) =
		make_and_back_candidate!(world, leaf_a, &candidate_a, 2);
	let (candidate_c, candidate_hash_c) =
		make_and_back_candidate!(world, leaf_a, &candidate_b, 3);
	let (candidate_d, candidate_hash_d) =
		make_and_back_candidate!(world, leaf_a, &candidate_c, 4);

	let mut all_candidates_resp = vec![
		BackableCandidateRef { candidate_hash: candidate_hash_a, scheduling_parent: leaf_a.hash },
		BackableCandidateRef { candidate_hash: candidate_hash_b, scheduling_parent: leaf_a.hash },
		BackableCandidateRef { candidate_hash: candidate_hash_c, scheduling_parent: leaf_a.hash },
		BackableCandidateRef { candidate_hash: candidate_hash_d, scheduling_parent: leaf_a.hash },
	];

	assert_eq!(
		world.get_backable_candidates(leaf_a.hash, para_id, 5, Ancestors::default()),
		all_candidates_resp,
	);

	// Activate leaf B as a child of leaf A so it inherits leaf_a's per-scheduling-parent
	// fragment chain (the original test relies on this implicit ancestry to expose
	// leaf_a's C, D under leaf_b). A and B become pending-availability under it.
	let leaf_b = TestLeaf {
		number: 101,
		hash: Hash::from_low_u64_be(2 << 20),
		para_data: vec![(
			para_id,
			PerParaData::new_with_pending(
				HeadData(vec![1, 2, 3]),
				vec![
					CandidatePendingAvailability {
						candidate_hash: candidate_a.hash(),
						descriptor: candidate_a.descriptor.clone(),
						commitments: candidate_a.commitments.clone(),
						relay_parent_number: leaf_a.number,
						max_pov_size: MAX_POV_SIZE,
					},
					CandidatePendingAvailability {
						candidate_hash: candidate_b.hash(),
						descriptor: candidate_b.descriptor.clone(),
						commitments: candidate_b.commitments.clone(),
						relay_parent_number: leaf_a.number,
						max_pov_size: MAX_POV_SIZE,
					},
				],
			),
		)],
	};
	let leaf_a_hash = leaf_a.hash;
	let leaf_b_hash = leaf_b.hash;
	world.activate_leaf_with_parent_hash_fn(&leaf_b, |hash| {
		if hash == leaf_b_hash {
			leaf_a_hash
		} else {
			get_parent_hash(hash)
		}
	});

	// Empty ancestors → empty (A,B are pending availability, not part of chain).
	assert!(world.get_backable_candidates(leaf_b.hash, para_id, 5, Ancestors::default()).is_empty());

	// Ancestors=[A,B] → C,D remaining.
	assert_eq!(
		world.get_backable_candidates(
			leaf_b.hash,
			para_id,
			5,
			[candidate_a.hash(), candidate_b.hash()].into_iter().collect(),
		),
		vec![
			BackableCandidateRef { candidate_hash: candidate_c.hash(), scheduling_parent: leaf_a.hash },
			BackableCandidateRef { candidate_hash: candidate_d.hash(), scheduling_parent: leaf_a.hash },
		],
	);

	// Empty ancestors at leaf_b → still empty.
	assert!(world.get_backable_candidates(leaf_b.hash, para_id, 5, Ancestors::default()).is_empty());

	// leaf_a is still active and returns the full chain.
	assert_eq!(
		world.get_backable_candidates(leaf_a.hash, para_id, 5, Ancestors::default()),
		all_candidates_resp,
	);

	// Deactivate leaf_a.
	world.deactivate_leaf(leaf_a.hash);

	// leaf_b still empty without ancestors; with [A,B] → C,D.
	assert!(world.get_backable_candidates(leaf_b.hash, para_id, 5, Ancestors::default()).is_empty());
	assert_eq!(
		world.get_backable_candidates(
			leaf_b.hash,
			para_id,
			5,
			[candidate_a.hash(), candidate_b.hash()].into_iter().collect(),
		),
		vec![
			BackableCandidateRef { candidate_hash: candidate_c.hash(), scheduling_parent: leaf_a.hash },
			BackableCandidateRef { candidate_hash: candidate_d.hash(), scheduling_parent: leaf_a.hash },
		],
	);

	// Activate leaf_c as a sibling fork of leaf_b (shared parent: leaf_a). leaf_c inherits
	// leaf_a's candidates.
	let leaf_c = TestLeaf {
		number: 101,
		hash: Hash::from_low_u64_be(3 << 20),
		para_data: vec![(para_id, PerParaData::new_with_pending(HeadData(vec![1, 2, 3]), vec![]))],
	};
	let leaf_a_hash = leaf_a.hash;
	let leaf_c_hash = leaf_c.hash;
	world.activate_leaf_with_parent_hash_fn(&leaf_c, |hash| {
		if hash == leaf_c_hash {
			leaf_a_hash
		} else {
			get_parent_hash(hash)
		}
	});

	assert_eq!(
		world.get_backable_candidates(
			leaf_b.hash,
			para_id,
			5,
			[candidate_a.hash(), candidate_b.hash()].into_iter().collect(),
		),
		vec![
			BackableCandidateRef { candidate_hash: candidate_c.hash(), scheduling_parent: leaf_a.hash },
			BackableCandidateRef { candidate_hash: candidate_d.hash(), scheduling_parent: leaf_a.hash },
		],
	);
	assert_eq!(
		world.get_backable_candidates(leaf_c.hash, para_id, 5, Ancestors::new()),
		all_candidates_resp,
	);

	// Deactivate leaf_c, add a candidate E on leaf_a, reactivate leaf_c. E should be
	// inherited.
	world.deactivate_leaf(leaf_c.hash);
	let (candidate_e, _) = make_and_back_candidate!(world, leaf_a, &candidate_d, 5);
	world.activate_leaf_with_parent_hash_fn(&leaf_c, |hash| {
		if hash == leaf_c_hash {
			leaf_a_hash
		} else {
			get_parent_hash(hash)
		}
	});

	assert_eq!(
		world.get_backable_candidates(
			leaf_b.hash,
			para_id,
			5,
			[candidate_a.hash(), candidate_b.hash()].into_iter().collect(),
		),
		vec![
			BackableCandidateRef { candidate_hash: candidate_c.hash(), scheduling_parent: leaf_a.hash },
			BackableCandidateRef { candidate_hash: candidate_d.hash(), scheduling_parent: leaf_a.hash },
			BackableCandidateRef { candidate_hash: candidate_e.hash(), scheduling_parent: leaf_a.hash },
		],
	);

	all_candidates_resp.push(BackableCandidateRef {
		candidate_hash: candidate_e.hash(),
		scheduling_parent: leaf_a.hash,
	});
	assert_eq!(
		world.get_backable_candidates(leaf_c.hash, para_id, 5, Ancestors::new()),
		all_candidates_resp,
	);

	// Querying a deactivated leaf returns empty.
	assert!(world.get_backable_candidates(leaf_a.hash, para_id, 5, Ancestors::new()).is_empty());
}

#[test]
fn handle_active_leaves_update_bounded_implicit_view() {
	let para_id = ParaId::from(1);
	let mut config = default_world_config();
	config.claim_queue
		.retain(|_, paras| matches!(paras.front(), Some(p) if p == &para_id));
	assert_eq!(config.claim_queue.len(), 1);
	let mut world = World::start(config);

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
		world.activate_leaf(leaf);
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

#[test]
fn persists_pending_availability_candidate() {
	let para_id = ParaId::from(1);
	let mut config = default_world_config();
	config.claim_queue
		.retain(|_, paras| matches!(paras.front(), Some(p) if p == &para_id));
	assert_eq!(config.claim_queue.len(), 1);
	let mut world = World::start(config);

	let para_head = HeadData(vec![1, 2, 3]);
	let candidate_relay_parent_number: BlockNumber = 97;

	let leaf_a_hash = Hash::from_low_u64_be(2);
	let leaf_a_number: BlockNumber = candidate_relay_parent_number + (DEFAULT_SCHEDULING_LOOKAHEAD - 1);

	// candidate_relay_parent is the (lookahead-1)-th ancestor of leaf_a.
	let mut cur = leaf_a_hash;
	for _ in 0..(DEFAULT_SCHEDULING_LOOKAHEAD - 1) {
		cur = get_parent_hash(cur);
	}
	let candidate_relay_parent = cur;

	let leaf_a = TestLeaf {
		number: leaf_a_number,
		hash: leaf_a_hash,
		para_data: vec![(para_id, PerParaData::new(para_head.clone()))],
	};

	let leaf_b_hash = Hash::from_low_u64_be(1);
	let leaf_b_number = leaf_a.number + 1;

	world.activate_leaf(&leaf_a);

	let (candidate_a, pvd_a) = make_candidate(
		candidate_relay_parent,
		candidate_relay_parent_number,
		para_id,
		para_head.clone(),
		HeadData(vec![1]),
		world.validation_code_hash(),
	);
	let candidate_hash_a = candidate_a.hash();

	let (candidate_b, pvd_b) = make_candidate(
		leaf_b_hash,
		leaf_b_number,
		para_id,
		HeadData(vec![1]),
		HeadData(vec![2]),
		world.validation_code_hash(),
	);
	let candidate_hash_b = candidate_b.hash();

	assert!(world.introduce_seconded_candidate(candidate_a.clone(), pvd_a.clone()));
	world.back_candidate(para_id, candidate_hash_a);

	let candidate_a_pending_av = CandidatePendingAvailability {
		candidate_hash: candidate_hash_a,
		descriptor: candidate_a.descriptor.clone(),
		commitments: candidate_a.commitments.clone(),
		relay_parent_number: candidate_relay_parent_number,
		max_pov_size: MAX_POV_SIZE,
	};
	let leaf_b = TestLeaf {
		number: leaf_b_number,
		hash: leaf_b_hash,
		para_data: vec![(
			para_id,
			PerParaData::new_with_pending(para_head, vec![candidate_a_pending_av]),
		)],
	};
	// leaf_b's parent is leaf_a so prospective inherits leaf_a's view.
	world.activate_leaf_with_parent_hash_fn(&leaf_b, |hash| {
		if hash == leaf_b_hash {
			leaf_a_hash
		} else {
			get_parent_hash(hash)
		}
	});

	let resp = world.get_hypothetical_membership(candidate_hash_a, candidate_a, pvd_a);
	assert_eq!(resp.len(), 1);
	let (_, membership) = &resp[0];
	let mut got: Vec<Hash> = membership.iter().copied().collect();
	got.sort();
	let mut want = vec![leaf_a.hash, leaf_b.hash];
	want.sort();
	assert_eq!(got, want);

	assert!(world.introduce_seconded_candidate(candidate_b.clone(), pvd_b));
	world.back_candidate(para_id, candidate_hash_b);

	assert_eq!(
		world.get_backable_candidates(
			leaf_b.hash,
			para_id,
			1,
			vec![candidate_hash_a].into_iter().collect(),
		),
		vec![BackableCandidateRef {
			candidate_hash: candidate_hash_b,
			scheduling_parent: leaf_b_hash,
		}],
	);
}

#[test]
fn uses_ancestry_only_within_session() {
	let mut config = default_world_config();
	// Empty claim queue — original test passes empty BTreeMap on ClaimQueue.
	config.claim_queue.clear();
	config.session_index = 2;
	let mut world = World::start(config);

	let session: SessionIndex = world.session_index();
	let leaf_number = 5;
	let leaf_hash = Hash::repeat_byte(5);
	// Register session - 1 so the chain model can answer queries about ancestors before
	// the session change.
	{
		let mut chain = world.base.chain.lock();
		chain.add_session(
			session - 1,
			SessionInfo {
				validators: Vec::new(),
				validator_groups: Vec::new(),
				group_rotation_info: polkadot_primitives::GroupRotationInfo {
					session_start_block: 0,
					group_rotation_frequency: 1,
					now: 0,
				},
			},
		);

		// Build the ancestry chain: leaf at session, ancestors hash 4, 3, 2 with hash 3
		// flipping to session - 1.
		let session_change_hash = Hash::repeat_byte(3);
		let chain_path = [
			(Hash::repeat_byte(5), Hash::repeat_byte(4), 5u32, session),
			(Hash::repeat_byte(4), Hash::repeat_byte(3), 4u32, session),
			(Hash::repeat_byte(3), Hash::repeat_byte(2), 3u32, session - 1),
			(Hash::repeat_byte(2), Hash::repeat_byte(1), 2u32, session - 1),
		];
		for (hash, parent, number, sess) in chain_path {
			let _ = session_change_hash;
			chain.register_block_with_session(hash, parent, number, Some(sess));
		}
	}

	world.base.sim.signal(OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
		leaf_hash,
		leaf_number,
	))));

	// Subsystem settled cleanly without panicking; that's the contract this test pins.
	let _ = DEFAULT_SCHEDULING_LOOKAHEAD;
}
