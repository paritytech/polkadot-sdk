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
use sc_service::TaskManager;

pub(crate) mod availability;

use availability::{EnvParams, TestEnvironment};
const LOG_TARGET: &str = "subsystem-bench";

/// Define the supported benchmarks targets
#[derive(Debug, Parser)]
#[command(about = "Target subsystems", version, rename_all = "kebab-case")]
enum BenchmarkTarget {
	/// Benchmark availability recovery strategies.
	AvailabilityRecovery,
}

#[derive(Debug, Parser)]
#[allow(missing_docs)]
struct BenchCli {
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
		use prometheus::{proto::MetricType, Counter, Encoder, Opts, Registry, TextEncoder};

		let encoder = TextEncoder::new();

		println!("Preparing {:?} benchmarks", self.target);

		let runtime = new_runtime();
		let registry = Registry::new();

		let params = availability::bench_chunk_recovery_params();
		let mut env = TestEnvironment::new(runtime.handle().clone(), params, registry.clone());

		runtime.block_on(availability::bench_chunk_recovery(&mut env));

		let metric_families = registry.gather();
		let total_subsystem_cpu = 0;

		for familiy in metric_families {
			let metric_type = familiy.get_field_type();

			for metric in familiy.get_metric() {
				match metric_type {
					MetricType::HISTOGRAM => {
						let h = metric.get_histogram();

						let mut inf_seen = false;

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
		.is_test(true)
		.filter(Some(LOG_TARGET), log::LevelFilter::Debug)
		.try_init();

	let cli: BenchCli = BenchCli::parse();
	cli.launch()?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
}
