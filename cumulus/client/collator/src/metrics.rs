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

//! Collator metrics.

use prometheus_endpoint::{register, Histogram, HistogramOpts, Registry};

/// Metrics for the collator.
#[derive(Clone)]
pub struct CollatorMetrics {
	/// Histogram of proof size deviation (actual - reported) in bytes.
	/// Positive = actual proof larger than reported.
	/// Negative = actual proof smaller than reported.
	pub proof_size_deviation: Histogram,
}

impl CollatorMetrics {
	/// Create a new [`CollatorMetrics`] and register it with the given registry.
	pub fn register(registry: &Registry) -> Result<Self, prometheus_endpoint::PrometheusError> {
		let proof_size_deviation = register(
			Histogram::with_opts(HistogramOpts::new(
				"parachain_proof_size_deviation_bytes",
				"Signed deviation between actual PoV proof size and runtime-reported BlockWeight proof size in bytes",
			).buckets(vec![
				-1_000_000.0, -500_000.0, -100_000.0, -50_000.0, -10_000.0, -1_000.0,
				0.0,
				1_000.0, 10_000.0, 50_000.0, 100_000.0, 500_000.0, 1_000_000.0,
			]))?,
			registry,
		)?;

		Ok(Self { proof_size_deviation })
	}
}
