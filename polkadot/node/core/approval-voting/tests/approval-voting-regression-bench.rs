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
};

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

#[test]
fn approvals_no_shows() {
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
	let (mut env, state) = prepare_test(test_configuration(), options);

	let usage = env.runtime().block_on(bench_approvals("approvals_no_shows", &mut env, state));

	println!("{usage}");

	assert!(false)
}

#[test]
fn approvals_throughput() {
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
	let (mut env, state) = prepare_test(test_configuration(), options);

	let usage = env.runtime().block_on(bench_approvals("approvals_throughput", &mut env, state));

	println!("{usage}");

	assert!(false)
}

#[test]
fn approvals_throughput_best_case() {
	let options = ApprovalsOptions {
		coalesce_mean: 4.0,
		coalesce_std_dev: 2.0,
		enable_assignments_v2: true,
		last_considered_tranche: 90,
		stop_when_approved: true,
		coalesce_tranche_diff: 13,
		num_no_shows_per_candidate: 1,
		workdir_prefix: "/tmp".to_string(),
	};
	let (mut env, state) = prepare_test(test_configuration(), options);

	let usage =
		env.runtime()
			.block_on(bench_approvals("approvals_throughput_best_case", &mut env, state));

	println!("{usage}");

	assert!(false)
}

#[test]
fn approvals_throughput_no_optimisations_enabled() {
	let options = ApprovalsOptions {
		coalesce_mean: 1.0,
		coalesce_std_dev: 0.0,
		enable_assignments_v2: false,
		last_considered_tranche: 89,
		stop_when_approved: false,
		coalesce_tranche_diff: 12,
		num_no_shows_per_candidate: 0,
		workdir_prefix: "/tmp".to_string(),
	};
	let (mut env, state) = prepare_test(test_configuration(), options);

	let usage = env.runtime().block_on(bench_approvals(
		"approvals_throughput_no_optimisations_enabled",
		&mut env,
		state,
	));

	println!("{usage}");

	assert!(false)
}
