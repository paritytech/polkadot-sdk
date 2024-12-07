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

use polkadot_node_subsystem::prometheus::HistogramVec;
use polkadot_node_subsystem_util::metrics::{
	self,
	prometheus::{
		self, prometheus::HistogramTimer, Counter, CounterVec, Histogram, Opts, PrometheusError,
		Registry, U64,
	},
};

/// Availability Distribution metrics.
#[derive(Clone, Default)]
pub struct Metrics(Option<MetricsInner>);

#[derive(Clone)]
struct MetricsInner {
	/// Number of sent chunk requests.
	///
	/// Gets incremented on each sent chunk requests.
	///
	/// Split by chunk type:
	/// - `regular_chunks`
	/// - `systematic_chunks`
	chunk_requests_issued: CounterVec<U64>,

	/// Total number of bytes recovered
	///
	/// Gets incremented on each successful recovery
	recovered_bytes_total: Counter<U64>,

	/// A counter for finished chunk requests.
	///
	/// Split by the chunk type (`regular_chunks` or `systematic_chunks`)
	///
	/// Also split by result:
	/// - `no_such_chunk` ... peer did not have the requested chunk
	/// - `timeout` ... request timed out.
	/// - `error` ... Some networking issue except timeout
	/// - `invalid` ... Chunk was received, but not valid.
	/// - `success`
	chunk_requests_finished: CounterVec<U64>,

	/// A counter for successful chunk requests, split by the network protocol version.
	chunk_request_protocols: CounterVec<U64>,

	/// Number of sent available data requests.
	full_data_requests_issued: Counter<U64>,

	/// Counter for finished available data requests.
	///
	/// Split by the result type:
	///
	/// - `no_such_data` ... peer did not have the requested data
	/// - `timeout` ... request timed out.
	/// - `error` ... Some networking issue except timeout
	/// - `invalid` ... data was received, but not valid.
	/// - `success`
	full_data_requests_finished: CounterVec<U64>,

	/// The duration of request to response.
	///
	/// Split by chunk type (`regular_chunks` or `systematic_chunks`).
	time_chunk_request: HistogramVec,

	/// The duration between the pure recovery and verification.
	///
	/// Split by recovery type (`regular_chunks`, `systematic_chunks` or `full_from_backers`).
	time_erasure_recovery: HistogramVec,

	/// How much time it takes to reconstruct the available data from chunks.
	///
	/// Split by chunk type (`regular_chunks` or `systematic_chunks`), as the algorithms are
	/// different.
	time_erasure_reconstruct: HistogramVec,

	/// How much time it takes to re-encode the data into erasure chunks in order to verify
	/// the root hash of the provided Merkle tree. See `reconstructed_data_matches_root`.
	time_reencode_chunks: Histogram,

	/// Time of a full recovery, including erasure decoding or until we gave
	/// up.
	time_full_recovery: Histogram,

	/// Number of full recoveries that have been finished one way or the other.
	///
	/// Split by recovery `strategy_type` (`full_from_backers, systematic_chunks, regular_chunks,
	/// all`). `all` is used for failed recoveries that tried all available strategies.
	/// Also split by `result` type.
	full_recoveries_finished: CounterVec<U64>,

	/// Number of full recoveries that have been started on this subsystem.
	///
	/// Note: Those are only recoveries which could not get served locally already - so in other
	/// words: Only real recoveries.
	full_recoveries_started: Counter<U64>,
}

impl Metrics {
	/// Create new dummy metrics, not reporting anything.
	pub fn new_dummy() -> Self {
		Metrics(None)
	}

	/// Increment counter for chunk requests.
	pub fn on_chunk_request_issued(&self, chunk_type: &str) {
		if let Some(metrics) = &self.0 {
			metrics.chunk_requests_issued.with_label_values(&[chunk_type]).inc()
		}
	}

	/// Increment counter for full data requests.
	pub fn on_full_request_issued(&self) {
		if let Some(metrics) = &self.0 {
			metrics.full_data_requests_issued.inc()
		}
	}

	/// A chunk request timed out.
	pub fn on_chunk_request_timeout(&self, chunk_type: &str) {
		if let Some(metrics) = &self.0 {
			metrics
				.chunk_requests_finished
				.with_label_values(&[chunk_type, "timeout"])
				.inc()
		}
	}

	/// A full data request timed out.
	pub fn on_full_request_timeout(&self) {
		if let Some(metrics) = &self.0 {
			metrics.full_data_requests_finished.with_label_values(&["timeout"]).inc()
		}
	}

	/// A chunk request failed because validator did not have its chunk.
	pub fn on_chunk_request_no_such_chunk(&self, chunk_type: &str) {
		if let Some(metrics) = &self.0 {
			metrics
				.chunk_requests_finished
				.with_label_values(&[chunk_type, "no_such_chunk"])
				.inc()
		}
	}

	/// A full data request failed because the validator did not have it.
	pub fn on_full_request_no_such_data(&self) {
		if let Some(metrics) = &self.0 {
			metrics.full_data_requests_finished.with_label_values(&["no_such_data"]).inc()
		}
	}

	/// A chunk request failed for some non timeout related network error.
	pub fn on_chunk_request_error(&self, chunk_type: &str) {
		if let Some(metrics) = &self.0 {
			metrics.chunk_requests_finished.with_label_values(&[chunk_type, "error"]).inc()
		}
	}

	/// A full data request failed for some non timeout related network error.
	pub fn on_full_request_error(&self) {
		if let Some(metrics) = &self.0 {
			metrics.full_data_requests_finished.with_label_values(&["error"]).inc()
		}
	}

	/// A chunk request succeeded, but was not valid.
	pub fn on_chunk_request_invalid(&self, chunk_type: &str) {
		if let Some(metrics) = &self.0 {
			metrics
				.chunk_requests_finished
				.with_label_values(&[chunk_type, "invalid"])
				.inc()
		}
	}

	/// A full data request succeeded, but was not valid.
	pub fn on_full_request_invalid(&self) {
		if let Some(metrics) = &self.0 {
			metrics.full_data_requests_finished.with_label_values(&["invalid"]).inc()
		}
	}

	/// A chunk request succeeded.
	pub fn on_chunk_request_succeeded(&self, chunk_type: &str) {
		if let Some(metrics) = &self.0 {
			metrics
				.chunk_requests_finished
				.with_label_values(&[chunk_type, "success"])
				.inc()
		}
	}

	/// A chunk response was received on the v1 protocol.
	pub fn on_chunk_response_v1(&self) {
		if let Some(metrics) = &self.0 {
			metrics.chunk_request_protocols.with_label_values(&["v1"]).inc()
		}
	}

	/// A chunk response was received on the v2 protocol.
	pub fn on_chunk_response_v2(&self) {
		if let Some(metrics) = &self.0 {
			metrics.chunk_request_protocols.with_label_values(&["v2"]).inc()
		}
	}

	/// A full data request succeeded.
	pub fn on_full_request_succeeded(&self) {
		if let Some(metrics) = &self.0 {
			metrics.full_data_requests_finished.with_label_values(&["success"]).inc()
		}
	}

	/// Get a timer to time request/response duration.
	pub fn time_chunk_request(&self, chunk_type: &str) -> Option<HistogramTimer> {
		self.0.as_ref().map(|metrics| {
			metrics.time_chunk_request.with_label_values(&[chunk_type]).start_timer()
		})
	}

	/// Get a timer to time erasure code recover.
	pub fn time_erasure_recovery(&self, chunk_type: &str) -> Option<HistogramTimer> {
		self.0.as_ref().map(|metrics| {
			metrics.time_erasure_recovery.with_label_values(&[chunk_type]).start_timer()
		})
	}

	/// Get a timer for available data reconstruction.
	pub fn time_erasure_reconstruct(&self, chunk_type: &str) -> Option<HistogramTimer> {
		self.0.as_ref().map(|metrics| {
			metrics.time_erasure_reconstruct.with_label_values(&[chunk_type]).start_timer()
		})
	}

	/// Get a timer to time chunk encoding.
	pub fn time_reencode_chunks(&self) -> Option<HistogramTimer> {
		self.0.as_ref().map(|metrics| metrics.time_reencode_chunks.start_timer())
	}

	/// Get a timer to measure the time of the complete recovery process.
	pub fn time_full_recovery(&self) -> Option<HistogramTimer> {
		self.0.as_ref().map(|metrics| metrics.time_full_recovery.start_timer())
	}

	/// A full recovery succeeded.
	pub fn on_recovery_succeeded(&self, strategy_type: &str, bytes: usize) {
		if let Some(metrics) = &self.0 {
			metrics
				.full_recoveries_finished
				.with_label_values(&["success", strategy_type])
				.inc();
			metrics.recovered_bytes_total.inc_by(bytes as u64)
		}
	}

	/// A full recovery failed (data not available).
	pub fn on_recovery_failed(&self, strategy_type: &str) {
		if let Some(metrics) = &self.0 {
			metrics
				.full_recoveries_finished
				.with_label_values(&["failure", strategy_type])
				.inc()
		}
	}

	/// A full recovery failed (data was recovered, but invalid).
	pub fn on_recovery_invalid(&self, strategy_type: &str) {
		if let Some(metrics) = &self.0 {
			metrics
				.full_recoveries_finished
				.with_label_values(&["invalid", strategy_type])
				.inc()
		}
	}

	/// A recover was started.
	pub fn on_recovery_started(&self) {
		if let Some(metrics) = &self.0 {
			metrics.full_recoveries_started.inc()
		}
	}
}

impl metrics::Metrics for Metrics {
	fn try_register(registry: &Registry) -> Result<Self, PrometheusError> {
		let metrics = MetricsInner {
			chunk_requests_issued: prometheus::register(
				CounterVec::new(
					Opts::new("polkadot_parachain_availability_recovery_chunk_requests_issued",
					"Total number of issued chunk requests."),
					&["type"]
				)?,
				registry,
			)?,
			full_data_requests_issued: prometheus::register(
				Counter::new(
					"polkadot_parachain_availability_recovery_full_data_requests_issued",
					"Total number of issued full data requests.",
				)?,
				registry,
			)?,
			recovered_bytes_total: prometheus::register(
				Counter::new(
					"polkadot_parachain_availability_recovery_bytes_total",
					"Total number of bytes recovered",
				)?,
				registry,
			)?,
			chunk_requests_finished: prometheus::register(
				CounterVec::new(
					Opts::new(
						"polkadot_parachain_availability_recovery_chunk_requests_finished",
						"Total number of chunk requests finished.",
					),
					&["result", "type"],
				)?,
				registry,
			)?,
			chunk_request_protocols: prometheus::register(
				CounterVec::new(
					Opts::new(
						"polkadot_parachain_availability_recovery_chunk_request_protocols",
						"Total number of successful chunk requests, mapped by the protocol version (v1 or v2).",
					),
					&["protocol"],
				)?,
				registry,
			)?,
			full_data_requests_finished: prometheus::register(
				CounterVec::new(
					Opts::new(
						"polkadot_parachain_availability_recovery_full_data_requests_finished",
						"Total number of full data requests finished.",
					),
					&["result"],
				)?,
				registry,
			)?,
			time_chunk_request: prometheus::register(
				prometheus::HistogramVec::new(prometheus::HistogramOpts::new(
					"polkadot_parachain_availability_recovery_time_chunk_request",
					"Time spent waiting for a response to a chunk request",
				), &["type"])?,
				registry,
			)?,
			time_erasure_recovery: prometheus::register(
				prometheus::HistogramVec::new(prometheus::HistogramOpts::new(
					"polkadot_parachain_availability_recovery_time_erasure_recovery",
					"Time spent to recover the erasure code and verify the merkle root by re-encoding as erasure chunks",
				), &["type"])?,
				registry,
			)?,
			time_erasure_reconstruct: prometheus::register(
				prometheus::HistogramVec::new(prometheus::HistogramOpts::new(
					"polkadot_parachain_availability_recovery_time_erasure_reconstruct",
					"Time spent to reconstruct the data from chunks",
				), &["type"])?,
				registry,
			)?,
			time_reencode_chunks: prometheus::register(
				prometheus::Histogram::with_opts(prometheus::HistogramOpts::new(
					"polkadot_parachain_availability_reencode_chunks",
					"Time spent re-encoding the data as erasure chunks",
				))?,
				registry,
			)?,
			time_full_recovery: prometheus::register(
				prometheus::Histogram::with_opts(prometheus::HistogramOpts::new(
					"polkadot_parachain_availability_recovery_time_total",
					"Time a full recovery process took, either until failure or successful erasure decoding.",
				))?,
				registry,
			)?,
			full_recoveries_finished: prometheus::register(
				CounterVec::new(
					Opts::new(
						"polkadot_parachain_availability_recovery_recoveries_finished",
						"Total number of recoveries that finished.",
					),
					&["result", "strategy_type"],
				)?,
				registry,
			)?,
			full_recoveries_started: prometheus::register(
				Counter::new(
					"polkadot_parachain_availability_recovery_recoveries_started",
					"Total number of started recoveries.",
				)?,
				registry,
			)?,
		};
		Ok(Metrics(Some(metrics)))
	}
}
