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

use polkadot_node_subsystem_util::metrics::{self, prometheus};

#[derive(Clone, Default)]
pub struct Metrics(Option<MetricsInner>);

impl Metrics {
	/// Record the time a collation took to be backed.
	pub fn on_collation_backed(&self, latency: f64) {
		if let Some(metrics) = &self.0 {
			metrics.collation_backing_latency.observe(latency);
		}
	}

	/// Record the time a collation took to be included.
	pub fn on_collation_included(&self, latency: f64) {
		if let Some(metrics) = &self.0 {
			metrics.collation_inclusion_latency.observe(latency);
		}
	}

	pub fn on_advertisement_made(&self) {
		if let Some(metrics) = &self.0 {
			metrics.advertisements_made.inc();
		}
	}

	pub fn on_collation_sent_requested(&self) {
		if let Some(metrics) = &self.0 {
			metrics.collations_send_requested.inc();
		}
	}

	pub fn on_collation_sent(&self) {
		if let Some(metrics) = &self.0 {
			metrics.collations_sent.inc();
		}
	}

	/// Provide a timer for `process_msg` which observes on drop.
	pub fn time_process_msg(&self) -> Option<prometheus::prometheus::HistogramTimer> {
		self.0.as_ref().map(|metrics| metrics.process_msg.start_timer())
	}

	/// Provide a timer for `distribute_collation` which observes on drop.
	pub fn time_collation_distribution(
		&self,
		label: &'static str,
	) -> Option<prometheus::prometheus::HistogramTimer> {
		self.0.as_ref().map(|metrics| {
			metrics.collation_distribution_time.with_label_values(&[label]).start_timer()
		})
	}

	/// Create a timer to measure how much time collations spend before being fetched.
	pub fn time_collation_fetch_latency(&self) -> Option<prometheus::prometheus::HistogramTimer> {
		self.0.as_ref().map(|metrics| metrics.collation_fetch_latency.start_timer())
	}

	/// Create a timer to measure how much time it takes for fetched collations to be backed.
	pub fn time_collation_backing_latency(&self) -> Option<prometheus::prometheus::HistogramTimer> {
		self.0
			.as_ref()
			.map(|metrics| metrics.collation_backing_latency_time.start_timer())
	}

	/// Record the time a collation took before expiring.
	/// Collations can expire in the following states: "advertised, fetched or backed"
	pub fn on_collation_expired(&self, latency: f64, state: &'static str) {
		if let Some(metrics) = &self.0 {
			metrics.collation_expired_total.with_label_values(&[state]).observe(latency);
		}
	}
}

#[derive(Clone)]
struct MetricsInner {
	advertisements_made: prometheus::Counter<prometheus::U64>,
	collations_sent: prometheus::Counter<prometheus::U64>,
	collations_send_requested: prometheus::Counter<prometheus::U64>,
	process_msg: prometheus::Histogram,
	collation_distribution_time: prometheus::HistogramVec,
	collation_fetch_latency: prometheus::Histogram,
	collation_backing_latency_time: prometheus::Histogram,
	collation_backing_latency: prometheus::Histogram,
	collation_inclusion_latency: prometheus::Histogram,
	collation_expired_total: prometheus::HistogramVec,
}

impl metrics::Metrics for Metrics {
	fn try_register(
		registry: &prometheus::Registry,
	) -> std::result::Result<Self, prometheus::PrometheusError> {
		let metrics = MetricsInner {
			advertisements_made: prometheus::register(
				prometheus::Counter::new(
					"polkadot_parachain_collation_advertisements_made_total",
					"A number of collation advertisements sent to validators.",
				)?,
				registry,
			)?,
			collations_send_requested: prometheus::register(
				prometheus::Counter::new(
					"polkadot_parachain_collations_sent_requested_total",
					"A number of collations requested to be sent to validators.",
				)?,
				registry,
			)?,
			collations_sent: prometheus::register(
				prometheus::Counter::new(
					"polkadot_parachain_collations_sent_total",
					"A number of collations sent to validators.",
				)?,
				registry,
			)?,
			process_msg: prometheus::register(
				prometheus::Histogram::with_opts(
					prometheus::HistogramOpts::new(
						"polkadot_parachain_collator_protocol_collator_process_msg",
						"Time spent within `collator_protocol_collator::process_msg`",
					)
					.buckets(vec![
						0.001, 0.002, 0.005, 0.01, 0.025, 0.05, 0.1, 0.15, 0.25, 0.35, 0.5, 0.75,
						1.0,
					]),
				)?,
				registry,
			)?,
			collation_distribution_time: prometheus::register(
				prometheus::HistogramVec::new(
					prometheus::HistogramOpts::new(
						"polkadot_parachain_collator_protocol_collator_distribution_time",
						"Time spent within `collator_protocol_collator::distribute_collation`",
					)
					.buckets(vec![
						0.001, 0.002, 0.005, 0.01, 0.025, 0.05, 0.1, 0.15, 0.25, 0.35, 0.5, 0.75,
						1.0,
					]),
					&["state"],
				)?,
				registry,
			)?,
			collation_fetch_latency: prometheus::register(
				prometheus::Histogram::with_opts(
					prometheus::HistogramOpts::new(
						"polkadot_parachain_collation_fetch_latency",
						"How much time collations spend waiting to be fetched",
					)
					.buckets(vec![
						0.001, 0.01, 0.025, 0.05, 0.1, 0.15, 0.25, 0.35, 0.5, 0.75, 1.0, 2.0, 5.0,
					]),
				)?,
				registry,
			)?,
			collation_backing_latency_time: prometheus::register(
				prometheus::Histogram::with_opts(
					prometheus::HistogramOpts::new(
						"polkadot_parachain_collation_backing_latency_time",
						"How much time it takes for a fetched collation to be backed",
					)
					.buckets(vec![
						1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0, 12.0, 15.0, 18.0, 24.0, 30.0,
					]),
				)?,
				registry,
			)?,
			collation_backing_latency: prometheus::register(
				prometheus::Histogram::with_opts(
					prometheus::HistogramOpts::new(
						"polkadot_parachain_collation_backing_latency",
						"How many blocks away from the relay parent are collations backed",
					)
					.buckets(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]),
				)?,
				registry,
			)?,
			collation_inclusion_latency: prometheus::register(
				prometheus::Histogram::with_opts(
					prometheus::HistogramOpts::new(
						"polkadot_parachain_collation_inclusion_latency",
						"How many blocks it takes for a backed collation to be included",
					)
					.buckets(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]),
				)?,
				registry,
			)?,
			collation_expired_total: prometheus::register(
				prometheus::HistogramVec::new(
					prometheus::HistogramOpts::new(
						"polkadot_parachain_collation_expired",
						"How many collations expired (not backed or not included)",
					)
					.buckets(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]),
					&["state"],
				)?,
				registry,
			)?,
		};

		Ok(Metrics(Some(metrics)))
	}
}
