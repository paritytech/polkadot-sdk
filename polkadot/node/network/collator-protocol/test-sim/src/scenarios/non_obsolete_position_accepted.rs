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

//! Scenario: a peer advertises at an ancestor relay parent R (parent of the active leaf L).
//! The claim queue at L is `[A, B, A]`; at offset=1 (R's position relative to L), the
//! valid_len-2 window covers positions [0, 1]. Para A is at position 0 → within window →
//! validator accepts and fires the fetch.
//!
//! Mirrors `validator_side/tests/prospective_parachains.rs::non_obsolete_position_accepted`.
//!
//! EXPECTED-FAILURE NOTE (experimental): the experimental side does not accept
//! advertisements at an ancestor relay parent in the same scope as legacy. Either its
//! `allowed_ancestry` resolution differs or its async-backing window is narrower for the
//! same `AsyncBackingParams::allowed_ancestry_len`. Investigate before merging
//! experimental as default; tooling that expects ancestor-RP advertisements to flow may
//! break.

use crate::{
	builders::{Candidate, Peer, ProtocolVersion},
	contract::{Effect, ReqKind},
	harness::SubsystemUnderTest,
};
use polkadot_node_subsystem::messages::{AllMessages, CollatorProtocolMessage};
use polkadot_primitives::{CoreIndex, HeadData, Id as ParaId};
use std::{collections::VecDeque, time::Duration};

#[crate::sim_test]
fn ancestor_in_view_with_para_at_valid_position_accepts<S>()
where
	S: SubsystemUnderTest<Message = CollatorProtocolMessage>,
	AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	let para_a = ParaId::from(2000);
	let para_b = ParaId::from(999);

	let world =
		crate::scenarios::shared::build_with_ancestors_world::<S>(1, &[(CoreIndex(0), para_a)]);
	let _ = para_b;
	let mut world = world;
	let r = world.ancestors[0];

	let peer = Peer::new(para_a, ProtocolVersion::V2);
	world.sim.send(peer.connected());
	world.sim.send(peer.declare());

	// Advertise at R, the ancestor.
	let candidate = Candidate::for_para_at(para_a, r);
	let parent_head_hash = HeadData(Vec::new()).hash();
	world.sim.send(peer.advertise(r, Some(candidate.hash()), Some(parent_head_hash)));

	let _ = world.sim.expect(
		|effect| {
			matches!(
				effect,
				Effect::SendRequest {
					kind: ReqKind::CollationFetchingV2,
					candidate_hash: Some(c),
					..
				} if c == &candidate.hash()
			)
		},
		Duration::from_millis(500),
		"Effect::SendRequest CollationFetchingV2 for the ancestor-relay-parent advertise",
	);
}
