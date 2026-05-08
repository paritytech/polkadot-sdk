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

//! Tests covering invariants that PR #11967 (rotation bug fix + capacity-tracking
//! simplification) introduces. Marked `bug_on = "experimental"` because the assertions
//! fail against pre-#11967 experimental — merging the PR flips the `should_panic`,
//! turns the test red, and prompts removal of the marker.
//!
//! Upstream PR: https://github.com/paritytech/polkadot-sdk/pull/11967
//!
//! Coverage:
//! - [`core_rotation_accepts_candidates_for_both_cores`] — under group rotation, an
//!   advertisement at an ancestor whose owned core differs from the leaf's owned core
//!   must still be accepted. Pre-#11967, the rotation moved the validator's ownership
//!   bookkeeping forward and orphaned the ancestor's advertisements.

use crate::{
	builders::ProtocolVersion::V2,
	chain::CoreSchedule,
	harness::CollatorSut,
	scenarios::shared::{build_multi_leaf_world_with_config, ChainConfig},
};
use polkadot_primitives::{CoreIndex, Id as ParaId, ValidatorIndex};

const PARA_A: ParaId = ParaId::new(100);
const PARA_B: ParaId = ParaId::new(600);

/// Group rotation: at leaf 1 (block 1) we own core 2 (PARA_A); at leaf 2 (block 2) we
/// own core 1 (PARA_B). After rotating to leaf 2 a new advertisement for PARA_A at the
/// (now-ancestor) leaf 1 must still fetch — the leaf 1 core's CQ slots are not
/// cancelled by the rotation.
///
/// Pre-#11967: advertisement at the old core silently dropped. Post-#11967: accepted.
#[crate::sim_test(
	bug_on = "experimental",
	bug_url = "github:paritytech/polkadot-sdk#11967"
)]
fn core_rotation_accepts_candidates_for_both_cores<S: CollatorSut>() {
	// 3 validator groups. With `group_rotation_frequency=1` and
	// `group_for_core(c, 3)` at `now=N` returning `(c + N) mod 3`, group 0 owns core
	// `c` iff `(c + N) mod 3 == 0`, i.e. `c == (3 - N mod 3) mod 3`.
	// - block 1: own core 2 (PARA_A)
	// - block 2: own core 1 (PARA_B)
	let validator_groups =
		vec![vec![ValidatorIndex(0)], vec![ValidatorIndex(1)], vec![ValidatorIndex(2)]];
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(2), CoreSchedule::always(PARA_A))
		.with_schedule(CoreIndex(1), CoreSchedule::always(PARA_B))
		.with_validator_groups(validator_groups)
		.with_group_rotation_frequency(1);
	let mut w = build_multi_leaf_world_with_config::<S>(2, config);

	let leaf_1 = w.leaves[0].hash; // we own core 2 → PARA_A
	let leaf_2 = w.leaves[1].hash; // we own core 1 → PARA_B

	let peer_a = w.declared_peer(PARA_A, V2);
	let cand_a = w.advertise(&peer_a, leaf_1, PARA_A);
	let _ = w.fetch_request(&cand_a);

	let peer_b = w.declared_peer(PARA_B, V2);
	let cand_b = w.advertise(&peer_b, leaf_2, PARA_B);
	let _ = w.fetch_request(&cand_b);

	// New PARA_A advertisement at the now-ancestor leaf 1: the rotation's owned-core
	// shift must not have orphaned leaf 1's CQ slot. Pre-#11967 silently drops; post-
	// #11967 fetches.
	let peer_a2 = w.declared_peer(PARA_A, V2);
	let cand_a2 = w.advertise(&peer_a2, leaf_1, PARA_A);
	let _ = w.fetch_request(&cand_a2);
}
