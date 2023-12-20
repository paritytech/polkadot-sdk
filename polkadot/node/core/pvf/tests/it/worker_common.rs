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

use polkadot_node_core_pvf::{
	testing::{build_workers_and_get_paths, spawn_with_program_path, SpawnErr},
	SecurityStatus,
};
use std::{env, time::Duration};

// Test spawning a program that immediately exits with a failure code.
#[tokio::test]
async fn spawn_immediate_exit() {
	let (prepare_worker_path, _) = build_workers_and_get_paths();

	// There's no explicit `exit` subcommand in the worker; it will panic on an unknown
	// subcommand anyway
	let spawn_timeout = Duration::from_secs(2);
	let result = spawn_with_program_path(
		"integration-test",
		prepare_worker_path,
		&env::temp_dir(),
		&["exit"],
		Duration::from_secs(2),
		SecurityStatus::default(),
	)
	.await;
	assert!(
		matches!(result, Err(SpawnErr::AcceptTimeout { spawn_timeout: s }) if s == spawn_timeout)
	);
}

#[tokio::test]
async fn spawn_timeout() {
	let (_, execute_worker_path) = build_workers_and_get_paths();

	let spawn_timeout = Duration::from_secs(2);
	let result = spawn_with_program_path(
		"integration-test",
		execute_worker_path,
		&env::temp_dir(),
		&["test-sleep"],
		spawn_timeout,
		SecurityStatus::default(),
	)
	.await;
	assert!(
		matches!(result, Err(SpawnErr::AcceptTimeout { spawn_timeout: s }) if s == spawn_timeout)
	);
}

#[tokio::test]
async fn should_connect() {
	let (prepare_worker_path, _) = build_workers_and_get_paths();

	let _ = spawn_with_program_path(
		"integration-test",
		prepare_worker_path,
		&env::temp_dir(),
		&["prepare-worker"],
		Duration::from_secs(2),
		SecurityStatus::default(),
	)
	.await
	.unwrap();
}
