// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

pub use global::GlobalMetrics;
pub use substrate_prometheus_endpoint::{register, Counter, CounterVec, Gauge, GaugeVec, Opts, Registry, F64, U64};

use async_trait::async_trait;
use std::time::Duration;

mod global;

/// Prometheus endpoint MetricsParams.
#[derive(Debug, Clone)]
pub struct MetricsParams {
	/// Serve HTTP requests at given host.
	pub host: String,
	/// Serve HTTP requests at given port.
	pub port: u16,
}

/// Metrics API.
pub trait Metrics: Clone + Send + Sync + 'static {
	/// Register metrics in the registry.
	fn register(&self, registry: &Registry) -> Result<(), String>;
}

/// Standalone metrics API.
///
/// Metrics of this kind know how to update themselves, so we may just spawn and forget the
/// asynchronous self-update task.
#[async_trait]
pub trait StandaloneMetrics: Metrics {
	/// Update metric values.
	async fn update(&self);

	/// Metrics update interval.
	fn update_interval(&self) -> Duration;

	/// Spawn the self update task that will keep update metric value at given intervals.
	fn spawn(self) {
		async_std::task::spawn(async move {
			let update_interval = self.update_interval();
			loop {
				self.update().await;
				async_std::task::sleep(update_interval).await;
			}
		});
	}
}

impl Default for MetricsParams {
	fn default() -> Self {
		MetricsParams {
			host: "127.0.0.1".into(),
			port: 9616,
		}
	}
}
