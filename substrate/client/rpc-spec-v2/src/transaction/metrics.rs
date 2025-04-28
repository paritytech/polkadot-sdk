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

//! Metrics for recording transaction events.

use std::time::Instant;

use prometheus_endpoint::{
	register, CounterVec, HistogramOpts, HistogramVec, Opts, PrometheusError, Registry, U64,
};

use super::TransactionEvent;

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

/// Labels for transaction status.
mod labels {
	/// The initial state of the transaction.
	pub const SUBMITTED: &str = "submitted";

	/// Represents the `TransactionEvent::Validated` event.
	pub const VALIDATED: &str = "validated";

	/// Represents the `TransactionEvent::BestChainBlockIncluded(Some (..))` event.
	pub const IN_BLOCK: &str = "in_block";

	/// Represents the `TransactionEvent::BestChainBlockIncluded(None)` event.
	pub const RETRACTED: &str = "retracted";

	/// Represents the `TransactionEvent::Finalized` event.
	pub const FINALIZED: &str = "finalized";

	/// Represents the `TransactionEvent::Error` event.
	pub const ERROR: &str = "error";

	/// Represents the `TransactionEvent::Invalid` event.
	pub const INVALID: &str = "invalid";

	/// Represents the `TransactionEvent::Dropped` event.
	pub const DROPPED: &str = "dropped";
}

/// Convert a transaction event to a metric label.
fn transaction_event_label<Hash>(event: &TransactionEvent<Hash>) -> &'static str {
	match event {
		TransactionEvent::Validated => labels::VALIDATED,
		TransactionEvent::BestChainBlockIncluded(Some(_)) => labels::IN_BLOCK,
		TransactionEvent::BestChainBlockIncluded(None) => labels::RETRACTED,
		TransactionEvent::Finalized(..) => labels::FINALIZED,
		TransactionEvent::Error(..) => labels::ERROR,
		TransactionEvent::Dropped(..) => labels::DROPPED,
		TransactionEvent::Invalid(..) => labels::INVALID,
	}
}

#[derive(Debug, Clone)]
pub struct ExecutionState {
	/// The time when the transaction was submitted.
	submitted_at: Instant,
	/// The time when the transaction entered this state.
	started_at: Instant,
	/// The initial state.
	initial_state: &'static str,
}

impl ExecutionState {
	/// Creates a new [`ExecutionState`].
	pub fn new() -> Self {
		Self {
			submitted_at: Instant::now(),
			started_at: Instant::now(),
			initial_state: labels::SUBMITTED,
		}
	}

	/// Advance the state of the transaction.
	fn advance_state(&mut self, state: &'static str) {
		self.initial_state = state;
		self.started_at = Instant::now();
	}
}

/// RPC layer metrics for transaction pool.
#[derive(Debug, Clone)]
pub struct Metrics {
	/// Counter for transaction status.
	pub status: CounterVec<U64>,

	/// Histogram for transaction execution time in each event.
	execution_time: HistogramVec,
}

impl Metrics {
	/// Creates a new [`TransactionMetrics`] instance.
	pub fn new(registry: &Registry) -> Result<Self, PrometheusError> {
		let status = register(
			CounterVec::new(
				Opts::new("rpc_transaction_status", "Number of transactions by status"),
				&["state"],
			)?,
			registry,
		)?;

		let execution_time = register(
			HistogramVec::new(
				HistogramOpts::new(
					"rpc_transaction_execution_time",
					"Transaction execution time in each event",
				)
				.buckets(HISTOGRAM_BUCKETS.to_vec()),
				&["initial_state", "final_state"],
			)?,
			registry,
		)?;

		// The execution state will be initialized when the transaction is submitted.
		Ok(Metrics { status, execution_time })
	}
}

/// Transaction metrics for a single transaction instance.
pub struct InstanceMetrics {
	metrics: Option<Metrics>,

	/// The execution state of the transaction.
	execution_state: ExecutionState,
}

impl InstanceMetrics {
	/// Creates a new [`InstanceMetrics`] instance.
	pub fn new(metrics: Option<Metrics>) -> Self {
		if let Some(ref metrics) = metrics {
			// Register the initial state of the transaction.
			metrics.status.with_label_values(&[labels::SUBMITTED]).inc();
		}

		Self { metrics, execution_state: ExecutionState::new() }
	}

	/// Record the execution time of a transaction state.
	///
	/// This represents how long it took for the transaction to move to the next state.
	///
	/// The method must be called before the transaction event is provided to the user.
	pub fn register_event<Hash>(&mut self, event: &TransactionEvent<Hash>) {
		let Some(ref metrics) = self.metrics else {
			return;
		};

		let final_state = transaction_event_label(event);
		if final_state == labels::FINALIZED {
			let elapsed = self.execution_state.submitted_at.elapsed().as_micros() as f64;
			metrics
				.execution_time
				.with_label_values(&[labels::SUBMITTED, final_state])
				.observe(elapsed);
		}

		metrics.status.with_label_values(&[final_state]).inc();

		let elapsed = self.execution_state.started_at.elapsed().as_micros() as f64;
		metrics
			.execution_time
			.with_label_values(&[self.execution_state.initial_state, final_state])
			.observe(elapsed);

		self.execution_state.advance_state(final_state);
	}
}
