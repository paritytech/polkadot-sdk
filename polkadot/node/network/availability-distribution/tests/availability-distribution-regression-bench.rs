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
	availability::{benchmark_availability_write, prepare_test, TestDataAvailability, TestState},
	configuration::TestConfiguration,
	utils::{warm_up_and_benchmark, WarmUpOptions},
};

fn main() -> Result<(), String> {
	let mut messages = vec![];
	let mut config = TestConfiguration::default();
	// A single node effort roughly
	config.n_cores = 10;
	config.n_validators = 500;
	config.num_blocks = 3;
	config.generate_pov_sizes();

	let usage = warm_up_and_benchmark(
		WarmUpOptions::new(&[
			("availability-distribution", 0.06),
			("bitfield-distribution", 0.03),
			("availability-store", 0.01),
		]),
		|| {
			let mut state = TestState::new(&config);
			let (mut env, _protocol_config) =
				prepare_test(config.clone(), &mut state, TestDataAvailability::Write, false);
			env.runtime().block_on(benchmark_availability_write(
				"data_availability_write",
				&mut env,
				state,
			))
		},
	)?;
	println!("{}", usage);

	messages.extend(usage.check_network_usage(&[
		("Received from peers", 433.333, 0.05),
		("Sent to peers", 18480.000, 0.05),
	]));
	messages.extend(usage.check_cpu_usage(&[
		("availability-distribution", 0.012, 0.10),
		("bitfield-distribution", 0.048, 0.10),
		("availability-store", 0.155, 0.10),
	]));

	if messages.is_empty() {
		Ok(())
	} else {
		eprintln!("{}", messages.join("\n"));
		Err("Regressions found".to_string())
	}
}
