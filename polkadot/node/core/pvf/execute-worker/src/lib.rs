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

//! Contains the logic for executing PVFs. Used by the polkadot-execute-worker binary.

pub use polkadot_node_core_pvf_common::{executor_interface::execute_artifact, worker_dir};

// NOTE: Initializing logging in e.g. tests will not have an effect in the workers, as they are
//       separate spawned processes. Run with e.g. `RUST_LOG=parachain::pvf-execute-worker=trace`.
const LOG_TARGET: &str = "parachain::pvf-execute-worker";

use cpu_time::ProcessTime;
use nix::{
	errno::Errno,
	sys::{
		resource::{Usage, UsageWho},
		wait::WaitStatus,
	},
	unistd::{ForkResult, Pid},
};
use os_pipe::{self, PipeReader, PipeWriter};
use parity_scale_codec::{Decode, Encode};
use polkadot_node_core_pvf_common::{
	error::InternalValidationError,
	execute::{Handshake, JobError, JobResponse, JobResult, WorkerResponse},
	framed_recv_blocking, framed_send_blocking,
	worker::{
		cpu_time_monitor_loop, run_worker, stringify_panic_payload,
		thread::{self, WaitOutcome},
		WorkerKind,
	},
};
use polkadot_parachain_primitives::primitives::ValidationResult;
use polkadot_primitives::{executor_params::DEFAULT_NATIVE_STACK_MAX, ExecutorParams};
use std::{
	io::{self, Read},
	os::unix::net::UnixStream,
	path::PathBuf,
	process,
	sync::{mpsc::channel, Arc},
	time::Duration,
};

// Wasmtime powers the Substrate Executor. It compiles the wasm bytecode into native code.
// That native code does not create any stacks and just reuses the stack of the thread that
// wasmtime was invoked from.
//
// Also, we configure the executor to provide the deterministic stack and that requires
// supplying the amount of the native stack space that wasm is allowed to use. This is
// realized by supplying the limit into `wasmtime::Config::max_wasm_stack`.
//
// There are quirks to that configuration knob:
//
// 1. It only limits the amount of stack space consumed by wasm but does not ensure nor check that
//    the stack space is actually available.
//
//    That means, if the calling thread has 1 MiB of stack space left and the wasm code consumes
//    more, then the wasmtime limit will **not** trigger. Instead, the wasm code will hit the
//    guard page and the Rust stack overflow handler will be triggered. That leads to an
//    **abort**.
//
// 2. It cannot and does not limit the stack space consumed by Rust code.
//
//    Meaning that if the wasm code leaves no stack space for Rust code, then the Rust code
//    will abort and that will abort the process as well.
//
// Typically on Linux the main thread gets the stack size specified by the `ulimit` and
// typically it's configured to 8 MiB. Rust's spawned threads are 2 MiB. OTOH, the
// DEFAULT_NATIVE_STACK_MAX is set to 256 MiB. Not nearly enough.
//
// Hence we need to increase it. The simplest way to fix that is to spawn a thread with the desired
// stack limit.
//
// The reasoning why we pick this particular size is:
//
// The default Rust thread stack limit 2 MiB + 256 MiB wasm stack.
/// The stack size for the execute thread.
pub const EXECUTE_THREAD_STACK_SIZE: usize = 2 * 1024 * 1024 + DEFAULT_NATIVE_STACK_MAX as usize;

/// Receives a handshake with information specific to the execute worker.
fn recv_execute_handshake(stream: &mut UnixStream) -> io::Result<Handshake> {
	let handshake_enc = framed_recv_blocking(stream)?;
	let handshake = Handshake::decode(&mut &handshake_enc[..]).map_err(|_| {
		io::Error::new(
			io::ErrorKind::Other,
			"execute pvf recv_execute_handshake: failed to decode Handshake".to_owned(),
		)
	})?;
	Ok(handshake)
}

fn recv_request(stream: &mut UnixStream) -> io::Result<(Vec<u8>, Duration)> {
	let params = framed_recv_blocking(stream)?;
	let execution_timeout = framed_recv_blocking(stream)?;
	let execution_timeout = Duration::decode(&mut &execution_timeout[..]).map_err(|_| {
		io::Error::new(
			io::ErrorKind::Other,
			"execute pvf recv_request: failed to decode duration".to_string(),
		)
	})?;
	Ok((params, execution_timeout))
}

fn send_response(stream: &mut UnixStream, response: WorkerResponse) -> io::Result<()> {
	framed_send_blocking(stream, &response.encode())
}

/// The entrypoint that the spawned execute worker should start with.
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
pub fn worker_entrypoint(
	socket_path: PathBuf,
	worker_dir_path: PathBuf,
	node_version: Option<&str>,
	worker_version: Option<&str>,
) {
	run_worker(
		WorkerKind::Execute,
		socket_path,
		worker_dir_path,
		node_version,
		worker_version,
		|mut stream, worker_dir_path| {
			let worker_pid = process::id();
			let artifact_path = worker_dir::execute_artifact(&worker_dir_path);

			let Handshake { executor_params } = recv_execute_handshake(&mut stream)?;

			loop {
				let (params, execution_timeout) = recv_request(&mut stream)?;
				gum::debug!(
					target: LOG_TARGET,
					%worker_pid,
					"worker: validating artifact {}",
					artifact_path.display(),
				);

				// Get the artifact bytes.
				let compiled_artifact_blob = match std::fs::read(&artifact_path) {
					Ok(bytes) => bytes,
					Err(err) => {
						let response = WorkerResponse::InternalError(
							InternalValidationError::CouldNotOpenFile(err.to_string()),
						);
						send_response(&mut stream, response)?;
						continue
					},
				};

				let (pipe_reader, pipe_writer) = os_pipe::pipe()?;

				let usage_before = match nix::sys::resource::getrusage(UsageWho::RUSAGE_CHILDREN) {
					Ok(usage) => usage,
					Err(errno) => {
						let response = internal_error_from_errno("getrusage before", errno);
						send_response(&mut stream, response)?;
						continue
					},
				};

				// SAFETY: new process is spawned within a single threaded process. This invariant
				// is enforced by tests.
				let response = match unsafe { nix::unistd::fork() } {
					Err(errno) => internal_error_from_errno("fork", errno),
					Ok(ForkResult::Child) => {
						// Dropping the stream closes the underlying socket. We want to make sure
						// that the sandboxed child can't get any kind of information from the
						// outside world. The only IPC it should be able to do is sending its
						// response over the pipe.
						drop(stream);
						// Drop the read end so we don't have too many FDs open.
						drop(pipe_reader);

						handle_child_process(
							pipe_writer,
							compiled_artifact_blob,
							executor_params,
							params,
							execution_timeout,
						)
					},
					Ok(ForkResult::Parent { child }) => {
						// the read end will wait until all write ends have been closed,
						// this drop is necessary to avoid deadlock
						drop(pipe_writer);

						handle_parent_process(
							pipe_reader,
							child,
							worker_pid,
							usage_before,
							execution_timeout,
						)?
					},
				};

				send_response(&mut stream, response)?;
			}
		},
	);
}

fn validate_using_artifact(
	compiled_artifact_blob: &[u8],
	executor_params: &ExecutorParams,
	params: &[u8],
) -> JobResponse {
	let descriptor_bytes = match unsafe {
		// SAFETY: this should be safe since the compiled artifact passed here comes from the
		//         file created by the prepare workers. These files are obtained by calling
		//         [`executor_interface::prepare`].
		execute_artifact(compiled_artifact_blob, executor_params, params)
	} {
		Err(err) => return JobResponse::format_invalid("execute", &err),
		Ok(d) => d,
	};

	let result_descriptor = match ValidationResult::decode(&mut &descriptor_bytes[..]) {
		Err(err) =>
			return JobResponse::format_invalid(
				"validation result decoding failed",
				&err.to_string(),
			),
		Ok(r) => r,
	};

	JobResponse::Ok { result_descriptor }
}

/// This is used to handle child process during pvf execute worker.
/// It execute the artifact and pipes back the response to the parent process
///
/// # Arguments
///
/// - `pipe_write`: A `PipeWriter` structure, the writing end of a pipe.
///
/// - `compiled_artifact_blob`: The artifact bytes from compiled by the prepare worker`.
///
/// - `executor_params`: Deterministically serialized execution environment semantics.
///
/// - `params`: Validation parameters.
///
/// - `execution_timeout`: The timeout in `Duration`.
///
/// # Returns
///
/// - pipe back `JobResponse` to the parent process.
fn handle_child_process(
	mut pipe_write: PipeWriter,
	compiled_artifact_blob: Vec<u8>,
	executor_params: ExecutorParams,
	params: Vec<u8>,
	execution_timeout: Duration,
) -> ! {
	// Terminate if the parent thread dies. Parent thread == worker process (it is single-threaded).
	//
	// RACE: the worker may die before we install the death signal. In practice this is unlikely,
	// and most of the time the job process should terminate on its own when it completes.
	#[cfg(target_os = "linux")]
	nix::sys::prctl::set_pdeathsig(nix::sys::signal::Signal::SIGTERM).unwrap_or_else(|err| {
		send_child_response(&mut pipe_write, Err(JobError::CouldNotSetPdeathsig(err.to_string())))
	});

	gum::debug!(
		target: LOG_TARGET,
		worker_job_pid = %process::id(),
		"worker job: executing artifact",
	);

	// Conditional variable to notify us when a thread is done.
	let condvar = thread::get_condvar();
	let cpu_time_start = ProcessTime::now();

	// Spawn a new thread that runs the CPU time monitor.
	let (cpu_time_monitor_tx, cpu_time_monitor_rx) = channel::<()>();
	let cpu_time_monitor_thread = thread::spawn_worker_thread(
		"cpu time monitor thread",
		move || cpu_time_monitor_loop(cpu_time_start, execution_timeout, cpu_time_monitor_rx),
		Arc::clone(&condvar),
		WaitOutcome::TimedOut,
	)
	.unwrap_or_else(|err| {
		send_child_response(&mut pipe_write, Err(JobError::CouldNotSpawnThread(err.to_string())))
	});

	let executor_params_2 = executor_params.clone();
	let execute_thread = thread::spawn_worker_thread_with_stack_size(
		"execute thread",
		move || validate_using_artifact(&compiled_artifact_blob, &executor_params_2, &params),
		Arc::clone(&condvar),
		WaitOutcome::Finished,
		EXECUTE_THREAD_STACK_SIZE,
	)
	.unwrap_or_else(|err| {
		send_child_response(&mut pipe_write, Err(JobError::CouldNotSpawnThread(err.to_string())))
	});

	let outcome = thread::wait_for_threads(condvar);

	let response = match outcome {
		WaitOutcome::Finished => {
			let _ = cpu_time_monitor_tx.send(());
			execute_thread.join().map_err(|e| JobError::Panic(stringify_panic_payload(e)))
		},
		// If the CPU thread is not selected, we signal it to end, the join handle is
		// dropped and the thread will finish in the background.
		WaitOutcome::TimedOut => match cpu_time_monitor_thread.join() {
			Ok(Some(_cpu_time_elapsed)) => Err(JobError::TimedOut),
			Ok(None) => Err(JobError::CpuTimeMonitorThread(
				"error communicating over finished channel".into(),
			)),
			Err(e) => Err(JobError::CpuTimeMonitorThread(stringify_panic_payload(e))),
		},
		WaitOutcome::Pending =>
			unreachable!("we run wait_while until the outcome is no longer pending; qed"),
	};

	send_child_response(&mut pipe_write, response);
}

/// Waits for child process to finish and handle child response from pipe.
///
/// # Arguments
///
/// - `pipe_read`: A `PipeReader` used to read data from the child process.
///
/// - `child`: The child pid.
///
/// - `usage_before`: Resource usage statistics before executing the child process.
///
/// - `timeout`: The maximum allowed time for the child process to finish, in `Duration`.
///
/// # Returns
///
/// - The response, either `Ok` or some error state.
fn handle_parent_process(
	mut pipe_read: PipeReader,
	job_pid: Pid,
	worker_pid: u32,
	usage_before: Usage,
	timeout: Duration,
) -> io::Result<WorkerResponse> {
	// Read from the child. Don't decode unless the process exited normally, which we check later.
	let mut received_data = Vec::new();
	pipe_read
		.read_to_end(&mut received_data)
		// Could not decode job response. There is either a bug or the job was hijacked.
		// Should retry at any rate.
		.map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;

	let status = nix::sys::wait::waitpid(job_pid, None);
	gum::trace!(
		target: LOG_TARGET,
		%worker_pid,
		%job_pid,
		"execute worker received wait status from job: {:?}",
		status,
	);

	let usage_after = match nix::sys::resource::getrusage(UsageWho::RUSAGE_CHILDREN) {
		Ok(usage) => usage,
		Err(errno) => return Ok(internal_error_from_errno("getrusage after", errno)),
	};

	// Using `getrusage` is needed to check whether child has timedout since we cannot rely on
	// child to report its own time.
	// As `getrusage` returns resource usage from all terminated child processes,
	// it is necessary to subtract the usage before the current child process to isolate its cpu
	// time
	let cpu_tv = get_total_cpu_usage(usage_after) - get_total_cpu_usage(usage_before);
	if cpu_tv >= timeout {
		gum::warn!(
			target: LOG_TARGET,
			%worker_pid,
			%job_pid,
			"execute job took {}ms cpu time, exceeded execute timeout {}ms",
			cpu_tv.as_millis(),
			timeout.as_millis(),
		);
		return Ok(WorkerResponse::JobTimedOut)
	}

	match status {
		Ok(WaitStatus::Exited(_, exit_status)) => {
			let mut reader = io::BufReader::new(received_data.as_slice());
			let result = match recv_child_response(&mut reader) {
				Ok(result) => result,
				Err(err) => return Ok(WorkerResponse::JobError(err.to_string())),
			};

			match result {
				Ok(JobResponse::Ok { result_descriptor }) => {
					// The exit status should have been zero if no error occurred.
					if exit_status != 0 {
						return Ok(WorkerResponse::JobError(format!(
							"unexpected exit status: {}",
							exit_status
						)))
					}

					Ok(WorkerResponse::Ok { result_descriptor, duration: cpu_tv })
				},
				Ok(JobResponse::InvalidCandidate(err)) => Ok(WorkerResponse::InvalidCandidate(err)),
				Err(job_error) => {
					gum::warn!(
						target: LOG_TARGET,
						%worker_pid,
						%job_pid,
						"execute job error: {}",
						job_error,
					);
					if matches!(job_error, JobError::TimedOut) {
						Ok(WorkerResponse::JobTimedOut)
					} else {
						Ok(WorkerResponse::JobError(job_error.to_string()))
					}
				},
			}
		},
		// The job was killed by the given signal.
		//
		// The job gets SIGSYS on seccomp violations, but this signal may have been sent for some
		// other reason, so we still need to check for seccomp violations elsewhere.
		Ok(WaitStatus::Signaled(_pid, signal, _core_dump)) => Ok(WorkerResponse::JobDied {
			err: format!("received signal: {signal:?}"),
			job_pid: job_pid.as_raw(),
		}),
		Err(errno) => Ok(internal_error_from_errno("waitpid", errno)),

		// It is within an attacker's power to send an unexpected exit status. So we cannot treat
		// this as an internal error (which would make us abstain), but must vote against.
		Ok(unexpected_wait_status) => Ok(WorkerResponse::JobDied {
			err: format!("unexpected status from wait: {unexpected_wait_status:?}"),
			job_pid: job_pid.as_raw(),
		}),
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

/// Get a job response.
fn recv_child_response(received_data: &mut io::BufReader<&[u8]>) -> io::Result<JobResult> {
	let response_bytes = framed_recv_blocking(received_data)?;
	JobResult::decode(&mut response_bytes.as_slice()).map_err(|e| {
		io::Error::new(
			io::ErrorKind::Other,
			format!("execute pvf recv_child_response: decode error: {:?}", e),
		)
	})
}

/// Write response to the pipe and exit process after.
///
/// # Arguments
///
/// - `pipe_write`: A `PipeWriter` structure, the writing end of a pipe.
///
/// - `response`: Child process response, or error.
fn send_child_response(pipe_write: &mut PipeWriter, response: JobResult) -> ! {
	framed_send_blocking(pipe_write, response.encode().as_slice())
		.unwrap_or_else(|_| process::exit(libc::EXIT_FAILURE));

	if response.is_ok() {
		process::exit(libc::EXIT_SUCCESS)
	} else {
		process::exit(libc::EXIT_FAILURE)
	}
}

fn internal_error_from_errno(context: &'static str, errno: Errno) -> WorkerResponse {
	WorkerResponse::InternalError(InternalValidationError::Kernel(format!(
		"{}: {}: {}",
		context,
		errno,
		io::Error::last_os_error()
	)))
}
