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

//! RPC middleware to collect prometheus metrics on RPC calls.

use std::time::Instant;

use jsonrpsee::{types::Request, MethodResponse};
use prometheus_endpoint::{
	register, Counter, CounterVec, HistogramOpts, HistogramVec, Opts, PrometheusError, Registry,
	U64,
};

/// Histogram time buckets in microseconds.
const HISTOGRAM_BUCKETS: [f64; 11] = [
	5.0,
	25.0,
	100.0,
	500.0,
	1_000.0,
	2_500.0,
	10_000.0,
	25_000.0,
	100_000.0,
	1_000_000.0,
	10_000_000.0,
];

/// Metrics for RPC middleware storing information about the number of requests started/completed,
/// calls started/completed and their timings.
#[derive(Debug, Clone)]
pub struct RpcMetrics {
	/// Histogram over RPC execution times.
	calls_time: HistogramVec,
	/// Number of calls started.
	calls_started: CounterVec<U64>,
	/// Number of calls completed.
	calls_finished: CounterVec<U64>,
	/// Number of Websocket sessions opened.
	ws_sessions_opened: Option<Counter<U64>>,
	/// Number of Websocket sessions closed.
	ws_sessions_closed: Option<Counter<U64>>,
	/// Histogram over RPC websocket sessions.
	ws_sessions_time: HistogramVec,
}

impl RpcMetrics {
	/// Create an instance of metrics
	pub fn new(metrics_registry: Option<&Registry>) -> Result<Option<Self>, PrometheusError> {
		if let Some(metrics_registry) = metrics_registry {
			Ok(Some(Self {
				calls_time: register(
					HistogramVec::new(
						HistogramOpts::new(
							"substrate_rpc_calls_time",
							"Total time [μs] of processed RPC calls",
						)
						.buckets(HISTOGRAM_BUCKETS.to_vec()),
						&["protocol", "method", "is_rate_limited"],
					)?,
					metrics_registry,
				)?,
				calls_started: register(
					CounterVec::new(
						Opts::new(
							"substrate_rpc_calls_started",
							"Number of received RPC calls (unique un-batched requests)",
						),
						&["protocol", "method"],
					)?,
					metrics_registry,
				)?,
				calls_finished: register(
					CounterVec::new(
						Opts::new(
							"substrate_rpc_calls_finished",
							"Number of processed RPC calls (unique un-batched requests)",
						),
						&["protocol", "method", "is_error", "is_rate_limited"],
					)?,
					metrics_registry,
				)?,
				ws_sessions_opened: register(
					Counter::new(
						"substrate_rpc_sessions_opened",
						"Number of persistent RPC sessions opened",
					)?,
					metrics_registry,
				)?
				.into(),
				ws_sessions_closed: register(
					Counter::new(
						"substrate_rpc_sessions_closed",
						"Number of persistent RPC sessions closed",
					)?,
					metrics_registry,
				)?
				.into(),
				ws_sessions_time: register(
					HistogramVec::new(
						HistogramOpts::new(
							"substrate_rpc_sessions_time",
							"Total time [s] for each websocket session",
						)
						.buckets(HISTOGRAM_BUCKETS.to_vec()),
						&["protocol"],
					)?,
					metrics_registry,
				)?,
			}))
		} else {
			Ok(None)
		}
	}

	pub(crate) fn ws_connect(&self) {
		self.ws_sessions_opened.as_ref().map(|counter| counter.inc());
	}

	pub(crate) fn ws_disconnect(&self, now: Instant) {
		let micros = now.elapsed().as_secs();

		self.ws_sessions_closed.as_ref().map(|counter| counter.inc());
		self.ws_sessions_time.with_label_values(&["ws"]).observe(micros as _);
	}

	pub(crate) fn on_call(&self, req: &Request, transport_label: &'static str) {
		log::trace!(
			target: "rpc_metrics",
			"[{transport_label}] on_call name={} params={:?}",
			req.method_name(),
			req.params(),
		);

		self.calls_started
			.with_label_values(&[transport_label, req.method_name()])
			.inc();
	}

	pub(crate) fn on_response(
		&self,
		req: &Request,
		rp: &MethodResponse,
		is_rate_limited: bool,
		transport_label: &'static str,
		now: Instant,
	) {
		log::trace!(target: "rpc_metrics", "[{transport_label}] on_response started_at={:?}", now);
		log::trace!(target: "rpc_metrics::extra", "[{transport_label}] result={}", rp.as_result());

		let micros = now.elapsed().as_micros();
		log::debug!(
			target: "rpc_metrics",
			"[{transport_label}] {} call took {} μs",
			req.method_name(),
			micros,
		);
		self.calls_time
			.with_label_values(&[
				transport_label,
				req.method_name(),
				if is_rate_limited { "true" } else { "false" },
			])
			.observe(micros as _);
		self.calls_finished
			.with_label_values(&[
				transport_label,
				req.method_name(),
				// the label "is_error", so `success` should be regarded as false
				// and vice-versa to be registered correctly.
				if rp.is_success() { "false" } else { "true" },
				if is_rate_limited { "true" } else { "false" },
			])
			.inc();
	}
}

/// Metrics with transport label.
#[derive(Clone, Debug)]
pub struct Metrics {
	pub(crate) inner: RpcMetrics,
	pub(crate) transport_label: &'static str,
}

impl Metrics {
	/// Create a new [`Metrics`].
	pub fn new(metrics: RpcMetrics, transport_label: &'static str) -> Self {
		Self { inner: metrics, transport_label }
	}

	pub(crate) fn ws_connect(&self) {
		self.inner.ws_connect();
	}

	pub(crate) fn ws_disconnect(&self, now: Instant) {
		self.inner.ws_disconnect(now)
	}

	pub(crate) fn on_call(&self, req: &Request) {
		self.inner.on_call(req, self.transport_label)
	}

	pub(crate) fn on_response(
		&self,
		req: &Request,
		rp: &MethodResponse,
		is_rate_limited: bool,
		now: Instant,
	) {
		self.inner.on_response(req, rp, is_rate_limited, self.transport_label, now)
	}
}
