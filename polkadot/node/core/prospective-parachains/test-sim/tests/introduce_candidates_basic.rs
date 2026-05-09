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

//! Faithful port of the in-crate `introduce_candidates_basic` test from
//! `polkadot/node/core/prospective-parachains/src/tests.rs`.
//!
//! Differences from the original:
//! - The original asserts on `view.active_leaves.len()` (internal subsystem state). The
//!   port asserts the equivalent observable property: `GetBackableCandidates` on each
//!   activated leaf returns the matching candidate.
//! - The original constructs candidates with `validation_code_hash =
//!   Hash::repeat_byte(42)` and overrides nothing on the chain side. ChainModel's default
//!   backing constraints expect `dummy_validation_code().hash()`; the port uses the
//!   default hash so candidates pass the constraint check (the test's intent —
//!   "subsystem accepts and tracks introduced candidates" — is preserved).
//! - The original uses leaf hashes 130, 131, 132 — but `get_parent_hash(h) = h + 1`, so
//!   leaf_a's ancestors collide with leaf_b/leaf_c's identities. Production
//!   prospective-parachains tolerates this because `handle_leaf_activation` mocks each
//!   leaf's ancestor walk independently. ChainModel maintains a single coherent graph and
//!   panics on inconsistent ancestry. The port spreads leaf hashes by `1 << 20` so each
//!   leaf's ancestor walk has its own private chain.

use polkadot_node_subsystem::messages::{Ancestors, BackableCandidateRef};
use polkadot_primitives::{CoreIndex, HeadData, Hash, Id as ParaId};
use polkadot_primitives_test_helpers::make_candidate;
use polkadot_prospective_parachains_test_sim::world::{WorldExt as _, PerParaData, TestLeaf, TestState, World};
use std::collections::{BTreeMap, VecDeque};

#[test]
fn introduce_candidates_basic() {
	let mut test_state = TestState::default();

	let chain_a = ParaId::from(1);
	let chain_b = ParaId::from(2);
	let mut claim_queue: BTreeMap<CoreIndex, VecDeque<ParaId>> = BTreeMap::new();
	claim_queue.insert(CoreIndex(0), [chain_a, chain_b].into_iter().collect());
	test_state.claim_queue = claim_queue;

	let mut world = World::start(&test_state);

	let leaf_a = TestLeaf {
		number: 100,
		hash: Hash::from_low_u64_be(1 << 20),
		para_data: vec![
			(chain_a, PerParaData::new(HeadData(vec![1, 2, 3]))),
			(chain_b, PerParaData::new(HeadData(vec![2, 3, 4]))),
		],
	};
	let leaf_b = TestLeaf {
		number: 101,
		hash: Hash::from_low_u64_be(2 << 20),
		para_data: vec![
			(chain_a, PerParaData::new(HeadData(vec![3, 4, 5]))),
			(chain_b, PerParaData::new(HeadData(vec![4, 5, 6]))),
		],
	};
	let leaf_c = TestLeaf {
		number: 102,
		hash: Hash::from_low_u64_be(3 << 20),
		para_data: vec![
			(chain_a, PerParaData::new(HeadData(vec![5, 6, 7]))),
			(chain_b, PerParaData::new(HeadData(vec![6, 7, 8]))),
		],
	};

	world.activate_leaf(&leaf_a, &test_state);
	world.activate_leaf(&leaf_b, &test_state);
	world.activate_leaf(&leaf_c, &test_state);

	let (candidate_a1, pvd_a1) = make_candidate(
		leaf_a.hash,
		leaf_a.number,
		chain_a,
		HeadData(vec![1, 2, 3]),
		HeadData(vec![1]),
		test_state.validation_code_hash,
	);
	let candidate_hash_a1 = candidate_a1.hash();
	let response_a1 = vec![BackableCandidateRef {
		candidate_hash: candidate_hash_a1,
		scheduling_parent: leaf_a.hash,
	}];

	let (candidate_a2, pvd_a2) = make_candidate(
		leaf_a.hash,
		leaf_a.number,
		chain_b,
		HeadData(vec![2, 3, 4]),
		HeadData(vec![2]),
		test_state.validation_code_hash,
	);
	let candidate_hash_a2 = candidate_a2.hash();
	let response_a2 = vec![BackableCandidateRef {
		candidate_hash: candidate_hash_a2,
		scheduling_parent: leaf_a.hash,
	}];

	let (candidate_b, pvd_b) = make_candidate(
		leaf_b.hash,
		leaf_b.number,
		chain_a,
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
		chain_b,
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

	world.back_candidate(chain_a, candidate_hash_a1);
	world.back_candidate(chain_b, candidate_hash_a2);
	world.back_candidate(chain_a, candidate_hash_b);
	world.back_candidate(chain_b, candidate_hash_c);

	assert_eq!(
		world.get_backable_candidates(leaf_a.hash, chain_a, 5, Ancestors::default()),
		response_a1,
	);
	assert_eq!(
		world.get_backable_candidates(leaf_a.hash, chain_b, 5, Ancestors::default()),
		response_a2,
	);
	assert_eq!(
		world.get_backable_candidates(leaf_b.hash, chain_a, 5, Ancestors::default()),
		response_b,
	);
	assert_eq!(
		world.get_backable_candidates(leaf_c.hash, chain_b, 5, Ancestors::default()),
		response_c,
	);

	// Cross-leaf membership checks: each candidate is *only* known under its activating leaf.
	assert_eq!(
		world.get_backable_candidates(leaf_b.hash, chain_b, 5, Ancestors::default()),
		Vec::<BackableCandidateRef>::new(),
	);
	assert_eq!(
		world.get_backable_candidates(leaf_c.hash, chain_a, 5, Ancestors::default()),
		Vec::<BackableCandidateRef>::new(),
	);

	// All three leaves were activated successfully — the original test asserted this via
	// `view.active_leaves.len() == 3`. With access only to the public surface, the proof
	// that all three are active is that each leaf-keyed `GetBackableCandidates` query
	// returned the right candidate (would return empty for an unknown leaf).
	assert_eq!(world.base.leaves.len(), 3);
}
