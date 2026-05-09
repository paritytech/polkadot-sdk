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

//! Mirrors `validator_side/tests/prospective_parachains.rs::group_rotation_uses_correct_core_per_relay_parent`.
//!
//! 3 cores + 3 groups + rotation_frequency=1. Group 0 (the validator) cycles cores across
//! blocks. At each leaf, an advertisement for the para assigned to group 0's current core
//! triggers a fetch.
//!
//! Setup: para A on core 0, para B on core 2. Two active leaves at sequential blocks where
//! group 0 is on core 0 then core 2.

use crate::scenarios::shared::WorldExt as _;
use crate::{
	builders::ProtocolVersion::V2,
	chain::CoreSchedule,
	harness::CollatorSut,
	scenarios::shared::{build_multi_leaf_world_with_config, ChainConfig},
};
use polkadot_primitives::{CoreIndex, Id as ParaId, ValidatorIndex};

const PARA_A: ParaId = ParaId::new(2000);
const PARA_B: ParaId = ParaId::new(2001);

/// KNOWN BUG (experimental): does not honor per-block group rotation when computing
/// "is this core mine at this RP" — neither candidate gets fetched. See
/// `memory:project_collator_experimental_group_rotation_per_rp`.
#[crate::sim_test(
	bug_on = "experimental",
	bug_url = "memory:project_collator_experimental_group_rotation_per_rp"
)]
fn group_rotation_uses_correct_core_per_relay_parent<S: CollatorSut>() {
	// 3 validator groups; validator (Alice = idx 0) is in group 0.
	// `GroupRotationInfo::group_for_core(c, 3)` at `now=N` returns `(c + N) mod 3`.
	// We (group 0) own core c iff `(c + N) mod 3 == 0`, i.e. `c == (3 - N mod 3) mod 3`.
	// - block 1 (leaves[0]): we own core 2 (para B is here)
	// - block 2 (leaves[1]): we own core 1 (no para scheduled — gap)
	// - block 3 (leaves[2]): we own core 0 (para A is here)
	let validator_groups =
		vec![vec![ValidatorIndex(0)], vec![ValidatorIndex(1)], vec![ValidatorIndex(2)]];
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), CoreSchedule::always(PARA_A))
		.with_schedule(CoreIndex(2), CoreSchedule::always(PARA_B))
		.with_validator_groups(validator_groups)
		.with_group_rotation_frequency(1);
	let mut w = build_multi_leaf_world_with_config::<S>(3, config);

	let block1 = w.base.leaves[0].hash; // group 0 → core 2 → PARA_B
	let block3 = w.base.leaves[2].hash; // group 0 → core 0 → PARA_A

	let peer_a = w.declared_peer(PARA_A, V2);
	let peer_b = w.declared_peer(PARA_B, V2);

	// Correct pairing: A@block3, B@block1. Both should fetch.
	let cand_a = w.advertise(&peer_a, block3, PARA_A);
	let cand_b = w.advertise(&peer_b, block1, PARA_B);

	let _ = w.fetch_request(&cand_a);
	let _ = w.fetch_request(&cand_b);

	// Incorrect pairing: A@block1, B@block3. Neither core is ours at the wrong leaf.
	let cand_a_at_b1 = w.advertise(&peer_a, block1, PARA_A);
	let cand_b_at_b3 = w.advertise(&peer_b, block3, PARA_B);
	w.no_fetch_for(&cand_a_at_b1, std::time::Duration::from_millis(150));
	w.no_fetch_for(&cand_b_at_b3, std::time::Duration::from_millis(50));
}
