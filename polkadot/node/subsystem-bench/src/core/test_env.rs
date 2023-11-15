// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use polkadot_node_subsystem_util::metrics::prometheus::{
	self, Gauge, Histogram, PrometheusError, Registry, U64,
};

const MIB: f64 = 1024.0 * 1024.0;

/// Test environment/configuration metrics
#[derive(Clone)]
pub struct TestEnvironmentMetrics {
	/// Number of bytes sent per peer.
	n_validators: Gauge<U64>,
	/// Number of received sent per peer.
	n_cores: Gauge<U64>,
	/// PoV size
	pov_size: Histogram,
	/// Current block
	current_block: Gauge<U64>,
}

impl TestEnvironmentMetrics {
	pub fn new(registry: &Registry) -> Result<Self, PrometheusError> {
		let mut buckets = prometheus::exponential_buckets(16384.0, 2.0, 9)
			.expect("arguments are always valid; qed");
		buckets.extend(vec![5.0 * MIB, 6.0 * MIB, 7.0 * MIB, 8.0 * MIB, 9.0 * MIB, 10.0 * MIB]);

		Ok(Self {
			n_validators: prometheus::register(
				Gauge::new(
					"subsystem_benchmark_n_validators",
					"Total number of validators in the test",
				)?,
				registry,
			)?,
			n_cores: prometheus::register(
				Gauge::new(
					"subsystem_benchmark_n_cores",
					"Number of cores we fetch availability for each block",
				)?,
				registry,
			)?,
			current_block: prometheus::register(
				Gauge::new("subsystem_benchmark_current_block", "The current test block")?,
				registry,
			)?,
			pov_size: prometheus::register(
				Histogram::with_opts(
					prometheus::HistogramOpts::new(
						"subsystem_benchmark_pov_size",
						"The compressed size of the proof of validity of a candidate",
					)
					.buckets(buckets),
				)?,
				registry,
			)?,
		})
	}

	pub fn set_n_validators(&self, n_validators: usize) {
		self.n_validators.set(n_validators as u64);
	}

	pub fn set_n_cores(&self, n_cores: usize) {
		self.n_cores.set(n_cores as u64);
	}

	pub fn set_current_block(&self, current_block: usize) {
		self.current_block.set(current_block as u64);
	}

	pub fn on_pov_size(&self, pov_size: usize) {
		self.pov_size.observe(pov_size as f64);
	}
}
