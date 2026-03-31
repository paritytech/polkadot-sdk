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

//! Prometheus metrics for the statement store RPC handler

use std::sync::Arc;

use prometheus_endpoint::{
	register, CounterVec, Gauge, Histogram, HistogramOpts, Opts, PrometheusError, Registry, U64,
};

#[derive(Clone, Default)]
pub struct MetricsLink(pub(crate) Arc<Option<Metrics>>);

impl MetricsLink {
	pub fn new(registry: Option<&Registry>) -> Self {
		Self(Arc::new(registry.and_then(|registry| {
			Metrics::register(registry)
				.map_err(|err| {
					log::warn!("Failed to register statement RPC prometheus metrics: {}", err);
				})
				.ok()
		})))
	}

	pub fn report(&self, do_this: impl FnOnce(&Metrics)) {
		if let Some(metrics) = self.0.as_ref() {
			do_this(metrics);
		}
	}
}

pub struct Metrics {
	pub submit_calls_total: CounterVec<U64>,
	pub submit_duration_seconds: Histogram,
	pub subscribe_calls_total: CounterVec<U64>,
	pub active_subscriptions: Gauge<U64>,
}

impl Metrics {
	pub fn register(registry: &Registry) -> Result<Self, PrometheusError> {
		Ok(Self {
			submit_calls_total: register(
				CounterVec::new(
					Opts::new(
						"substrate_sub_statement_store_rpc_submit_calls_total",
						"Total statement RPC submit calls by result",
					),
					&["result"],
				)?,
				registry,
			)?,
			submit_duration_seconds: register(
				Histogram::with_opts(
					HistogramOpts::new(
						"substrate_sub_statement_store_rpc_submit_duration_seconds",
						"End-to-end latency of the statement RPC submit call",
					)
					.buckets(vec![0.000_01, 0.000_1, 0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0]),
				)?,
				registry,
			)?,
			subscribe_calls_total: register(
				CounterVec::new(
					Opts::new(
						"substrate_sub_statement_store_rpc_subscribe_calls_total",
						"Total statement RPC subscribe calls by result",
					),
					&["result"],
				)?,
				registry,
			)?,
			active_subscriptions: register(
				Gauge::new(
					"substrate_sub_statement_store_rpc_active_subscriptions",
					"Current number of active statement RPC subscriptions",
				)?,
				registry,
			)?,
		})
	}
}
