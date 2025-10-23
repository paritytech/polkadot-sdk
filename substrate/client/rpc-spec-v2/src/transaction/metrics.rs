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

use prometheus_endpoint::{
	exponential_buckets, linear_buckets, register, Histogram, HistogramOpts, PrometheusError,
	Registry,
};

use sc_transaction_pool::histograms as tx_pool_histograms;

use super::TransactionEvent;

/// RPC layer metrics for transaction pool.
#[derive(Debug, Clone)]
pub struct Metrics {
	validated: Histogram,
	in_block: Histogram,
	finalized: Histogram,
	dropped: Histogram,
	invalid: Histogram,
	error: Histogram,
}

impl Metrics {
	/// Creates a new [`Metrics`] instance.
	pub fn new(registry: &Registry) -> Result<Self, PrometheusError> {
		let validated = register(
			tx_pool_histograms::ready_future(
				"rpc_transaction_validation_time",
				"RPC Transaction validation time in seconds",
			)?,
			registry,
		)?;

		let in_block = register(
			tx_pool_histograms::in_block(
				"rpc_transaction_in_block_time",
				"RPC Transaction in block time in seconds",
			)?,
			registry,
		)?;

		let finalized = register(
			tx_pool_histograms::finalized(
				"rpc_transaction_finalized_time",
				"RPC Transaction finalized time in seconds",
			)?,
			registry,
		)?;

		let dropped = register(
			tx_pool_histograms::dropped(
				"rpc_transaction_dropped_time",
				"RPC Transaction dropped time in seconds",
			)?,
			registry,
		)?;

		let invalid = register(
			tx_pool_histograms::invalid(
				"rpc_transaction_invalid_time",
				"RPC Transaction invalid time in seconds",
			)?,
			registry,
		)?;

		let error = register(
			tx_pool_histograms::invalid(
				"rpc_transaction_error_time",
				"RPC Transaction error time in seconds",
			)?,
			registry,
		)?;

		Ok(Metrics { validated, in_block, finalized, dropped, invalid, error })
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

		let (histogram, target_state) = match event {
			TransactionEvent::Validated => (&metrics.validated, "validated"),
			TransactionEvent::BestChainBlockIncluded(Some(_)) => (&metrics.in_block, "in_block"),
			TransactionEvent::BestChainBlockIncluded(None) => (&metrics.in_block, "retracted"),
			TransactionEvent::Finalized(..) => (&metrics.finalized, "finalized"),
			TransactionEvent::Error(..) => (&metrics.error, "error"),
			TransactionEvent::Dropped(..) => (&metrics.dropped, "dropped"),
			TransactionEvent::Invalid(..) => (&metrics.invalid, "invalid"),
		};

		// Only record the state if it hasn't been reported before.
		if self.reported_states.insert(target_state) {
			histogram.observe(self.submitted_at.elapsed().as_secs_f64());
		}
	}
}
