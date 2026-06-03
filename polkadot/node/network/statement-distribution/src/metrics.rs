// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Metrics for the statement distribution module

use std::time::Duration;

use polkadot_node_subsystem_util::metrics::{self, prometheus};

use crate::v2::{FetchOutcome, FetchSlot};

/// Buckets more suitable for checking the typical latency values
const HISTOGRAM_LATENCY_BUCKETS: &[f64] = &[
	0.000025, 0.00005, 0.000075, 0.0001, 0.0003125, 0.000625, 0.00125, 0.0025, 0.005, 0.01, 0.025,
	0.05, 0.1,
];

/// Buckets for `AttestedCandidateRequest` completion latency.
const HISTOGRAM_FETCH_BUCKETS: &[f64] =
	&[0.010, 0.025, 0.050, 0.100, 0.150, 0.250, 0.500, 0.750, 1.000, 1.500, 2.000, 2.500, 5.000];

/// Buckets for end-to-end "first knew about candidate -> have candidate" latency.
const HISTOGRAM_LEARN_TO_FETCH_BUCKETS: &[f64] = &[
	0.025, 0.050, 0.100, 0.200, 0.300, 0.400, 0.500, 0.600, 0.700, 1.0, 1.5, 2.0, 2.5, 3.0, 5.0,
	7.5, 10.0, 15.0, 30.0,
];

#[derive(Clone)]
struct MetricsInner {
	// V1
	sent_requests: prometheus::Counter<prometheus::U64>,
	received_responses: prometheus::CounterVec<prometheus::U64>,
	network_bridge_update: prometheus::HistogramVec,
	statements_unexpected: prometheus::CounterVec<prometheus::U64>,
	created_message_size: prometheus::Gauge<prometheus::U64>,
	// V1+
	statements_distributed: prometheus::Counter<prometheus::U64>,
	active_leaves_update: prometheus::Histogram,
	share: prometheus::Histogram,
	// V2+
	peer_rate_limit_request_drop: prometheus::Counter<prometheus::U64>,
	max_parallel_requests_reached: prometheus::Counter<prometheus::U64>,
	// Parallel `AttestedCandidate` fetch metrics.
	parallel_fetch_fired: prometheus::Counter<prometheus::U64>,
	parallel_fetch_won: prometheus::Counter<prometheus::U64>,
	parallel_fetch_skipped_no_alt_peer: prometheus::Counter<prometheus::U64>,
	fetch_completion_seconds: prometheus::HistogramVec,
	learn_to_fetch_seconds: prometheus::Histogram,
}

/// Statement Distribution metrics.
#[derive(Default, Clone)]
pub struct Metrics(Option<MetricsInner>);

impl Metrics {
	/// Update statements distributed counter
	pub fn on_statement_distributed(&self) {
		if let Some(metrics) = &self.0 {
			metrics.statements_distributed.inc();
		}
	}

	/// Update statements distributed counter by an amount
	pub fn on_statements_distributed(&self, n: usize) {
		if let Some(metrics) = &self.0 {
			metrics.statements_distributed.inc_by(n as u64);
		}
	}

	/// Update sent requests counter
	/// This counter is updated merely for the statements sent via request/response method,
	/// meaning that it counts large statements only
	pub fn on_sent_request(&self) {
		if let Some(metrics) = &self.0 {
			metrics.sent_requests.inc();
		}
	}

	/// Update counters for the received responses with `succeeded` or `failed` labels
	/// These counters are updated merely for the statements received via request/response method,
	/// meaning that they count large statements only
	pub fn on_received_response(&self, success: bool) {
		if let Some(metrics) = &self.0 {
			let label = if success { "succeeded" } else { "failed" };
			metrics.received_responses.with_label_values(&[label]).inc();
		}
	}

	/// Provide a timer for `active_leaves_update` which observes on drop.
	pub fn time_active_leaves_update(
		&self,
	) -> Option<metrics::prometheus::prometheus::HistogramTimer> {
		self.0.as_ref().map(|metrics| metrics.active_leaves_update.start_timer())
	}

	/// Provide a timer for `share` which observes on drop.
	pub fn time_share(&self) -> Option<metrics::prometheus::prometheus::HistogramTimer> {
		self.0.as_ref().map(|metrics| metrics.share.start_timer())
	}

	/// Provide a timer for `network_bridge_update` which observes on drop.
	pub fn time_network_bridge_update(
		&self,
		message_type: &'static str,
	) -> Option<metrics::prometheus::prometheus::HistogramTimer> {
		self.0.as_ref().map(|metrics| {
			metrics.network_bridge_update.with_label_values(&[message_type]).start_timer()
		})
	}

	/// Update the out-of-view statements counter for unexpected valid statements
	pub fn on_unexpected_statement_valid(&self) {
		if let Some(metrics) = &self.0 {
			metrics.statements_unexpected.with_label_values(&["valid"]).inc();
		}
	}

	/// Update the out-of-view statements counter for unexpected seconded statements
	pub fn on_unexpected_statement_seconded(&self) {
		if let Some(metrics) = &self.0 {
			metrics.statements_unexpected.with_label_values(&["seconded"]).inc();
		}
	}

	/// Update the out-of-view statements counter for unexpected large statements
	pub fn on_unexpected_statement_large(&self) {
		if let Some(metrics) = &self.0 {
			metrics.statements_unexpected.with_label_values(&["large"]).inc();
		}
	}

	/// Report size of a created message.
	pub fn on_created_message(&self, size: usize) {
		if let Some(metrics) = &self.0 {
			metrics.created_message_size.set(size as u64);
		}
	}

	/// Update sent dropped requests counter when request dropped because
	/// of peer rate limit
	pub fn on_request_dropped_peer_rate_limit(&self) {
		if let Some(metrics) = &self.0 {
			metrics.peer_rate_limit_request_drop.inc();
		}
	}

	/// Update max parallel requests reached counter
	/// This counter is updated when the maximum number of parallel requests is reached
	/// and we are waiting for one of the requests to finish
	pub fn on_max_parallel_requests_reached(&self) {
		if let Some(metrics) = &self.0 {
			metrics.max_parallel_requests_reached.inc();
		}
	}

	/// Increment the counter for parallel `AttestedCandidateRequest` fires.
	pub fn on_parallel_fetch_fired(&self) {
		if let Some(metrics) = &self.0 {
			metrics.parallel_fetch_fired.inc();
		}
	}

	/// Increment the counter for cases where a parallel-slot fetch arrived before the
	/// first-slot fetch and provided a valid response. Read together with
	/// `parallel_fetch_fired_total` to see how often parallelism actually pays off.
	pub fn on_parallel_fetch_won(&self) {
		if let Some(metrics) = &self.0 {
			metrics.parallel_fetch_won.inc();
		}
	}

	/// Increment the counter for cases where a parallel second-slot fetch would have been
	/// dispatched but no alternate advertiser was known.
	pub fn on_parallel_fetch_skipped_no_alt_peer(&self) {
		if let Some(metrics) = &self.0 {
			metrics.parallel_fetch_skipped_no_alt_peer.inc();
		}
	}

	/// Observe the end-to-end duration of a single `AttestedCandidateRequest`.
	pub fn on_fetch_completion(&self, slot: FetchSlot, outcome: FetchOutcome, duration: Duration) {
		if let Some(metrics) = &self.0 {
			metrics
				.fetch_completion_seconds
				.with_label_values(&[slot.as_str(), outcome.as_str()])
				.observe(duration.as_secs_f64());
		}
	}

	/// Observe how much it took us to fetch a candidate from when we first learned about it
	/// to when we got a complete response.
	pub fn on_learn_to_fetch(&self, duration: Duration) {
		if let Some(metrics) = &self.0 {
			metrics.learn_to_fetch_seconds.observe(duration.as_secs_f64());
		}
	}
}

impl metrics::Metrics for Metrics {
	fn try_register(
		registry: &prometheus::Registry,
	) -> std::result::Result<Self, prometheus::PrometheusError> {
		let metrics = MetricsInner {
			statements_distributed: prometheus::register(
				prometheus::Counter::new(
					"polkadot_parachain_statements_distributed_total",
					"Number of candidate validity statements distributed to other peers.",
				)?,
				registry,
			)?,
			sent_requests: prometheus::register(
				prometheus::Counter::new(
					"polkadot_parachain_statement_distribution_sent_requests_total",
					"Number of large statement fetching requests sent.",
				)?,
				registry,
			)?,
			received_responses: prometheus::register(
				prometheus::CounterVec::new(
					prometheus::Opts::new(
						"polkadot_parachain_statement_distribution_received_responses_total",
						"Number of received responses for large statement data.",
					),
					&["success"],
				)?,
				registry,
			)?,
			active_leaves_update: prometheus::register(
				prometheus::Histogram::with_opts(
					prometheus::HistogramOpts::new(
						"polkadot_parachain_statement_distribution_active_leaves_update",
						"Time spent within `statement_distribution::active_leaves_update`",
					)
					.buckets(HISTOGRAM_LATENCY_BUCKETS.into()),
				)?,
				registry,
			)?,
			share: prometheus::register(
				prometheus::Histogram::with_opts(
					prometheus::HistogramOpts::new(
						"polkadot_parachain_statement_distribution_share",
						"Time spent within `statement_distribution::share`",
					)
					.buckets(HISTOGRAM_LATENCY_BUCKETS.into()),
				)?,
				registry,
			)?,
			network_bridge_update: prometheus::register(
				prometheus::HistogramVec::new(
					prometheus::HistogramOpts::new(
						"polkadot_parachain_statement_distribution_network_bridge_update",
						"Time spent within `statement_distribution::network_bridge_update`",
					)
					.buckets(HISTOGRAM_LATENCY_BUCKETS.into()),
					&["message_type"],
				)?,
				registry,
			)?,
			statements_unexpected: prometheus::register(
				prometheus::CounterVec::new(
					prometheus::Opts::new(
						"polkadot_parachain_statement_distribution_statements_unexpected",
						"Number of statements that were not expected to be received.",
					),
					&["type"],
				)?,
				registry,
			)?,
			created_message_size: prometheus::register(
				prometheus::Gauge::with_opts(prometheus::Opts::new(
					"polkadot_parachain_statement_distribution_created_message_size",
					"Size of created messages containing Seconded statements.",
				))?,
				registry,
			)?,
			peer_rate_limit_request_drop: prometheus::register(
				prometheus::Counter::new(
					"polkadot_parachain_statement_distribution_peer_rate_limit_request_drop_total",
					"Number of statement distribution requests dropped because of the peer rate limiting.",
				)?,
				registry,
			)?,
			max_parallel_requests_reached: prometheus::register(
				prometheus::Counter::new(
					"polkadot_parachain_statement_distribution_max_parallel_requests_reached_total",
					"Number of times the maximum number of parallel requests was reached.",
				)?,
				registry,
			)?,
			parallel_fetch_fired: prometheus::register(
				prometheus::Counter::new(
					"polkadot_parachain_statement_distribution_parallel_fetch_fired_total",
					"Number of times a parallel (second-slot) AttestedCandidate fetch was \
					 dispatched after the PARALLEL_FETCH_THRESHOLD elapsed without a response on \
					 the first slot.",
				)?,
				registry,
			)?,
			parallel_fetch_won: prometheus::register(
				prometheus::Counter::new(
					"polkadot_parachain_statement_distribution_parallel_fetch_won_total",
					"Number of times a parallel-slot AttestedCandidate fetch arrived before the \
					 first slot and provided a valid response.",
				)?,
				registry,
			)?,
			parallel_fetch_skipped_no_alt_peer: prometheus::register(
				prometheus::Counter::new(
					"polkadot_parachain_statement_distribution_parallel_fetch_skipped_no_alt_peer_total",
					"Number of times a parallel (second-slot) AttestedCandidate fetch could have \
					 been dispatched but no alternate advertiser was available.",
				)?,
				registry,
			)?,
			fetch_completion_seconds: prometheus::register(
				prometheus::HistogramVec::new(
					prometheus::HistogramOpts::new(
						"polkadot_parachain_statement_distribution_fetch_completion_seconds",
						"End-to-end duration of an AttestedCandidate fetch (per slot, per \
						 outcome). Anchored at the 500ms parallel-fetch threshold and the \
						 2500ms hard transport timeout.",
					)
					.buckets(HISTOGRAM_FETCH_BUCKETS.into()),
					&["slot", "outcome"],
				)?,
				registry,
			)?,
			learn_to_fetch_seconds: prometheus::register(
				prometheus::Histogram::with_opts(
					prometheus::HistogramOpts::new(
						"polkadot_parachain_statement_distribution_learn_to_fetch_seconds",
						"Time from first learning about a candidate (manifest received or \
						 cluster statement) to successfully fetching it (Complete response). \
						 Captures queue wait + retry-cooldown + winning-slot fetch RTT \
						 end-to-end.",
					)
					.buckets(HISTOGRAM_LEARN_TO_FETCH_BUCKETS.into()),
				)?,
				registry,
			)?,
		};
		Ok(Metrics(Some(metrics)))
	}
}
