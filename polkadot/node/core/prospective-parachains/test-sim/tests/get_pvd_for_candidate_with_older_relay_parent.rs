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

//! Faithful port of `get_pvd_for_candidate_with_older_relay_parent` rstest cases.
//!
//! The original parameterises on `runtime_api_version` (constraints-only vs the
//! `AncestorRelayParentInfo` API). ChainModel always answers
//! `AncestorRelayParentInfo` with success when the block is registered, so both rstest
//! cases collapse to a single test in the port.

use polkadot_primitives::{
	BlockNumber, HeadData, Hash, Id as ParaId, PersistedValidationData,
	DEFAULT_SCHEDULING_LOOKAHEAD,
};
use polkadot_primitives_test_helpers::make_candidate_v3;
use polkadot_prospective_parachains_test_sim::world::{PerParaData, TestLeaf, TestState, World};

const LEAF_NUMBER: BlockNumber = 100;
const OLDER_RELAY_PARENT_NUMBER: BlockNumber =
	LEAF_NUMBER - 4 * DEFAULT_SCHEDULING_LOOKAHEAD;
const MAX_POV_SIZE: u32 = 1_000_000;

#[test]
fn get_pvd_for_candidate_with_older_relay_parent() {
	let para_id = ParaId::from(1);
	let mut test_state = TestState::default();
	test_state.min_relay_parent_number_override = Some(OLDER_RELAY_PARENT_NUMBER);
	let mut world = World::start(&test_state);

	let leaf_a = TestLeaf {
		number: LEAF_NUMBER,
		hash: Hash::from_low_u64_be(1 << 20),
		para_data: vec![
			(para_id, PerParaData::new(HeadData(vec![1, 2, 3]))),
			(ParaId::from(2), PerParaData::new(HeadData(vec![2, 3, 4]))),
		],
	};
	world.activate_leaf(&leaf_a, &test_state);

	let older_relay_parent = Hash::from_low_u64_be(9999);
	{
		let mut chain = world.chain.lock();
		chain.register_block_with_session(
			older_relay_parent,
			Hash::zero(),
			OLDER_RELAY_PARENT_NUMBER,
			Some(test_state.session_index),
		);
	}

	let (candidate_a, pvd_a) = make_candidate_v3(
		older_relay_parent,
		OLDER_RELAY_PARENT_NUMBER,
		leaf_a.hash,
		para_id,
		HeadData(vec![1, 2, 3]),
		HeadData(vec![1]),
		test_state.validation_code_hash,
	);
	assert!(world.introduce_seconded_candidate(candidate_a, pvd_a));

	let pvd = world.get_pvd(para_id, older_relay_parent, HeadData(vec![1]), test_state.session_index);
	assert_eq!(
		pvd,
		Some(PersistedValidationData {
			parent_head: HeadData(vec![1]),
			relay_parent_number: OLDER_RELAY_PARENT_NUMBER,
			relay_parent_storage_root: Hash::zero(),
			max_pov_size: MAX_POV_SIZE,
		}),
	);
}
