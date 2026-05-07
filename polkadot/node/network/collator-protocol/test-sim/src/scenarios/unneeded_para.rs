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

//! Scenario: a peer declares as a collator for a para that isn't on any of our assigned cores.
//! The validator drops the connection and (eventually) reports the peer with a non-malicious
//! `Performance` cost.
//!
//! Spec the test is checking, in plain English:
//!
//! - Given a fresh validator side that has not received a view update (so no para is assigned).
//! - When a peer connects and declares for some para `X`.
//! - Then the validator immediately disconnects that peer.
//!
//! The reputation hit (`COST_UNNEEDED_COLLATOR`) is non-malicious, so it lives in the batched
//! reputation aggregator and is only flushed on the periodic tick. The disconnect is what is
//! immediately observable. A separate scenario (`reputation_tick_flushes_batched_changes`)
//! covers the periodic flush in a future commit.

use crate::{
	builders::{Peer, ProtocolVersion},
	contract::{Effect, Query},
	harness::{dispatcher::AnswerQuery, Sim, SimConfig},
	impls::LegacyValidator,
};
use polkadot_node_network_protocol::peer_set::PeerSet;
use polkadot_primitives::Id as ParaId;
use std::time::Duration;

struct PanicResponder;
impl AnswerQuery for PanicResponder {
	fn answer(&mut self, query: Query) {
		panic!(
			"unneeded-para scenario expected no queries before disconnect; got {:?}",
			query
		);
	}
}

#[crate::sim_test]
fn declare_for_unneeded_para_disconnects_peer() {
	let mut sim = Sim::<LegacyValidator>::start(SimConfig::default(), PanicResponder);

	let peer = Peer::new(ParaId::from(2000), ProtocolVersion::V1);

	sim.send(peer.connected());
	sim.send(peer.declare());

	let observed = sim.expect(
		|effect| matches!(effect, Effect::DisconnectPeers { peers, peer_set: PeerSet::Collation } if peers.contains(&peer.peer_id)),
		Duration::from_millis(50),
		"Effect::DisconnectPeers containing the unneeded-para peer",
	);
	match observed {
		Effect::DisconnectPeers { peers, peer_set } => {
			assert!(peers.contains(&peer.peer_id));
			assert_eq!(peer_set, PeerSet::Collation);
		},
		other => panic!("predicate matched but variant unexpected: {:?}", other),
	}
}
