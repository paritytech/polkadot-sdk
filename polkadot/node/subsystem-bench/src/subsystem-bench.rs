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
use serde::{Deserialize, Serialize};

use colored::Colorize;

use color_eyre::eyre;
use pyroscope::PyroscopeAgent;
use pyroscope_pprofrs::{pprof_backend, PprofConfig};

use std::path::Path;

pub(crate) mod approval;
pub(crate) mod availability;
pub(crate) mod core;
mod valgrind;

const LOG_TARGET: &str = "subsystem-bench";

use availability::{prepare_test, NetworkEmulation, TestState};

use approval::{bench_approvals, ApprovalsOptions};
use availability::DataAvailabilityReadOptions;
use core::{
	configuration::TestConfiguration,
	display::display_configuration,
	environment::{TestEnvironment, GENESIS_HASH},
};

use clap_num::number_range;

fn le_100(s: &str) -> Result<usize, String> {
	number_range(s, 0, 100)
}

fn le_5000(s: &str) -> Result<usize, String> {
	number_range(s, 0, 5000)
}

/// Supported test objectives
#[derive(Debug, Clone, Parser, Serialize, Deserialize)]
#[command(rename_all = "kebab-case")]
pub enum TestObjective {
	/// Benchmark availability recovery strategies.
	DataAvailabilityRead(DataAvailabilityReadOptions),
	/// Benchmark availability and bitfield distribution.
	DataAvailabilityWrite,
	/// Benchmark the approval-voting and approval-distribution subsystems.
	ApprovalVoting(ApprovalsOptions),
	Unimplemented,
}

impl std::fmt::Display for TestObjective {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(
			f,
			"{}",
			match self {
				Self::DataAvailabilityRead(_) => "DataAvailabilityRead",
				Self::DataAvailabilityWrite => "DataAvailabilityWrite",
				Self::ApprovalVoting(_) => "ApprovalVoting",
				Self::Unimplemented => "Unimplemented",
			}
		)
	}
}

#[derive(Debug, Parser)]
#[allow(missing_docs)]
struct BenchCli {
	#[arg(long, value_enum, ignore_case = true, default_value_t = NetworkEmulation::Ideal)]
	/// The type of network to be emulated
	pub network: NetworkEmulation,

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

	#[arg(required = true)]
	/// Path to the test sequence configuration file
	pub path: String,
}

impl BenchCli {
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

		let test_sequence = core::configuration::TestSequence::new_from_file(Path::new(&self.path))
			.expect("File exists")
			.into_vec();
		let num_steps = test_sequence.len();
		gum::info!("{}", format!("Sequence contains {} step(s)", num_steps).bright_purple());
		for (index, test_config) in test_sequence.into_iter().enumerate() {
			let benchmark_name = format!("{} #{} {}", &self.path, index + 1, test_config.objective);
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
					env.runtime().block_on(bench_approvals(&benchmark_name, &mut env, state))
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
				TestObjective::Unimplemented => todo!(),
			};

			let output = if self.yaml_output {
				serde_yaml::to_string(&vec![usage])?
			} else {
				usage.to_string()
			};
			println!("{}", output);
		}

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
		.format_timestamp_millis()
		// .filter(None, log::LevelFilter::Trace)
		.try_init()
		.unwrap();

	let cli: BenchCli = BenchCli::parse();
	cli.launch()?;
	Ok(())
}
