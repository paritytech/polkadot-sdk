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

//! Scenario: a declared collator that keeps advertising at intervals shorter than the
//! `inactive_collator` policy window stays connected. Once it stops advertising, the
//! validator disconnects after the policy window.

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
fn activity_keeps_peer_alive_then_disconnects_when_silent<S>()
where
	S: SubsystemUnderTest<Message = CollatorProtocolMessage>,
	AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	let para = ParaId::from(2000);
	// Three leaves in view so we can vary advertise relay parent across leaves, mirroring
	// the upstream test's intent of "activity on any in-view leaf counts."
	let mut world = crate::scenarios::shared::build_multi_leaf_world::<S>(
		3,
		&[(CoreIndex(0), para)],
	);

	let peer = Peer::new(para, ProtocolVersion::V1);
	world.sim.send(peer.connected());
	world.sim.send(peer.declare());

	// Production CollatorEvictionPolicy::inactive_collator = 24s. Step in 16s chunks
	// (~2/3 of the window) and advertise on a different leaf each step. Each
	// advertisement should reset the activity timer.
	let step = Duration::from_secs(16);

	world.sim.advance(step);
	world.sim.send(peer.advertise(world.leaves[0], None, None));

	world.sim.advance(step);
	world.sim.send(peer.advertise(world.leaves[1], None, None));

	world.sim.advance(step);
	world.sim.send(peer.advertise(world.leaves[2], None, None));

	// At this point ~48s have elapsed but the peer has been continuously advertising —
	// no DisconnectPeers effect targeting the peer should be observed yet.
	world.sim.expect_count(
		|e| matches!(
			e,
			Effect::DisconnectPeers { peers, peer_set: PeerSet::Collation } if peers.contains(&peer.peer_id),
		),
		0,
		"DisconnectPeers targeting the actively-advertising peer (must be zero so far)",
	);

	// Stop advertising. Advance well past the inactive_collator window.
	world.sim.advance(Duration::from_secs(36));

	let _ = world.sim.expect(
		|effect| matches!(
			effect,
			Effect::DisconnectPeers { peers, peer_set: PeerSet::Collation } if peers.contains(&peer.peer_id),
		),
		Duration::from_secs(2),
		"Effect::DisconnectPeers for the collator after it falls silent",
	);
}
