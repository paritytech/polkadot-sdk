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

//! approval-voting throughput test
//!
//! Approval Voting benchmark based on Kusama parameters and scale.
//!
//! Subsystems involved:
//! - approval-distribution
//! - approval-voting

use polkadot_subsystem_bench::{
	self,
	approval::{bench_approvals, prepare_test, ApprovalsOptions},
	configuration::TestConfiguration,
	usage::BenchmarkUsage,
	utils::save_to_file,
};
use std::io::Write;

const BENCH_COUNT: usize = 10;

fn main() -> Result<(), String> {
	let mut messages = vec![];
	let mut config = TestConfiguration::default();
	config.n_cores = 100;
	config.n_validators = 500;
	config.num_blocks = 10;
	config.peer_bandwidth = 524288000000;
	config.bandwidth = 524288000000;
	config.latency = None;
	config.connectivity = 100;
	config.generate_pov_sizes();
	let options = ApprovalsOptions {
		last_considered_tranche: 89,
		coalesce_mean: 3.0,
		coalesce_std_dev: 1.0,
		coalesce_tranche_diff: 12,
		enable_assignments_v2: true,
		stop_when_approved: false,
		workdir_prefix: "/tmp".to_string(),
		num_no_shows_per_candidate: 0,
	};

	println!("Benchmarking...");
	let usages: Vec<BenchmarkUsage> = (0..BENCH_COUNT)
		.map(|n| {
			print!("\r[{}{}]", "#".repeat(n), "_".repeat(BENCH_COUNT - n));
			std::io::stdout().flush().unwrap();
			let (mut env, state) = prepare_test(config.clone(), options.clone(), false);
			env.runtime().block_on(bench_approvals(&mut env, state))
		})
		.collect();
	println!("\rDone!{}", " ".repeat(BENCH_COUNT));

	let average_usage = BenchmarkUsage::average(&usages);
	save_to_file(
		"charts/approval-voting-regression-bench.json",
		average_usage.to_chart_json().map_err(|e| e.to_string())?,
	)
	.map_err(|e| e.to_string())?;
	println!("{}", average_usage);

	// We expect no variance for received and sent
	// but use 0.001 because we operate with floats
	messages.extend(average_usage.check_network_usage(&[
		("Received from peers", 52942.4600, 0.001),
		("Sent to peers", 63547.0330, 0.001),
	]));
	messages.extend(average_usage.check_cpu_usage(&[
		("approval-distribution", 7.4075, 0.1),
		("approval-voting", 9.9873, 0.1),
	]));

	if messages.is_empty() {
		Ok(())
	} else {
		eprintln!("{}", messages.join("\n"));
		Err("Regressions found".to_string())
	}
}
