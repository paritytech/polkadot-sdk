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
use nix::sys::resource::{Resource, Usage, UsageWho};
use os_pipe::{self, PipeWriter};
use parity_scale_codec::{Decode, Encode};
use polkadot_node_core_pvf_common::{
	error::{PrepareError, PrepareResult},
	executor_intf::create_runtime_from_artifact_bytes,
	framed_recv_blocking, framed_send_blocking,
	prepare::{MemoryStats, PrepareJobKind, PrepareStats},
	pvf::PvfPrepData,
	worker::{
		stringify_panic_payload,
		thread::{self, spawn_worker_thread, WaitOutcome},
		worker_event_loop, WorkerKind,
	},
	worker_dir, SecurityStatus,
};
use polkadot_primitives::ExecutorParams;
use std::{
	fs,
	io::{self, Read, Write},
	os::unix::net::UnixStream,
	path::PathBuf,
	process,
	sync::Arc,
	time::Duration,
};

/// Contains the bytes for a successfully compiled artifact.
#[derive(Encode, Decode)]
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

fn recv_request(stream: &mut UnixStream) -> io::Result<PvfPrepData> {
	let pvf = framed_recv_blocking(stream)?;
	let pvf = PvfPrepData::decode(&mut &pvf[..]).map_err(|e| {
		io::Error::new(
			io::ErrorKind::Other,
			format!("prepare pvf recv_request: failed to decode PvfPrepData: {}", e),
		)
	})?;
	Ok(pvf)
}

fn send_response(stream: &mut UnixStream, result: PrepareResult) -> io::Result<()> {
	framed_send_blocking(stream, &result.encode())
}

/// The entrypoint that the spawned prepare worker should start with.
///
/// # Parameters
///
/// - `socket_path`: specifies the path to the socket used to communicate with the host.
///
/// - `worker_dir_path`: specifies the path to the worker-specific temporary directory.
///
/// - `node_version`: if `Some`, is checked against the `worker_version`. A mismatch results in
///   immediate worker termination. `None` is used for tests and in other situations when version
///   check is not necessary.
///
/// - `worker_version`: see above
///
/// - `security_status`: contains the detected status of security features.
///
/// # Flow
///
/// This runs the following in a loop:
///
/// 1. Get the code and parameters for preparation from the host.
///
/// 2. Start a new child process
///
/// 3. Start the memory tracker and the actual preparation in two separate threads.
///
/// 4. Wait on the two threads created in step 3.
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
	socket_path: PathBuf,
	worker_dir_path: PathBuf,
	node_version: Option<&str>,
	worker_version: Option<&str>,
	security_status: SecurityStatus,
) {
	worker_event_loop(
		WorkerKind::Prepare,
		socket_path,
		worker_dir_path,
		node_version,
		worker_version,
		&security_status,
		|mut stream, worker_dir_path| {
			let worker_pid = process::id();
			let temp_artifact_dest = worker_dir::prepare_tmp_artifact(&worker_dir_path);

			loop {
				let pvf = recv_request(&mut stream)?;
				gum::debug!(
					target: LOG_TARGET,
					%worker_pid,
					"worker: preparing artifact",
				);

				let preparation_timeout = pvf.prep_timeout();
				let prepare_job_kind = pvf.prep_kind();
				let executor_params = pvf.executor_params();

				let (pipe_reader, pipe_writer) = os_pipe::pipe()?;

				let usage_before = nix::sys::resource::getrusage(UsageWho::RUSAGE_CHILDREN)?;

				// SAFETY: new process is spawned within a single threaded process
				let result = match unsafe { libc::fork() } {
					// error
					-1 => Err(PrepareError::Panic(String::from("error forking"))),
					// child
					0 => {
						drop(stream);
						handle_child_process(
							pvf,
							pipe_writer,
							preparation_timeout,
							prepare_job_kind,
							executor_params,
						)
					},
					// parent
					_ => {
						// the read end will wait until all ends have been closed,
						// this drop is necessary to avoid deadlock
						drop(pipe_writer);
						handle_parent_process(
							pipe_reader,
							temp_artifact_dest.clone(),
							worker_pid,
							usage_before,
							preparation_timeout.as_secs(),
						)
					},
				};
				send_response(&mut stream, result)?;
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
	executor_params: &ExecutorParams,
) -> Result<(), PrepareError> {
	// SAFETY: We just compiled this artifact.
	let result = unsafe { create_runtime_from_artifact_bytes(artifact_bytes, executor_params) };
	result
		.map(|_runtime| ())
		.map_err(|err| PrepareError::RuntimeConstruction(format!("{:?}", err)))
}

#[derive(Encode, Decode)]
struct Response {
	artifact: CompiledArtifact,
	memory_stats: MemoryStats,
}

/// This is used to handle child process during pvf prepare worker.
/// It prepare the artifact and track memory stats during preparation
/// and pipes back the response to the parent process
///
/// # Arguments
///
/// - `pvf`: `PvfPrepData` structure, containing data to prepare the artifact
///
/// - `pipe_write`: A `os_pipe::PipeWriter` structure, the writing end of a pipe.
///
/// - `preparation_timeout`: The timeout in `Duration`.
///
/// - `prepare_job_kind`: The kind of prepare job.
///
/// - `executor_params`: Deterministically serialized execution environment semantics.
///
/// # Returns
///
/// - If any error occur, pipe response back with `PrepareError`.
///
/// - If success, pipe back `Response`.
fn handle_child_process(
	pvf: PvfPrepData,
	pipe_write: os_pipe::PipeWriter,
	preparation_timeout: Duration,
	prepare_job_kind: PrepareJobKind,
	executor_params: Arc<ExecutorParams>,
) -> ! {
	// Set a hard CPU time limit for the child process.
	nix::sys::resource::setrlimit(
		Resource::RLIMIT_CPU,
		preparation_timeout.as_secs(),
		preparation_timeout.as_secs(),
	)
	.unwrap_or_else(|err| {
		send_child_response(&pipe_write, Err(PrepareError::Panic(err.to_string())))
	});

	// Conditional variable to notify us when a thread is done.
	let condvar = thread::get_condvar();

	// Run the memory tracker in a regular, non-worker thread.
	#[cfg(any(target_os = "linux", feature = "jemalloc-allocator"))]
	let condvar_memory = Arc::clone(&condvar);
	#[cfg(any(target_os = "linux", feature = "jemalloc-allocator"))]
	let memory_tracker_thread = std::thread::spawn(|| memory_tracker_loop(condvar_memory));

	let prepare_thread = spawn_worker_thread(
		"prepare worker",
		move || {
			#[allow(unused_mut)]
			let mut result = prepare_artifact(pvf);

			// Get the `ru_maxrss` stat. If supported, call getrusage for the thread.
			#[cfg(target_os = "linux")]
			let mut result = result.map(|artifact| (artifact, get_max_rss_thread()));

			// If we are pre-checking, check for runtime construction errors.
			//
			// As pre-checking is more strict than just preparation in terms of memory
			// and time, it is okay to do extra checks here. This takes negligible time
			// anyway.
			if let PrepareJobKind::Prechecking = prepare_job_kind {
				result = result.and_then(|output| {
					#[cfg(target_os = "linux")]
					runtime_construction_check(output.0.as_ref(), &executor_params)?;
					#[cfg(not(target_os = "linux"))]
					runtime_construction_check(output.as_ref(), executor_params.as_ref())?;
					Ok(output)
				});
			}
			result
		},
		Arc::clone(&condvar),
		WaitOutcome::Finished,
	)
	.unwrap_or_else(|err| {
		send_child_response(&pipe_write, Err(PrepareError::Panic(err.to_string())))
	});

	// There's only one thread that can trigger the condvar, so ignore the condvar outcome and
	// simply join. We don't have to be concerned with timeouts, setrlimit will kill the process.
	let result = prepare_thread.join().unwrap_or_else(|err| {
		send_child_response(&pipe_write, Err(PrepareError::Panic(stringify_panic_payload(err))))
	});

	let response: Result<Response, PrepareError> = match result {
		Ok(ok) => {
			cfg_if::cfg_if! {
				if #[cfg(target_os = "linux")] {
					let (artifact, max_rss) = ok;
				} else {
					let artifact = ok;
				}
			}

			// Stop the memory stats worker and get its observed memory stats.
			#[cfg(any(target_os = "linux", feature = "jemalloc-allocator"))]
			let memory_tracker_stats = get_memory_tracker_loop_stats(memory_tracker_thread, process::id());

			let memory_stats = MemoryStats {
				#[cfg(any(target_os = "linux", feature = "jemalloc-allocator"))]
				memory_tracker_stats,
				#[cfg(target_os = "linux")]
				max_rss: extract_max_rss_stat(max_rss, process::id()),
			};

			Ok(Response { artifact, memory_stats })
		},
		Err(err) => Err(err),
	};

	send_child_response(&pipe_write, response);
}

/// Waits for child process to finish and handle child response from pipe.
///
/// # Arguments
///
/// - `pipe_read`: A `PipeReader` used to read data from the child process.
///
/// - `temp_artifact_dest`: The destination `PathBuf` to write the temporary artifact file.
///
/// - `worker_pid`: The PID of the child process.
///
/// - `usage_before`: Resource usage statistics before executing the child process.
///
/// - `timeout`: The maximum allowed time for the child process to finish, in milliseconds.
///
/// # Returns
///
/// - If the child send response without an error, this function returns `Ok(PrepareStats)`
///   containing memory and CPU usage statistics.
///
/// - If the child send response with an error, it returns a `PrepareError`.
///
/// - If the child process timeout, it returns `PrepareError::TimedOut`.
///
/// - If the child process exits with an unknown status, it returns `PrepareError`.
fn handle_parent_process(
	mut pipe_read: os_pipe::PipeReader,
	temp_artifact_dest: PathBuf,
	worker_pid: u32,
	usage_before: Usage,
	timeout: u64,
) -> Result<PrepareStats, PrepareError> {
	let mut received_data = Vec::new();

	pipe_read
		.read_to_end(&mut received_data)
		.map_err(|err| PrepareError::Panic(err.to_string()))?;
	let status = nix::sys::wait::wait();
	let usage_after = nix::sys::resource::getrusage(UsageWho::RUSAGE_CHILDREN)
		.map_err(|err| PrepareError::Panic(err.to_string()))?;

	// Using `getrusage` is needed to check whether `setrlimit` was triggered.
	// As `getrusage` returns resource usage from all terminated child processes,
	// it is necessary to subtract the usage before the current child process to isolate its cpu
	// time
	let cpu_tv = get_total_cpu_usage(usage_after) - get_total_cpu_usage(usage_before);

	if cpu_tv.as_secs() >= timeout {
		return Err(PrepareError::TimedOut)
	}

	return match status {
		Ok(_) => {
			let result: Result<Response, PrepareError> =
				Result::decode(&mut received_data.as_slice())
					.map_err(|e| PrepareError::Panic(e.to_string()))?;
			match result {
				Err(err) => Err(err),
				Ok(response) => {
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
					if let Err(err) = fs::write(&temp_artifact_dest, &response.artifact) {
						return Err(PrepareError::Panic(format!("{:?}", err)))
					};

					Ok(PrepareStats {
						memory_stats: response.memory_stats,
						cpu_time_elapsed: cpu_tv,
					})
				},
			}
		},
		_ => Err(PrepareError::Panic("child finished with unknown status".to_string())),
	}
}

/// Calculate the total CPU time from the given `usage` structure, returned from
/// [`nix::sys::resource::getrusage`], and calculates the total CPU time spent, including both user
/// and system time.
///
/// # Arguments
///
/// - `rusage`: Contains resource usage information.
///
/// # Returns
///
/// Returns a `Duration` representing the total CPU time.
fn get_total_cpu_usage(rusage: Usage) -> Duration {
	let micros = (((rusage.user_time().tv_sec() + rusage.system_time().tv_sec()) * 1_000_000) +
		(rusage.system_time().tv_usec() + rusage.user_time().tv_usec()) as i64) as u64;

	return Duration::from_micros(micros)
}

/// Write response to the pipe and exit process after.
///
/// # Arguments
///
/// - `pipe_write`: A `os_pipe::PipeWriter` structure, the writing end of a pipe.
///
/// - `response`: Child process response
fn send_child_response(mut pipe_write: &PipeWriter, response: Result<Response, PrepareError>) -> ! {
	pipe_write
		.write_all(response.encode().as_slice())
		.unwrap_or_else(|_| process::exit(libc::EXIT_FAILURE));

	process::exit(libc::EXIT_SUCCESS)
}
