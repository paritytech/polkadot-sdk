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

//! Headline full-seconding scenario: a peer advertises, the validator fetches, the test
//! delivers a valid `CollationFetchingResponse`, and the validator emits an
//! `Effect::SecondCandidate` for that candidate.
//!
//! Exercises the entire seconding flow end-to-end: real prospective + real backing,
//! always-valid candidate-validation stub, always-OK availability-store stub, drop-on-floor
//! stubs for statement-distribution / provisioner / availability-distribution.

use crate::{
	builders::{Candidate, Peer, ProtocolVersion},
	contract::{Effect, ReqKind},
	harness::SubsystemUnderTest,
};
use codec::Encode;
use polkadot_node_network_protocol::request_response::v2 as protocol_v2;
use polkadot_node_primitives::{BlockData, PoV};
use polkadot_node_subsystem::messages::{AllMessages, CollatorProtocolMessage};
use polkadot_primitives::{
	CoreIndex, HeadData, Id as ParaId, MutateDescriptorV2, PersistedValidationData,
};
use sc_network::ProtocolName;
use std::time::Duration;

#[crate::sim_test]
fn advertise_fetch_respond_yields_second_candidate<S>()
where
	S: SubsystemUnderTest<Message = CollatorProtocolMessage>,
	AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	let para = ParaId::from(2000);
	let mut world = crate::scenarios::shared::activated_world::<S>(&[(CoreIndex(0), para)]);

	let peer = Peer::new(para, ProtocolVersion::V2);
	// Build the candidate so its `persisted_validation_data_hash` matches the PVD prospective
	// will derive: parent head = HeadData(empty) (from constraints' required_parent), relay
	// parent number = leaf's number (1), storage root = Hash::zero() (synthetic header),
	// max_pov_size = 5MB (from default_constraints).
	let pvd = PersistedValidationData {
		parent_head: HeadData(Vec::new()),
		relay_parent_number: world.chain.lock().block(&world.leaf).unwrap().number,
		relay_parent_storage_root: polkadot_primitives::Hash::zero(),
		max_pov_size: 5 * 1024 * 1024,
	};
	let mut candidate = Candidate::for_para_at(para, world.leaf);
	candidate
		.receipt
		.descriptor
		.set_persisted_validation_data_hash(pvd.hash());

	world.sim.send(peer.connected());
	world.sim.send(peer.declare());
	let parent_head_hash = HeadData(Vec::new()).hash();
	world.sim.send(peer.advertise(world.leaf, Some(candidate.hash()), Some(parent_head_hash)));

	// Wait for the fetch request.
	let send_request = world.sim.expect(
		|effect| matches!(
			effect,
			Effect::SendRequest {
				kind: ReqKind::CollationFetchingV2,
				candidate_hash: Some(c),
				..
			} if c == &candidate.hash()
		),
		Duration::from_millis(200),
		"Effect::SendRequest CollationFetchingV2 for the advertised candidate",
	);
	let request_id = send_request.request_id().expect("SendRequest carries a RequestId");

	// Deliver a valid V2 collation response.
	let pov = PoV { block_data: BlockData(vec![]) };
	let response = protocol_v2::CollationFetchingResponse::Collation(
		candidate.receipt.clone().into(),
		pov,
	);
	let payload = response.encode();
	world.sim.respond_fetch(request_id, Ok((payload, ProtocolName::from(""))));

	// Validator should now emit Effect::SecondCandidate for this candidate.
	let _ = world.sim.expect(
		|effect| matches!(
			effect,
			Effect::SecondCandidate { candidate_hash, .. } if candidate_hash == &candidate.hash()
		),
		Duration::from_millis(500),
		"Effect::SecondCandidate for the fetched candidate",
	);

	// Sanity counterpart for project_collator_experimental_no_invalid_reputation_event:
	// a *valid* candidate must NOT produce a Reputation::Malicious effect for the peer
	// that delivered it. Confirms legacy's emission of Malicious on Invalid is the
	// divergence — not a generic "always emit malicious on every fetch outcome" bug.
	world.sim.expect_no(
		|e| matches!(
			e,
			Effect::Reputation {
				peer: p,
				bucket: crate::contract::RepBucket::Malicious,
			} if *p == peer.peer_id,
		),
		Duration::from_millis(50),
		"Reputation::Malicious for a peer that delivered a valid candidate",
	);
}
