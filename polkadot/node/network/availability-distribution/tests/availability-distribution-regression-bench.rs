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

use polkadot_subsystem_bench::{
	availability::{benchmark_availability_write, prepare_test, TestDataAvailability, TestState},
	configuration::{PeerLatency, TestConfiguration},
	usage::BenchmarkUsage,
};

const BENCH_COUNT: usize = 10;

fn main() -> Result<(), String> {
	let mut messages = vec![];
	let mut config = TestConfiguration::default();
	config.latency = Some(PeerLatency { mean_latency_ms: 30, std_dev: 2.0 });
	config.n_validators = 1000;
	config.n_cores = 200;
	config.max_validators_per_core = 5;
	config.min_pov_size = 5120;
	config.max_pov_size = 5120;
	config.peer_bandwidth = 52428800;
	config.bandwidth = 52428800;
	config.connectivity = 75;
	config.num_blocks = 3;
	config.generate_pov_sizes();

	let usages: Vec<BenchmarkUsage> = (0..BENCH_COUNT)
		.map(|_| {
			let mut state = TestState::new(&config);
			let (mut env, _protocol_config) =
				prepare_test(config.clone(), &mut state, TestDataAvailability::Write, false);
			env.runtime().block_on(benchmark_availability_write(
				"data_availability_write",
				&mut env,
				state,
			))
		})
		.collect();
	let usage = BenchmarkUsage::average(&usages);
	println!("{}", usage);

	messages.extend(usage.check_network_usage(&[
		("Received from peers", 4110.000, 4550.000),
		("Sent to peers", 15100.000, 16700.000),
	]));
	messages.extend(usage.check_cpu_usage(&[
		("availability-distribution", 0.024, 0.026),
		("bitfield-distribution", 0.082, 0.090),
		("availability-store", 0.168, 0.186),
	]));

	if messages.is_empty() {
		Ok(())
	} else {
		eprintln!("{}", messages.join("\n"));
		Err("Regressions found".to_string())
	}
}
