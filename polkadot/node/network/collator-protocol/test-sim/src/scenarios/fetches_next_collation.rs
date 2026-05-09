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

//! Mirrors `validator_side/tests/mod.rs::fetches_next_collation`.
//!
//! Three V1 peers all advertise. The validator fetches from one (or two concurrently);
//! we don't respond. After `MAX_UNSHARED_DOWNLOAD_TIME` the validator falls back to
//! another peer. Property under test: a stalled fetch doesn't block the queue forever.

use crate::scenarios::shared::WorldExt as _;
use crate::{
	builders::ProtocolVersion::V1,
	contract::{Effect, ReqKind},
	harness::CollatorSut,
	scenarios::shared::activated_world,
};
use polkadot_collator_protocol::validator_side_consts::MAX_UNSHARED_DOWNLOAD_TIME;
use polkadot_primitives::{CoreIndex, Id as ParaId};
use std::time::Duration;

const PARA: ParaId = ParaId::new(2000);

#[crate::sim_test]
fn stalled_fetch_falls_back_to_next_peer_after_timeout<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let leaf = w.leaf();

	let peers = [
		w.declared_peer(PARA, V1),
		w.declared_peer(PARA, V1),
		w.declared_peer(PARA, V1),
	];
	for p in &peers {
		w.base.sim.send(p.advertise(leaf, None, None));
	}

	// First fetch fires (which peer is unspecified).
	let (_first_peer, _, _) = w.expect_any_fetch();

	// Don't respond. Advance past the deadline; ≥1 follow-up fetch must fire.
	let barrier = w.base.sim.now_sim_t();
	w.base.sim.advance(MAX_UNSHARED_DOWNLOAD_TIME + Duration::from_millis(100));
	w.base.sim.expect_at_least_after(
		barrier,
		|e| matches!(e, Effect::SendRequest { kind: ReqKind::CollationFetchingV1, .. }),
		1,
		"a follow-up SendRequest fires after the first peer's deadline",
	);
}
