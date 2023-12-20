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
use pyroscope::PyroscopeAgent;
use pyroscope_pprofrs::{pprof_backend, PprofConfig};

use colored::Colorize;
use std::{path::Path, time::Duration};

pub(crate) mod availability;
pub(crate) mod cli;
pub(crate) mod core;

use availability::{prepare_test, NetworkEmulation, TestState};
use cli::TestObjective;

use core::{
	configuration::TestConfiguration,
	environment::{TestEnvironment, GENESIS_HASH},
};

use clap_num::number_range;

use crate::core::display::display_configuration;

fn le_100(s: &str) -> Result<usize, String> {
	number_range(s, 0, 100)
}

fn le_5000(s: &str) -> Result<usize, String> {
	number_range(s, 0, 5000)
}

#[derive(Debug, Parser)]
#[allow(missing_docs)]
struct BenchCli {
	#[arg(long, value_enum, ignore_case = true, default_value_t = NetworkEmulation::Ideal)]
	/// The type of network to be emulated
	pub network: NetworkEmulation,

	#[clap(flatten)]
	pub standard_configuration: cli::StandardTestOptions,

	#[clap(short, long)]
	/// The bandwidth of simulated remote peers in KiB
	pub peer_bandwidth: Option<usize>,

	#[clap(short, long)]
	/// The bandwidth of our simulated node in KiB
	pub bandwidth: Option<usize>,

	#[clap(long, value_parser=le_100)]
	/// Simulated conection error ratio [0-100].
	pub peer_error: Option<usize>,

	#[clap(long, value_parser=le_5000)]
	/// Minimum remote peer latency in milliseconds [0-5000].
	pub peer_min_latency: Option<u64>,

	#[clap(long, value_parser=le_5000)]
	/// Maximum remote peer latency in milliseconds [0-5000].
	pub peer_max_latency: Option<u64>,

	#[clap(long, default_value_t = false)]
	/// Enable CPU Profiling with Pyroscope
	pub profile: bool,

	#[clap(long, requires = "profile", default_value_t = String::from("http://localhost:4040"))]
	/// Pyroscope Server URL
	pub pyroscope_url: String,

	#[clap(long, requires = "profile", default_value_t = 113)]
	/// Pyroscope Sample Rate
	pub pyroscope_sample_rate: u32,

	#[command(subcommand)]
	pub objective: cli::TestObjective,
}

impl BenchCli {
	fn launch(self) -> eyre::Result<()> {
		let agent_running = if self.profile {
			let agent = PyroscopeAgent::builder(self.pyroscope_url.as_str(), "subsystem-bench")
				.backend(pprof_backend(PprofConfig::new().sample_rate(self.pyroscope_sample_rate)))
				.build()?;

			Some(agent.start()?)
		} else {
			None
		};

		let configuration = self.standard_configuration;
		let mut test_config = match self.objective {
			TestObjective::TestSequence(options) => {
				let test_sequence =
					core::configuration::TestSequence::new_from_file(Path::new(&options.path))
						.expect("File exists")
						.into_vec();
				let num_steps = test_sequence.len();
				gum::info!(
					"{}",
					format!("Sequence contains {} step(s)", num_steps).bright_purple()
				);
				for (index, test_config) in test_sequence.into_iter().enumerate() {
					gum::info!("{}", format!("Step {}/{}", index + 1, num_steps).bright_purple(),);
					display_configuration(&test_config);

					let mut state = TestState::new(&test_config);
					let (mut env, _protocol_config) = prepare_test(test_config, &mut state);
					env.runtime()
						.block_on(availability::benchmark_availability_read(&mut env, state));
				}
				return Ok(())
			},
			TestObjective::DataAvailabilityRead(ref _options) => match self.network {
				NetworkEmulation::Healthy => TestConfiguration::healthy_network(
					self.objective,
					configuration.num_blocks,
					configuration.n_validators,
					configuration.n_cores,
					configuration.min_pov_size,
					configuration.max_pov_size,
				),
				NetworkEmulation::Degraded => TestConfiguration::degraded_network(
					self.objective,
					configuration.num_blocks,
					configuration.n_validators,
					configuration.n_cores,
					configuration.min_pov_size,
					configuration.max_pov_size,
				),
				NetworkEmulation::Ideal => TestConfiguration::ideal_network(
					self.objective,
					configuration.num_blocks,
					configuration.n_validators,
					configuration.n_cores,
					configuration.min_pov_size,
					configuration.max_pov_size,
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

		display_configuration(&test_config);

		let mut state = TestState::new(&test_config);
		let (mut env, _protocol_config) = prepare_test(test_config, &mut state);
		// test_config.write_to_disk();
		env.runtime()
			.block_on(availability::benchmark_availability_read(&mut env, state));

		if let Some(agent_running) = agent_running {
			let agent_ready = agent_running.stop()?;
			agent_ready.shutdown();
		}

		Ok(())
	}
}

fn main() -> eyre::Result<()> {
	color_eyre::install()?;
	env_logger::builder()
		.filter(Some("hyper"), log::LevelFilter::Info)
		// Avoid `Terminating due to subsystem exit subsystem` warnings
		.filter(Some("polkadot_overseer"), log::LevelFilter::Error)
		.filter(None, log::LevelFilter::Info)
		// .filter(None, log::LevelFilter::Trace)
		.try_init()
		.unwrap();

	let cli: BenchCli = BenchCli::parse();
	cli.launch()?;
	Ok(())
}
