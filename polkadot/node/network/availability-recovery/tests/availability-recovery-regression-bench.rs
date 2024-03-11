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

//! availability-write regression tests
//!
//! TODO: Explain the test case after configuration adjusted to Kusama
//!
//! Subsystems involved:
//! - availability-recovery

use polkadot_subsystem_bench::{
	availability::{
		benchmark_availability_read, prepare_test, DataAvailabilityReadOptions,
		TestDataAvailability, TestState,
	},
	configuration::{PeerLatency, TestConfiguration},
	usage::BenchmarkUsage,
};

const BENCH_COUNT: usize = 3;
const WARM_UP_COUNT: usize = 10;
const WARM_UP_PRECISION: f64 = 0.01;

fn main() -> Result<(), String> {
	let mut messages = vec![];

	// TODO: Adjust the test configurations to Kusama values
	let options = DataAvailabilityReadOptions { fetch_from_backers: true };
	let config = TestConfiguration::builder()
		.with_n_cores(20)
		.with_min_pov_size(5120)
		.with_max_pov_size(5120)
		.with_latency(PeerLatency { mean_latency_ms: 100, std_dev: 1.0 })
		.with_n_validators(300)
		.with_peer_bandwidth(52428800)
		.with_bandwidth(52428800)
		.with_num_blocks(3)
		.with_connectivity(90)
		.build();

	warm_up(config.clone(), options.clone())?;
	let usage = benchmark(config.clone(), options.clone());

	messages.extend(usage.check_network_usage(&[
		("Received from peers", 102400.000, 0.05),
		("Sent to peers", 0.335, 0.05),
	]));
	messages.extend(usage.check_cpu_usage(&[("availability-recovery", 3.850, 0.05)]));

	if messages.is_empty() {
		Ok(())
	} else {
		eprintln!("{}", messages.join("\n"));
		Err("Regressions found".to_string())
	}
}

fn warm_up(config: TestConfiguration, options: DataAvailabilityReadOptions) -> Result<(), String> {
	println!("Warming up...");
	let mut prev_run: Option<BenchmarkUsage> = None;
	for _ in 0..WARM_UP_COUNT {
		let curr = run(config.clone(), options.clone());
		if let Some(ref prev) = prev_run {
			let diff = curr.cpu_usage_diff(prev, "availability-recovery").expect("Must exist");
			if diff < WARM_UP_PRECISION {
				return Ok(())
			}
		}
		prev_run = Some(curr);
	}

	Err("Can't warm up".to_string())
}

fn benchmark(config: TestConfiguration, options: DataAvailabilityReadOptions) -> BenchmarkUsage {
	println!("Benchmarking...");
	let usages: Vec<BenchmarkUsage> =
		(0..BENCH_COUNT).map(|_| run(config.clone(), options.clone())).collect();
	let usage = BenchmarkUsage::average(&usages);
	println!("{}", usage);
	usage
}

fn run(config: TestConfiguration, options: DataAvailabilityReadOptions) -> BenchmarkUsage {
	let mut state = TestState::new(&config);
	let (mut env, _protocol_config) =
		prepare_test(config.clone(), &mut state, TestDataAvailability::Read(options), false);
	env.runtime()
		.block_on(benchmark_availability_read("data_availability_read", &mut env, state))
}
