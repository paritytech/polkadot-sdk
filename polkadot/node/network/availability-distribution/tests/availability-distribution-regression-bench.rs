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
};

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

	let mut state = TestState::new(&config);
	let (mut env, _protocol_config) =
		prepare_test(config, &mut state, TestDataAvailability::Write, false);
	let usage = env.runtime().block_on(benchmark_availability_write(
		"data_availability_write",
		&mut env,
		state,
	));

	println!("{}", usage);

	messages.extend(usage.check_network_usage(&[
		("Received from peers", 4300.000, 4400.000),
		("Sent to peers", 15900.000, 16000.000),
	]));
	messages.extend(usage.check_cpu_usage(&[
		("availability-distribution", 0.000, 0.100),
		("bitfield-distribution", 0.000, 0.100),
		("availability-store", 0.100, 0.200),
	]));

	if messages.is_empty() {
		Ok(())
	} else {
		eprintln!("{}", messages.join("\n"));
		Err("Regressions found".to_string())
	}
}
