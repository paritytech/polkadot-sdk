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

use crate::{tick::TickOutcome, LOG_TARGET};
use prometheus_endpoint::{
	exponential_buckets, prometheus::HistogramTimer, register, CounterVec, Gauge, GaugeVec,
	Histogram, HistogramOpts, HistogramVec, Opts, PrometheusError, Registry, I64, U64,
};
use sp_price_oracle::{market::MarketId, PairId};
use std::time::Duration;

/// Metrics of the price oracle service. Recording is a no-op when metrics are disabled.
#[derive(Clone)]
pub(crate) struct Metrics(Option<Inner>);

#[derive(Clone)]
struct Inner {
	ticks: CounterVec<U64>,
	tick_duration: Histogram,
	market_results: CounterVec<U64>,
	market_fetch_duration: HistogramVec,
	pair_market_margins: GaugeVec<I64>,
	received_reports: CounterVec<U64>,
	pool_reports: Gauge<U64>,
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
	pub(crate) fn on_tick(&self, outcome: TickOutcome) {
		if let Some(metrics) = &self.0 {
			metrics.ticks.with_label_values(&[outcome.label()]).inc();
		}
	}

	/// Start timing a tick that fetches the markets. The duration is recorded when the timer is
	/// dropped.
	pub(crate) fn tick_timer(&self) -> Option<HistogramTimer> {
		self.0.as_ref().map(|metrics| metrics.tick_duration.start_timer())
	}

	/// Count `market` ending a tick with `result`: `priced`, how the fetch failed, or the reason
	/// the runtime did not price it, prefixed with `parse_`.
	pub(crate) fn on_market_result(&self, market: MarketId, result: &str) {
		if let Some(metrics) = &self.0 {
			metrics.market_results.with_label_values(&[&market.0.to_string(), result]).inc();
		}
	}

	/// Record `market` being fetched `elapsed` after the start of a tick.
	/// Only markets whose responses were delivered are recorded; failed fetches are counted in
	/// the market results.
	pub(crate) fn on_market_fetched(&self, market: MarketId, elapsed: Duration) {
		if let Some(metrics) = &self.0 {
			metrics
				.market_fetch_duration
				.with_label_values(&[&market.0.to_string()])
				.observe(elapsed.as_secs_f64());
		}
	}

	/// Drop the market margins of the previous tick, so pairs no longer aggregated disappear.
	pub(crate) fn reset_pair_market_margins(&self) {
		if let Some(metrics) = &self.0 {
			metrics.pair_market_margins.reset();
		}
	}

	/// Set the `markets` that went into `pair` in a tick beyond its `quorum`.
	pub(crate) fn on_pair_market_margin(&self, pair: PairId, markets: u32, quorum: u32) {
		if let Some(metrics) = &self.0 {
			metrics
				.pair_market_margins
				.with_label_values(&[&pair.0.to_string()])
				.set(i64::from(markets) - i64::from(quorum));
		}
	}

	/// Count a report received from a peer with validation `result`.
	pub(crate) fn on_received_report(&self, result: &str) {
		if let Some(metrics) = &self.0 {
			metrics.received_reports.with_label_values(&[result]).inc();
		}
	}

	/// Set the number of reports in the pool.
	pub(crate) fn on_pool_reports(&self, count: usize) {
		if let Some(metrics) = &self.0 {
			metrics.pool_reports.set(count as u64);
		}
	}
}

impl Inner {
	fn register(registry: &Registry) -> Result<Self, PrometheusError> {
		let duration_buckets = exponential_buckets(0.05, 2.0, 10)?;
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
					.buckets(duration_buckets.clone()),
				)?,
				registry,
			)?,
			market_results: register(
				CounterVec::new(
					Opts::new(
						"substrate_price_oracle_market_results_total",
						"Number of ticks a market ended with a result, by market and result: priced, \
						how the fetch failed, or the reason the runtime did not price it, prefixed \
						with parse_",
					),
					&["market", "result"],
				)?,
				registry,
			)?,
			market_fetch_duration: register(
				HistogramVec::new(
					HistogramOpts::new(
						"substrate_price_oracle_market_fetch_duration_seconds",
						"Time from the start of a tick until the responses of a market were \
						delivered for parsing, by market. Failed fetches are not recorded",
					)
					.buckets(duration_buckets),
					&["market"],
				)?,
				registry,
			)?,
			pair_market_margins: register(
				GaugeVec::new(
					Opts::new(
						"substrate_price_oracle_pair_market_margin",
						"Markets that went into a pair beyond its quorum in the last tick that \
						aggregated, by pair",
					),
					&["pair"],
				)?,
				registry,
			)?,
			received_reports: register(
				CounterVec::new(
					Opts::new(
						"substrate_price_oracle_received_reports_total",
						"Number of reports received from peers, by validation result",
					),
					&["result"],
				)?,
				registry,
			)?,
			pool_reports: register(
				Gauge::new(
					"substrate_price_oracle_pool_reports",
					"Number of reports in the pool at the start of the last tick",
				)?,
				registry,
			)?,
		})
	}
}
