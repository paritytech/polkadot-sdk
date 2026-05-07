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

//! Scenario: a peer declares as a collator for a para that isn't on any of our assigned
//! cores. The validator drops the connection.
//!
//! The chain model has one leaf with an empty claim queue: nothing is scheduled, so any
//! para the peer declares for is unneeded. Both implementations need an ActiveLeaves
//! preamble to leave their startup-init guard.

use crate::{
	aux::{CandidateBackingAux, ProspectiveParachainsAux},
	builders::{Peer, ProtocolVersion},
	chain::{ChainModel, SessionInfo, SharedChain},
	contract::Effect,
	harness::{LayeredResponder, Sim, SimConfig, SubsystemUnderTest},
};
use polkadot_node_network_protocol::{peer_set::PeerSet, OurView};
use polkadot_node_subsystem::{
	messages::{AllMessages, CollatorProtocolMessage, NetworkBridgeEvent},
	OverseerSignal,
};
use polkadot_node_subsystem_test_helpers::mock::new_leaf;
use polkadot_overseer::ActiveLeavesUpdate;
use polkadot_primitives::{GroupRotationInfo, Id as ParaId, ValidatorIndex};
use sp_consensus_slots::Slot;
use std::time::Duration;

#[crate::sim_test]
fn declare_for_unneeded_para_disconnects_peer<S>()
where
	S: SubsystemUnderTest<Message = CollatorProtocolMessage>,
	AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	let mut chain = ChainModel::new(Slot::from(0));
	chain.add_session(
		0,
		SessionInfo {
			validators: crate::builders::fixtures::default_validators(),
			validator_groups: vec![vec![ValidatorIndex(0), ValidatorIndex(1)]],
			group_rotation_info: GroupRotationInfo {
				session_start_block: 0,
				group_rotation_frequency: 1,
				now: 0,
			},
		},
	);
	let leaf = chain.extend(chain.genesis());
	let leaf_number = chain.block(&leaf).unwrap().number;

	let chain = SharedChain::new(chain);
	let mut responder = LayeredResponder::new();
	responder.push(chain.clone());
	responder.push(crate::scenarios::shared::PanicResponder);

	let mut sim = Sim::<S>::start(SimConfig::default(), responder);
	let (psp, psp_rx) = ProspectiveParachainsAux::spawn(&mut sim);
	let (cb, cb_rx) = CandidateBackingAux::spawn(&mut sim);
	sim.register_aux(psp, psp_rx);
	sim.register_aux(cb, cb_rx);

	sim.signal(OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
		leaf,
		leaf_number,
	))));
	sim.send(CollatorProtocolMessage::NetworkBridgeUpdate(NetworkBridgeEvent::OurViewChange(
		OurView::new(std::iter::once(leaf), 0),
	)));

	let peer = Peer::new(ParaId::from(2000), ProtocolVersion::V1);

	sim.send(peer.connected());
	sim.send(peer.declare());

	let observed = sim.expect(
		|effect| matches!(effect, Effect::DisconnectPeers { peers, peer_set: PeerSet::Collation } if peers.contains(&peer.peer_id)),
		Duration::from_millis(100),
		"Effect::DisconnectPeers containing the unneeded-para peer",
	);
	match observed {
		Effect::DisconnectPeers { peers, peer_set } => {
			assert!(peers.contains(&peer.peer_id));
			assert_eq!(peer_set, PeerSet::Collation);
		},
		other => panic!("predicate matched but variant unexpected: {:?}", other),
	}
}
