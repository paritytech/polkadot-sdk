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

//! Scenario: peer_a fetches; peer_b queues an advertisement, then disconnects. After
//! peer_a's fetch resolves and its candidate is seconded, no SendRequest fires for
//! peer_b — the disconnect cleared peer_b's queued advertisement.

use crate::scenarios::shared::WorldExt as _;
use crate::{
	builders::{Candidate, ProtocolVersion::V1},
	contract::Effect,
	harness::CollatorSut,
	scenarios::shared::activated_world,
};
use polkadot_node_subsystem::messages::{CollatorProtocolMessage, NetworkBridgeEvent};
use polkadot_primitives::{CoreIndex, Id as ParaId};
use std::time::Duration;

const PARA: ParaId = ParaId::new(2000);

#[crate::sim_test]
fn disconnect_clears_queued_advertisement<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let leaf = w.leaf();
	let leaf_n = w.leaf_number();

	// Build a candidate consistent with the framework's empty-parent-head PVD.
	let candidate = Candidate::builder()
		.para(PARA)
		.relay_parent(leaf)
		.relay_parent_number(leaf_n)
		.build();

	let peer_a = w.declared_peer(PARA, V1);
	let peer_b = w.declared_peer(PARA, V1);

	// peer_a advertises; first fetch fires for peer_a (only declared peer with an ad).
	w.base.sim.send(peer_a.advertise(leaf, None, None));
	let request_id = w.fetch_request(&candidate);

	// peer_b queues behind peer_a, then disconnects.
	w.base.sim.send(peer_b.advertise(leaf, None, None));
	w.base.sim.send(CollatorProtocolMessage::NetworkBridgeUpdate(
		NetworkBridgeEvent::PeerDisconnected(peer_b.peer_id),
	));

	// peer_a's fetch resolves valid → seconding emits.
	w.respond_fetch_v1(request_id, candidate.receipt.clone(), Candidate::empty_pov());
	w.expect_second(&candidate);

	// No fetch ever targets peer_b.
	w.base.sim.expect_no(
		|e| matches!(e, Effect::SendRequest { to, .. } if *to == peer_b.peer_id),
		Duration::from_millis(100),
		"SendRequest targeting peer_b after peer_b disconnected its advertisement",
	);
}
