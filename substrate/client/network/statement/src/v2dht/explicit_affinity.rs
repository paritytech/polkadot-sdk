// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Explicit topic affinity for the v2 DHT gossip path.

use crate::{affinity::AffinityFilter, LOG_TARGET};
use sc_network_types::PeerId;
use sp_statement_store::{Statement, Topic};

/// Tracks explicit topic affinity: the local node's own topics and the filters peers advertise.
///
/// The local topics produce the [`AffinityFilter`] this node advertises; the stored peer filters
/// let the node decide whom to forward a statement to. This answers only the *explicit* half of the
/// store/forward decision — the DHT-closeness half lives in the peers-topology module.
#[allow(dead_code)]
pub(crate) struct ExplicitAffinity {
	/// Seed for the advertised filter. Encoded on the wire so peers rebuild the same bloom; it
	/// only needs to stay stable for the node's lifetime, so a fresh random value per node
	/// suffices.
	// TODO: source it from the protocol config (as the light client does) once that is plumbed.
	seed: u128,
}

#[allow(dead_code)]
impl ExplicitAffinity {
	pub(crate) fn new() -> Self {
		Self { seed: rand::random() }
	}

	// === Local topics ===

	pub(crate) fn add_topics(&mut self, topics: &[Topic]) {
		// TODO: track the local topic set; fed by RPC subscriptions and configured topics.
		log::trace!(target: LOG_TARGET, "explicit_affinity: add_topics {} (stub)", topics.len());
	}

	pub(crate) fn remove_topics(&mut self, topics: &[Topic]) {
		// TODO: track the local topic set; fed by RPC subscriptions and configured topics.
		log::trace!(target: LOG_TARGET, "explicit_affinity: remove_topics {} (stub)", topics.len());
	}

	// === Advertise ===

	pub(crate) fn local_filter(&self) -> AffinityFilter {
		// TODO: build from the tracked local topics; empty for now.
		log::trace!(target: LOG_TARGET, "explicit_affinity: local_filter (stub)");
		AffinityFilter::from_topics(core::iter::empty::<&[u8; 32]>(), self.seed)
	}

	// === Peer filters ===

	pub(crate) fn update_peer_filter(&mut self, peer: PeerId, _filter: AffinityFilter) {
		// TODO: store the peer's advertised filter; subsumes the per-peer affinity state in lib.rs.
		log::trace!(target: LOG_TARGET, "explicit_affinity: update_peer_filter {peer} (stub)");
	}

	pub(crate) fn on_peer_disconnected(&mut self, peer: PeerId) {
		// TODO: drop the peer's stored filter.
		log::trace!(target: LOG_TARGET, "explicit_affinity: on_peer_disconnected {peer} (stub)");
	}

	// === Queries ===

	pub(crate) fn local_has_explicit_affinity(&self, _stmt: &Statement) -> bool {
		// TODO: true if any of the statement's topics is in the local topic set.
		log::trace!(target: LOG_TARGET, "explicit_affinity: local_has_explicit_affinity (stub)");
		false
	}

	pub(crate) fn peer_has_explicit_affinity(&self, peer: PeerId, _stmt: &Statement) -> bool {
		// TODO: true if the peer's stored filter matches any of the statement's topics.
		log::trace!(target: LOG_TARGET, "explicit_affinity: peer_has_explicit_affinity {peer} (stub)");
		false
	}
}
