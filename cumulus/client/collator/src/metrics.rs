// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

use prometheus_endpoint::{
	register, Counter, Histogram, HistogramOpts, PrometheusError, Registry, U64,
};

#[derive(Clone)]
struct MetricsInner {
	collations_generated_total: Counter<U64>,
	submit_collation: Histogram,
}

/// Collator metrics
#[derive(Default, Clone)]
pub struct Metrics(Option<MetricsInner>);

impl Metrics {
	/// Register metrics with the given Prometheus registry. Returns a no-op `Metrics` on `None`.
	pub fn register(registry: Option<&Registry>) -> Result<Self, PrometheusError> {
		let Some(registry) = registry else { return Ok(Metrics(None)) };
		let inner = MetricsInner {
			collations_generated_total: register(
				Counter::new(
					"polkadot_parachain_collations_generated_total",
					"Number of collations generated.",
				)?,
				registry,
			)?,
			submit_collation: register(
				Histogram::with_opts(HistogramOpts::new(
					"polkadot_parachain_collation_generation_submit_collation",
					"Time spent preparing and submitting a collation to the network protocol",
				))?,
				registry,
			)?,
		};
		Ok(Metrics(Some(inner)))
	}

	/// Increment the per-candidate collation counter by `n`.
	pub fn on_collations_generated(&self, n: u64) {
		if let Some(inner) = &self.0 {
			inner.collations_generated_total.inc_by(n);
		}
	}

	/// Start a timer for the prepare-and-submit span; drops when the span ends.
	pub fn time_submit_collation(&self) -> Option<prometheus_endpoint::prometheus::HistogramTimer> {
		self.0.as_ref().map(|inner| inner.submit_collation.start_timer())
	}
}
