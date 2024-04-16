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

//! A tool for running subsystem benchmark tests
//! designed for development and CI regression testing.

use clap::Parser;
use color_eyre::eyre;
use colored::Colorize;
use polkadot_subsystem_bench::{approval, availability, configuration};
use pyroscope::PyroscopeAgent;
use pyroscope_pprofrs::{pprof_backend, PprofConfig};
use serde::{Deserialize, Serialize};
use std::path::Path;

mod valgrind;

const LOG_TARGET: &str = "subsystem-bench::cli";

/// Supported test objectives
#[derive(Debug, Clone, Parser, Serialize, Deserialize)]
#[command(rename_all = "kebab-case")]
pub enum TestObjective {
	/// Benchmark availability recovery strategies.
	DataAvailabilityRead(availability::DataAvailabilityReadOptions),
	/// Benchmark availability and bitfield distribution.
	DataAvailabilityWrite,
	/// Benchmark the approval-voting and approval-distribution subsystems.
	ApprovalVoting(approval::ApprovalsOptions),
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
			}
		)
	}
}

/// The test input parameters
#[derive(Clone, Debug, Serialize, Deserialize)]
struct CliTestConfiguration {
	/// Test Objective
	pub objective: TestObjective,
	/// Test Configuration
	#[serde(flatten)]
	pub test_config: configuration::TestConfiguration,
}

#[derive(Serialize, Deserialize)]
pub struct TestSequence {
	#[serde(rename(serialize = "TestConfiguration", deserialize = "TestConfiguration"))]
	test_configurations: Vec<CliTestConfiguration>,
}

impl TestSequence {
	fn new_from_file(path: &Path) -> std::io::Result<TestSequence> {
		let string = String::from_utf8(std::fs::read(path)?).expect("File is valid UTF8");
		Ok(serde_yaml::from_str(&string).expect("File is valid test sequence YA"))
	}
}

#[derive(Debug, Parser)]
#[allow(missing_docs)]
struct BenchCli {
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

		let test_sequence = TestSequence::new_from_file(Path::new(&self.path))
			.expect("File exists")
			.test_configurations;
		let num_steps = test_sequence.len();
		gum::info!("{}", format!("Sequence contains {} step(s)", num_steps).bright_purple());

		for (index, CliTestConfiguration { objective, mut test_config }) in
			test_sequence.into_iter().enumerate()
		{
			let benchmark_name = format!("{} #{} {}", &self.path, index + 1, objective);
			gum::info!(target: LOG_TARGET, "{}", format!("Step {}/{}", index + 1, num_steps).bright_purple(),);
			gum::info!(target: LOG_TARGET, "[{}] {}", format!("objective = {:?}", objective).green(), test_config);
			test_config.generate_pov_sizes();

			let usage = match objective {
				TestObjective::DataAvailabilityRead(opts) => {
					let state = availability::TestState::new(&test_config);
					let (mut env, _protocol_config) = availability::prepare_test(
						&state,
						availability::TestDataAvailability::Read(opts),
						true,
					);
					env.runtime().block_on(availability::benchmark_availability_read(
						&benchmark_name,
						&mut env,
						&state,
					))
				},
				TestObjective::DataAvailabilityWrite => {
					let state = availability::TestState::new(&test_config);
					let (mut env, _protocol_config) = availability::prepare_test(
						&state,
						availability::TestDataAvailability::Write,
						true,
					);
					env.runtime().block_on(availability::benchmark_availability_write(
						&benchmark_name,
						&mut env,
						&state,
					))
				},
				TestObjective::ApprovalVoting(ref options) => {
					let (mut env, state) =
						approval::prepare_test(test_config.clone(), options.clone(), true);
					env.runtime().block_on(approval::bench_approvals(
						&benchmark_name,
						&mut env,
						state,
					))
				},
			};
			println!("{}", usage);
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
		.try_init()
		.unwrap();

	let cli: BenchCli = BenchCli::parse();
	cli.launch()?;
	Ok(())
}
