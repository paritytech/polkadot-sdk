// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

use prometheus::{Error as PrometheusError, HistogramTimer, Registry};
use prometheus_endpoint::{HistogramOpts, HistogramVec, Opts};

/// Gathers metrics about the blockchain RPC client.
#[derive(Clone)]
pub(crate) struct RelaychainRpcMetrics {
	rpc_request: HistogramVec,
}

impl RelaychainRpcMetrics {
	pub(crate) fn register(registry: &Registry) -> Result<Self, PrometheusError> {
		Ok(Self {
			rpc_request: prometheus_endpoint::register(
				HistogramVec::new(
					HistogramOpts {
						common_opts: Opts::new(
							"relay_chain_rpc_interface",
							"Tracks stats about cumulus relay chain RPC interface",
						),
						buckets: prometheus::exponential_buckets(0.001, 4.0, 9)
							.expect("function parameters are constant and always valid; qed"),
					},
					&["method"],
				)?,
				registry,
			)?,
		})
	}

	pub(crate) fn start_request_timer(&self, method: &str) -> HistogramTimer {
		self.rpc_request.with_label_values(&[method]).start_timer()
	}
}
