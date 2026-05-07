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

//! Scenario: peer_a is being fetched. Peer_b also advertises (queued behind peer_a).
//! Peer_b disconnects. Peer_a's fetch resolves and its candidate is seconded. After
//! settling, the validator should NOT fire a fetch for peer_b — the disconnect cleaned
//! peer_b's queued advertisement, so when the next fetch slot opens (which the legacy
//! impl does after seconding — see project_collator_legacy_spurious_fetch_after_second
//! note) it has nothing for peer_b to fetch.

use crate::{
	builders::{Candidate, Peer, ProtocolVersion},
	contract::{Effect, ReqKind},
	harness::SubsystemUnderTest,
};
use codec::Encode;
use polkadot_node_network_protocol::request_response::v1 as protocol_v1;
use polkadot_node_primitives::{BlockData, PoV};
use polkadot_node_subsystem::messages::{
	AllMessages, CollatorProtocolMessage, NetworkBridgeEvent,
};
use polkadot_primitives::{
	CoreIndex, HeadData, Id as ParaId, MutateDescriptorV2, PersistedValidationData,
};
use sc_network::ProtocolName;
use std::time::Duration;

#[crate::sim_test]
fn disconnect_clears_queued_advertisement<S>()
where
	S: SubsystemUnderTest<Message = CollatorProtocolMessage>,
	AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	let para = ParaId::from(2000);
	let mut world = crate::scenarios::shared::activated_world::<S>(&[(CoreIndex(0), para)]);

	let pvd = PersistedValidationData {
		parent_head: HeadData(Vec::new()),
		relay_parent_number: world.chain.lock().block(&world.leaf).unwrap().number,
		relay_parent_storage_root: polkadot_primitives::Hash::zero(),
		max_pov_size: 5 * 1024 * 1024,
	};
	let mut candidate = Candidate::for_para_at(para, world.leaf);
	candidate.receipt.descriptor.set_persisted_validation_data_hash(pvd.hash());

	let peer_a = Peer::new(para, ProtocolVersion::V1);
	let peer_b = Peer::new(para, ProtocolVersion::V1);

	world.sim.send(peer_a.connected());
	world.sim.send(peer_a.declare());
	world.sim.send(peer_a.advertise(world.leaf, None, None));

	// First fetch goes to peer_a (only declared peer so far).
	let first = world.sim.expect(
		|effect| matches!(effect, Effect::SendRequest { kind: ReqKind::CollationFetchingV1, .. }),
		Duration::from_millis(100),
		"first Effect::SendRequest CollationFetchingV1",
	);
	let request_id = first.request_id().unwrap();

	// peer_b joins, declares, advertises — queued behind peer_a.
	world.sim.send(peer_b.connected());
	world.sim.send(peer_b.declare());
	world.sim.send(peer_b.advertise(world.leaf, None, None));

	// peer_b disconnects.
	world.sim.send(CollatorProtocolMessage::NetworkBridgeUpdate(
		NetworkBridgeEvent::PeerDisconnected(peer_b.peer_id),
	));

	// peer_a fetch resolves with a valid response.
	let pov = PoV { block_data: BlockData(vec![]) };
	let response = protocol_v1::CollationFetchingResponse::Collation(
		candidate.receipt.clone().into(),
		pov,
	);
	world
		.sim
		.respond_fetch(request_id, Ok((response.encode(), ProtocolName::from(""))));

	let _ = world.sim.expect(
		|effect| matches!(
			effect,
			Effect::SecondCandidate { candidate_hash, .. } if candidate_hash == &candidate.hash()
		),
		Duration::from_millis(500),
		"Effect::SecondCandidate after peer_a's fetch",
	);

	world.sim.expect_no(
		|e| matches!(e, Effect::SendRequest { to, .. } if *to == peer_b.peer_id),
		Duration::from_millis(100),
		"SendRequest targeting peer_b after peer_b disconnected its advertisement",
	);
}
