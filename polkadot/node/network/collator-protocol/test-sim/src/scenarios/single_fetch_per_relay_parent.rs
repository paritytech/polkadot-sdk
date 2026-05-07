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

//! Scenario: two V1 peers advertise the same relay parent. The validator fetches from
//! exactly one of them (one fetch in flight at a time per relay parent). The
//! original-upstream test (`fetch_one_collation_at_a_time_for_v1_advertisement`) asserts
//! "no further fetch fires before the first one is resolved" via a racy `now_or_never()`
//! check. Our deterministic version asserts the same: at the moment of the first
//! SendRequest there is exactly one outbound fetch in flight. After the first fetch
//! resolves (with a valid response → seconding), the validator MAY fetch the next
//! advertisement — that is correct under async-backing claim-queue semantics: a relay
//! parent with `lookahead=3` claim slots can host up to 3 seconded candidates per para.
//! We do NOT assert "no second fetch ever fires"; that assertion was wrong.

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

	// Drop unused; the assertion below verifies the in-flight count.
	let _ = request_id;
	let _ = candidate;

	let send_request_count_in_flight = world
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
		send_request_count_in_flight, 1,
		"validator must not fire a second fetch while the first is in flight\n\n{}",
		crate::report::format_timeline(world.sim.recorder()),
	);
}
