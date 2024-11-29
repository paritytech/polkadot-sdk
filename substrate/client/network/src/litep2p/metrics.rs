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

//! Metrics implementation for litep2p.

use litep2p::metrics::{
	MetricCounter, MetricCounterT, MetricGauge, MetricGaugeT, MetricsRegistryT,
};
use prometheus_endpoint::{Counter, Gauge, Registry, U64};

use std::sync::Arc;

/// A registry for metrics that uses the Prometheus metrics crate.
pub struct Litep2pMetricsRegistry {
	registry: Registry,
}

impl Litep2pMetricsRegistry {
	/// Create a new [`Litep2pMetricsRegistry`] from provided registry.
	pub fn from_registry(registry: Registry) -> Self {
		Self { registry }
	}
}

impl MetricsRegistryT for Litep2pMetricsRegistry {
	fn register_counter(
		&self,
		name: String,
		help: String,
	) -> Result<MetricCounter, litep2p::error::Error> {
		let counter = Counter::<U64>::new(name, help)
			.map_err(|err| litep2p::error::Error::MetricError(err.to_string()))?;
		self.registry
			.register(Box::new(counter.clone()))
			.map_err(|err| litep2p::error::Error::MetricError(err.to_string()))?;
		Ok(Arc::new(Litep2pCounter { counter }))
	}

	fn register_gauge(
		&self,
		name: String,
		help: String,
	) -> Result<MetricGauge, litep2p::error::Error> {
		let gauge = Gauge::<U64>::new(name, help)
			.map_err(|err| litep2p::error::Error::MetricError(err.to_string()))?;
		self.registry
			.register(Box::new(gauge.clone()))
			.map_err(|err| litep2p::error::Error::MetricError(err.to_string()))?;

		Ok(Arc::new(Litep2pGauge { gauge }))
	}
}

/// Litep2p counter.
struct Litep2pCounter {
	counter: Counter<U64>,
}

impl MetricCounterT for Litep2pCounter {
	fn inc(&self, value: u64) {
		self.counter.inc_by(value);
	}
}

/// Litep2p gauge.
struct Litep2pGauge {
	gauge: Gauge<U64>,
}

impl MetricGaugeT for Litep2pGauge {
	fn set(&self, value: u64) {
		self.gauge.set(value);
	}

	fn inc(&self) {
		self.gauge.inc();
	}

	fn dec(&self) {
		self.gauge.dec();
	}
}
