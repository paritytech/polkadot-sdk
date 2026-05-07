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

//! Scenario: a peer declares for a para that is *not* in the validator's claim queue, even
//! though the validator has scheduled paras. The validator disconnects the peer.

use crate::{
	builders::{Peer, ProtocolVersion},
	contract::Effect,
	harness::SubsystemUnderTest,
};
use polkadot_node_network_protocol::peer_set::PeerSet;
use polkadot_node_subsystem::messages::{AllMessages, CollatorProtocolMessage};
use polkadot_primitives::{CoreIndex, Id as ParaId};
use std::time::Duration;

#[crate::sim_test]
fn peer_disconnected_after_declaring_for_wrong_para<S>()
where
	S: SubsystemUnderTest<Message = CollatorProtocolMessage>,
	AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	let scheduled = ParaId::from(2000);
	let wrong = ParaId::from(3000);
	let mut world =
		crate::scenarios::shared::activated_world::<S>(&[(CoreIndex(0), scheduled)]);

	// Peer declares for a para that's not in the claim queue.
	let peer = Peer::new(wrong, ProtocolVersion::V1);
	world.sim.send(peer.connected());
	world.sim.send(peer.declare());

	let _ = world.sim.expect(
		|effect| matches!(
			effect,
			Effect::DisconnectPeers { peers, peer_set: PeerSet::Collation } if peers.contains(&peer.peer_id),
		),
		Duration::from_millis(100),
		"Effect::DisconnectPeers for the wrongly-declared peer",
	);
}

/// Sanity counterpart: same setup, but the peer declares for the *scheduled* para. The
/// validator must NOT disconnect. This pairs with `peer_disconnected_after_declaring_for_wrong_para`
/// to rule out "the test setup itself triggers a disconnect" as a false positive.
#[crate::sim_test]
fn peer_with_correct_declare_is_not_disconnected<S>()
where
	S: SubsystemUnderTest<Message = CollatorProtocolMessage>,
	AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	let scheduled = ParaId::from(2000);
	let mut world =
		crate::scenarios::shared::activated_world::<S>(&[(CoreIndex(0), scheduled)]);

	let peer = Peer::new(scheduled, ProtocolVersion::V1);
	world.sim.send(peer.connected());
	world.sim.send(peer.declare());

	world.sim.advance(Duration::from_millis(200));

	let disconnected = world.sim.recorder().entries().iter().any(|o| match o {
		crate::harness::Observation::Effect(s) => matches!(
			&s.value,
			Effect::DisconnectPeers { peers, peer_set: PeerSet::Collation } if peers.contains(&peer.peer_id),
		),
	});
	assert!(
		!disconnected,
		"peer that correctly declares for a scheduled para must NOT be disconnected\n\n{}",
		crate::report::format_timeline(world.sim.recorder()),
	);
}
