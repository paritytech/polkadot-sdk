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

//! V1 advertisements must target the active leaf, not its ancestor. A V1 advertisement at
//! L's parent is protocol misuse → no fetch fires for it.
//!
//! Both impls reject the advertisement. The reputation *signal* of rejection diverges —
//! legacy emits `Malicious` on the bus, experimental updates a persistent store silently —
//! and that's tested in [`crate::scenarios::divergent::reputation_emission`]. Here we
//! assert only the shared invariant: no fetch.

use crate::{
	builders::ProtocolVersion::V1,
	contract::Effect,
	harness::CollatorSut,
	scenarios::shared::build_with_ancestors_world,
};
use polkadot_primitives::{CoreIndex, Id as ParaId};
use std::time::Duration;

const PARA: ParaId = ParaId::new(2000);

#[crate::sim_test]
fn v1_advertisement_at_parent_of_leaf_is_rejected<S: CollatorSut>() {
	let mut w = build_with_ancestors_world::<S>(1, &[(CoreIndex(0), PARA)]);
	let parent = w.ancestors()[0];

	let peer = w.declared_peer(PARA, V1);
	w.sim.send(peer.advertise(parent, None, None));

	// No fetch should fire for the misuse advertisement.
	w.sim.expect_no(
		|e| matches!(e, Effect::SendRequest { .. }),
		Duration::from_millis(300),
		"SendRequest after V1 advertisement at non-leaf (must NOT fire)",
	);
}
