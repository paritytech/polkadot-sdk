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

//! A declared collator that keeps advertising at sub-window intervals stays connected.
//! Once it stops, validator disconnects after the policy window. Multi-leaf to confirm
//! "activity on any in-view leaf counts."
//!
//! KNOWN-FAILING (experimental): per #616, experimental drops time-based eviction.

use crate::{
	builders::ProtocolVersion::V1,
	contract::Effect,
	harness::CollatorSut,
	scenarios::shared::build_multi_leaf_world,
};
use polkadot_collator_protocol::CollatorEvictionPolicy;
use polkadot_node_network_protocol::peer_set::PeerSet;
use polkadot_primitives::{CoreIndex, Id as ParaId};
use std::time::Duration;

const PARA: ParaId = ParaId::new(2000);

#[crate::sim_test]
fn activity_keeps_peer_alive_then_disconnects_when_silent<S: CollatorSut>() {
	let mut w = build_multi_leaf_world::<S>(3, &[(CoreIndex(0), PARA)]);
	let peer = w.declared_peer(PARA, V1);

	// Step in chunks ~2/3 of the production `inactive_collator` window so each advertise
	// resets the activity timer before it would fire. After 3 steps the peer has been
	// continuously advertising for ~2× the window — must NOT have been evicted yet.
	let inactive = CollatorEvictionPolicy::default().inactive_collator;
	let step = inactive * 2 / 3;
	for i in 0..3 {
		w.sim.advance(step);
		w.sim.send(peer.advertise(w.leaves[i].hash, None, None));
	}

	w.sim.expect_count(
		|e| matches!(
			e,
			Effect::DisconnectPeers { peers, peer_set: PeerSet::Collation } if peers.contains(&peer.peer_id),
		),
		0,
		"DisconnectPeers targeting the actively-advertising peer (must be zero so far)",
	);

	// Fall silent — advance well past the window, peer must be disconnected now.
	w.sim.advance(inactive + Duration::from_secs(1));
	w.expect_disconnect(&peer);
}
