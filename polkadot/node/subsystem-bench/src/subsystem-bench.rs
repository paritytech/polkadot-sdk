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
use std::path::Path;

pub(crate) mod availability;
pub(crate) mod cli;
pub(crate) mod core;

use crate::availability::{prepare_test, NetworkEmulation, TestState};
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
	/// The bandwidth of emulated remote peers in KiB
	pub peer_bandwidth: Option<usize>,

	#[clap(short, long)]
	/// The bandwidth of our node in KiB
	pub bandwidth: Option<usize>,

	#[clap(long, value_parser=le_100)]
	/// Emulated peer connection ratio [0-100].
	pub connectivity: Option<usize>,

	#[clap(long, value_parser=le_5000)]
	/// Mean remote peer latency in milliseconds [0-5000].
	pub peer_mean_latency: Option<usize>,

	#[clap(long, value_parser=le_5000)]
	/// Remote peer latency standard deviation
	pub peer_latency_std_dev: Option<f64>,

	#[command(subcommand)]
	pub objective: cli::TestObjective,
}

impl BenchCli {
	fn create_test_configuration(&self) -> TestConfiguration {
		let configuration = &self.standard_configuration;

		match self.network {
			NetworkEmulation::Healthy => TestConfiguration::healthy_network(
				self.objective.clone(),
				configuration.num_blocks,
				configuration.n_validators,
				configuration.n_cores,
				configuration.min_pov_size,
				configuration.max_pov_size,
			),
			NetworkEmulation::Degraded => TestConfiguration::degraded_network(
				self.objective.clone(),
				configuration.num_blocks,
				configuration.n_validators,
				configuration.n_cores,
				configuration.min_pov_size,
				configuration.max_pov_size,
			),
			NetworkEmulation::Ideal => TestConfiguration::ideal_network(
				self.objective.clone(),
				configuration.num_blocks,
				configuration.n_validators,
				configuration.n_cores,
				configuration.min_pov_size,
				configuration.max_pov_size,
			),
		}
	}

	fn launch(self) -> eyre::Result<()> {
		let mut test_config = match self.objective {
			TestObjective::TestSequence(options) => {
				let test_sequence =
					core::configuration::TestSequence::new_from_file(Path::new(&options.path))
						.expect("File exists")
						.to_vec();
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
			TestObjective::DataAvailabilityRead(ref _options) => self.create_test_configuration(),
			TestObjective::DataAvailabilityWrite => self.create_test_configuration(),
		};

		let mut latency_config = test_config.latency.clone().unwrap_or_default();

		if let Some(latency) = self.peer_mean_latency {
			latency_config.mean_latency_ms = latency;
		}

		if let Some(std_dev) = self.peer_latency_std_dev {
			latency_config.std_dev = std_dev;
		}

		// Write back the updated latency.
		test_config.latency = Some(latency_config);

		if let Some(connectivity) = self.connectivity {
			test_config.connectivity = connectivity;
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

		match self.objective {
			TestObjective::DataAvailabilityRead(_options) => {
				env.runtime()
					.block_on(availability::benchmark_availability_read(&mut env, state));
			},
			TestObjective::DataAvailabilityWrite => {
				env.runtime()
					.block_on(availability::benchmark_availability_write(&mut env, state));
			},
			TestObjective::TestSequence(_options) => {},
		}

		Ok(())
	}
}

fn main() -> eyre::Result<()> {
	color_eyre::install()?;
	let _ = env_logger::builder()
		.filter(Some("hyper"), log::LevelFilter::Info)
		// Avoid `Terminating due to subsystem exit subsystem` warnings
		.filter(Some("polkadot_overseer"), log::LevelFilter::Error)
		.filter(None, log::LevelFilter::Info)
		.format_timestamp_millis()
		// .filter(None, log::LevelFilter::Trace)
		.try_init()
		.unwrap();

	let cli: BenchCli = BenchCli::parse();
	cli.launch()?;
	Ok(())
}
