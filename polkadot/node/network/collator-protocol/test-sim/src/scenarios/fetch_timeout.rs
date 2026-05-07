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

//! Two peers advertise the same candidate. Validator fetches from one; the test does not
//! respond. After `MAX_UNSHARED_DOWNLOAD_TIME` (400ms prod, 100ms with fast-test-validator)
//! the per-fetch deadline expires; validator fetches from the other peer.

use crate::{
	builders::{Candidate, ProtocolVersion::V2},
	contract::{Effect, ReqKind},
	harness::CollatorSut,
	scenarios::shared::activated_world,
};
use polkadot_primitives::{CoreIndex, Id as ParaId};
use std::time::Duration;

const PARA: ParaId = ParaId::new(2000);

#[crate::sim_test]
fn fetch_timeout_advances_to_next_peer<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let leaf = w.leaf();

	let candidate = Candidate::for_para_at(PARA, leaf);
	let head_hash = candidate.receipt.descriptor.para_head();

	let peer_a = w.declared_peer(PARA, V2);
	let peer_b = w.declared_peer(PARA, V2);
	for peer in [&peer_a, &peer_b] {
		w.sim.send(peer.advertise(leaf, Some(candidate.hash()), Some(head_hash)));
	}

	let first = w.sim.expect(
		|e| matches!(e, Effect::SendRequest { kind: ReqKind::CollationFetchingV2, .. }),
		Duration::from_millis(50),
		"first Effect::SendRequest CollationFetchingV2 from one of the two peers",
	);
	let first_peer = match first {
		Effect::SendRequest { to, .. } => to,
		_ => unreachable!(),
	};
	let other_peer = if first_peer == peer_a.peer_id { peer_b.peer_id } else { peer_a.peer_id };

	// Don't respond. Advance past the per-fetch deadline.
	w.sim.advance(Duration::from_millis(500));

	let _ = w.sim.expect(
		|e| matches!(
			e,
			Effect::SendRequest { kind: ReqKind::CollationFetchingV2, to, .. } if *to == other_peer
		),
		Duration::from_millis(50),
		"Effect::SendRequest CollationFetchingV2 to the other peer after timeout",
	);
}
