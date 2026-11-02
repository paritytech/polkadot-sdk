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
//! Prometheus metrics of the price oracle service.

use prometheus_endpoint::{
	exponential_buckets, prometheus::HistogramTimer, register, CounterVec, Histogram,
	HistogramOpts, Opts, PrometheusError, Registry, U64,
};

/// Metrics of the price oracle service.
#[derive(Clone)]
pub(crate) struct Metrics {
	/// Ticks by outcome.
	ticks: CounterVec<U64>,
	/// Duration of the ticks that fetched the markets.
	tick_duration: Histogram,
}

impl Metrics {
	/// Register the metrics with `registry`.
	pub(crate) fn register(registry: &Registry) -> Result<Self, PrometheusError> {
		Ok(Self {
			ticks: register(
				CounterVec::new(
					Opts::new("substrate_price_oracle_ticks_total", "Number of ticks by outcome"),
					&["outcome"],
				)?,
				registry,
			)?,
			tick_duration: register(
				Histogram::with_opts(
					HistogramOpts::new(
						"substrate_price_oracle_tick_duration_seconds",
						"Duration of the ticks that fetched the markets",
					)
					.buckets(exponential_buckets(0.1, 2.0, 10)?),
				)?,
				registry,
			)?,
		})
	}

	/// Count a tick ending with `outcome`.
	pub(crate) fn on_tick(&self, outcome: &str) {
		self.ticks.with_label_values(&[outcome]).inc();
	}

	/// Start timing a tick that fetches the markets. The duration is recorded when the timer is
	/// dropped.
	pub(crate) fn tick_timer(&self) -> HistogramTimer {
		self.tick_duration.start_timer()
	}
}
