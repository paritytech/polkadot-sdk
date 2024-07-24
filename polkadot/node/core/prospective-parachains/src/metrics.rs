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
	prune_view_candidate_storage: prometheus::Histogram,
	introduce_seconded_candidate: prometheus::Histogram,
	hypothetical_membership: prometheus::Histogram,
	candidate_storage_count: prometheus::GaugeVec<U64>,
}

/// Candidate backing metrics.
#[derive(Default, Clone)]
pub struct Metrics(pub(crate) Option<MetricsInner>);

impl Metrics {
	/// Provide a timer for handling `prune_view_candidate_storage` which observes on drop.
	pub fn time_prune_view_candidate_storage(
		&self,
	) -> Option<metrics::prometheus::prometheus::HistogramTimer> {
		self.0
			.as_ref()
			.map(|metrics| metrics.prune_view_candidate_storage.start_timer())
	}

	/// Provide a timer for handling `IntroduceSecondedCandidate` which observes on drop.
	pub fn time_introduce_seconded_candidate(
		&self,
	) -> Option<metrics::prometheus::prometheus::HistogramTimer> {
		self.0
			.as_ref()
			.map(|metrics| metrics.introduce_seconded_candidate.start_timer())
	}

	/// Provide a timer for handling `GetHypotheticalMembership` which observes on drop.
	pub fn time_hypothetical_membership_request(
		&self,
	) -> Option<metrics::prometheus::prometheus::HistogramTimer> {
		self.0.as_ref().map(|metrics| metrics.hypothetical_membership.start_timer())
	}

	/// Record the size of the candidate storage. First param is the connected candidates count,
	/// second param is the unconnected candidates count.
	pub fn record_candidate_storage_size(&self, connected_count: u64, unconnected_count: u64) {
		self.0.as_ref().map(|metrics| {
			metrics
				.candidate_storage_count
				.with_label_values(&["connected"])
				.set(connected_count)
		});

		self.0.as_ref().map(|metrics| {
			metrics
				.candidate_storage_count
				.with_label_values(&["unconnected"])
				.set(unconnected_count)
		});
	}
}

impl metrics::Metrics for Metrics {
	fn try_register(registry: &prometheus::Registry) -> Result<Self, prometheus::PrometheusError> {
		let metrics = MetricsInner {
			prune_view_candidate_storage: prometheus::register(
				prometheus::Histogram::with_opts(prometheus::HistogramOpts::new(
					"polkadot_parachain_prospective_parachains_prune_view_candidate_storage",
					"Time spent within `prospective_parachains::prune_view_candidate_storage`",
				))?,
				registry,
			)?,
			introduce_seconded_candidate: prometheus::register(
				prometheus::Histogram::with_opts(prometheus::HistogramOpts::new(
					"polkadot_parachain_prospective_parachains_introduce_seconded_candidate",
					"Time spent within `prospective_parachains::handle_introduce_seconded_candidate`",
				))?,
				registry,
			)?,
			hypothetical_membership: prometheus::register(
				prometheus::Histogram::with_opts(prometheus::HistogramOpts::new(
					"polkadot_parachain_prospective_parachains_hypothetical_membership",
					"Time spent responding to `GetHypotheticalMembership`",
				))?,
				registry,
			)?,
			candidate_storage_count: prometheus::register(
				GaugeVec::new(
					Opts::new(
						"polkadot_parachain_prospective_parachains_candidate_storage_count",
						"Number of candidates present in the candidate storage, split by connected and unconnected"
					),
					&["type"],
				)?,
				registry,
			)?,
		};
		Ok(Metrics(Some(metrics)))
	}
}
