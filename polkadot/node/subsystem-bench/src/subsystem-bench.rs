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
use prometheus::proto::LabelPair;
use std::time::Duration;

pub(crate) mod availability;

use availability::{TestConfiguration, TestEnvironment, TestState};
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

	#[clap(short, long, default_value_t = false)]
	/// Turbo boost AD Read by fetching from backers first. Tipically this is only faster if nodes
	/// have enough bandwidth.
	pub fetch_from_backers: bool,

	#[clap(short, long, ignore_case = true, default_value_t = 1)]
	/// Number of times to loop fetching for each core.
	pub num_loops: usize,
}
/// Define the supported benchmarks targets
#[derive(Debug, Parser)]
#[command(about = "Target subsystems", version, rename_all = "kebab-case")]
enum BenchmarkTarget {
	/// Benchmark availability recovery strategies.
	DataAvailabilityRead(DataAvailabilityReadOptions),
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
	pub target: BenchmarkTarget,
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
	/// Launch a malus node.
	fn launch(self) -> eyre::Result<()> {
		use prometheus::{proto::MetricType, Registry, TextEncoder};

		println!("Preparing {:?} benchmarks", self.target);

		let runtime = new_runtime();
		let registry = Registry::new();

		let mut pov_sizes = Vec::new();
		pov_sizes.append(&mut vec![10 * 1024 * 1024; 200]);

		let mut test_config = match self.target {
			BenchmarkTarget::DataAvailabilityRead(options) => match self.network {
				NetworkEmulation::Healthy => TestConfiguration::healthy_network(
					options.num_loops,
					options.fetch_from_backers,
					options.n_validators,
					options.n_cores,
					pov_sizes,
				),
				NetworkEmulation::Degraded => TestConfiguration::degraded_network(
					options.num_loops,
					options.fetch_from_backers,
					options.n_validators,
					options.n_cores,
					pov_sizes,
				),
				NetworkEmulation::Ideal => TestConfiguration::ideal_network(
					options.num_loops,
					options.fetch_from_backers,
					options.n_validators,
					options.n_cores,
					pov_sizes,
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

		let candidate_count = test_config.n_cores * test_config.num_loops;

		let mut state = TestState::new(test_config);
		state.generate_candidates(candidate_count);
		let mut env = TestEnvironment::new(runtime.handle().clone(), state, registry.clone());

		println!("{:?}", env.config());

		runtime.block_on(availability::bench_chunk_recovery(&mut env));

		let metric_families = registry.gather();

		for familiy in metric_families {
			let metric_type = familiy.get_field_type();

			for metric in familiy.get_metric() {
				match metric_type {
					MetricType::HISTOGRAM => {
						let h = metric.get_histogram();

						let labels = metric.get_label();
						// Skip test env usage.
						let mut env_label = LabelPair::default();
						env_label.set_name("task_group".into());
						env_label.set_value("test-environment".into());

						let mut is_env_metric = false;
						for label_pair in labels {
							if &env_label == label_pair {
								is_env_metric = true;
								break
							}
						}

						if !is_env_metric {
							println!(
								"{:?} CPU seconds used: {:?}",
								familiy.get_name(),
								h.get_sample_sum()
							);
						}
					},
					_ => {},
				}
			}
		}
		// encoder.encode(&metric_families, &mut buffer).unwrap();

		// Output to the standard output.
		// println!("Metrics: {}", String::from_utf8(buffer).unwrap());
		Ok(())
	}
}

fn main() -> eyre::Result<()> {
	color_eyre::install()?;
	let _ = env_logger::builder()
		.filter(Some("hyper"), log::LevelFilter::Info)
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
