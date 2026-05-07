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

//! Scenario: two peers advertise. The validator fetches and seconds the first peer's
//! candidate. Backing later reports the candidate as `Invalid` — the validator penalises
//! the offending peer (malicious reputation hit) and fetches the next queued advertisement.
//!
//! EXPECTED-FAILURE NOTE (experimental): the experimental side fetches the next peer but
//! does NOT emit `Effect::Reputation { Malicious }` for the offending peer. Reputation
//! handling is fundamentally different in experimental (persistent reputation store; no
//! per-event Malicious bus traffic to NetworkBridge). Captured as a divergence.

use crate::{
	builders::{Candidate, Peer, ProtocolVersion},
	contract::{Effect, RepBucket, ReqKind},
	harness::SubsystemUnderTest,
};
use codec::Encode;
use polkadot_node_network_protocol::request_response::v1 as protocol_v1;
use polkadot_node_primitives::{BlockData, PoV};
use polkadot_node_subsystem::messages::{AllMessages, CollatorProtocolMessage};
use polkadot_primitives::{
	CoreIndex, HeadData, Id as ParaId, MutateDescriptorV2, PersistedValidationData,
};
use sc_network::ProtocolName;
use std::time::Duration;

#[crate::sim_test]
fn invalid_signal_penalises_peer_and_fetches_next<S>()
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

	let peer_b = Peer::new(para, ProtocolVersion::V1);
	let peer_c = Peer::new(para, ProtocolVersion::V1);
	for peer in [&peer_b, &peer_c] {
		world.sim.send(peer.connected());
		world.sim.send(peer.declare());
	}
	for peer in [&peer_b, &peer_c] {
		world.sim.send(peer.advertise(world.leaf, None, None));
	}

	// Fetch from one of them (whichever wins).
	let first = world.sim.expect(
		|effect| matches!(effect, Effect::SendRequest { kind: ReqKind::CollationFetchingV1, .. }),
		Duration::from_millis(100),
		"first Effect::SendRequest CollationFetchingV1",
	);
	let request_id = first.request_id().unwrap();
	let first_peer = match first {
		Effect::SendRequest { to, .. } => to,
		_ => unreachable!(),
	};

	// Deliver a valid response.
	let pov = PoV { block_data: BlockData(vec![]) };
	let response = protocol_v1::CollationFetchingResponse::Collation(
		candidate.receipt.clone().into(),
		pov,
	);
	world
		.sim
		.respond_fetch(request_id, Ok((response.encode(), ProtocolName::from(""))));

	// Wait for the seconding effect.
	let _ = world.sim.expect(
		|effect| matches!(
			effect,
			Effect::SecondCandidate { candidate_hash, .. } if candidate_hash == &candidate.hash()
		),
		Duration::from_millis(500),
		"Effect::SecondCandidate after the first fetch",
	);

	// Backing reports the candidate as Invalid.
	world.sim.send(CollatorProtocolMessage::Invalid(world.leaf, candidate.receipt.clone().into()));

	// First peer gets the malicious reputation hit.
	let _ = world.sim.expect(
		|effect| matches!(
			effect,
			Effect::Reputation { peer, bucket: RepBucket::Malicious } if *peer == first_peer
		),
		Duration::from_millis(100),
		"Effect::Reputation Malicious for the peer that produced the invalid candidate",
	);

	// And the validator fires a *new* fetch — for the other peer's queued advertisement.
	let other_peer = if first_peer == peer_b.peer_id { peer_c.peer_id } else { peer_b.peer_id };
	let _ = world.sim.expect(
		|effect| matches!(
			effect,
			Effect::SendRequest {
				to,
				kind: ReqKind::CollationFetchingV1,
				..
			} if *to == other_peer
		),
		Duration::from_millis(100),
		"Effect::SendRequest CollationFetchingV1 to the second peer after Invalid",
	);
}
