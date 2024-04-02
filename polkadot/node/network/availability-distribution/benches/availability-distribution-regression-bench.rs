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

//! availability-read regression tests
//!
//! Availability read benchmark based on Kusama parameters and scale.
//!
//! Subsystems involved:
//! - availability-distribution
//! - bitfield-distribution
//! - availability-store

use polkadot_subsystem_bench::{
	availability::{benchmark_availability_write, prepare_test, TestState},
	configuration::TestConfiguration,
	usage::BenchmarkUsage,
	utils::save_to_file,
};
use std::io::Write;

const BENCH_COUNT: usize = 50;

fn main() -> Result<(), String> {
	let mut messages = vec![];
	let mut config = TestConfiguration::default();
	// A single node effort roughly
	config.n_cores = 10;
	config.n_validators = 500;
	config.num_blocks = 3;
	config.generate_pov_sizes();
	let state = TestState::new(&config);

	println!("Benchmarking...");
	let usages: Vec<BenchmarkUsage> = (0..BENCH_COUNT)
		.map(|n| {
			print!("\r[{}{}]", "#".repeat(n), "_".repeat(BENCH_COUNT - n));
			std::io::stdout().flush().unwrap();
			let (mut env, _cfgs) = prepare_test(
				&state,
				polkadot_subsystem_bench::availability::TestDataAvailability::Write,
				false,
			);
			env.runtime().block_on(benchmark_availability_write(
				"data_availability_write",
				&mut env,
				&state,
			))
		})
		.collect();
	println!("\rDone!{}", " ".repeat(BENCH_COUNT));

	let average_usage = BenchmarkUsage::average(&usages);
	save_to_file(
		"charts/availability-distribution-regression-bench.json",
		average_usage.to_chart_json().map_err(|e| e.to_string())?,
	)
	.map_err(|e| e.to_string())?;
	println!("{}", average_usage);

	// We expect no variance for received and sent
	// but use 0.001 because we operate with floats
	messages.extend(average_usage.check_network_usage(&[
		("Received from peers", 433.3, 0.001),
		("Sent to peers", 18480.0, 0.001),
	]));
	messages.extend(average_usage.check_cpu_usage(&[
		("availability-distribution", 0.012, 0.05),
		("availability-store", 0.153, 0.05),
		("bitfield-distribution", 0.026, 0.05),
	]));

	if messages.is_empty() {
		Ok(())
	} else {
		eprintln!("{}", messages.join("\n"));
		Err("Regressions found".to_string())
	}
}
