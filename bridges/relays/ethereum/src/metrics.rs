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

pub use substrate_prometheus_endpoint::{register, Gauge, GaugeVec, Opts, Registry, F64, U64};

use std::net::SocketAddr;
use substrate_prometheus_endpoint::init_prometheus;
use sysinfo::{ProcessExt, System, SystemExt};

/// Prometheus endpoint MetricsParams.
#[derive(Debug, Clone)]
pub struct MetricsParams {
	/// Serve HTTP requests at given host.
	pub host: String,
	/// Serve HTTP requests at given port.
	pub port: u16,
}

/// Global Prometheus metrics.
#[derive(Debug)]
pub struct GlobalMetrics {
	system: System,
	system_average_load: GaugeVec<F64>,
	process_cpu_usage_percentage: Gauge<F64>,
	process_memory_usage_bytes: Gauge<U64>,
}

/// Start Prometheus endpoint with given metrics registry.
pub async fn start(params: MetricsParams, registry: Registry) -> Result<(), String> {
	init_prometheus(
		SocketAddr::new(
			params
				.host
				.parse()
				.map_err(|err| format!("Invalid Prometheus host {}: {}", params.host, err))?,
			params.port,
		),
		registry,
	)
	.await
	.map_err(|err| format!("Error starting Prometheus endpoint: {}", err))
}

impl Default for MetricsParams {
	fn default() -> Self {
		MetricsParams {
			host: "127.0.0.1".into(),
			port: 9616,
		}
	}
}

impl GlobalMetrics {
	/// Creates global metrics.
	pub fn new() -> Self {
		GlobalMetrics {
			system: System::new(),
			system_average_load: GaugeVec::new(Opts::new("system_average_load", "System load average"), &["over"])
				.expect("metric is static and thus valid; qed"),
			process_cpu_usage_percentage: Gauge::new("process_cpu_usage_percentage", "Process CPU usage")
				.expect("metric is static and thus valid; qed"),
			process_memory_usage_bytes: Gauge::new(
				"process_memory_usage_bytes",
				"Process memory (resident set size) usage",
			)
			.expect("metric is static and thus valid; qed"),
		}
	}

	/// Registers global metrics in the metrics registry.
	pub fn register(&self, registry: &Registry) -> Result<(), String> {
		register(self.system_average_load.clone(), registry).map_err(|e| e.to_string())?;
		register(self.process_cpu_usage_percentage.clone(), registry).map_err(|e| e.to_string())?;
		register(self.process_memory_usage_bytes.clone(), registry).map_err(|e| e.to_string())?;
		Ok(())
	}

	/// Update metrics.
	pub fn update(&mut self) {
		// update system-wide metrics
		let load = self.system.get_load_average();
		self.system_average_load.with_label_values(&["1min"]).set(load.one);
		self.system_average_load.with_label_values(&["5min"]).set(load.five);
		self.system_average_load.with_label_values(&["15min"]).set(load.fifteen);

		// update process-related metrics
		let pid = sysinfo::get_current_pid().expect(
			"only fails where pid is unavailable (os=unknown || arch=wasm32);\
				relay is not supposed to run in such MetricsParamss;\
				qed",
		);
		let is_process_refreshed = self.system.refresh_process(pid);
		match (is_process_refreshed, self.system.get_process(pid)) {
			(true, Some(process_info)) => {
				self.process_cpu_usage_percentage.set(process_info.cpu_usage() as f64);
				self.process_memory_usage_bytes.set(process_info.memory() * 1024);
			}
			_ => {
				log::warn!(
					target: "bridge",
					"Failed to refresh process information. Metrics may show obsolete values",
				);
			}
		}
	}
}
