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

//! Prometheus metrics for the v2 DHT gossip path.

use prometheus_endpoint::{register, Gauge, PrometheusError, Registry, U64};

#[derive(Clone)]
pub(crate) struct V2DhtMetrics {
	/// Statement-store peers known to the topology, including peers without confirmed protocol
	/// support.
	known_peers: Gauge<U64>,
	/// Statement-store peers with an open statement notification substream.
	connected_peers: Gauge<U64>,
	/// Known peers with confirmed statement-protocol support: the DHT storage, affinity and
	/// forwarding candidates.
	eligible_peers: Gauge<U64>,
}

impl V2DhtMetrics {
	pub(crate) fn register(r: &Registry) -> Result<Self, PrometheusError> {
		Ok(Self {
			known_peers: register(
				Gauge::new(
					"substrate_sync_statement_v2dht_known_peers",
					"Statement-store peers known to the v2 DHT topology, including peers without confirmed protocol support",
				)?,
				r,
			)?,
			connected_peers: register(
				Gauge::new(
					"substrate_sync_statement_v2dht_connected_peers",
					"Statement-store peers with an open statement notification substream",
				)?,
				r,
			)?,
			eligible_peers: register(
				Gauge::new(
					"substrate_sync_statement_v2dht_eligible_peers",
					"Known statement-store peers with confirmed protocol support, the DHT storage, affinity and forwarding candidates",
				)?,
				r,
			)?,
		})
	}

	pub(crate) fn set_topology_size(
		&self,
		known_peers: usize,
		connected_peers: usize,
		eligible_peers: usize,
	) {
		self.known_peers.set(known_peers as u64);
		self.connected_peers.set(connected_peers as u64);
		self.eligible_peers.set(eligible_peers as u64);
	}
}
