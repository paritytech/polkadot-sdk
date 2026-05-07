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

//! Scenario: a peer connects but never sends a `Declare`. After the eviction policy's
//! `undeclared` window elapses, the validator disconnects the peer.

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
fn peer_disconnected_when_undeclared_window_elapses<S>()
where
	S: SubsystemUnderTest<Message = CollatorProtocolMessage>,
	AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	let para = ParaId::from(2000);
	let mut world = crate::scenarios::shared::activated_world::<S>(&[(CoreIndex(0), para)]);

	let peer = Peer::new(para, ProtocolVersion::V1);
	world.sim.send(peer.connected());

	// Production CollatorEvictionPolicy::undeclared defaults to 1s. Advance ~1.5s to clear.
	world.sim.advance(Duration::from_millis(1500));

	let _ = world.sim.expect(
		|effect| matches!(
			effect,
			Effect::DisconnectPeers { peers, peer_set: PeerSet::Collation } if peers.contains(&peer.peer_id),
		),
		Duration::from_millis(50),
		"Effect::DisconnectPeers for the never-declared peer",
	);
}

/// Sanity counterpart: a peer that DOES declare on time stays connected past the
/// undeclared window (declare resets the lifecycle). Pairs with the no-declare test to
/// confirm the test setup itself isn't trivially disconnecting peers.
#[crate::sim_test]
fn declared_peer_not_disconnected_when_undeclared_window_elapses<S>()
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

	// Past the 1s undeclared window. Peer is already declared, so eviction shouldn't fire.
	world.sim.advance(Duration::from_millis(1500));

	let disconnected = world.sim.recorder().entries().iter().any(|o| match o {
		crate::harness::Observation::Effect(s) => matches!(
			&s.value,
			Effect::DisconnectPeers { peers, peer_set: PeerSet::Collation } if peers.contains(&peer.peer_id),
		),
	});
	assert!(
		!disconnected,
		"declared peer must NOT be disconnected on undeclared-window timer\n\n{}",
		crate::report::format_timeline(world.sim.recorder()),
	);
}
