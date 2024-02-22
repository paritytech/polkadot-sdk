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

const BENCH_COUNT: usize = 3;
const WARM_UP_COUNT: usize = 20;
const WARM_UP_PRECISION: f64 = 0.01;

fn main() -> Result<(), String> {
	let mut messages = vec![];

	messages.extend(approvals_no_shows()?);
	// messages.extend(approvals_throughput()?);
	// messages.extend(approvals_throughput_best_case()?);

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

fn warm_up(test_case: &str, options: ApprovalsOptions) -> Result<(), String> {
	println!("Warming up...");
	let mut prev_run: Option<BenchmarkUsage> = None;
	for _ in 0..WARM_UP_COUNT {
		let curr = run(test_case, options.clone());
		if let Some(ref prev) = prev_run {
			let ap_distr_diff =
				curr.cpu_usage_diff(prev, "approval-distribution").expect("Must exist");
			let ap_vot_diff = curr.cpu_usage_diff(prev, "approval-voting").expect("Must exist");
			if ap_distr_diff < WARM_UP_PRECISION && ap_vot_diff < WARM_UP_PRECISION {
				return Ok(())
			}
		}
		prev_run = Some(curr);
	}

	Err("Can't warm up".to_string())
}

fn benchmark(test_case: &str, options: ApprovalsOptions) -> BenchmarkUsage {
	println!("Benchmarking...");
	let usages: Vec<BenchmarkUsage> =
		(0..BENCH_COUNT).map(|_| run(test_case, options.clone())).collect();
	let usage = BenchmarkUsage::average(&usages);
	println!("{}", usage);
	usage
}

fn run(test_case: &str, options: ApprovalsOptions) -> BenchmarkUsage {
	let (mut env, state) = prepare_test(test_configuration(), options.clone(), false);
	env.runtime().block_on(bench_approvals(test_case, &mut env, state))
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

	warm_up(test_case, options.clone())?;
	let usage = benchmark(test_case, options.clone());

	messages.extend(usage.check_network_usage(&[
		("Received from peers", 6900.0, 0.05),
		("Sent to peers", 8050.0, 0.05),
	]));
	messages.extend(usage.check_cpu_usage(&[
		("approval-distribution", 0.99, 0.05),
		("approval-voting", 1.37, 0.05),
	]));

	Ok(messages)
}

#[allow(dead_code)]
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

	warm_up(test_case, options.clone())?;
	let usage = benchmark(test_case, options.clone());

	messages.extend(usage.check_network_usage(&[
		("Received from peers", 52850.0, 0.05),
		("Sent to peers", 63500.0, 0.05),
	]));
	messages.extend(usage.check_cpu_usage(&[
		("approval-distribution", 8.645, 0.05),
		("approval-voting", 11.085, 0.05),
	]));

	Ok(messages)
}

#[allow(dead_code)]
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

	warm_up(test_case, options.clone())?;
	let usage = benchmark(test_case, options.clone());

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
