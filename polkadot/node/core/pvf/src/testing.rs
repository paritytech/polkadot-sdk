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

//! Various things for testing other crates.

pub use crate::{
	host::{EXECUTE_BINARY_NAME, PREPARE_BINARY_NAME},
	worker_intf::{spawn_with_program_path, SpawnErr},
};

use crate::get_worker_version;
use is_executable::IsExecutable;
use polkadot_node_primitives::NODE_VERSION;
use polkadot_primitives::ExecutorParams;
use std::{
	path::PathBuf,
	sync::{Mutex, OnceLock},
};

/// A function that emulates the stitches together behaviors of the preparation and the execution
/// worker in a single synchronous function.
pub fn validate_candidate(
	code: &[u8],
	params: &[u8],
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
	use polkadot_node_core_pvf_execute_worker::execute_artifact;
	use polkadot_node_core_pvf_prepare_worker::{prepare, prevalidate};

	let code = sp_maybe_compressed_blob::decompress(code, 10 * 1024 * 1024)
		.expect("Decompressing code failed");

	let blob = prevalidate(&code)?;
	let executor_params = ExecutorParams::default();
	let compiled_artifact_blob = prepare(blob, &executor_params)?;

	let result = unsafe {
		// SAFETY: This is trivially safe since the artifact is obtained by calling `prepare`
		//         and is written into a temporary directory in an unmodified state.
		execute_artifact(&compiled_artifact_blob, &executor_params, params)?
	};

	Ok(result)
}

/// Retrieves the worker paths, checks that they exist and does a version check.
///
/// NOTE: This should only be called in dev code (tests, benchmarks) as it relies on the relative
/// paths of the built workers.
pub fn get_and_check_worker_paths() -> (PathBuf, PathBuf) {
	// Only needs to be called once for the current process.
	static WORKER_PATHS: OnceLock<Mutex<(PathBuf, PathBuf)>> = OnceLock::new();
	let mutex = WORKER_PATHS.get_or_init(|| {
		let mut workers_path = std::env::current_exe().unwrap();
		workers_path.pop();
		workers_path.pop();
		let mut prepare_worker_path = workers_path.clone();
		prepare_worker_path.push(PREPARE_BINARY_NAME);
		let mut execute_worker_path = workers_path.clone();
		execute_worker_path.push(EXECUTE_BINARY_NAME);

		// Check that the workers are valid.
		if !prepare_worker_path.is_executable() || !execute_worker_path.is_executable() {
			panic!("ERROR: Workers do not exist or are not executable. Workers directory: {:?}", workers_path);
		}

		let worker_version =
			get_worker_version(&prepare_worker_path).expect("checked for worker existence");
		if worker_version != NODE_VERSION {
			panic!("ERROR: Prepare worker version {worker_version} does not match node version {NODE_VERSION}; worker path: {prepare_worker_path:?}");
		}
		let worker_version =
			get_worker_version(&execute_worker_path).expect("checked for worker existence");
		if worker_version != NODE_VERSION {
			panic!("ERROR: Execute worker version {worker_version} does not match node version {NODE_VERSION}; worker path: {execute_worker_path:?}");
		}

		// We don't want to check against the commit hash because we'd have to always rebuild
		// the calling crate on every commit.
		eprintln!("WARNING: Workers match the node version, but may have changed in recent commits. Please rebuild them if anything funny happens. Workers path: {workers_path:?}");

		Mutex::new((prepare_worker_path, execute_worker_path))
	});

	let guard = mutex.lock().unwrap();
	(guard.0.clone(), guard.1.clone())
}
