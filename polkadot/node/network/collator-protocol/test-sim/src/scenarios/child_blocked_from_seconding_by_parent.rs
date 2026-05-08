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

//! Mirrors `validator_side/tests/prospective_parachains.rs::child_blocked_from_seconding_by_parent`.
//!
//! Child B (parent_head=[1]) advertised before parent A (output=[1]). B fetched but
//! blocks on parent.
//!
//! - `valid_parent=true`: parent A advertised + fetched + seconded → B becomes a chain
//!   member and seconds.
//! - `valid_parent=false`: parent A is reported `Invalid` after fetch → B never seconds;
//!   parent's collator gets a Malicious rep hit.

use crate::{
	builders::{Candidate, ProtocolVersion::V2},
	contract::{Effect, RepBucket},
	harness::CollatorSut,
	scenarios::shared::activated_world,
};
use polkadot_node_subsystem::messages::CollatorProtocolMessage;
use polkadot_primitives::{CoreIndex, HeadData, Id as ParaId};
use std::time::Duration;

const PARA: ParaId = ParaId::new(2000);

#[crate::sim_test]
fn child_advertised_first_blocks_then_unblocks_after_parent_seconds<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let leaf = w.leaf();

	let parent = w.candidate_at(leaf)
		.para(PARA).parent_head(HeadData(Vec::new())).head_data(HeadData(vec![1])).build();
	let child = w.candidate_at(leaf)
		.para(PARA).parent_head(HeadData(vec![1])).head_data(HeadData(vec![2])).build();

	let peer = w.declared_peer(PARA, V2);
	w.outputs.insert(child.hash(), child.commitments.clone(), child.pvd.clone());

	// Child first → fetched but seconding held until parent enters fragment chain.
	w.advertise_with_parent_head(&peer, leaf, child.hash(), child.parent_head_hash());
	let child_req = w.fetch_request(&child);
	w.respond_fetch_v2(child_req, child.receipt.clone(), Candidate::empty_pov());

	// Parent flows through full advertise → fetch → second.
	w.full_second(&peer, &parent);

	// Child unblocks.
	w.expect_second(&child);
}

/// Upstream's `valid_parent=false` variant: parent A reported Invalid before being
/// seconded → B never seconded.
///
/// In upstream, backing is mocked and the test explicitly chooses whether to dispatch
/// `send_seconded_statement` or `Invalid`. With **real** backing, validation-stub's "valid"
/// verdict auto-seconds parent and unblocks child before `Invalid` can fire. To preserve
/// the upstream invariant, install `CanSecondStub(false)` so backing never auto-seconds.
#[crate::sim_test]
fn child_remains_blocked_when_parent_reported_invalid<S: CollatorSut>() {
	use crate::{
		chain::CoreSchedule,
		scenarios::shared::{build_with_ancestors_world_with_config, ChainConfig},
	};
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), CoreSchedule::always(PARA))
		.with_can_second_stub(false);
	let mut w = build_with_ancestors_world_with_config::<S>(0, config);
	let leaf = w.leaf();

	let parent = w.candidate_at(leaf)
		.para(PARA).parent_head(HeadData(Vec::new())).head_data(HeadData(vec![1])).build();
	let child = w.candidate_at(leaf)
		.para(PARA).parent_head(HeadData(vec![1])).head_data(HeadData(vec![2])).build();

	let peer = w.declared_peer(PARA, V2);

	// Both ads. CanSecond stub answers false → both held without backing dispatch.
	w.advertise_with_parent_head(&peer, leaf, child.hash(), child.parent_head_hash());
	w.advertise_with_parent_head(&peer, leaf, parent.hash(), parent.parent_head_hash());

	// Drive Invalid signal for parent (upstream test's `valid_parent=false`).
	w.sim
		.send(CollatorProtocolMessage::Invalid(leaf, parent.receipt.clone().into()));

	// Parent never seconded → child never seconded.
	w.sim.expect_no(
		|e| matches!(
			e,
			crate::contract::Effect::SecondCandidate { candidate_hash, .. } if candidate_hash == &child.hash()
		),
		Duration::from_millis(200),
		"SecondCandidate for child after parent reported Invalid (must NOT fire)",
	);
}
