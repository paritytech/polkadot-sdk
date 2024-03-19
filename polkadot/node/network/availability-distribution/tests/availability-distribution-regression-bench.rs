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
	availability::{
		benchmark_availability_write, prepare_data, prepare_test, TestDataAvailability,
	},
	configuration::TestConfiguration,
	usage::BenchmarkUsage,
};

fn main() -> Result<(), String> {
	let mut messages = vec![];
	let mut config = TestConfiguration::default();
	// A single node effort roughly
	config.n_cores = 10;
	config.n_validators = 500;
	config.num_blocks = 3;
	config.generate_pov_sizes();
	let (
		state,
		test_authorities,
		block_headers,
		block_infos,
		chunk_fetching_requests,
		signed_bitfields,
		availability_state,
		runtime_api,
	) = prepare_data(&config);

	let usages: Vec<BenchmarkUsage> = (0..10)
		.map(|_| {
			let mut env = prepare_test(
				&config,
				&state,
				&test_authorities,
				&block_headers,
				&availability_state,
				&runtime_api,
				TestDataAvailability::Write,
				false,
			);
			env.runtime().block_on(benchmark_availability_write(
				"data_availability_write",
				&mut env,
				&state,
				&block_infos,
				&chunk_fetching_requests,
				&signed_bitfields,
			))
		})
		.collect();
	let usage = BenchmarkUsage::average(&usages);
	println!("{}", usage);

	messages.extend(usage.check_network_usage(&[
		("Received from peers", 433.3, 0.05),
		("Sent to peers", 18480.0, 0.05),
	]));
	messages.extend(usage.check_cpu_usage(&[
		("availability-distribution", 0.012, 0.05),
		("bitfield-distribution", 0.057, 0.05),
		("availability-store", 0.157, 0.05),
	]));

	if messages.is_empty() {
		Ok(())
	} else {
		eprintln!("{}", messages.join("\n"));
		Err("Regressions found".to_string())
	}
}
