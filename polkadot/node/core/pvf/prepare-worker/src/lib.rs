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

//! Contains the logic for preparing PVFs. Used by the polkadot-prepare-worker binary.

mod executor_intf;
mod memory_stats;

pub use executor_intf::{prepare, prevalidate};
use libc;

// NOTE: Initializing logging in e.g. tests will not have an effect in the workers, as they are
//       separate spawned processes. Run with e.g. `RUST_LOG=parachain::pvf-prepare-worker=trace`.
const LOG_TARGET: &str = "parachain::pvf-prepare-worker";

#[cfg(target_os = "linux")]
use crate::memory_stats::max_rss_stat::{extract_max_rss_stat, get_max_rss_thread};
#[cfg(any(target_os = "linux", feature = "jemalloc-allocator"))]
use crate::memory_stats::memory_tracker::{get_memory_tracker_loop_stats, memory_tracker_loop};
use parity_scale_codec::{Decode, Encode};
use polkadot_node_core_pvf_common::{
	error::{PrepareError, PrepareResult},
	executor_intf::Executor,
	framed_recv, framed_send,
	prepare::{MemoryStats, PrepareJobKind, PrepareStats},
	pvf::PvfPrepData,
	worker::{
		bytes_to_path,
		security::LandlockStatus,
		thread::{self},
		worker_event_loop,
	},
};
use polkadot_primitives::ExecutorParams;
use std::{mem, path::PathBuf, process, time::Duration};
use std::io::{Read, Write};
use std::sync::Arc;
use futures::TryFutureExt;
use nix::sys::resource::Resource;
use os_pipe::PipeWriter;
use serde::{Deserialize, Serialize};
use tokio::{io, net::UnixStream};
use polkadot_node_core_pvf_common::worker::stringify_panic_payload;
use polkadot_node_core_pvf_common::worker::thread::{spawn_worker_thread, WaitOutcome};

/// Contains the bytes for a successfully compiled artifact.
#[derive(Serialize, Deserialize)]
pub struct CompiledArtifact(Vec<u8>);

impl CompiledArtifact {
	/// Creates a `CompiledArtifact`.
	pub fn new(code: Vec<u8>) -> Self {
		Self(code)
	}
}

impl AsRef<[u8]> for CompiledArtifact {
	fn as_ref(&self) -> &[u8] {
		self.0.as_slice()
	}
}

async fn recv_request(stream: &mut UnixStream) -> io::Result<(PvfPrepData, PathBuf)> {
	let pvf = framed_recv(stream).await?;
	let pvf = PvfPrepData::decode(&mut &pvf[..]).map_err(|e| {
		io::Error::new(
			io::ErrorKind::Other,
			format!("prepare pvf recv_request: failed to decode PvfPrepData: {}", e),
		)
	})?;
	let tmp_file = framed_recv(stream).await?;
	let tmp_file = bytes_to_path(&tmp_file).ok_or_else(|| {
		io::Error::new(
			io::ErrorKind::Other,
			"prepare pvf recv_request: non utf-8 artifact path".to_string(),
		)
	})?;
	Ok((pvf, tmp_file))
}

async fn send_response(stream: &mut UnixStream, result: PrepareResult) -> io::Result<()> {
	framed_send(stream, &result.encode()).await
}

/// The entrypoint that the spawned prepare worker should start with.
///
/// # Parameters
///
/// The `socket_path` specifies the path to the socket used to communicate with the host. The
/// `node_version`, if `Some`, is checked against the worker version. A mismatch results in
/// immediate worker termination. `None` is used for tests and in other situations when version
/// check is not necessary.
///
/// # Flow
///
/// This runs the following in a loop:
///
/// 1. Get the code and parameters for preparation from the host.
///
/// 2. Start a new child process
///
/// 3. Start a memory tracker in a separate thread.
///
/// 3. Start the actual preparation in a separate thread.
///
/// 4. Wait on the thread created in step 3.
///
/// 5. Stop the memory tracker and get the stats.
///
/// 6. Pipe the result back to the parent process and exit from child process.
///
/// 7. If compilation succeeded, write the compiled artifact into a temporary file.
///
/// 8. Send the result of preparation back to the host. If any error occurred in the above steps, we
///    send that in the `PrepareResult`.
pub fn worker_entrypoint(
	socket_path: &str,
	node_version: Option<&str>,
	worker_version: Option<&str>,
) {
	worker_event_loop(
		"prepare",
		socket_path,
		node_version,
		worker_version,
		|mut stream| async move {
			let worker_pid = std::process::id();

			loop {
				let (pvf, temp_artifact_dest) = recv_request(&mut stream).await?;
				gum::debug!(
					target: LOG_TARGET,
					%worker_pid,
					"worker: preparing artifact",
				);

				let preparation_timeout = pvf.prep_timeout();
				let prepare_job_kind = pvf.prep_kind();
				let executor_params = (*pvf.executor_params()).clone();

				let (pipe_reader, pipe_writer) = os_pipe::pipe()?;

				// SAFETY: new process is spawned within a single threaded process
				let result = match unsafe { libc::fork() } {
					// error
					-1 => Err(PrepareError::Panic(String::from("error forking"))),
					// child
					0 => handle_child_process(
							pvf,
							pipe_writer,
							preparation_timeout,
							prepare_job_kind,
							executor_params,
						)
						.await,
					// parent
					_ => handle_parent_process(
							pipe_reader,
							pipe_writer,
							temp_artifact_dest,
							preparation_timeout,
							worker_pid,
						)
						.await
				};
				send_response(&mut stream, result).await?;
			}
		},
	);
}

fn prepare_artifact(pvf: PvfPrepData) -> Result<CompiledArtifact, PrepareError> {
	let blob = match prevalidate(&pvf.code()) {
		Err(err) => return Err(PrepareError::Prevalidation(format!("{:?}", err))),
		Ok(b) => b,
	};

	match prepare(blob, &pvf.executor_params()) {
		Ok(compiled_artifact) => Ok(CompiledArtifact::new(compiled_artifact)),
		Err(err) => Err(PrepareError::Preparation(format!("{:?}", err))),
	}
}

/// Try constructing the runtime to catch any instantiation errors during pre-checking.
fn runtime_construction_check(
	artifact_bytes: &[u8],
	executor_params: ExecutorParams,
) -> Result<(), PrepareError> {
	let executor = Executor::new(executor_params)
		.map_err(|e| PrepareError::RuntimeConstruction(format!("cannot create executor: {}", e)))?;

	// SAFETY: We just compiled this artifact.
	let result = unsafe { executor.create_runtime_from_bytes(&artifact_bytes) };
	result
		.map(|_runtime| ())
		.map_err(|err| PrepareError::RuntimeConstruction(format!("{:?}", err)))
}

#[derive(Serialize, Deserialize)]
struct Response {
	artifact_result: Result<CompiledArtifact, PrepareError>,
	landlock_status: Result<LandlockStatus, String>,
	memory_stats: MemoryStats,
}

async fn handle_child_process(
	pvf: PvfPrepData,
	mut pipe_write: os_pipe::PipeWriter,
	preparation_timeout: Duration,
	prepare_job_kind: PrepareJobKind,
	executor_params: ExecutorParams,
) -> ! {

	let worker_pid = std::process::id();

	nix::sys::resource::setrlimit(
		Resource::RLIMIT_CPU,
		preparation_timeout.as_secs(),
		preparation_timeout.as_secs()
	).unwrap_or_else(|e| {
		process::exit(libc::EXIT_FAILURE)
	});


	let condvar = thread::get_condvar();

	// Run the memory tracker in a regular, non-worker thread.
	#[cfg(any(target_os = "linux", feature = "jemalloc-allocator"))]
		let condvar_memory = Arc::clone(&condvar);
	#[cfg(any(target_os = "linux", feature = "jemalloc-allocator"))]
		let memory_tracker_thread = std::thread::spawn(|| memory_tracker_loop(condvar_memory));

	let prepare_thread = spawn_worker_thread(
		"prepare worker",
		move || {
			// Try to enable landlock.
			#[cfg(target_os = "linux")]
				let landlock_status =
				polkadot_node_core_pvf_common::worker::security::landlock::try_restrict_thread()
					.map(LandlockStatus::from_ruleset_status)
					.map_err(|e| e.to_string());
			#[cfg(not(target_os = "linux"))]
				let landlock_status: Result<LandlockStatus, String> = Ok(LandlockStatus::NotEnforced);

			#[allow(unused_mut)]
				let mut artifact_result = prepare_artifact(pvf);

			// If we are pre-checking, check for runtime construction errors.
			//
			// As pre-checking is more strict than just preparation in terms of memory
			// and time, it is okay to do extra checks here. This takes negligible time
			// anyway.
			if let PrepareJobKind::Prechecking = prepare_job_kind {
				artifact_result = artifact_result.and_then(|output| {
					runtime_construction_check(output.as_ref(), executor_params)?;
					Ok(output)
				});
			}
			(artifact_result, landlock_status)
		},
		Arc::clone(&condvar),
		WaitOutcome::Finished
	).unwrap_or_else(|_| {
		process::exit(libc::EXIT_FAILURE)
	});

	let outcome = thread::wait_for_threads(condvar);

	match outcome {
		WaitOutcome::Finished => {
			 let (artifact_result, landlock_status) = prepare_thread.join().unwrap_or_else(|err| {
				(
					Err(PrepareError::Panic(stringify_panic_payload(err))),
					Ok(LandlockStatus::Unavailable),
				)
			});

			// Get the `ru_maxrss` stat. If supported, call getrusage for the thread.
			#[cfg(target_os = "linux")]
				let max_rss = get_max_rss_thread();

			// Stop the memory stats worker and get its observed memory stats.
			#[cfg(any(target_os = "linux", feature = "jemalloc-allocator"))]
				let memory_tracker_stats = get_memory_tracker_loop_stats(memory_tracker_thread, worker_pid).await;

			let memory_stats = MemoryStats {
				#[cfg(any(target_os = "linux", feature = "jemalloc-allocator"))]
				memory_tracker_stats,
				#[cfg(target_os = "linux")]
				max_rss: extract_max_rss_stat(max_rss, worker_pid),
			};

			let response = Response { artifact_result, landlock_status, memory_stats };

			let bytes = bincode::serialize(&response).unwrap_or_else(|_| {
				process::exit(libc::EXIT_FAILURE)
			});

			pipe_write.write_all(bytes.as_slice()).unwrap_or_else(|_| {
				process::exit(libc::EXIT_FAILURE)
			});

			process::exit(libc::EXIT_SUCCESS);

		},
		_ => {
			process::exit(libc::EXIT_FAILURE)
		}
	}
}

async fn handle_parent_process(
	mut pipe_read: os_pipe::PipeReader,
	pipe_write: PipeWriter,
	temp_artifact_dest: PathBuf,
	preparation_timeout: Duration,
	worker_pid: u32,
) -> Result<PrepareStats, PrepareError> {
	drop(pipe_write);

	return match nix::sys::wait::wait() {
		Ok(nix::sys::wait::WaitStatus::Exited(_, libc::EXIT_SUCCESS)) => {
			let data_size = mem::size_of::<Response>();

			let mut received_data = Vec::new();

			pipe_read.read_to_end(&mut received_data).map_err(|_| {
				PrepareError::Panic(format!("error reading pipe for worker id {}", worker_pid))
			})?;

			let result: Response = bincode::deserialize(received_data.as_slice()).map_err(|e| {
				PrepareError::Panic(e.to_string())
			})?;

			drop(pipe_read);

			match result.artifact_result {
				Err(err) => Err(err),
				Ok(artifact) => {
					// Log if landlock threw an error.
					if let Err(err) = &result.landlock_status {
						gum::warn!(
							target: LOG_TARGET,
							%worker_pid,
							"error enabling landlock: {}",
							err
						);
					}

					// Write the serialized artifact into a temp file.
					//
					// PVF host only keeps artifacts statuses in its memory,
					// successfully compiled code gets stored on the disk (and
					// consequently deserialized by execute-workers). The prepare worker
					// is only required to send `Ok` to the pool to indicate the
					// success.
					gum::debug!(
						target: LOG_TARGET,
						%worker_pid,
						"worker: writing artifact to {}",
						temp_artifact_dest.display(),
					);
					if let Err(err) = tokio::fs::write(&temp_artifact_dest, &artifact).await {
						return Err(PrepareError::Panic(format!("{:?}", err)))
					};

					Ok(PrepareStats {
						memory_stats: result.memory_stats,
					})
				},
			}
		},
		Ok(nix::sys::wait::WaitStatus::Signaled(_, nix::sys::signal::Signal::SIGXCPU, _)) => {
			Err(PrepareError::TimedOut)
		},
		_ => {
			Err(PrepareError::Panic(format!("child failed")))
		},
	}
}