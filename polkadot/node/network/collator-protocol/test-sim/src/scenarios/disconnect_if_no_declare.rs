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

//! `CollatorEvictionPolicy::undeclared` window.
//!
//! * [`peer_disconnected_when_undeclared_window_elapses`] — peer connects, never declares,
//!   and gets disconnected after the undeclared window (1s in production).
//! * [`declared_peer_not_disconnected_when_undeclared_window_elapses`] — sanity counterpart.
//!
//! KNOWN-FAILING (experimental): per
//! `project_collator_experimental_no_undeclared_eviction.md`, this is an intended
//! deviation in the experimental rewrite — #616 says no time-based eviction. Failure here
//! flags the divergence; not a bug in itself.

use crate::{
	builders::ProtocolVersion::V1,
	harness::CollatorSut,
	scenarios::shared::activated_world,
};
use polkadot_primitives::{CoreIndex, Id as ParaId};
use std::time::Duration;

const PARA: ParaId = ParaId::new(2000);

#[crate::sim_test]
fn peer_disconnected_when_undeclared_window_elapses<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let peer = w.connected_peer(PARA, V1);
	// Production CollatorEvictionPolicy::undeclared defaults to 1s. Advance ~1.5s to clear.
	w.sim.advance(Duration::from_millis(1500));
	w.expect_disconnect(&peer);
}

#[crate::sim_test]
fn declared_peer_not_disconnected_when_undeclared_window_elapses<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let peer = w.declared_peer(PARA, V1);
	w.expect_no_disconnect(&peer, Duration::from_millis(1500));
}
