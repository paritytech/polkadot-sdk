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

//! Time-based peer eviction: legacy yes, experimental no.
//!
//! Legacy uses `CollatorEvictionPolicy { undeclared, inactive_collator }` (1s / 24s in
//! production) to disconnect peers that connect-and-stall or declare-and-stall. After
//! either window elapses the peer is dropped via `DisconnectPeers`.
//!
//! Experimental, per RFC #616, removes time-based eviction entirely. Idle peers are
//! kept until *capacity pressure* causes one to be dropped (slot-eviction, see
//! [`super::reputation_behavior`] for the score-based replacement). The policy is
//! permissive by design: a slow but well-behaved collator should not get evicted just
//! because nothing is happening on its para for a while.
//!
//! See `memory:project_collator_experimental_no_undeclared_eviction` for the full
//! design rationale.
//!
//! # Test layout
//!
//! Each scenario has two filtered variants. The legacy variant advances past the
//! relevant policy window and asserts `DisconnectPeers`. The experimental variant
//! advances the same distance and asserts the absence of any disconnect.

use crate::{
	builders::{Peer, ProtocolVersion::V1},
	contract::Effect,
	harness::CollatorSut,
	scenarios::shared::{activated_world, build_multi_leaf_world, World},
};
use polkadot_collator_protocol::CollatorEvictionPolicy;
use polkadot_node_network_protocol::peer_set::PeerSet;
use polkadot_primitives::{CoreIndex, Id as ParaId};
use std::time::Duration;

const PARA: ParaId = ParaId::new(2000);

// ---------------------------------------------------------------------------
// Scenario 1: connected-but-undeclared peer
// ---------------------------------------------------------------------------

fn setup_undeclared<S: CollatorSut>() -> (World<S>, Peer) {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let peer = w.connected_peer(PARA, V1);
	(w, peer)
}

#[crate::sim_test(only = "legacy")]
fn undeclared_peer_disconnected_after_window<S: CollatorSut>() {
	let (mut w, peer) = setup_undeclared::<S>();
	w.sim.advance(CollatorEvictionPolicy::default().undeclared + Duration::from_millis(500));
	w.expect_disconnect(&peer);
}

#[crate::sim_test(only = "experimental")]
fn undeclared_peer_kept_indefinitely<S: CollatorSut>() {
	let (mut w, peer) = setup_undeclared::<S>();
	// Advance the same distance the legacy variant uses; experimental must not evict.
	let dur = CollatorEvictionPolicy::default().undeclared + Duration::from_millis(500);
	w.expect_no_disconnect(&peer, dur);
}

// ---------------------------------------------------------------------------
// Scenario 2: declared-but-inactive peer
// ---------------------------------------------------------------------------

fn setup_inactive<S: CollatorSut>() -> (World<S>, Peer) {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let peer = w.declared_peer(PARA, V1);
	(w, peer)
}

#[crate::sim_test(only = "legacy")]
fn declared_but_inactive_peer_evicted_after_window<S: CollatorSut>() {
	let (mut w, peer) = setup_inactive::<S>();
	w.sim.advance(CollatorEvictionPolicy::default().inactive_collator + Duration::from_secs(1));
	w.expect_disconnect(&peer);
}

#[crate::sim_test(only = "experimental")]
fn declared_but_inactive_peer_kept_indefinitely<S: CollatorSut>() {
	let (mut w, peer) = setup_inactive::<S>();
	let dur = CollatorEvictionPolicy::default().inactive_collator + Duration::from_secs(1);
	w.expect_no_disconnect(&peer, dur);
}

// ---------------------------------------------------------------------------
// Scenario 3: activity extends life (legacy); irrelevant on experimental
// ---------------------------------------------------------------------------

/// On legacy this asserts the activity-resets-timer behaviour: a peer that keeps
/// advertising at sub-window intervals stays connected; once it falls silent, the
/// inactive-collator window kicks in and it gets evicted.
///
/// On experimental there is no inactive-collator window at all (the entire concept is
/// gone), so the "fall silent → eviction" tail has no analogue. Tested via the simpler
/// "declared-but-inactive peer kept indefinitely" above. We document the asymmetry
/// here rather than write a vacuous experimental variant.
#[crate::sim_test(only = "legacy")]
fn activity_extends_life_then_silence_evicts<S: CollatorSut>() {
	let mut w = build_multi_leaf_world::<S>(3, &[(CoreIndex(0), PARA)]);
	let peer = w.declared_peer(PARA, V1);

	let inactive = CollatorEvictionPolicy::default().inactive_collator;
	let step = inactive * 2 / 3;
	for i in 0..3 {
		w.sim.advance(step);
		w.sim.send(peer.advertise(w.leaves[i].hash, None, None));
	}

	// After ~2× the window of continuous activity, peer must still be connected.
	w.sim.expect_count(
		|e| matches!(
			e,
			Effect::DisconnectPeers { peers, peer_set: PeerSet::Collation } if peers.contains(&peer.peer_id),
		),
		0,
		"DisconnectPeers targeting the actively-advertising peer (must be zero so far)",
	);

	// Fall silent — advance well past the window; peer must be disconnected.
	w.sim.advance(inactive + Duration::from_secs(1));
	w.expect_disconnect(&peer);
}
