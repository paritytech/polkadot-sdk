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

//! Scenario: V1 advertisement → fetch → second → `Seconded` notification flows back from
//! backing to collator-protocol → validator notifies the original collator peer with a
//! `CollationSeconded` wire message and a `BENEFIT_NOTIFY_GOOD` reputation bump.
//!
//! Mirrors `validator_side/tests/prospective_parachains.rs::v1_advertisement_accepted_and_seconded`.
//!
//! Distinct from `full_seconding`: that scenario uses V2 and asserts only up to
//! `SecondCandidate`. This one drives the full V1 path including the back-notification
//! to the collator.
//!
//! KNOWN-FAILING (both impls): real candidate-backing's seconding flow gets as far as
//! emitting `Second{...}` (which we record as `SecondCandidate`) but the subsequent
//! `IntroduceSecondedCandidate` round-trip to real prospective-parachains likely fails
//! because our chain has no pending availability / fragment chain shape that matches the
//! candidate's parent_head_data. The CollatorProtocolMessage::Seconded back-notification
//! never reaches the validator, so the CollationSeconded wire message is never sent.
//!
//! TO FIX: extend the harness to seed prospective-parachains with the candidate's
//! parent_head_data via either (a) a `ProspectiveParachainsMessage::IntroduceSecondedCandidate`
//! pre-step that the test issues directly, or (b) configure the chain's pending availability
//! such that prospective accepts the candidate as a fresh fork-base.

use crate::{
	builders::{Candidate, Peer, ProtocolVersion},
	contract::{Effect, RepBucket, WireMsgKind},
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
fn v1_advertise_fetch_second_and_collator_notified<S>()
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

	let peer = Peer::new(para, ProtocolVersion::V1);
	world.sim.send(peer.connected());
	world.sim.send(peer.declare());
	world.sim.send(peer.advertise(world.leaf, None, None));

	let send_request = world.sim.expect(
		|e| matches!(
			e,
			Effect::SendRequest {
				kind: crate::contract::ReqKind::CollationFetchingV1,
				..
			},
		),
		Duration::from_millis(200),
		"Effect::SendRequest CollationFetchingV1",
	);
	let request_id = send_request.request_id().expect("RequestId");

	let pov = PoV { block_data: BlockData(vec![1]) };
	let response = protocol_v1::CollationFetchingResponse::Collation(
		candidate.receipt.clone().into(),
		pov,
	);
	world
		.sim
		.respond_fetch(request_id, Ok((response.encode(), ProtocolName::from(""))));

	// Seconding is observed.
	let _ = world.sim.expect(
		|e| matches!(
			e,
			Effect::SecondCandidate { candidate_hash, .. } if candidate_hash == &candidate.hash()
		),
		Duration::from_millis(500),
		"Effect::SecondCandidate",
	);

	// After backing sends CollatorProtocolMessage::Seconded back to collator-protocol, the
	// validator notifies the original collator with a CollationSeconded wire message AND
	// a BENEFIT_NOTIFY_GOOD reputation bump.
	let _ = world.sim.expect(
		|e| matches!(
			e,
			Effect::SendCollation {
				peers,
				kind: WireMsgKind::CollationSeconded { .. },
			} if peers.contains(&peer.peer_id),
		),
		Duration::from_millis(500),
		"Effect::SendCollation CollationSeconded targeting the collator peer",
	);
	let _ = world.sim.expect(
		|e| matches!(
			e,
			Effect::Reputation { peer: p, bucket: RepBucket::Benefit } if *p == peer.peer_id,
		),
		Duration::from_millis(500),
		"Effect::Reputation { Benefit } for the seconded collator",
	);
}
