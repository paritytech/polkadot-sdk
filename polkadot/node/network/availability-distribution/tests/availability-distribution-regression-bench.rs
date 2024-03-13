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
//! TODO: Explain the test case after configuration adjusted to Kusama
//!
//! Subsystems involved:
//! - availability-distribution
//! - bitfield-distribution
//! - availability-store

use polkadot_subsystem_bench::{
	availability::{benchmark_availability_write, prepare_test, TestDataAvailability, TestState},
	configuration::{PeerLatency, TestConfiguration},
	usage::BenchmarkUsage,
};

const BENCH_COUNT: usize = 3;
const WARM_UP_COUNT: usize = 20;
const WARM_UP_PRECISION: f64 = 0.01;

fn main() -> Result<(), String> {
	let mut messages = vec![];

	// TODO: Adjust the test configurations to Kusama values
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

	warm_up(config.clone())?;
	let usage = benchmark(config.clone());

	messages.extend(usage.check_network_usage(&[
		("Received from peers", 4330.0, 0.05),
		("Sent to peers", 15900.0, 0.05),
	]));
	messages.extend(usage.check_cpu_usage(&[
		("availability-distribution", 0.025, 0.05),
		("bitfield-distribution", 0.085, 0.05),
		("availability-store", 0.180, 0.05),
	]));

	if messages.is_empty() {
		Ok(())
	} else {
		eprintln!("{}", messages.join("\n"));
		Err("Regressions found".to_string())
	}
}

fn warm_up(config: TestConfiguration) -> Result<(), String> {
	println!("Warming up...");
	let mut prev_run: Option<BenchmarkUsage> = None;
	for _ in 0..WARM_UP_COUNT {
		let curr = run(config.clone());
		if let Some(ref prev) = prev_run {
			let av_distr_diff =
				curr.cpu_usage_diff(prev, "availability-distribution").expect("Must exist");
			let bitf_distr_diff =
				curr.cpu_usage_diff(prev, "bitfield-distribution").expect("Must exist");
			let av_store_diff =
				curr.cpu_usage_diff(prev, "availability-store").expect("Must exist");
			if av_distr_diff < WARM_UP_PRECISION &&
				bitf_distr_diff < WARM_UP_PRECISION &&
				av_store_diff < WARM_UP_PRECISION
			{
				return Ok(())
			}
		}
		prev_run = Some(curr);
	}

	Err("Can't warm up".to_string())
}

fn benchmark(config: TestConfiguration) -> BenchmarkUsage {
	println!("Benchmarking...");
	let usages: Vec<BenchmarkUsage> = (0..BENCH_COUNT).map(|_| run(config.clone())).collect();
	let usage = BenchmarkUsage::average(&usages);
	println!("{}", usage);
	usage
}

fn run(config: TestConfiguration) -> BenchmarkUsage {
	let mut state = TestState::new(&config);
	let (mut env, _protocol_config) =
		prepare_test(config.clone(), &mut state, TestDataAvailability::Write, false);
	env.runtime()
		.block_on(benchmark_availability_write("data_availability_write", &mut env, state))
}
