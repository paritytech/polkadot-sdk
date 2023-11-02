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

use nix::{sys::{resource::{Resource, Usage, UsageWho}, wait::WaitStatus, signal::Signal}, unistd::ForkResult};
use os_pipe::PipeWriter;
pub use polkadot_node_core_pvf_common::{
	executor_intf::execute_artifact, worker_dir, SecurityStatus,
};

// NOTE: Initializing logging in e.g. tests will not have an effect in the workers, as they are
//       separate spawned processes. Run with e.g. `RUST_LOG=parachain::pvf-execute-worker=trace`.
const LOG_TARGET: &str = "parachain::pvf-execute-worker";

use parity_scale_codec::{Decode, Encode};
use polkadot_node_core_pvf_common::{
	error::InternalValidationError,
	execute::{Handshake, Response},
	framed_recv_blocking, framed_send_blocking,
	worker::{
		stringify_panic_payload,
		thread::{self, WaitOutcome},
		worker_event_loop, WorkerKind,
	},
};
use polkadot_parachain_primitives::primitives::ValidationResult;
use polkadot_primitives::{executor_params::DEFAULT_NATIVE_STACK_MAX, ExecutorParams};
use std::{
	io::{self, Write, Read},
	os::unix::net::UnixStream,
	path::PathBuf,
	sync::Arc,
	time::Duration, process,
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

fn recv_handshake(stream: &mut UnixStream) -> io::Result<Handshake> {
	let handshake_enc = framed_recv_blocking(stream)?;
	let handshake = Handshake::decode(&mut &handshake_enc[..]).map_err(|_| {
		io::Error::new(
			io::ErrorKind::Other,
			"execute pvf recv_handshake: failed to decode Handshake".to_owned(),
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

fn send_response(stream: &mut UnixStream, response: Response) -> io::Result<()> {
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
	security_status: SecurityStatus,
) {
	worker_event_loop(
		WorkerKind::Execute,
		socket_path,
		worker_dir_path,
		node_version,
		worker_version,
		&security_status,
		|mut stream, worker_dir_path| {
			let worker_pid = std::process::id();
			let artifact_path = worker_dir::execute_artifact(&worker_dir_path);

			let Handshake { executor_params } = recv_handshake(&mut stream)?;

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
						let response = Response::InternalError(
							InternalValidationError::CouldNotOpenFile(err.to_string()),
						);
						send_response(&mut stream, response)?;
						continue
					},
				};


				let (pipe_reader, pipe_writer) = os_pipe::pipe()?;

				let usage_before = nix::sys::resource::getrusage(UsageWho::RUSAGE_CHILDREN)?;

			// SAFETY: new process is spawned within a single threaded process
			let response = match unsafe { nix::unistd::fork() } {
				Err(_errno) => Response::Panic(String::from("error forking")),
				Ok(ForkResult::Child) => {
					// Dropping the stream closes the underlying socket. We want to make sure
					// that the sandboxed child can't get any kind of information from the
					// outside world. The only IPC it should be able to do is sending its
					// response over the pipe.
					drop(stream);

					handle_child_process(
						pipe_writer,
						compiled_artifact_blob,
						executor_params,
						params,
						execution_timeout,
					)
				},
				// parent
				Ok(ForkResult::Parent { child: _child }) => {
					// the read end will wait until all write ends have been closed,
					// this drop is necessary to avoid deadlock
					drop(pipe_writer);

					handle_parent_process(
						pipe_reader,
						usage_before,
						execution_timeout,
					)
				},
			};

				gum::trace!(
					target: LOG_TARGET,
					%worker_pid,
					"worker: sending response to host: {:?}",
					response
				);
				send_response(&mut stream, response)?;
			}
		},
	);
}

fn validate_using_artifact(
	compiled_artifact_blob: &[u8],
	executor_params: &ExecutorParams,
	params: &[u8],
) -> Response {
	let descriptor_bytes = match unsafe {
		// SAFETY: this should be safe since the compiled artifact passed here comes from the
		//         file created by the prepare workers. These files are obtained by calling
		//         [`executor_intf::prepare`].
		execute_artifact(compiled_artifact_blob, executor_params, params)
	} {
		Err(err) => return Response::format_invalid("execute", &err),
		Ok(d) => d,
	};

	let result_descriptor = match ValidationResult::decode(&mut &descriptor_bytes[..]) {
		Err(err) =>
			return Response::format_invalid("validation result decoding failed", &err.to_string()),
		Ok(r) => r,
	};

	// duration is set to 0 here because the process duration is calculated on the parent process
	Response::Ok { result_descriptor, duration: Duration::from_secs(0) }
}



/// This is used to handle child process during pvf execute worker.
/// It execute the artifact and pipes back the response to the parent process
///
/// # Arguments
///
/// - `pipe_write`: A `os_pipe::PipeWriter` structure, the writing end of a pipe.
///
/// - `compiled_artifact_blob`: The artifact bytes from compiled by the prepare worker`.
///
/// - `executor_params`: Deterministically serialized execution environment semantics.
///
/// - `params`:
///
/// - `execution_timeout`: The timeout in `Duration`.
///
/// # Returns
///
/// - pipe back `Response` to the parent process.
fn handle_child_process(
	pipe_write: os_pipe::PipeWriter,
	compiled_artifact_blob: Vec<u8>,
	executor_params: ExecutorParams,
	params: Vec<u8>,
	execution_timeout: Duration
) -> ! {
	gum::debug!(
		target: LOG_TARGET,
		worker_job_pid = %std::process::id(),
		"worker job: executing artifact",
	);

	// Set a hard CPU time limit for the child process.
	nix::sys::resource::setrlimit(
		Resource::RLIMIT_CPU,
		execution_timeout.as_secs(),
		execution_timeout.as_secs(),
	)
	.unwrap_or_else(|err| {
		send_child_response(&pipe_write, Response::Panic(err.to_string()));
	});

		// Conditional variable to notify us when a thread is done.
		let condvar = thread::get_condvar();

		let executor_params_2 = executor_params.clone();
		let execute_thread = thread::spawn_worker_thread_with_stack_size(
			"execute thread",
			move || {
				validate_using_artifact(
					&compiled_artifact_blob,
					&executor_params_2,
					&params,
				)
			},
			Arc::clone(&condvar),
			WaitOutcome::Finished,
			EXECUTE_THREAD_STACK_SIZE,
		)
		.unwrap_or_else(|err| {
			send_child_response(&pipe_write, Response::Panic(err.to_string()))
		});

	    // There's only one thread that can trigger the condvar, so ignore the condvar outcome and
	    // simply join. We don't have to be concerned with timeouts, setrlimit will kill the process.
		let response = execute_thread
					.join()
					.unwrap_or_else(|e| Response::Panic(stringify_panic_payload(e)));

		
	send_child_response(&pipe_write, response);
}

/// Waits for child process to finish and handle child response from pipe.
///
/// # Arguments
///
/// - `pipe_read`: A `PipeReader` used to read data from the child process.
///
/// - `usage_before`: Resource usage statistics before executing the child process.
///
/// - `timeout`: The maximum allowed time for the child process to finish, in `Duration`.
///
/// # Returns
///
/// - If no unexpected error occurr, this function return child response
///
/// - If an unexpected error occurr, this function returns `Response::Panic`
///
/// - If the child process timeout, it returns `Response::TimedOut`.
fn handle_parent_process(
	mut pipe_read: os_pipe::PipeReader,
	usage_before: Usage,
	timeout: Duration,
) -> Response {
	let mut received_data = Vec::new();

	// Read from the child.
	if let Err(err) = pipe_read
	.read_to_end(&mut received_data) {
		return Response::Panic(err.to_string())
	}
	  
		let status = nix::sys::wait::wait();

	let usage_after: Usage;
	
	match nix::sys::resource::getrusage(UsageWho::RUSAGE_CHILDREN) {
		Ok(usage) => {
			usage_after = usage
		},
		Err(err) => {
			return Response::Panic(err.to_string())
		}
	};

	// Using `getrusage` is needed to check whether `setrlimit` was triggered.
	// As `getrusage` returns resource usage from all terminated child processes,
	// it is necessary to subtract the usage before the current child process to isolate its cpu
	// time
	let cpu_tv = get_total_cpu_usage(usage_after) - get_total_cpu_usage(usage_before);

	if cpu_tv.as_secs() >= timeout.as_secs() {
		return Response::TimedOut
	}

	match status {
		Ok(WaitStatus::Exited(_, libc::EXIT_SUCCESS)) => {
			match Response::decode(&mut received_data.as_slice()) {
				Ok(Response::Ok { result_descriptor, duration: _ }) => Response::Ok { result_descriptor, duration: cpu_tv },
				Ok(response) => response,
				Err(err) => Response::Panic(err.to_string())
			}
		},
		Ok(WaitStatus::Exited(_, libc::EXIT_FAILURE)) => {
			Response::Panic("child exited with failure".to_string())
		},
		Ok(WaitStatus::Exited(_, exit_status)) => {
			Response::Panic(format!("child exited with unexpected status {}", exit_status))
		},
		Ok(WaitStatus::Signaled(_, sig, _)) => {
			Response::Panic(format!("child ended with unexpected signal {:?}, timeout {} cpu_tv {} after {} before {}", sig, timeout.as_secs(), cpu_tv.as_micros(),
			 usage_after.user_time().tv_sec() + usage_after.system_time().tv_sec(), get_total_cpu_usage(usage_before).as_micros()))
		}
		Ok(_) => {
			Response::Panic("child ended unexpectedly".to_string())
		}
		Err(err) => Response::Panic(err.to_string())
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
fn send_child_response(mut pipe_write: &PipeWriter, response: Response) -> ! {
	pipe_write
		.write_all(response.encode().as_slice())
		.unwrap_or_else(|_| process::exit(libc::EXIT_FAILURE));

	process::exit(libc::EXIT_SUCCESS)
}
