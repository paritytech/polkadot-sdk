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

//! Scenario: two V1 peers advertise the same relay parent. The validator fetches from one
//! of them and (after the test delivers a successful response) seconds the candidate. The
//! second peer's advertisement is *not* fetched because the relay parent already has a
//! seconded candidate.

use crate::{
	builders::{Candidate, Peer, ProtocolVersion},
	contract::{Effect, ReqKind},
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
fn one_fetch_per_relay_parent_until_seconded<S>()
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

	// Two V1 peers, both declared for the same para.
	let peer_b = Peer::new(para, ProtocolVersion::V1);
	let peer_c = Peer::new(para, ProtocolVersion::V1);
	for peer in [&peer_b, &peer_c] {
		world.sim.send(peer.connected());
		world.sim.send(peer.declare());
	}
	// Both advertise.
	for peer in [&peer_b, &peer_c] {
		world.sim.send(peer.advertise(world.leaf, None, None));
	}

	// Exactly one fetch should be in flight.
	let first = world.sim.expect(
		|effect| matches!(effect, Effect::SendRequest { kind: ReqKind::CollationFetchingV1, .. }),
		Duration::from_millis(100),
		"first Effect::SendRequest CollationFetchingV1",
	);
	let request_id = first.request_id().expect("SendRequest carries a RequestId");

	let initial_send_request_count = world
		.sim
		.recorder()
		.entries()
		.iter()
		.filter(|o| match o {
			crate::harness::Observation::Effect(s) =>
				matches!(s.value, Effect::SendRequest { .. }),
		})
		.count();
	assert_eq!(initial_send_request_count, 1, "validator should fire exactly one fetch initially");

	// Deliver a valid V1 response.
	let pov = PoV { block_data: BlockData(vec![]) };
	let response = protocol_v1::CollationFetchingResponse::Collation(
		candidate.receipt.clone().into(),
		pov,
	);
	world
		.sim
		.respond_fetch(request_id, Ok((response.encode(), ProtocolName::from(""))));

	// Validator seconds the candidate.
	let _ = world.sim.expect(
		|effect| matches!(
			effect,
			Effect::SecondCandidate { candidate_hash, .. } if candidate_hash == &candidate.hash()
		),
		Duration::from_millis(500),
		"Effect::SecondCandidate after the fetch response",
	);

	// Settle for a moment more — no second fetch should be issued.
	//
	// EXPECTED-FAILURE NOTE (legacy): the legacy upstream test
	// (validator_side/tests/mod.rs:fetch_one_collation_at_a_time_for_v1_advertisement)
	// asserts the same "no other fetch" property using
	// `virtual_overseer.recv().now_or_never() == None`, which is racy and happens to win
	// upstream. With our deterministic settling, the legacy validator fires a *second*
	// fetch on peer C's advertise after the candidate has already been seconded — likely
	// because peer C's advertise was queued in `per_scheduling_parent.collations` before
	// the seconding completed and `dequeue_next_collation_and_fetch` is run on the
	// post-seconding tick.
	//
	// Either the legacy contract is "exactly one fetch in flight at a time, but
	// post-seconding the next queued advertise gets fetched" (in which case the
	// upstream test docs are misleading), or this is a real spurious-fetch bug. Marked
	// as a divergence to investigate.
	world.sim.advance(Duration::from_millis(50));
	let send_request_count = world
		.sim
		.recorder()
		.entries()
		.iter()
		.filter(|o| match o {
			crate::harness::Observation::Effect(s) =>
				matches!(s.value, Effect::SendRequest { .. }),
		})
		.count();
	assert_eq!(
		send_request_count, 1,
		"after seconding, the validator must not fire a second fetch for the same relay parent",
	);
}
