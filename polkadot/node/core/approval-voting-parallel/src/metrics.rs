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

//! The Metrics for Approval Voting Parallel Subsystem.

use std::collections::HashMap;

use polkadot_node_metrics::{metered::Meter, metrics};
use polkadot_overseer::prometheus;

#[derive(Default, Clone)]
pub struct Metrics(Option<MetricsInner>);

/// Approval Voting parallel metrics.
#[derive(Clone)]
pub struct MetricsInner {
	// The inner metrics of the approval distribution workers.
	approval_distribution: polkadot_approval_distribution::metrics::Metrics,
	// The inner metrics of the approval voting workers.
	approval_voting: polkadot_node_core_approval_voting::Metrics,

	// Time of flight metrics for bounded channels.
	to_worker_bounded_tof: prometheus::HistogramVec,
	// Number of elements sent to the worker's bounded queue.
	to_worker_bounded_sent: prometheus::GaugeVec<prometheus::U64>,
	// Number of elements received by the worker's bounded queue.
	to_worker_bounded_received: prometheus::GaugeVec<prometheus::U64>,
	// Number of times senders blocked while sending messages to the worker.
	to_worker_bounded_blocked: prometheus::GaugeVec<prometheus::U64>,
	// Time of flight metrics for unbounded channels.
	to_worker_unbounded_tof: prometheus::HistogramVec,
	// Number of elements sent to the worker's unbounded queue.
	to_worker_unbounded_sent: prometheus::GaugeVec<prometheus::U64>,
	// Number of elements received by the worker's unbounded queue.
	to_worker_unbounded_received: prometheus::GaugeVec<prometheus::U64>,
}

impl Metrics {
	/// Get the approval distribution metrics.
	pub fn approval_distribution_metrics(
		&self,
	) -> polkadot_approval_distribution::metrics::Metrics {
		self.0
			.as_ref()
			.map(|metrics_inner| metrics_inner.approval_distribution.clone())
			.unwrap_or_default()
	}

	/// Get the approval voting metrics.
	pub fn approval_voting_metrics(&self) -> polkadot_node_core_approval_voting::Metrics {
		self.0
			.as_ref()
			.map(|metrics_inner| metrics_inner.approval_voting.clone())
			.unwrap_or_default()
	}
}

impl metrics::Metrics for Metrics {
	/// Try to register the metrics.
	fn try_register(
		registry: &prometheus::Registry,
	) -> std::result::Result<Self, prometheus::PrometheusError> {
		Ok(Metrics(Some(MetricsInner {
			approval_distribution: polkadot_approval_distribution::metrics::Metrics::try_register(
				registry,
			)?,
			approval_voting: polkadot_node_core_approval_voting::Metrics::try_register(registry)?,
			to_worker_bounded_tof: prometheus::register(
				prometheus::HistogramVec::new(
					prometheus::HistogramOpts::new(
						"polkadot_approval_voting_parallel_worker_bounded_tof",
						"Duration spent in a particular approval voting worker channel from entrance to removal",
					)
					.buckets(vec![
						0.0001, 0.0004, 0.0016, 0.0064, 0.0256, 0.1024, 0.4096, 1.6384, 3.2768,
						4.9152, 6.5536,
					]),
					&["worker_name"],
				)?,
				registry,
			)?,
			to_worker_bounded_sent: prometheus::register(
				prometheus::GaugeVec::<prometheus::U64>::new(
					prometheus::Opts::new(
						"polkadot_approval_voting_parallel_worker_bounded_sent",
						"Number of elements sent to approval voting workers' bounded queues",
					),
					&["worker_name"],
				)?,
				registry,
			)?,
			to_worker_bounded_received: prometheus::register(
				prometheus::GaugeVec::<prometheus::U64>::new(
					prometheus::Opts::new(
						"polkadot_approval_voting_parallel_worker_bounded_received",
						"Number of elements received by approval voting workers' bounded queues",
					),
					&["worker_name"],
				)?,
				registry,
			)?,
			to_worker_bounded_blocked: prometheus::register(
				prometheus::GaugeVec::<prometheus::U64>::new(
					prometheus::Opts::new(
						"polkadot_approval_voting_parallel_worker_bounded_blocked",
						"Number of times approval voting workers blocked while sending messages to a subsystem",
					),
					&["worker_name"],
				)?,
				registry,
			)?,
			to_worker_unbounded_tof: prometheus::register(
				prometheus::HistogramVec::new(
					prometheus::HistogramOpts::new(
						"polkadot_approval_voting_parallel_worker_unbounded_tof",
						"Duration spent in a particular approval voting worker channel from entrance to removal",
					)
					.buckets(vec![
						0.0001, 0.0004, 0.0016, 0.0064, 0.0256, 0.1024, 0.4096, 1.6384, 3.2768,
						4.9152, 6.5536,
					]),
					&["worker_name"],
				)?,
				registry,
			)?,
			to_worker_unbounded_sent: prometheus::register(
				prometheus::GaugeVec::<prometheus::U64>::new(
					prometheus::Opts::new(
						"polkadot_approval_voting_parallel_worker_unbounded_sent",
						"Number of elements sent to approval voting workers' unbounded queues",
					),
					&["worker_name"],
				)?,
				registry,
			)?,
			to_worker_unbounded_received: prometheus::register(
				prometheus::GaugeVec::<prometheus::U64>::new(
					prometheus::Opts::new(
						"polkadot_approval_voting_parallel_worker_unbounded_received",
						"Number of elements received by approval voting workers' unbounded queues",
					),
					&["worker_name"],
				)?,
				registry,
			)?,
		})))
	}
}

/// The meters to watch.
#[derive(Clone)]
pub struct Meters {
	bounded: Meter,
	unbounded: Meter,
}

impl Meters {
	pub fn new(bounded: &Meter, unbounded: &Meter) -> Self {
		Self { bounded: bounded.clone(), unbounded: unbounded.clone() }
	}
}

/// A metrics watcher that watches the meters and updates the metrics.
pub struct MetricsWatcher {
	to_watch: HashMap<String, Meters>,
	metrics: Metrics,
}

impl MetricsWatcher {
	/// Create a new metrics watcher.
	pub fn new(metrics: Metrics) -> Self {
		Self { to_watch: HashMap::new(), metrics }
	}

	/// Watch the meters of a worker with this name.
	pub fn watch(&mut self, worker_name: String, meters: Meters) {
		self.to_watch.insert(worker_name, meters);
	}

	/// Collect all the metrics.
	pub fn collect_metrics(&self) {
		for (name, meter) in &self.to_watch {
			let bounded_readouts = meter.bounded.read();
			let unbounded_readouts = meter.unbounded.read();
			if let Some(metrics) = self.metrics.0.as_ref() {
				metrics
					.to_worker_bounded_sent
					.with_label_values(&[name])
					.set(bounded_readouts.sent as u64);

				metrics
					.to_worker_bounded_received
					.with_label_values(&[name])
					.set(bounded_readouts.received as u64);

				metrics
					.to_worker_bounded_blocked
					.with_label_values(&[name])
					.set(bounded_readouts.blocked as u64);

				metrics
					.to_worker_unbounded_sent
					.with_label_values(&[name])
					.set(unbounded_readouts.sent as u64);

				metrics
					.to_worker_unbounded_received
					.with_label_values(&[name])
					.set(unbounded_readouts.received as u64);

				let hist_bounded = metrics.to_worker_bounded_tof.with_label_values(&[name]);
				for tof in bounded_readouts.tof {
					hist_bounded.observe(tof.as_f64());
				}

				let hist_unbounded = metrics.to_worker_unbounded_tof.with_label_values(&[name]);
				for tof in unbounded_readouts.tof {
					hist_unbounded.observe(tof.as_f64());
				}
			}
		}
	}
}
