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

use crate::LOG_TARGET;
use prometheus_endpoint::{
	exponential_buckets, prometheus::HistogramTimer, register, CounterVec, Gauge, Histogram,
	HistogramOpts, HistogramVec, Opts, PrometheusError, Registry, U64,
};
use sp_price_oracle::market::MarketId;
use std::time::Duration;

/// Metrics of the price oracle service. Recording is a no-op when metrics are disabled.
#[derive(Clone)]
pub(crate) struct Metrics(Option<Inner>);

#[derive(Clone)]
struct Inner {
	ticks: CounterVec<U64>,
	tick_duration: Histogram,
	markets: Gauge<U64>,
	markets_priced: Gauge<U64>,
	market_failures: CounterVec<U64>,
	market_fetch_duration: HistogramVec,
}

impl Metrics {
	/// Register the metrics with `registry`, if given. Without a registry, or if registration
	/// fails, nothing is recorded.
	pub(crate) fn register(registry: Option<&Registry>) -> Self {
		Self(registry.and_then(|registry| {
			Inner::register(registry)
				.map_err(|e| log::warn!(target: LOG_TARGET, "Failed to register metrics: {e}"))
				.ok()
		}))
	}

	/// Count a tick ending with `outcome`.
	pub(crate) fn on_tick(&self, outcome: &str) {
		if let Some(metrics) = &self.0 {
			metrics.ticks.with_label_values(&[outcome]).inc();
		}
	}

	/// Start timing a tick that fetches the markets. The duration is recorded when the timer is
	/// dropped.
	pub(crate) fn tick_timer(&self) -> Option<HistogramTimer> {
		self.0.as_ref().map(|metrics| metrics.tick_duration.start_timer())
	}

	/// Set the number of markets registered at the best block.
	pub(crate) fn on_markets(&self, count: usize) {
		if let Some(metrics) = &self.0 {
			metrics.markets.set(count as u64);
		}
	}

	/// Set the number of markets priced in a tick.
	pub(crate) fn on_markets_priced(&self, count: usize) {
		if let Some(metrics) = &self.0 {
			metrics.markets_priced.set(count as u64);
		}
	}

	/// Count `market` not being priced in a tick for `reason`.
	pub(crate) fn on_market_failure(&self, market: MarketId, reason: &str) {
		if let Some(metrics) = &self.0 {
			metrics
				.market_failures
				.with_label_values(&[&market.0.to_string(), reason])
				.inc();
		}
	}

	/// Record `market` being fetched `elapsed` after the start of a tick.
	pub(crate) fn on_market_fetched(&self, market: MarketId, elapsed: Duration) {
		if let Some(metrics) = &self.0 {
			metrics
				.market_fetch_duration
				.with_label_values(&[&market.0.to_string()])
				.observe(elapsed.as_secs_f64());
		}
	}
}

impl Inner {
	fn register(registry: &Registry) -> Result<Self, PrometheusError> {
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
			markets: register(
				Gauge::new(
					"substrate_price_oracle_markets",
					"Number of markets registered at the best block",
				)?,
				registry,
			)?,
			markets_priced: register(
				Gauge::new(
					"substrate_price_oracle_markets_priced",
					"Number of markets priced in the last tick that fetched the markets",
				)?,
				registry,
			)?,
			market_failures: register(
				CounterVec::new(
					Opts::new(
						"substrate_price_oracle_market_failures_total",
						"Number of times a market was not priced in a tick, by market and reason",
					),
					&["market", "reason"],
				)?,
				registry,
			)?,
			market_fetch_duration: register(
				HistogramVec::new(
					HistogramOpts::new(
						"substrate_price_oracle_market_fetch_duration_seconds",
						"Time from the start of a tick until all queries of a market answered, \
						by market",
					)
					.buckets(exponential_buckets(0.05, 2.0, 10)?),
					&["market"],
				)?,
				registry,
			)?,
		})
	}
}
