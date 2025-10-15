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

use polkadot_node_subsystem::prometheus::Opts;
use polkadot_node_subsystem_util::metrics::{
	self,
	prometheus::{self, Gauge, GaugeVec, U64},
};

#[derive(Clone)]
pub(crate) struct MetricsInner {
	approvals_usage_total: prometheus::Counter<U64>,
}


/// Candidate backing metrics.
#[derive(Default, Clone)]
pub struct Metrics(pub(crate) Option<MetricsInner>);

impl Metrics {
	pub fn record_approvals_usage(&self, collected: u64) {
		self.0.as_ref().map(|metrics| {
			metrics.approvals_usage_total.inc_by(collected);
		});
	}
}

impl metrics::Metrics for Metrics {
	fn try_register(registry: &prometheus::Registry) -> Result<Self, prometheus::PrometheusError> {
		let metrics = MetricsInner {
			approvals_usage_total: prometheus::register(
				prometheus::Counter::new(
					"polkadot_parachain_rewards_statistics_collector_approvals_usage_total",
					"Total of collected meaningfull approvals used to approve a candidate",
					)?,
				registry,
			)?,
		};
		Ok(Metrics(Some(metrics)))
	}
}
