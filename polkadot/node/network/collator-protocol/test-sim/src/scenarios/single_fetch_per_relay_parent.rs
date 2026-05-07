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

//! Two V1 peers advertise the same relay parent. Validator fetches from exactly one of
//! them (one in-flight fetch per relay parent). After the first fetch resolves with a
//! valid response → seconded, a second fetch *may* fire — claim-queue lookahead allows up
//! to N seconded candidates per RP. This scenario only pins the in-flight cap.

use crate::{
	builders::ProtocolVersion::V1,
	contract::{Effect, ReqKind},
	harness::CollatorSut,
	scenarios::shared::activated_world,
};
use polkadot_primitives::{CoreIndex, Id as ParaId};
use std::time::Duration;

const PARA: ParaId = ParaId::new(2000);

#[crate::sim_test]
fn one_fetch_per_relay_parent_until_seconded<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let leaf = w.leaf();

	let peer_b = w.declared_peer(PARA, V1);
	let peer_c = w.declared_peer(PARA, V1);
	w.sim.send(peer_b.advertise(leaf, None, None));
	w.sim.send(peer_c.advertise(leaf, None, None));

	let _ = w.sim.expect(
		|e| matches!(e, Effect::SendRequest { kind: ReqKind::CollationFetchingV1, .. }),
		Duration::from_millis(100),
		"first Effect::SendRequest CollationFetchingV1",
	);

	w.sim.expect_count(
		|e| matches!(e, Effect::SendRequest { .. }),
		1,
		"SendRequest while one fetch is in flight (no second concurrent fetch allowed)",
	);
}
