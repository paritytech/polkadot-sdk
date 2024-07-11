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

//! Transaction pool Prometheus metrics for single-state transaction pool.

use crate::common::metrics::{GenericMetricsLink, MetricsRegistrant};
use prometheus_endpoint::{register, Counter, PrometheusError, Registry, U64};

pub type MetricsLink = GenericMetricsLink<Metrics>;

/// Transaction pool Prometheus metrics.
pub struct Metrics {
	pub submitted_transactions: Counter<U64>,
	pub validations_invalid: Counter<U64>,
	pub block_transactions_pruned: Counter<U64>,
	pub block_transactions_resubmitted: Counter<U64>,
}

impl MetricsRegistrant for Metrics {
	fn register(registry: &Registry) -> Result<Box<Self>, PrometheusError> {
		Ok(Box::from(Self {
			submitted_transactions: register(
				Counter::new(
					"substrate_sub_txpool_submitted_transactions",
					"Total number of transactions submitted",
				)?,
				registry,
			)?,
			validations_invalid: register(
				Counter::new(
					"substrate_sub_txpool_validations_invalid",
					"Total number of transactions that were removed from the pool as invalid",
				)?,
				registry,
			)?,
			block_transactions_pruned: register(
				Counter::new(
					"substrate_sub_txpool_block_transactions_pruned",
					"Total number of transactions that was requested to be pruned by block events",
				)?,
				registry,
			)?,
			block_transactions_resubmitted: register(
				Counter::new(
					"substrate_sub_txpool_block_transactions_resubmitted",
					"Total number of transactions that was requested to be resubmitted by block events",
				)?,
				registry,
			)?,
		}))
	}
}
