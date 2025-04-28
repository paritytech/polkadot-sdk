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

use std::{collections::HashSet, time::Instant};

use prometheus_endpoint::{register, HistogramOpts, HistogramVec, PrometheusError, Registry};

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
/// RPC layer metrics for transaction pool.
#[derive(Debug, Clone)]
pub struct Metrics {
	/// This measures the time it took the transaction since the moment it was
	/// submitted until a target state was reached.
	execution_time: HistogramVec,
}

impl Metrics {
	/// Creates a new [`TransactionMetrics`] instance.
	pub fn new(registry: &Registry) -> Result<Self, PrometheusError> {
		let execution_time = register(
			HistogramVec::new(
				HistogramOpts::new(
					"rpc_transaction_execution_time",
					"Transaction execution time since submitted to a target state",
				)
				.buckets(HISTOGRAM_BUCKETS.to_vec()),
				&["target_state"],
			)?,
			registry,
		)?;

		// The execution state will be initialized when the transaction is submitted.
		Ok(Metrics { execution_time })
	}
}

/// Transaction metrics for a single transaction instance.
pub struct InstanceMetrics {
	/// The metrics instance.
	metrics: Option<Metrics>,
	/// The time when the transaction was submitted.
	submitted_at: Instant,
	/// Ensure the states are reported once.
	reported_states: HashSet<&'static str>,
}

impl InstanceMetrics {
	/// Creates a new [`InstanceMetrics`] instance.
	pub fn new(metrics: Option<Metrics>) -> Self {
		Self { metrics, submitted_at: Instant::now(), reported_states: HashSet::new() }
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

		let target_state = transaction_event_label(event);
		if self.reported_states.insert(target_state) {
			let elapsed = self.submitted_at.elapsed().as_micros() as f64;
			metrics.execution_time.with_label_values(&[target_state]).observe(elapsed);
		}
	}
}
