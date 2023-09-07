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

use std::time::Duration;

#[cfg(test)]
use polkadot_node_core_pvf::testing::{spawn_with_program_path, SpawnErr};

// Test spawning a program that immediately exits with a failure code.
#[tokio::test]
async fn spawn_immediate_exit() {
	let mut worker_path = std::env::current_exe().unwrap();
	worker_path.pop();
	worker_path.pop();
	worker_path.push("polkadot-prepare-worker");
	let result =
		spawn_with_program_path("integration-test", worker_path, &["exit"], Duration::from_secs(2))
			.await;
	assert!(matches!(result, Err(SpawnErr::AcceptTimeout)));
}

#[tokio::test]
async fn spawn_timeout() {
	let mut worker_path = std::env::current_exe().unwrap();
	worker_path.pop();
	worker_path.pop();
	worker_path.push("polkadot-execute-worker");
	let result = spawn_with_program_path(
		"integration-test",
		worker_path,
		&["sleep"],
		Duration::from_secs(2),
	)
	.await;
	assert!(matches!(result, Err(SpawnErr::AcceptTimeout)));
}

#[tokio::test]
async fn should_connect() {
	let mut worker_path = std::env::current_exe().unwrap();
	worker_path.pop();
	worker_path.pop();
	worker_path.push("polkadot-prepare-worker");
	let _ = spawn_with_program_path(
		"integration-test",
		worker_path,
		&["prepare-worker"],
		Duration::from_secs(2),
	)
	.await
	.unwrap();
}
