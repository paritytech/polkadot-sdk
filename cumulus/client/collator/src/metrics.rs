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

use polkadot_primitives::Id as ParaId;
use prometheus_endpoint::{
	register, Counter, CounterVec, Histogram, HistogramOpts, HistogramVec, Opts, PrometheusError,
	Registry, U64,
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
	pub fn register(registry: Option<&Registry>, para_id: ParaId) -> Result<Self, PrometheusError> {
		let Some(registry) = registry else { return Ok(Metrics(None)) };
		let para_id = para_id.to_string();

		let collations_generated_total = register(
			CounterVec::new(
				Opts::new(
					"polkadot_parachain_collations_generated_total",
					"Number of collations generated.",
				),
				&["para_id"],
			)?,
			registry,
		)?;
		let submit_collation = register(
			HistogramVec::new(
				HistogramOpts::new(
					"polkadot_parachain_collation_generation_submit_collation",
					"Time spent preparing and submitting a collation to the network protocol",
				),
				&["para_id"],
			)?,
			registry,
		)?;

		let inner = MetricsInner {
			collations_generated_total: collations_generated_total.with_label_values(&[&para_id]),
			submit_collation: submit_collation.with_label_values(&[&para_id]),
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
