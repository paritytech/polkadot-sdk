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
use prometheus_endpoint::{
	histogram_opts, linear_buckets, register, Counter, Gauge, Histogram, PrometheusError, Registry,
	U64,
};

pub type MetricsLink = GenericMetricsLink<Metrics>;

/// Transaction pool Prometheus metrics.
pub struct Metrics {
	pub submitted_transactions: Counter<U64>,
	pub active_views: Gauge<U64>,
	pub inactive_views: Gauge<U64>,
	pub watched_txs: Gauge<U64>,
	pub unwatched_txs: Gauge<U64>,
	pub removed_invalid_txs: Counter<U64>,
	pub finalized_txs: Counter<U64>,
	pub maintain_duration: Histogram,
	pub resubmitted_retracted_txs: Counter<U64>,
	pub submitted_from_mempool_txs: Counter<U64>,
	pub mempool_revalidation_invalid_txs: Counter<U64>,
	pub view_revalidation_invalid_txs: Counter<U64>,
	pub view_revalidation_resubmitted_txs: Counter<U64>,
	pub view_revalidation_duration: Histogram,
	pub non_cloned_views: Counter<U64>,
}

impl MetricsRegistrant for Metrics {
	fn register(registry: &Registry) -> Result<Box<Self>, PrometheusError> {
		Ok(Box::from(Self {
			submitted_transactions: register(
				Counter::new(
					"substrate_sub_txpool_submitted_txs_total",
					"Total number of transactions submitted",
				)?,
				registry,
			)?,
			active_views: register(
				Gauge::new(
					"substrate_sub_txpool_active_views",
					"Total number of currently maintained views.",
				)?,
				registry,
			)?,
			inactive_views: register(
				Gauge::new(
					"substrate_sub_txpool_inactive_views",
					"Total number of current inactive views.",
				)?,
				registry,
			)?,
			watched_txs: register(
				Gauge::new(
					"substrate_sub_txpool_watched_txs",
					"Total number of watched transactions in txpool.",
				)?,
				registry,
			)?,
			unwatched_txs: register(
				Gauge::new(
					"substrate_sub_txpool_unwatched_txs",
					"Total number of unwatched transactions in txpool.",
				)?,
				registry,
			)?,
			removed_invalid_txs: register(
				Counter::new(
					"substrate_sub_txpool_removed_invalid_txs_total",
					"Total number of transactions reported as invalid.",
				)?,
				registry,
			)?,
			finalized_txs: register(
				Counter::new(
					"substrate_sub_txpool_finalized_txs_total",
					"Total number of finalized transactions.",
				)?,
				registry,
			)?,
			maintain_duration: register(
				Histogram::with_opts(histogram_opts!(
					"substrate_sub_txpool_maintain_duration_seconds",
					"Histogram of maintain durations.",
					linear_buckets(0.0, 0.25, 13).unwrap()
				))?,
				registry,
			)?,
			resubmitted_retracted_txs: register(
				Counter::new(
					"substrate_sub_txpool_resubmitted_retracted_txs_total",
					"Total number of transactions resubmitted from retracted forks.",
				)?,
				registry,
			)?,
			submitted_from_mempool_txs: register(
				Counter::new(
					"substrate_sub_txpool_submitted_from_mempool_txs_total",
					"Total number of transactions submitted from mempool to views.",
				)?,
				registry,
			)?,
			mempool_revalidation_invalid_txs: register(
				Counter::new(
					"substrate_sub_txpool_mempool_revalidation_invalid_txs_total",
					"Total number of transactions found as invalid during mempool revalidation.",
				)?,
				registry,
			)?,
			view_revalidation_invalid_txs: register(
				Counter::new(
					"substrate_sub_txpool_view_revalidation_invalid_txs_total",
					"Total number of transactions found as invalid during view revalidation.",
				)?,
				registry,
			)?,
			view_revalidation_resubmitted_txs: register(
				Counter::new(
					"substrate_sub_txpool_view_revalidation_resubmitted_txs_total",
					"Total number of valid transactions processed during view revalidation.",
				)?,
				registry,
			)?,
			view_revalidation_duration: register(
				Histogram::with_opts(histogram_opts!(
					"substrate_sub_txpool_view_revalidation_duration_seconds",
					"Histogram of view revalidation durations.",
					linear_buckets(0.0, 0.25, 13).unwrap()
				))?,
				registry,
			)?,
			non_cloned_views: register(
				Counter::new(
					"substrate_sub_txpool_non_cloned_views_total",
					"Total number of the views created w/o cloning existing view.",
				)?,
				registry,
			)?,
		}))
	}
}
