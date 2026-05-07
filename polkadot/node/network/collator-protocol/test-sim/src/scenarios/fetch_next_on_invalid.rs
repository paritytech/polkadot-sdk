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
//! Validator emits Reputation::Malicious for the offending peer and fetches the next
//! queued advertisement.
//!
//! KNOWN-FAILING (experimental): per
//! `project_collator_experimental_no_invalid_reputation_event.md` — experimental updates
//! the persistent reputation store directly rather than emitting a bus event.

use crate::{
	builders::{Candidate, ProtocolVersion::V1},
	contract::{Effect, RepBucket, ReqKind},
	harness::CollatorSut,
	scenarios::shared::activated_world,
};
use polkadot_node_subsystem::messages::CollatorProtocolMessage;
use polkadot_primitives::{CoreIndex, Id as ParaId};
use std::time::Duration;

const PARA: ParaId = ParaId::new(2000);

#[crate::sim_test]
fn invalid_signal_penalises_peer_and_fetches_next<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let leaf = w.leaf();
	let leaf_n = w.leaf_number();

	let candidate = Candidate::builder()
		.para(PARA)
		.relay_parent(leaf)
		.relay_parent_number(leaf_n)
		.build();

	let peer_b = w.declared_peer(PARA, V1);
	let peer_c = w.declared_peer(PARA, V1);
	w.sim.send(peer_b.advertise(leaf, None, None));
	w.sim.send(peer_c.advertise(leaf, None, None));

	// One fetch fires (whichever peer wins the queue).
	let first = w.sim.expect(
		|e| matches!(e, Effect::SendRequest { kind: ReqKind::CollationFetchingV1, .. }),
		Duration::from_millis(100),
		"first Effect::SendRequest CollationFetchingV1",
	);
	let request_id = first.request_id().expect("SendRequest carries a RequestId");
	let first_peer = match first {
		Effect::SendRequest { to, .. } => to,
		_ => unreachable!(),
	};

	w.respond_fetch_v1(request_id, candidate.receipt.clone(), Candidate::empty_pov());
	w.expect_second(&candidate);

	// Invalid signal → Malicious reputation hit + next fetch fires for the other peer.
	w.sim
		.send(CollatorProtocolMessage::Invalid(leaf, candidate.receipt.clone().into()));

	let _ = w.sim.expect(
		|e| matches!(
			e,
			Effect::Reputation { peer, bucket: RepBucket::Malicious } if *peer == first_peer
		),
		Duration::from_millis(100),
		"Effect::Reputation Malicious for the peer that produced the invalid candidate",
	);

	let other_peer = if first_peer == peer_b.peer_id { peer_c.peer_id } else { peer_b.peer_id };
	let _ = w.sim.expect(
		|e| matches!(
			e,
			Effect::SendRequest { to, kind: ReqKind::CollationFetchingV1, .. } if *to == other_peer
		),
		Duration::from_millis(100),
		"Effect::SendRequest CollationFetchingV1 to the second peer after Invalid",
	);
}
