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

//! Two peers advertise; first peer's candidate seconded; backing then signals `Invalid`.
//! Validator fetches the next queued advertisement from the other peer.
//!
//! Both impls fetch the next candidate. Legacy *also* emits Reputation::Malicious on
//! the bus for the peer whose candidate turned out invalid; experimental updates its
//! persistent rep store silently. The rep emission divergence is documented in
//! [`crate::scenarios::divergent::reputation_emission`].

use crate::scenarios::shared::WorldExt as _;
use crate::{
	builders::{Candidate, ProtocolVersion::V1},
	harness::CollatorSut,
	scenarios::shared::activated_world,
};
use polkadot_node_subsystem::messages::CollatorProtocolMessage;
use polkadot_primitives::{CoreIndex, Id as ParaId};

const PARA: ParaId = ParaId::new(2000);

#[crate::sim_test]
fn invalid_signal_fetches_next<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let leaf = w.leaf();
	let candidate = w.candidate_at(leaf).para(PARA).build();

	let peer_b = w.declared_peer(PARA, V1);
	let peer_c = w.declared_peer(PARA, V1);
	w.base.sim.send(peer_b.advertise(leaf, None, None));
	w.base.sim.send(peer_c.advertise(leaf, None, None));

	// One fetch fires (whichever peer wins the queue).
	let (first_peer, request_id, _) = w.expect_any_fetch();
	let other_peer = if first_peer == peer_b.peer_id { peer_c.peer_id } else { peer_b.peer_id };

	w.respond_fetch_v1(request_id, candidate.receipt.clone(), Candidate::empty_pov());
	w.expect_second(&candidate);

	// Invalid signal → next fetch fires for the other peer. Rep emission (if any) is
	// covered by the divergent suite.
	w.base.sim.send(CollatorProtocolMessage::Invalid(leaf, candidate.receipt.clone().into()));
	let _ = w.expect_fetch_to(other_peer);
}
