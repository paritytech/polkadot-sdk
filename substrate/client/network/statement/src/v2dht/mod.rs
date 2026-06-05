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

//! DHT-targeted gossip path for the statement protocol.

mod explicit_affinity;
pub mod peers_topology;

use crate::{affinity::AffinityFilter, LOG_TARGET};
use explicit_affinity::ExplicitAffinity;
use sc_network_types::PeerId;
use sp_statement_store::SubmitResult;

/// Coordinates the v2 DHT-affinity statement gossip path.
#[allow(dead_code)]
pub(crate) struct V2DhtOrchestrator {
	/// Tracks the local node's topic affinity and the filters peers advertise.
	explicit_affinity: ExplicitAffinity,
}

#[allow(dead_code)]
impl V2DhtOrchestrator {
	pub(crate) fn new() -> Self {
		Self { explicit_affinity: ExplicitAffinity::new() }
	}

	// === Peer-set events ===

	pub(crate) fn on_peer_connected(&mut self, peer: PeerId) {
		// TODO: we may need it for the topology, remove if not
		log::trace!(target: LOG_TARGET, "v2dht: on_peer_connected {peer} (stub)");
	}

	pub(crate) fn on_peer_disconnected(&mut self, peer: PeerId) {
		// TODO: we may need it for the topology, remove if not
		log::trace!(target: LOG_TARGET, "v2dht: on_peer_disconnected {peer} (stub)");
	}

	// === Notification-substream events ===

	pub(crate) fn on_validate_inbound_substream(&mut self, peer: PeerId) {
		// TODO: we may need it for the peer steering, remove if not
		log::trace!(target: LOG_TARGET, "v2dht: on_validate_inbound_substream {peer} (stub)");
	}

	pub(crate) fn on_substream_opened(&mut self, peer: PeerId) {
		// TODO: we may need it for the topology or explicit affinity, remove if not
		log::trace!(target: LOG_TARGET, "v2dht: on_substream_opened {peer} (stub)");
	}

	pub(crate) fn on_substream_closed(&mut self, peer: PeerId) {
		// TODO: we may need it for the topology or explicit affinity, remove if not
		log::trace!(target: LOG_TARGET, "v2dht: on_substream_closed {peer} (stub)");
	}

	pub(crate) fn on_peer_filter_update(&mut self, peer: PeerId, _filter: AffinityFilter) {
		// TODO: we may need it for the topology or explicit affinity, remove if not
		log::trace!(target: LOG_TARGET, "v2dht: on_peer_filter_update {peer} (stub)");
	}

	// === Post-submit hook ===

	pub(crate) fn on_statement_imported(&mut self, peer: PeerId, _result: &SubmitResult) {
		// TODO: We may need to reflect the import result in the peer's score, remove if not
		log::trace!(target: LOG_TARGET, "v2dht: on_statement_imported {peer} (stub)");
	}

	// === Periodic ticks & post-iteration hooks ===

	pub(crate) async fn propagate_statements(&mut self) {
		// TODO: We need to know where to propagate
		log::trace!(target: LOG_TARGET, "v2dht: propagate_statements (stub)");
	}

	pub(crate) async fn on_initial_sync(&mut self) {
		// TODO: We need to know what to propagate
		log::trace!(target: LOG_TARGET, "v2dht: on_initial_sync (stub)");
	}

	pub(crate) fn on_pending_affinities(&mut self) {
		// TODO: We need to know what to propagate
		log::trace!(target: LOG_TARGET, "v2dht: on_pending_affinities (stub)");
	}

	pub(crate) fn on_major_sync_end(&mut self) {
		// TODO: The major sync processing may be different
		log::trace!(target: LOG_TARGET, "v2dht: on_major_sync_end (stub)");
	}
}
