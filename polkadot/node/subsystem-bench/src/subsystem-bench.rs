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

//! A tool for running subsystem benchmark tests designed for development and
//! CI regression testing.
use clap::Parser;
use color_eyre::eyre;

use colored::Colorize;
use std::{path::Path, time::Duration};

pub(crate) mod availability;
pub(crate) mod core;

use availability::{random_pov_size, TestConfiguration, TestEnvironment, TestState};
const LOG_TARGET: &str = "subsystem-bench";

use clap_num::number_range;

fn le_100(s: &str) -> Result<usize, String> {
	number_range(s, 0, 100)
}

fn le_5000(s: &str) -> Result<usize, String> {
	number_range(s, 0, 5000)
}

#[derive(Debug, clap::Parser, Clone)]
#[clap(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub struct NetworkOptions {}

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq)]
#[value(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum NetworkEmulation {
	Ideal,
	Healthy,
	Degraded,
}

#[derive(Debug, clap::Parser)]
#[clap(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub struct DataAvailabilityReadOptions {
	#[clap(long, ignore_case = true, default_value_t = 100)]
	/// Number of cores to fetch availability for.
	pub n_cores: usize,

	#[clap(long, ignore_case = true, default_value_t = 500)]
	/// Number of validators to fetch chunks from.
	pub n_validators: usize,

	#[clap(long, ignore_case = true, default_value_t = 5120)]
	/// The minimum pov size in KiB
	pub min_pov_size: usize,

	#[clap(long, ignore_case = true, default_value_t = 5120)]
	/// The maximum pov size bytes
	pub max_pov_size: usize,

	#[clap(short, long, default_value_t = false)]
	/// Turbo boost AD Read by fetching from backers first. Tipically this is only faster if nodes
	/// have enough bandwidth.
	pub fetch_from_backers: bool,

	#[clap(short, long, ignore_case = true, default_value_t = 1)]
	/// Number of times to block fetching for each core.
	pub num_blocks: usize,
}

#[derive(Debug, clap::Parser)]
#[clap(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub struct TestSequenceOptions {
	#[clap(short, long, ignore_case = true)]
	pub path: String,
}

/// Define the supported benchmarks targets
#[derive(Debug, Parser)]
#[command(about = "Test objectives", version, rename_all = "kebab-case")]
enum TestObjective {
	/// Benchmark availability recovery strategies.
	DataAvailabilityRead(DataAvailabilityReadOptions),
	/// Run a test sequence specified in a file
	TestSequence(TestSequenceOptions),
}

#[derive(Debug, Parser)]
#[allow(missing_docs)]
struct BenchCli {
	#[arg(long, value_enum, ignore_case = true, default_value_t = NetworkEmulation::Ideal)]
	/// The type of network to be emulated
	pub network: NetworkEmulation,

	#[clap(short, long)]
	/// The bandwidth of simulated remote peers in KiB
	pub peer_bandwidth: Option<usize>,

	#[clap(short, long)]
	/// The bandwidth of our simulated node in KiB
	pub bandwidth: Option<usize>,

	#[clap(long, value_parser=le_100)]
	/// Simulated connection error rate [0-100].
	pub peer_error: Option<usize>,

	#[clap(long, value_parser=le_5000)]
	/// Minimum remote peer latency in milliseconds [0-5000].
	pub peer_min_latency: Option<u64>,

	#[clap(long, value_parser=le_5000)]
	/// Maximum remote peer latency in milliseconds [0-5000].
	pub peer_max_latency: Option<u64>,

	#[command(subcommand)]
	pub objective: TestObjective,
}

fn new_runtime() -> tokio::runtime::Runtime {
	tokio::runtime::Builder::new_multi_thread()
		.thread_name("subsystem-bench")
		.enable_all()
		.thread_stack_size(3 * 1024 * 1024)
		.build()
		.unwrap()
}

impl BenchCli {
	fn launch(self) -> eyre::Result<()> {
		use prometheus::Registry;
		use pyroscope::PyroscopeAgent;
		use pyroscope_pprofrs::{pprof_backend, PprofConfig};

		// Pyroscope must be running on port 4040
		// See https://grafana.com/docs/pyroscope/latest/get-started/#download-and-configure-pyroscope
		let agent = PyroscopeAgent::builder("http://localhost:4040", "subsystem-bench")
			.backend(pprof_backend(PprofConfig::new().sample_rate(100)))
			.build()?;
		let agent_running = agent.start()?;

		let runtime = new_runtime();

		let mut test_config = match self.objective {
			TestObjective::TestSequence(options) => {
				let test_sequence =
					availability::TestSequence::new_from_file(Path::new(&options.path))
						.expect("File exists")
						.to_vec();
				let num_steps = test_sequence.len();
				gum::info!(
					"{}",
					format!("Sequence contains {} step(s)", num_steps).bright_purple()
				);
				for (index, test_config) in test_sequence.into_iter().enumerate() {
					gum::info!(
						"{}",
						format!("Current step {}/{}", index + 1, num_steps).bright_purple()
					);

					let candidate_count = test_config.n_cores * test_config.num_blocks;

					let mut state = TestState::new(test_config);
					state.generate_candidates(candidate_count);
					let mut env =
						TestEnvironment::new(runtime.handle().clone(), state, Registry::new());

					runtime.block_on(availability::bench_chunk_recovery(&mut env));
				}
				return Ok(())
			},
			TestObjective::DataAvailabilityRead(options) => match self.network {
				NetworkEmulation::Healthy => TestConfiguration::healthy_network(
					options.num_blocks,
					options.fetch_from_backers,
					options.n_validators,
					options.n_cores,
					(0..options.n_cores)
						.map(|_| {
							random_pov_size(
								options.min_pov_size * 1024,
								options.max_pov_size * 1024,
							)
						})
						.collect(),
				),
				NetworkEmulation::Degraded => TestConfiguration::degraded_network(
					options.num_blocks,
					options.fetch_from_backers,
					options.n_validators,
					options.n_cores,
					(0..options.n_cores)
						.map(|_| {
							random_pov_size(
								options.min_pov_size * 1024,
								options.max_pov_size * 1024,
							)
						})
						.collect(),
				),
				NetworkEmulation::Ideal => TestConfiguration::ideal_network(
					options.num_blocks,
					options.fetch_from_backers,
					options.n_validators,
					options.n_cores,
					(0..options.n_cores)
						.map(|_| {
							random_pov_size(
								options.min_pov_size * 1024,
								options.max_pov_size * 1024,
							)
						})
						.collect(),
				),
			},
		};

		let mut latency_config = test_config.latency.clone().unwrap_or_default();

		if let Some(latency) = self.peer_min_latency {
			latency_config.min_latency = Duration::from_millis(latency);
		}

		if let Some(latency) = self.peer_max_latency {
			latency_config.max_latency = Duration::from_millis(latency);
		}

		if let Some(error) = self.peer_error {
			test_config.error = error;
		}

		if let Some(bandwidth) = self.peer_bandwidth {
			// CLI expects bw in KiB
			test_config.peer_bandwidth = bandwidth * 1024;
		}

		if let Some(bandwidth) = self.bandwidth {
			// CLI expects bw in KiB
			test_config.bandwidth = bandwidth * 1024;
		}

		let candidate_count = test_config.n_cores * test_config.num_blocks;
		test_config.write_to_disk();

		let mut state = TestState::new(test_config);
		state.generate_candidates(candidate_count);
		let mut env = TestEnvironment::new(runtime.handle().clone(), state, Registry::new());

		runtime.block_on(availability::bench_chunk_recovery(&mut env));

		let agent_ready = agent_running.stop()?;
		agent_ready.shutdown();

		Ok(())
	}
}

fn main() -> eyre::Result<()> {
	color_eyre::install()?;
	let _ = env_logger::builder()
		.filter(Some("hyper"), log::LevelFilter::Info)
		.filter(None, log::LevelFilter::Info)
		.try_init()
		.unwrap();

	let cli: BenchCli = BenchCli::parse();
	cli.launch()?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
}
