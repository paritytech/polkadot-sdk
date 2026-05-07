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

//! Scenario: a peer declares for a para that is not in any of the validator's assigned
//! cores' claim queue. The validator drops the connection. The reputation hit is non-malicious
//! (CostMinor) and is only flushed on the periodic reputation tick — this scenario asserts on
//! the immediate `DisconnectPeers` effect, not the deferred reputation change.
//!
//! Distinct from `unneeded_para`: this version sets up a *real* chain (with claim queue
//! containing some para X) and the peer declares for a *different* para Y. Exercises the
//! validator-side path that runs view-update first, then sees the unrelated declaration.

use crate::{
	aux::{CandidateBackingAux, ProspectiveParachainsAux},
	builders::{Peer, ProtocolVersion},
	chain::{ChainModel, SessionInfo, SharedChain},
	contract::Effect,
	harness::{LayeredResponder, Sim, SimConfig},
	impls::LegacyValidator,
};
use polkadot_node_network_protocol::{peer_set::PeerSet, OurView};
use polkadot_node_subsystem::{
	messages::{CollatorProtocolMessage, NetworkBridgeEvent},
	OverseerSignal,
};
use polkadot_node_subsystem_test_helpers::mock::new_leaf;
use polkadot_overseer::ActiveLeavesUpdate;
use polkadot_primitives::{
	CoreIndex, GroupRotationInfo, Id as ParaId, ValidatorIndex,
};
use sp_consensus_slots::Slot;
use std::{collections::VecDeque, time::Duration};

#[crate::sim_test]
fn declare_for_unscheduled_para_disconnects_peer() {
	let scheduled_para = ParaId::from(2000);
	let unscheduled_para = ParaId::from(3000);

	let mut chain = ChainModel::new(Slot::from(100));
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
	let mut queue = std::collections::BTreeMap::new();
	queue.insert(
		CoreIndex(0),
		VecDeque::from_iter(std::iter::repeat(scheduled_para).take(3)),
	);
	chain.set_claim_queue_at(leaf, queue);
	let leaf_number = chain.block(&leaf).unwrap().number;

	let chain = SharedChain::new(chain);
	let mut responder = LayeredResponder::new();
	responder.push(chain.clone());
	responder.push(crate::scenarios::shared::PanicResponder);

	let mut sim = Sim::<LegacyValidator>::start(SimConfig::default(), responder);
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

	// Peer declares for a para NOT in the claim queue.
	let peer = Peer::new(unscheduled_para, ProtocolVersion::V2);
	sim.send(peer.connected());
	sim.send(peer.declare());

	let _ = sim.expect(
		|effect| matches!(effect, Effect::DisconnectPeers { peers, peer_set: PeerSet::Collation } if peers.contains(&peer.peer_id)),
		Duration::from_millis(100),
		"Effect::DisconnectPeers containing the unscheduled-para peer",
	);
}
