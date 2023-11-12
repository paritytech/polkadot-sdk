use super::*;
use polkadot_node_subsystem_util::metrics::{
	self,
	prometheus::{self, Counter, Gauge, Histogram, Opts, PrometheusError, Registry, U64},
};

/// Test environment/configuration metrics
#[derive(Clone)]
pub struct TestEnvironmentMetrics {
	/// Number of bytes sent per peer.
	n_validators: Gauge<U64>,
	/// Number of received sent per peer.
	n_cores: Gauge<U64>,
	/// PoV size
	pov_size: Gauge<U64>,
	/// Current loop
	current_loop: Gauge<U64>,
}

impl TestEnvironmentMetrics {
	pub fn new(registry: &Registry) -> Result<Self, PrometheusError> {
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
					"Number of cores we fetch availability for each loop",
				)?,
				registry,
			)?,
			pov_size: prometheus::register(
				Gauge::new("subsystem_benchmark_pov_size", "The pov size")?,
				registry,
			)?,
			current_loop: prometheus::register(
				Gauge::new("subsystem_benchmark_current_loop", "The current test loop")?,
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

	pub fn set_current_loop(&self, current_loop: usize) {
		self.current_loop.set(current_loop as u64);
	}

	pub fn set_pov_size(&self, pov_size: usize) {
		self.pov_size.set(pov_size as u64);
	}
}
