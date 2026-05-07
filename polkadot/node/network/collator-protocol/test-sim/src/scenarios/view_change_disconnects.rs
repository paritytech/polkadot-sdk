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

//! Peer declares while a leaf is active; validator's view then changes to empty;
//! validator disconnects the now-irrelevant peer. Sanity counterpart pins the assertion
//! to the view change rather than to the test setup itself.

use crate::{
	builders::ProtocolVersion::V1,
	harness::CollatorSut,
	scenarios::shared::activated_world,
};
use polkadot_node_network_protocol::OurView;
use polkadot_node_subsystem::messages::{CollatorProtocolMessage, NetworkBridgeEvent};
use polkadot_primitives::{CoreIndex, Id as ParaId};
use std::time::Duration;

const PARA: ParaId = ParaId::new(2000);

#[crate::sim_test]
fn empty_view_disconnects_declared_peer<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let peer = w.declared_peer(PARA, V1);

	w.sim.send(CollatorProtocolMessage::NetworkBridgeUpdate(
		NetworkBridgeEvent::OurViewChange(OurView::new(std::iter::empty(), 0)),
	));

	w.expect_disconnect(&peer);
}

#[crate::sim_test]
fn declared_peer_stays_connected_when_view_unchanged<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let peer = w.declared_peer(PARA, V1);
	w.expect_no_disconnect(&peer, Duration::from_millis(200));
}
