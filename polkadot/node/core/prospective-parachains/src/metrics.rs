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
	prometheus::{self, GaugeVec, U64},
};

#[derive(Clone)]
pub(crate) struct MetricsInner {
	time_active_leaves_update: prometheus::Histogram,
	time_introduce_seconded_candidate: prometheus::Histogram,
	time_candidate_backed: prometheus::Histogram,
	time_hypothetical_membership: prometheus::Histogram,
	candidate_count: prometheus::GaugeVec<U64>,
}

/// Candidate backing metrics.
#[derive(Default, Clone)]
pub struct Metrics(pub(crate) Option<MetricsInner>);

impl Metrics {
	/// Provide a timer for handling `ActiveLeavesUpdate` which observes on drop.
	pub fn time_handle_active_leaves_update(
		&self,
	) -> Option<metrics::prometheus::prometheus::HistogramTimer> {
		self.0.as_ref().map(|metrics| metrics.time_active_leaves_update.start_timer())
	}

	/// Provide a timer for handling `IntroduceSecondedCandidate` which observes on drop.
	pub fn time_introduce_seconded_candidate(
		&self,
	) -> Option<metrics::prometheus::prometheus::HistogramTimer> {
		self.0
			.as_ref()
			.map(|metrics| metrics.time_introduce_seconded_candidate.start_timer())
	}

	/// Provide a timer for handling `CandidateBacked` which observes on drop.
	pub fn time_candidate_backed(&self) -> Option<metrics::prometheus::prometheus::HistogramTimer> {
		self.0.as_ref().map(|metrics| metrics.time_candidate_backed.start_timer())
	}

	/// Provide a timer for handling `GetHypotheticalMembership` which observes on drop.
	pub fn time_hypothetical_membership_request(
		&self,
	) -> Option<metrics::prometheus::prometheus::HistogramTimer> {
		self.0
			.as_ref()
			.map(|metrics| metrics.time_hypothetical_membership.start_timer())
	}

	/// Record number of candidates across all fragment chains. First param is the connected
	/// candidates count, second param is the unconnected candidates count.
	pub fn record_candidate_count(&self, connected_count: u64, unconnected_count: u64) {
		self.0.as_ref().map(|metrics| {
			metrics.candidate_count.with_label_values(&["connected"]).set(connected_count)
		});

		self.0.as_ref().map(|metrics| {
			metrics
				.candidate_count
				.with_label_values(&["unconnected"])
				.set(unconnected_count)
		});
	}
}

impl metrics::Metrics for Metrics {
	fn try_register(registry: &prometheus::Registry) -> Result<Self, prometheus::PrometheusError> {
		let metrics = MetricsInner {
			time_active_leaves_update: prometheus::register(
				prometheus::Histogram::with_opts(prometheus::HistogramOpts::new(
					"polkadot_parachain_prospective_parachains_time_active_leaves_update",
					"Time spent within `prospective_parachains::handle_active_leaves_update`",
				))?,
				registry,
			)?,
			time_introduce_seconded_candidate: prometheus::register(
				prometheus::Histogram::with_opts(prometheus::HistogramOpts::new(
					"polkadot_parachain_prospective_parachains_time_introduce_seconded_candidate",
					"Time spent within `prospective_parachains::handle_introduce_seconded_candidate`",
				))?,
				registry,
			)?,
			time_candidate_backed: prometheus::register(
				prometheus::Histogram::with_opts(prometheus::HistogramOpts::new(
					"polkadot_parachain_prospective_parachains_time_candidate_backed",
					"Time spent within `prospective_parachains::handle_candidate_backed`",
				))?,
				registry,
			)?,
			time_hypothetical_membership: prometheus::register(
				prometheus::Histogram::with_opts(prometheus::HistogramOpts::new(
					"polkadot_parachain_prospective_parachains_time_hypothetical_membership",
					"Time spent responding to `GetHypotheticalMembership`",
				))?,
				registry,
			)?,
			candidate_count: prometheus::register(
				GaugeVec::new(
					Opts::new(
						"polkadot_parachain_prospective_parachains_candidate_count",
						"Number of candidates present across all fragment chains, split by connected and unconnected"
					),
					&["type"],
				)?,
				registry,
			)?,
		};
		Ok(Metrics(Some(metrics)))
	}
}
