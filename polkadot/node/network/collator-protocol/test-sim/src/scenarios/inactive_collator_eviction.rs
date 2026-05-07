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

//! Collator declares (so it's "known") but never advertises. After the eviction policy's
//! `inactive_collator` window elapses (production default: 24s), validator disconnects.
//!
//! KNOWN-FAILING (experimental): per #616, experimental drops time-based eviction —
//! permissive connection policy, only evicts under capacity pressure.

use crate::{
	builders::ProtocolVersion::V1,
	harness::CollatorSut,
	scenarios::shared::activated_world,
};
use polkadot_primitives::{CoreIndex, Id as ParaId};
use std::time::Duration;

const PARA: ParaId = ParaId::new(2000);

#[crate::sim_test]
fn declared_but_inactive_collator_evicted_after_window<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let peer = w.declared_peer(PARA, V1);

	// Production CollatorEvictionPolicy::inactive_collator defaults to 24s; advance past it.
	w.sim.advance(Duration::from_secs(25));

	w.expect_disconnect(&peer);
}
