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
	approval::{bench_approvals, prepare_test, ApprovalsOptions},
	configuration::TestConfiguration,
	usage::BenchmarkUsage,
};

const BENCH_COUNT: usize = 10;

fn main() -> Result<(), String> {
	let mut messages = vec![];

	messages.extend(approvals_no_shows());
	messages.extend(approvals_throughput());
	messages.extend(approvals_throughput_best_case());

	if messages.is_empty() {
		Ok(())
	} else {
		eprintln!("{}", messages.join("\n"));
		Err("Regressions found".to_string())
	}
}

fn test_configuration() -> TestConfiguration {
	let mut config = TestConfiguration::default();
	config.n_validators = 500;
	config.n_cores = 100;
	config.min_pov_size = 1120;
	config.max_pov_size = 5120;
	config.peer_bandwidth = 524288000000;
	config.bandwidth = 524288000000;
	config.num_blocks = 10;
	config.generate_pov_sizes();

	config
}

fn run_benchmark(test_case: &str, options: ApprovalsOptions) -> BenchmarkUsage {
	let usages: Vec<BenchmarkUsage> = (0..BENCH_COUNT)
		.map(|_| {
			let (mut env, state) = prepare_test(test_configuration(), options.clone(), false);
			env.runtime().block_on(bench_approvals(test_case, &mut env, state))
		})
		.collect();
	let usage = BenchmarkUsage::average(&usages);
	println!("{}", usage);
	usage
}

fn approvals_no_shows() -> Vec<String> {
	let mut messages = vec![];
	let usage = run_benchmark(
		"approvals_no_shows",
		ApprovalsOptions {
			coalesce_mean: 3.0,
			coalesce_std_dev: 1.0,
			enable_assignments_v2: true,
			last_considered_tranche: 89,
			stop_when_approved: true,
			coalesce_tranche_diff: 12,
			num_no_shows_per_candidate: 10,
			workdir_prefix: "/tmp".to_string(),
		},
	);

	messages.extend(usage.check_network_usage(&[
		("Received from peers", 6600.000, 7200.000),
		("Sent to peers", 7600.000, 8500.00),
	]));
	messages.extend(usage.check_cpu_usage(&[
		("approval-distribution", 0.940, 1.040),
		("approval-voting", 1.300, 1.440),
	]));

	messages
}

fn approvals_throughput() -> Vec<String> {
	let mut messages = vec![];
	let usage = run_benchmark(
		"approvals_throughput",
		ApprovalsOptions {
			coalesce_mean: 3.0,
			coalesce_std_dev: 1.0,
			enable_assignments_v2: true,
			last_considered_tranche: 89,
			stop_when_approved: false,
			coalesce_tranche_diff: 12,
			num_no_shows_per_candidate: 0,
			workdir_prefix: "/tmp".to_string(),
		},
	);

	messages.extend(usage.check_network_usage(&[
		("Received from peers", 50200.000, 55500.000),
		("Sent to peers", 60300.000, 66700.000),
	]));
	messages.extend(usage.check_cpu_usage(&[
		("approval-distribution", 8.210, 9.080),
		("approval-voting", 10.530, 11.640),
	]));

	messages
}

fn approvals_throughput_best_case() -> Vec<String> {
	let mut messages = vec![];
	let usage = run_benchmark(
		"approvals_throughput_best_case",
		ApprovalsOptions {
			coalesce_mean: 3.0,
			coalesce_std_dev: 1.0,
			enable_assignments_v2: true,
			last_considered_tranche: 89,
			stop_when_approved: true,
			coalesce_tranche_diff: 12,
			num_no_shows_per_candidate: 0,
			workdir_prefix: "/tmp".to_string(),
		},
	);

	// CPU usage, seconds                     total   per block
	// approval-distribution                  4.799       0.480
	// approval-voting                        6.248       0.625
	// Test environment                       3.057       0.306
	messages.extend(usage.check_network_usage(&[
		("Received from peers", 2800.000, 3100.000),
		("Sent to peers", 3100.000, 3400.000),
	]));
	messages.extend(usage.check_cpu_usage(&[
		("approval-distribution", 0.460, 0.500),
		("approval-voting", 0.590, 0.660),
	]));

	messages
}
