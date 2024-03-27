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

//! approval-voting regression tests
//!
//! Approval Voting benchmark based on Kusama parameters and scale.
//!
//! Subsystems involved:
//! - approval-distribution
//! - approval-voting

use polkadot_subsystem_bench::{
	approval::{bench_approvals, prepare_test, ApprovalsOptions},
	configuration::TestConfiguration,
	usage::BenchmarkUsage,
	utils::save_to_file,
};
use std::io::Write;

const BENCH_COUNT: usize = 3;

fn main() -> Result<(), String> {
	let mut messages = vec![];

	messages.extend(approvals_no_shows()?);
	messages.extend(approvals_throughput()?);
	messages.extend(approvals_throughput_best_case()?);

	if messages.is_empty() {
		Ok(())
	} else {
		eprintln!("{}", messages.join("\n"));
		Err("Regressions found".to_string())
	}
}

fn test_configuration() -> TestConfiguration {
	let mut config = TestConfiguration::default();
	config.num_blocks = 3;
	config.generate_pov_sizes();

	config
}

fn run(test_case: &str, options: ApprovalsOptions) -> Result<BenchmarkUsage, String> {
	println!("Benchmarking...");
	let usages: Vec<BenchmarkUsage> = (0..BENCH_COUNT)
		.map(|n| {
			print!("\r[{}{}]", "#".repeat(n), "_".repeat(BENCH_COUNT - n));
			std::io::stdout().flush().unwrap();

			let (mut env, state) = prepare_test(test_configuration(), options.clone(), false);
			env.runtime().block_on(bench_approvals(test_case, &mut env, state))
		})
		.collect();
	println!("\rDone!{}", " ".repeat(BENCH_COUNT));

	let average_usage = BenchmarkUsage::average(&usages);
	save_to_file(
		&format!("charts/availability-recovery-regression-bench-{}.json", test_case),
		average_usage.to_chart_json().map_err(|e| e.to_string())?,
	)
	.map_err(|e| e.to_string())?;
	println!("{}", average_usage);

	Ok(average_usage)
}

fn approvals_no_shows() -> Result<Vec<String>, String> {
	let mut messages = vec![];
	let test_case = "approvals_no_shows";
	let options = ApprovalsOptions {
		coalesce_mean: 3.0,
		coalesce_std_dev: 1.0,
		enable_assignments_v2: true,
		last_considered_tranche: 89,
		stop_when_approved: true,
		coalesce_tranche_diff: 12,
		num_no_shows_per_candidate: 10,
		workdir_prefix: "/tmp".to_string(),
	};

	let usage = run(test_case, options)?;

	// We expect no variance for received and sent
	// but use 0.001 because we operate with floats
	messages.extend(usage.check_network_usage(&[
		("Received from peers", 7000.0, 0.001),
		("Sent to peers", 8100.0, 0.001),
	]));
	messages.extend(usage.check_cpu_usage(&[
		("approval-distribution", 0.990, 0.05),
		("approval-voting", 1.310, 0.05),
	]));

	Ok(messages)
}

fn approvals_throughput() -> Result<Vec<String>, String> {
	let mut messages = vec![];
	let test_case = "approvals_throughput";
	let options = ApprovalsOptions {
		coalesce_mean: 3.0,
		coalesce_std_dev: 1.0,
		enable_assignments_v2: true,
		last_considered_tranche: 89,
		stop_when_approved: false,
		coalesce_tranche_diff: 12,
		num_no_shows_per_candidate: 0,
		workdir_prefix: "/tmp".to_string(),
	};

	let usage = run(test_case, options)?;

	// We expect no variance for received and sent
	// but use 0.001 because we operate with floats
	messages.extend(usage.check_network_usage(&[
		("Received from peers", 52850.0, 0.001),
		("Sent to peers", 63500.0, 0.001),
	]));
	messages.extend(usage.check_cpu_usage(&[
		("approval-distribution", 8.645, 0.05),
		("approval-voting", 11.085, 0.05),
	]));

	Ok(messages)
}

fn approvals_throughput_best_case() -> Result<Vec<String>, String> {
	let mut messages = vec![];
	let test_case = "approvals_throughput_best_case";
	let options = ApprovalsOptions {
		coalesce_mean: 3.0,
		coalesce_std_dev: 1.0,
		enable_assignments_v2: true,
		last_considered_tranche: 89,
		stop_when_approved: true,
		coalesce_tranche_diff: 12,
		num_no_shows_per_candidate: 0,
		workdir_prefix: "/tmp".to_string(),
	};

	let usage = run(test_case, options)?;

	// We expect no variance for received and sent
	// but use 0.001 because we operate with floats
	messages.extend(usage.check_network_usage(&[
		("Received from peers", 2950.0, 0.05),
		("Sent to peers", 3250.0, 0.05),
	]));
	messages.extend(usage.check_cpu_usage(&[
		("approval-distribution", 0.48, 0.05),
		("approval-voting", 0.625, 0.05),
	]));

	Ok(messages)
}
