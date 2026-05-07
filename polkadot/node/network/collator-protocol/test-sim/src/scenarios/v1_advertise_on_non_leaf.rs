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

//! Scenario: V1 advertisements must target the active leaf, not its ancestor. A V1
//! advertisement at L's parent is treated as protocol misuse — the validator emits
//! `Reputation::Malicious`.
//!
//! Mirrors `validator_side/tests/prospective_parachains.rs::v1_advertisement_rejected_on_non_active_leaf`.
//!
//! EXPECTED-FAILURE NOTE (experimental): drops the V1-on-non-leaf advertisement silently;
//! no Reputation::Malicious event. Same bus-silent-rejection theme as
//! project_collator_experimental_no_invalid_reputation_event.md.

use crate::{
	builders::{Peer, ProtocolVersion},
	contract::{Effect, RepBucket},
	harness::SubsystemUnderTest,
};
use polkadot_node_subsystem::messages::{AllMessages, CollatorProtocolMessage};
use polkadot_primitives::{CoreIndex, Id as ParaId};
use std::time::Duration;

#[crate::sim_test]
fn v1_advertisement_at_parent_of_leaf_is_protocol_misuse<S>()
where
	S: SubsystemUnderTest<Message = CollatorProtocolMessage>,
	AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	let para = ParaId::from(2000);
	let world = crate::scenarios::shared::build_with_ancestors_world::<S>(
		1,
		&[(CoreIndex(0), para)],
	);
	let mut world = world;

	let parent = world.ancestors[0];

	let peer = Peer::new(para, ProtocolVersion::V1);
	world.sim.send(peer.connected());
	world.sim.send(peer.declare());
	world.sim.send(peer.advertise(parent, None, None));

	let _ = world.sim.expect(
		|effect| matches!(
			effect,
			Effect::Reputation { peer: p, bucket: RepBucket::Malicious } if *p == peer.peer_id,
		),
		Duration::from_millis(100),
		"Effect::Reputation { Malicious } for V1 advertise at non-leaf relay parent",
	);
}
