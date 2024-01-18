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
use sp_core::H256;
use std::time::{Duration, Instant};

use super::{configuration::TestConfiguration, display::MetricCollection, network::NetworkEmulatorHandle};

/// A test run.
#[derive(Clone)]
pub struct TestRun {
	// The configuration used in the test.
	config: TestConfiguration,
	// When it started.
	start: Instant,
    // Effective test start.
    effective_start: Option<Instant>,
	// Total time spent running tests. This is usually smaller than `end` - `start` which includes
	// any per test or per block orchestration delay. 
	effective_test_time: Duration,
	// When it ended.
	end: Option<Instant>,
	// Prometheus stats snapshot at end of test.
	metrics: Option<MetricCollection>,
	// Subsystems under test.
	subsystems: Vec<String>,
    // A handle to the test network.
    network: Option<NetworkEmulatorHandle>,
}

impl TestRun {
    /// Create a new test run with the `configuration` and `subsystems` under test.
	pub fn start(config: TestConfiguration, subsystems: &[&str]) -> TestRun {
		let subsystems = subsystems.into_iter().cloned().map(String::from).collect::<Vec<_>>();
		TestRun {
			config,
			effective_start: None,
			effective_test_time: Duration::ZERO,
			start: Instant::now(),
			end: None,
			metrics: None,
			subsystems,
            network: None,
		}
	}

    /// Start record effective test time.
    pub fn effective_start(&mut self) {
        self.effective_start = Some(Instant::now());
    }

    /// Start record effective test time.
    pub fn effective_stop(&mut self) {
        match self.effective_start {
            None => gum::warn!(target: LOG_TARGET, "effective_stop called without start"),
            Some(start) => {
                self.effective_test_time.saturating_add(start.elapsed());
            }
        }
        self.effective_start = None;
    }

    /// Record the test stop.
    pub fn stop(&mut self, metrics: MetricCollection, network: NetworkEmulatorHandle) {
        self.end = Some(Instant::now());
        self.network = Some(network);
        self.metrics = Some(metrics);
    }

    // Display test run information in CLI.
    pub fn display(&self) {
        println
    }
}


	/// Display network usage stats.
	pub fn display_network_usage(&self) {
		let stats = self.network().peer_stats(0);

		let total_node_received = stats.received() / 1024;
		let total_node_sent = stats.sent() / 1024;

		println!(
			"\nPayload bytes received from peers: {}, {}",
			format!("{:.2} KiB total", total_node_received).blue(),
			format!("{:.2} KiB/block", total_node_received / self.config().num_blocks)
				.bright_blue()
		);

		println!(
			"Payload bytes sent to peers: {}, {}",
			format!("{:.2} KiB total", total_node_sent).blue(),
			format!("{:.2} KiB/block", total_node_sent / self.config().num_blocks).bright_blue()
		);
	}

	/// Print CPU usage stats in the CLI.
	pub fn display_cpu_usage(&self, subsystems_under_test: &[&str]) {
		let test_metrics = super::display::parse_metrics(self.registry());

		for subsystem in subsystems_under_test.iter() {
			let subsystem_cpu_metrics =
				test_metrics.subset_with_label_value("task_group", subsystem);
			let total_cpu = subsystem_cpu_metrics.sum_by("substrate_tasks_polling_duration_sum");
			println!(
				"{} CPU usage {}",
				subsystem.to_string().bright_green(),
				format!("{:.3}s", total_cpu).bright_purple()
			);
			println!(
				"{} CPU usage per block {}",
				subsystem.to_string().bright_green(),
				format!("{:.3}s", total_cpu / self.config().num_blocks as f64).bright_purple()
			);
		}

		let test_env_cpu_metrics =
			test_metrics.subset_with_label_value("task_group", "test-environment");
		let total_cpu = test_env_cpu_metrics.sum_by("substrate_tasks_polling_duration_sum");
		println!(
			"Total test environment CPU usage {}",
			format!("{:.3}s", total_cpu).bright_purple()
		);
		println!(
			"Test environment CPU usage per block {}",
			format!("{:.3}s", total_cpu / self.config().num_blocks as f64).bright_purple()
		)
	}
    
/// An utility to manage test runs and their results.
pub struct BenchmarkReport {
	run_id: H256,
}

impl BenchmarkReport {
	pub fn new() -> BenchmarkReport {
		BenchmarkReport { run_id: H256::random() }
	}
}
