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

//! Scenario: two peers advertise the same scheduling parent. The validator fetches from the
//! first peer; the test does not respond. After the validator's per-fetch timeout fires
//! (driven by [`MockClock::advance`]), the validator fetches from the second peer.
//!
//! Production constant `MAX_UNSHARED_DOWNLOAD_TIME` is 400ms (100ms under the
//! `fast-test-validator` feature flag). The test advances ~500ms to clear either.
//!
//! [`MockClock::advance`]: crate::runtime::MockClock::advance

use crate::{
	aux::{CandidateBackingAux, ProspectiveParachainsAux},
	builders::{Candidate, Peer, ProtocolVersion},
	chain::{ChainModel, SessionInfo, SharedChain},
	contract::{Effect, ReqKind},
	harness::{LayeredResponder, Sim, SimConfig, SubsystemUnderTest},
};
use polkadot_node_network_protocol::OurView;
use polkadot_node_subsystem::{
	messages::{AllMessages, CollatorProtocolMessage, NetworkBridgeEvent},
	OverseerSignal,
};
use polkadot_node_subsystem_test_helpers::mock::new_leaf;
use polkadot_overseer::ActiveLeavesUpdate;
use polkadot_primitives::{
	CoreIndex, GroupRotationInfo, Id as ParaId, ValidatorIndex,
};
use sc_network_types::PeerId;
use sp_consensus_slots::Slot;
use std::{collections::VecDeque, time::Duration};

#[crate::sim_test]
fn fetch_timeout_advances_to_next_peer<S>()
where
	S: SubsystemUnderTest<Message = CollatorProtocolMessage>,
	AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	let para = ParaId::from(2000);

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
	queue.insert(CoreIndex(0), VecDeque::from_iter(std::iter::repeat(para).take(3)));
	chain.set_claim_queue_at(leaf, queue);
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

	// Two peers, deterministic identities so the test is reproducible.
	let peer_a = Peer::new(para, ProtocolVersion::V2).with_peer_id(PeerId::random());
	let peer_b = Peer::new(para, ProtocolVersion::V2).with_peer_id(PeerId::random());
	let candidate = Candidate::for_para_at(para, leaf);
	let head_hash = candidate.receipt.descriptor.para_head();

	for peer in [&peer_a, &peer_b] {
		sim.send(peer.connected());
		sim.send(peer.declare());
		sim.send(peer.advertise(leaf, Some(candidate.hash()), Some(head_hash)));
	}

	// The validator picks one of the two peers to fetch from. We don't assert which one —
	// just that exactly one fetch is in flight before timeout.
	let first = sim.expect(
		|effect| matches!(effect, Effect::SendRequest { kind: ReqKind::CollationFetchingV2, .. }),
		Duration::from_millis(50),
		"first Effect::SendRequest CollationFetchingV2 from one of the two peers",
	);
	let first_peer = match first {
		Effect::SendRequest { to, .. } => to,
		_ => unreachable!(),
	};
	let other_peer = if first_peer == peer_a.peer_id { peer_b.peer_id } else { peer_a.peer_id };

	// Drop nothing: we deliberately do not respond. Advance time past
	// MAX_UNSHARED_DOWNLOAD_TIME (400ms in production, 100ms with the fast-test-validator
	// feature) so the validator's per-fetch deadline expires.
	sim.advance(Duration::from_millis(500));

	// The validator should now have fired a second fetch — to the *other* peer.
	let _ = sim.expect(
		|effect| matches!(
			effect,
			Effect::SendRequest {
				kind: ReqKind::CollationFetchingV2,
				to,
				..
			} if *to == other_peer
		),
		Duration::from_millis(50),
		"Effect::SendRequest CollationFetchingV2 to the other peer after timeout",
	);
}
