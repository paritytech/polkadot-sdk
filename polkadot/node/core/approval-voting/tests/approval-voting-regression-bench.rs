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

	config
}

fn run_benchmark(test_case: &str, options: ApprovalsOptions) -> BenchmarkUsage {
	let (mut env, state) = prepare_test(test_configuration(), options, false);
	let usage = env.runtime().block_on(bench_approvals(test_case, &mut env, state));
	println!("{}", usage);
	usage
}

fn approvals_no_shows() {
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
		("Received from peers", 6900.000, 7000.000),
		("Sent to peers", 8000.000, 8100.000),
	]));
	messages.extend(usage.check_cpu_usage(&[
		("approval-distribution", 0.700, 0.800),
		("approval-voting", 1.100, 1.200),
	]));

	messages
}

fn approvals_throughput() {
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
		("Received from peers", 52900.000, 53000.000),
		("Sent to peers", 63500.000, 63600.000),
	]));
	messages.extend(usage.check_cpu_usage(&[
		("approval-distribution", 6.300, 6.400),
		("approval-voting", 9.400, 9.500),
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

	messages.extend(usage.check_network_usage(&[
		("Received from peers", 2900.000, 3000.000),
		("Sent to peers", 3200.000, 3300.000),
	]));
	messages.extend(usage.check_cpu_usage(&[
		("approval-distribution", 0.400, 0.500),
		("approval-voting", 0.600, 0.700),
	]));

	messages
}
