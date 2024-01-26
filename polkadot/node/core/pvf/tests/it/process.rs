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

//! Test unexpected behaviors of the spawned processes. We test both worker processes (directly
//! spawned by the host) and job processes (spawned by the workers to securely perform PVF jobs).

use super::TestHost;
use assert_matches::assert_matches;
use polkadot_node_core_pvf::{
	InvalidCandidate, PossiblyInvalidError, PrepareError, ValidationError,
};
use polkadot_parachain_primitives::primitives::{BlockData, ValidationParams};
use procfs::process;
use rusty_fork::rusty_fork_test;
use std::time::Duration;

const PREPARE_PROCESS_NAME: &'static str = "polkadot-prepare-worker";
const EXECUTE_PROCESS_NAME: &'static str = "polkadot-execute-worker";

const SIGNAL_KILL: i32 = 9;
const SIGNAL_STOP: i32 = 19;

fn send_signal_by_sid_and_name(
	sid: i32,
	exe_name: &'static str,
	is_direct_child: bool,
	signal: i32,
) {
	let process = find_process_by_sid_and_name(sid, exe_name, is_direct_child);
	assert_eq!(unsafe { libc::kill(process.pid(), signal) }, 0);
}
fn get_num_threads_by_sid_and_name(sid: i32, exe_name: &'static str, is_direct_child: bool) -> i64 {
	let process = find_process_by_sid_and_name(sid, exe_name, is_direct_child);
	process.stat().unwrap().num_threads
}

fn find_process_by_sid_and_name(
	sid: i32,
	exe_name: &'static str,
	is_direct_child: bool,
) -> process::Process {
	let all_processes: Vec<process::Process> = process::all_processes()
		.expect("Can't read /proc")
		.filter_map(|p| match p {
			Ok(p) => Some(p), // happy path
			Err(e) => match e {
				// process vanished during iteration, ignore it
				procfs::ProcError::NotFound(_) => None,
				x => {
					panic!("some unknown error: {}", x);
				},
			},
		})
		.collect();

	let mut found = None;
	for process in all_processes {
		let stat = process.stat().unwrap();

		if stat.session != sid || !process.exe().unwrap().to_str().unwrap().contains(exe_name) {
			continue
		}
		// The workers are direct children of the current process, the worker job processes are not
		// (they are children of the workers).
		let process_is_direct_child = stat.ppid as u32 == std::process::id();
		if is_direct_child != process_is_direct_child {
			continue
		}

		if found.is_some() {
			panic!("Found more than one process")
		}
		found = Some(process);
	}
	found.expect("Should have found the expected process")
}

// Run these tests in their own processes with rusty-fork. They work by each creating a new session,
// then doing something with the child process that matches the session ID and expected process
// name.
rusty_fork_test! {
	// What happens when the prepare worker (not the job) times out?
	#[test]
	fn prepare_worker_timeout() {
		let rt  = tokio::runtime::Runtime::new().unwrap();
		rt.block_on(async {
			let host = TestHost::new().await;

			// Create a new session and get the session ID.
			let sid = unsafe { libc::setsid() };
			assert!(sid > 0);

			let (result, _) = futures::join!(
				// Choose a job that would normally take the entire timeout.
				host.precheck_pvf(rococo_runtime::WASM_BINARY.unwrap(), Default::default()),
				// Send a stop signal to pause the worker.
				async {
					tokio::time::sleep(Duration::from_secs(1)).await;
					send_signal_by_sid_and_name(sid, PREPARE_PROCESS_NAME, true, SIGNAL_STOP);
				}
			);

			assert_matches!(result, Err(PrepareError::TimedOut));
		})
	}

	// What happens when the execute worker (not the job) times out?
	#[test]
	fn execute_worker_timeout() {
		let rt  = tokio::runtime::Runtime::new().unwrap();
		rt.block_on(async {
			let host = TestHost::new().await;

			// Create a new session and get the session ID.
			let sid = unsafe { libc::setsid() };
			assert!(sid > 0);

			// Prepare the artifact ahead of time.
			let binary = halt::wasm_binary_unwrap();
			host.precheck_pvf(binary, Default::default()).await.unwrap();

			let (result, _) = futures::join!(
				// Choose an job that would normally take the entire timeout.
				host.validate_candidate(
					binary,
					ValidationParams {
						block_data: BlockData(Vec::new()),
						parent_head: Default::default(),
						relay_parent_number: 1,
						relay_parent_storage_root: Default::default(),
					},
					Default::default(),
				),
				// Send a stop signal to pause the worker.
				async {
					tokio::time::sleep(Duration::from_secs(1)).await;
					send_signal_by_sid_and_name(sid, EXECUTE_PROCESS_NAME, true, SIGNAL_STOP);
				}
			);

			assert_matches!(
				result,
				Err(ValidationError::Invalid(InvalidCandidate::HardTimeout))
			);
		})
	}

	// What happens when the prepare worker dies in the middle of a job?
	#[test]
	fn prepare_worker_killed_during_job() {
		let rt  = tokio::runtime::Runtime::new().unwrap();
		rt.block_on(async {
			let host = TestHost::new().await;

			// Create a new session and get the session ID.
			let sid = unsafe { libc::setsid() };
			assert!(sid > 0);

			let (result, _) = futures::join!(
				// Choose a job that would normally take the entire timeout.
				host.precheck_pvf(rococo_runtime::WASM_BINARY.unwrap(), Default::default()),
				// Run a future that kills the job while it's running.
				async {
					tokio::time::sleep(Duration::from_secs(1)).await;
					send_signal_by_sid_and_name(sid, PREPARE_PROCESS_NAME, true, SIGNAL_KILL);
				}
			);

			assert_matches!(result, Err(PrepareError::IoErr(_)));
		})
	}

	// What happens when the execute worker dies in the middle of a job?
	#[test]
	fn execute_worker_killed_during_job() {
		let rt  = tokio::runtime::Runtime::new().unwrap();
		rt.block_on(async {
			let host = TestHost::new().await;

			// Create a new session and get the session ID.
			let sid = unsafe { libc::setsid() };
			assert!(sid > 0);

			// Prepare the artifact ahead of time.
			let binary = halt::wasm_binary_unwrap();
			host.precheck_pvf(binary, Default::default()).await.unwrap();

			let (result, _) = futures::join!(
				// Choose an job that would normally take the entire timeout.
				host.validate_candidate(
					binary,
					ValidationParams {
						block_data: BlockData(Vec::new()),
						parent_head: Default::default(),
						relay_parent_number: 1,
						relay_parent_storage_root: Default::default(),
					},
					Default::default(),
				),
				// Run a future that kills the job while it's running.
				async {
					tokio::time::sleep(Duration::from_secs(1)).await;
					send_signal_by_sid_and_name(sid, EXECUTE_PROCESS_NAME, true, SIGNAL_KILL);
				}
			);

			assert_matches!(
				result,
				Err(ValidationError::PossiblyInvalid(PossiblyInvalidError::AmbiguousWorkerDeath))
			);
		})
	}

	// What happens when the forked prepare job dies in the middle of its job?
	#[test]
	fn forked_prepare_job_killed_during_job() {
		let rt  = tokio::runtime::Runtime::new().unwrap();
		rt.block_on(async {
			let host = TestHost::new().await;

			// Create a new session and get the session ID.
			let sid = unsafe { libc::setsid() };
			assert!(sid > 0);

			let (result, _) = futures::join!(
				// Choose a job that would normally take the entire timeout.
				host.precheck_pvf(rococo_runtime::WASM_BINARY.unwrap(), Default::default()),
				// Run a future that kills the job while it's running.
				async {
					tokio::time::sleep(Duration::from_secs(1)).await;
					send_signal_by_sid_and_name(sid, PREPARE_PROCESS_NAME, false, SIGNAL_KILL);
				}
			);

			// Note that we get a more specific error if the job died than if the whole worker died.
			assert_matches!(
				result,
				Err(PrepareError::JobDied{ err, job_pid: _ }) if err == "received signal: SIGKILL"
			);
		})
	}

	// What happens when the forked execute job dies in the middle of its job?
	#[test]
	fn forked_execute_job_killed_during_job() {
		let rt  = tokio::runtime::Runtime::new().unwrap();
		rt.block_on(async {
			let host = TestHost::new().await;

			// Create a new session and get the session ID.
			let sid = unsafe { libc::setsid() };
			assert!(sid > 0);

			// Prepare the artifact ahead of time.
			let binary = halt::wasm_binary_unwrap();
			host.precheck_pvf(binary, Default::default()).await.unwrap();

			let (result, _) = futures::join!(
				// Choose a job that would normally take the entire timeout.
				host.validate_candidate(
					binary,
					ValidationParams {
						block_data: BlockData(Vec::new()),
						parent_head: Default::default(),
						relay_parent_number: 1,
						relay_parent_storage_root: Default::default(),
					},
					Default::default(),
				),
				// Run a future that kills the job while it's running.
				async {
					tokio::time::sleep(Duration::from_secs(1)).await;
					send_signal_by_sid_and_name(sid, EXECUTE_PROCESS_NAME, false, SIGNAL_KILL);
				}
			);

			// Note that we get a more specific error if the job died than if the whole worker died.
			assert_matches!(
				result,
				Err(ValidationError::PossiblyInvalid(PossiblyInvalidError::AmbiguousJobDeath(err)))
					if err == "received signal: SIGKILL"
			);
		})
	}

	// Ensure that the spawned prepare worker is single-threaded.
	//
	// See `run_worker` for why we need this invariant.
	#[test]
	fn ensure_prepare_processes_have_correct_num_threads() {
		let rt  = tokio::runtime::Runtime::new().unwrap();
		rt.block_on(async {
			let host = TestHost::new().await;

			// Create a new session and get the session ID.
			let sid = unsafe { libc::setsid() };
			assert!(sid > 0);

			let _ = futures::join!(
				// Choose a job that would normally take the entire timeout.
				host.precheck_pvf(rococo_runtime::WASM_BINARY.unwrap(), Default::default()),
				// Run a future that kills the job while it's running.
				async {
					tokio::time::sleep(Duration::from_secs(1)).await;
					assert_eq!(
						get_num_threads_by_sid_and_name(sid, PREPARE_PROCESS_NAME, true),
						1
					);
					// Child job should have three threads: main thread, execute thread, CPU time
					// monitor, and memory tracking.
					assert_eq!(
						get_num_threads_by_sid_and_name(sid, PREPARE_PROCESS_NAME, false),
						4
					);

					// End the test.
					send_signal_by_sid_and_name(sid, PREPARE_PROCESS_NAME, true, SIGNAL_KILL);
				}
			);
		})
	}

	// Ensure that the spawned execute worker is single-threaded.
	//
	// See `run_worker` for why we need this invariant.
	#[test]
	fn ensure_execute_processes_have_correct_num_threads() {
		let rt  = tokio::runtime::Runtime::new().unwrap();
		rt.block_on(async {
			let host = TestHost::new().await;

			// Create a new session and get the session ID.
			let sid = unsafe { libc::setsid() };
			assert!(sid > 0);

			// Prepare the artifact ahead of time.
			let binary = halt::wasm_binary_unwrap();
			host.precheck_pvf(binary, Default::default()).await.unwrap();

			let _ = futures::join!(
				// Choose a job that would normally take the entire timeout.
				host.validate_candidate(
					binary,
					ValidationParams {
						block_data: BlockData(Vec::new()),
						parent_head: Default::default(),
						relay_parent_number: 1,
						relay_parent_storage_root: Default::default(),
					},
					Default::default(),
				),
				// Run a future that tests the thread count while the worker is running.
				async {
					tokio::time::sleep(Duration::from_secs(1)).await;
					assert_eq!(
						get_num_threads_by_sid_and_name(sid, EXECUTE_PROCESS_NAME, true),
						1
					);
					// Child job should have three threads: main thread, execute thread, and CPU
					// time monitor.
					assert_eq!(
						get_num_threads_by_sid_and_name(sid, EXECUTE_PROCESS_NAME, false),
						3
					);

					// End the test.
					send_signal_by_sid_and_name(sid, EXECUTE_PROCESS_NAME, true, SIGNAL_KILL);
				}
			);
		})
	}
}
