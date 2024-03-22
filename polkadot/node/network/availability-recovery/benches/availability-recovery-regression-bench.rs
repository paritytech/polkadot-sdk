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
//! - availability-recovery

use polkadot_subsystem_bench::{
	availability::{
		benchmark_availability_read, prepare_test, DataAvailabilityReadOptions,
		TestDataAvailability, TestState,
	},
	configuration::TestConfiguration,
	usage::BenchmarkUsage,
};
use std::io::Write;

const BENCH_COUNT: usize = 50;

fn main() -> Result<(), String> {
	let mut messages = vec![];

	let options = DataAvailabilityReadOptions { fetch_from_backers: true };
	let mut config = TestConfiguration::default();
	config.num_blocks = 3;
	config.generate_pov_sizes();

	let state = TestState::new(&config);

	println!("Benchmarking...");
	let usages: Vec<BenchmarkUsage> = (0..BENCH_COUNT)
		.map(|n| {
			print!("\r[{}{}]", "#".repeat(n), "_".repeat(BENCH_COUNT - n));
			std::io::stdout().flush().unwrap();
			let (mut env, _cfgs) =
				prepare_test(&state, TestDataAvailability::Read(options.clone()), false);
			env.runtime().block_on(benchmark_availability_read(
				"data_availability_read",
				&mut env,
				&state,
			))
		})
		.collect();
	println!("\rDone!{}", " ".repeat(BENCH_COUNT));
	let average_usage = BenchmarkUsage::average(&usages);
	println!("{}", average_usage);

	// We don't expect any other values for received and sent
	messages.extend(average_usage.check_network_usage(&[
		("Received from peers", 307200.000, 0.01),
		("Sent to peers", 1.667, 0.01),
	]));
	messages.extend(average_usage.check_cpu_usage(&[("availability-recovery", 11.500, 0.05)]));

	if messages.is_empty() {
		Ok(())
	} else {
		eprintln!("{}", messages.join("\n"));
		Err("Regressions found".to_string())
	}
}
