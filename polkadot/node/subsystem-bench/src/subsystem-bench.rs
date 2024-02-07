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

use colored::Colorize;

use color_eyre::eyre;
use pyroscope::PyroscopeAgent;
use pyroscope_pprofrs::{pprof_backend, PprofConfig};

use std::path::Path;

pub(crate) mod approval;
pub(crate) mod availability;
pub(crate) mod cli;
pub(crate) mod core;
mod valgrind;

const LOG_TARGET: &str = "subsystem-bench";

use availability::{prepare_test, NetworkEmulation, TestState};
use cli::TestObjective;

use core::{
	configuration::TestConfiguration,
	environment::{TestEnvironment, GENESIS_HASH},
};

use clap_num::number_range;

use crate::{approval::bench_approvals, core::display::display_configuration};

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

	#[clap(long, default_value_t = false)]
	/// Enable CPU Profiling with Pyroscope
	pub profile: bool,

	#[clap(long, requires = "profile", default_value_t = String::from("http://localhost:4040"))]
	/// Pyroscope Server URL
	pub pyroscope_url: String,

	#[clap(long, requires = "profile", default_value_t = 113)]
	/// Pyroscope Sample Rate
	pub pyroscope_sample_rate: u32,

	#[clap(long, default_value_t = false)]
	/// Enable Cache Misses Profiling with Valgrind. Linux only, Valgrind must be in the PATH
	pub cache_misses: bool,

	#[clap(long, default_value_t = false)]
	/// Shows the output in YAML format
	pub yaml_output: bool,

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
		let is_valgrind_running = valgrind::is_valgrind_running();
		if !is_valgrind_running && self.cache_misses {
			return valgrind::relaunch_in_valgrind_mode()
		}

		let agent_running = if self.profile {
			let agent = PyroscopeAgent::builder(self.pyroscope_url.as_str(), "subsystem-bench")
				.backend(pprof_backend(PprofConfig::new().sample_rate(self.pyroscope_sample_rate)))
				.build()?;

			Some(agent.start()?)
		} else {
			None
		};

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
					let benchmark_name =
						format!("{} #{} {}", &options.path, index + 1, test_config.objective);
					gum::info!(target: LOG_TARGET, "{}", format!("Step {}/{}", index + 1, num_steps).bright_purple(),);
					display_configuration(&test_config);

					let usage = match test_config.objective {
						TestObjective::DataAvailabilityRead(ref _opts) => {
							let mut state = TestState::new(&test_config);
							let (mut env, _protocol_config) = prepare_test(test_config, &mut state);
							env.runtime().block_on(availability::benchmark_availability_read(
								&benchmark_name,
								&mut env,
								state,
							))
						},
						TestObjective::ApprovalVoting(ref options) => {
							let (mut env, state) =
								approval::prepare_test(test_config.clone(), options.clone());
							env.runtime().block_on(bench_approvals(
								&benchmark_name,
								&mut env,
								state,
							))
						},
						TestObjective::DataAvailabilityWrite => {
							let mut state = TestState::new(&test_config);
							let (mut env, _protocol_config) = prepare_test(test_config, &mut state);
							env.runtime().block_on(availability::benchmark_availability_write(
								&benchmark_name,
								&mut env,
								state,
							))
						},
						TestObjective::TestSequence(_) => todo!(),
						TestObjective::Unimplemented => todo!(),
					};

					let output = if self.yaml_output {
						serde_yaml::to_string(&vec![usage])?
					} else {
						usage.to_string()
					};
					println!("{}", output);
				}

				return Ok(())
			},
			TestObjective::DataAvailabilityRead(ref _options) => self.create_test_configuration(),
			TestObjective::DataAvailabilityWrite => self.create_test_configuration(),
			TestObjective::ApprovalVoting(_) => todo!(),
			TestObjective::Unimplemented => todo!(),
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

		let benchmark_name = format!("{}", self.objective);
		let usage = match self.objective {
			TestObjective::DataAvailabilityRead(_options) => env.runtime().block_on(
				availability::benchmark_availability_read(&benchmark_name, &mut env, state),
			),
			TestObjective::DataAvailabilityWrite => env.runtime().block_on(
				availability::benchmark_availability_write(&benchmark_name, &mut env, state),
			),
			TestObjective::TestSequence(_options) => todo!(),
			TestObjective::ApprovalVoting(_) => todo!(),
			TestObjective::Unimplemented => todo!(),
		};

		if let Some(agent_running) = agent_running {
			let agent_ready = agent_running.stop()?;
			agent_ready.shutdown();
		}

		let output =
			if self.yaml_output { serde_yaml::to_string(&vec![usage])? } else { usage.to_string() };
		println!("{}", output);

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
		.format_timestamp_millis()
		// .filter(None, log::LevelFilter::Trace)
		.try_init()
		.unwrap();

	let cli: BenchCli = BenchCli::parse();
	cli.launch()?;
	Ok(())
}
