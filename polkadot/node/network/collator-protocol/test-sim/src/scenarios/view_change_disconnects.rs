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

//! Scenario: a peer declares as a collator while a leaf is active, then the validator's
//! view changes to empty. The validator disconnects the now-irrelevant peer.

use crate::{
	builders::{Peer, ProtocolVersion},
	contract::Effect,
	harness::SubsystemUnderTest,
};
use polkadot_node_network_protocol::{peer_set::PeerSet, OurView};
use polkadot_node_subsystem::messages::{
	AllMessages, CollatorProtocolMessage, NetworkBridgeEvent,
};
use polkadot_primitives::{CoreIndex, Id as ParaId};
use std::time::Duration;

#[crate::sim_test]
fn empty_view_disconnects_declared_peer<S>()
where
	S: SubsystemUnderTest<Message = CollatorProtocolMessage>,
	AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	let para = ParaId::from(2000);
	let mut world = crate::scenarios::shared::activated_world::<S>(&[(CoreIndex(0), para)]);

	let peer = Peer::new(para, ProtocolVersion::V1);
	world.sim.send(peer.connected());
	world.sim.send(peer.declare());

	// Empty-view update — no leaves at all.
	world.sim.send(CollatorProtocolMessage::NetworkBridgeUpdate(
		NetworkBridgeEvent::OurViewChange(OurView::new(std::iter::empty(), 0)),
	));

	let _ = world.sim.expect(
		|effect| matches!(
			effect,
			Effect::DisconnectPeers { peers, peer_set: PeerSet::Collation } if peers.contains(&peer.peer_id),
		),
		Duration::from_millis(100),
		"Effect::DisconnectPeers for the declared peer after view becomes empty",
	);
}
