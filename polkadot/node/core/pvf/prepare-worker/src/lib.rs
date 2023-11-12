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

mod memory_stats;

use polkadot_node_core_pvf_common::executor_intf::{prepare, prevalidate};

// NOTE: Initializing logging in e.g. tests will not have an effect in the workers, as they are
//       separate spawned processes. Run with e.g. `RUST_LOG=parachain::pvf-prepare-worker=trace`.
const LOG_TARGET: &str = "parachain::pvf-prepare-worker";

#[cfg(target_os = "linux")]
use crate::memory_stats::max_rss_stat::{extract_max_rss_stat, get_max_rss_thread};
#[cfg(any(target_os = "linux", feature = "jemalloc-allocator"))]
use crate::memory_stats::memory_tracker::{get_memory_tracker_loop_stats, memory_tracker_loop};
use parity_scale_codec::{Decode, Encode};
use polkadot_node_core_pvf_common::{
	error::{PrepareError, PrepareResult, OOM_PAYLOAD},
	executor_intf::create_runtime_from_artifact_bytes,
	framed_recv_blocking, framed_send_blocking,
	prepare::{MemoryStats, PrepareJobKind, PrepareStats},
	pvf::PvfPrepData,
	worker::{
		cpu_time_monitor_loop, stringify_panic_payload,
		thread::{self, WaitOutcome},
		worker_event_loop, WorkerKind,
	},
	worker_dir, ProcessTime, SecurityStatus,
};
use polkadot_primitives::ExecutorParams;
use std::{
	fs, io,
	os::{
		fd::{AsRawFd, RawFd},
		unix::net::UnixStream,
	},
	path::PathBuf,
	sync::{mpsc::channel, Arc},
	time::Duration,
};
use tracking_allocator::TrackingAllocator;

#[cfg(any(target_os = "linux", feature = "jemalloc-allocator"))]
#[global_allocator]
static ALLOC: TrackingAllocator<tikv_jemallocator::Jemalloc> =
	TrackingAllocator(tikv_jemallocator::Jemalloc);

#[cfg(not(any(target_os = "linux", feature = "jemalloc-allocator")))]
#[global_allocator]
static ALLOC: TrackingAllocator<std::alloc::System> = TrackingAllocator(std::alloc::System);

/// Contains the bytes for a successfully compiled artifact.
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

fn start_memory_tracking(fd: RawFd, limit: Option<isize>) {
	unsafe {
		// SAFETY: Inside the failure handler, the allocator is locked and no allocations or
		// deallocations are possible. For Linux, that always holds for the code below, so it's
		// safe. For MacOS, that technically holds at the time of writing, but there are no future
		// guarantees.
		// The arguments of unsafe `libc` calls are valid, the payload validity is covered with
		// a test.
		ALLOC.start_tracking(
			limit,
			Some(Box::new(move || {
				#[cfg(target_os = "linux")]
				{
					// Syscalls never allocate or deallocate, so this is safe.
					libc::syscall(libc::SYS_write, fd, OOM_PAYLOAD.as_ptr(), OOM_PAYLOAD.len());
					libc::syscall(libc::SYS_close, fd);
					libc::syscall(libc::SYS_exit, 1);
				}
				#[cfg(not(target_os = "linux"))]
				{
					// Syscalls are not available on MacOS, so we have to use `libc` wrappers.
					// Technicaly, there may be allocations inside, although they shouldn't be
					// there. In that case, we'll see deadlocks on MacOS after the OOM condition
					// triggered. As we consider running a validator on MacOS unsafe, and this
					// code is only run by a validator, it's a lesser evil.
					libc::write(fd, OOM_PAYLOAD.as_ptr().cast(), OOM_PAYLOAD.len());
					libc::close(fd);
					std::process::exit(1);
				}
			})),
		);
	}
}

fn end_memory_tracking() -> isize {
	ALLOC.end_tracking()
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
/// 2. Start a memory tracker in a separate thread.
///
/// 3. Start the CPU time monitor loop and the actual preparation in two separate threads.
///
/// 4. Wait on the two threads created in step 3.
///
/// 5. Stop the memory tracker and get the stats.
///
/// 6. If compilation succeeded, write the compiled artifact into a temporary file.
///
/// 7. Send the result of preparation back to the host. If any error occurred in the above steps, we
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
			let worker_pid = std::process::id();
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

				// Conditional variable to notify us when a thread is done.
				let condvar = thread::get_condvar();

				// Run the memory tracker in a regular, non-worker thread.
				#[cfg(any(target_os = "linux", feature = "jemalloc-allocator"))]
				let condvar_memory = Arc::clone(&condvar);
				#[cfg(any(target_os = "linux", feature = "jemalloc-allocator"))]
				let memory_tracker_thread = std::thread::spawn(|| memory_tracker_loop(condvar_memory));

				let cpu_time_start = ProcessTime::now();

				// Spawn a new thread that runs the CPU time monitor.
				let (cpu_time_monitor_tx, cpu_time_monitor_rx) = channel::<()>();
				let cpu_time_monitor_thread = thread::spawn_worker_thread(
					"cpu time monitor thread",
					move || {
						cpu_time_monitor_loop(
							cpu_time_start,
							preparation_timeout,
							cpu_time_monitor_rx,
						)
					},
					Arc::clone(&condvar),
					WaitOutcome::TimedOut,
				)?;

				start_memory_tracking(
					stream.as_raw_fd(),
					executor_params.prechecking_max_memory().map(|v| {
						v.try_into().unwrap_or_else(|_| {
							gum::warn!(
								LOG_TARGET,
								%worker_pid,
								"Illegal pre-checking max memory value {} discarded",
								v,
							);
							0
						})
					}),
				);

				// Spawn another thread for preparation.
				let prepare_thread = thread::spawn_worker_thread(
					"prepare thread",
					move || {
						#[allow(unused_mut)]
						let mut result = prepare_artifact(pvf, cpu_time_start);

						// Get the `ru_maxrss` stat. If supported, call getrusage for the thread.
						#[cfg(target_os = "linux")]
						let mut result = result
							.map(|(artifact, elapsed)| (artifact, elapsed, get_max_rss_thread()));

						// If we are pre-checking, check for runtime construction errors.
						//
						// As pre-checking is more strict than just preparation in terms of memory
						// and time, it is okay to do extra checks here. This takes negligible time
						// anyway.
						if let PrepareJobKind::Prechecking = prepare_job_kind {
							result = result.and_then(|output| {
								runtime_construction_check(
									output.0.as_ref(),
									executor_params.as_ref(),
								)?;
								Ok(output)
							});
						}

						result
					},
					Arc::clone(&condvar),
					WaitOutcome::Finished,
				)?;

				let outcome = thread::wait_for_threads(condvar);

				let peak_alloc = {
					let peak = end_memory_tracking();
					gum::debug!(
						target: LOG_TARGET,
						%worker_pid,
						"prepare job peak allocation is {} bytes",
						peak,
					);
					peak
				};

				let result = match outcome {
					WaitOutcome::Finished => {
						let _ = cpu_time_monitor_tx.send(());

						match prepare_thread.join().unwrap_or_else(|err| {
							Err(PrepareError::Panic(stringify_panic_payload(err)))
						}) {
							Err(err) => {
								// Serialized error will be written into the socket.
								Err(err)
							},
							Ok(ok) => {
								cfg_if::cfg_if! {
									if #[cfg(target_os = "linux")] {
										let (artifact, cpu_time_elapsed, max_rss) = ok;
									} else {
										let (artifact, cpu_time_elapsed) = ok;
									}
								}

								// Stop the memory stats worker and get its observed memory stats.
								#[cfg(any(target_os = "linux", feature = "jemalloc-allocator"))]
								let memory_tracker_stats = get_memory_tracker_loop_stats(memory_tracker_thread, worker_pid);
								let memory_stats = MemoryStats {
									#[cfg(any(
										target_os = "linux",
										feature = "jemalloc-allocator"
									))]
									memory_tracker_stats,
									#[cfg(target_os = "linux")]
									max_rss: extract_max_rss_stat(max_rss, worker_pid),
									// Negative peak allocation values are legit; they are narrow
									// corner cases and shouldn't affect overall statistics
									// significantly
									peak_tracked_alloc: if peak_alloc > 0 {
										peak_alloc as u64
									} else {
										0u64
									},
								};

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
								fs::write(&temp_artifact_dest, &artifact)?;

								Ok(PrepareStats { cpu_time_elapsed, memory_stats })
							},
						}
					},
					// If the CPU thread is not selected, we signal it to end, the join handle is
					// dropped and the thread will finish in the background.
					WaitOutcome::TimedOut => {
						match cpu_time_monitor_thread.join() {
							Ok(Some(cpu_time_elapsed)) => {
								// Log if we exceed the timeout and the other thread hasn't
								// finished.
								gum::warn!(
									target: LOG_TARGET,
									%worker_pid,
									"prepare job took {}ms cpu time, exceeded prepare timeout {}ms",
									cpu_time_elapsed.as_millis(),
									preparation_timeout.as_millis(),
								);
								Err(PrepareError::TimedOut)
							},
							Ok(None) => Err(PrepareError::IoErr(
								"error communicating over closed channel".into(),
							)),
							// Errors in this thread are independent of the PVF.
							Err(err) => Err(PrepareError::IoErr(stringify_panic_payload(err))),
						}
					},
					WaitOutcome::Pending => unreachable!(
						"we run wait_while until the outcome is no longer pending; qed"
					),
				};

				gum::trace!(
					target: LOG_TARGET,
					%worker_pid,
					"worker: sending response to host: {:?}",
					result
				);
				send_response(&mut stream, result)?;
			}
		},
	);
}

fn prepare_artifact(
	pvf: PvfPrepData,
	cpu_time_start: ProcessTime,
) -> Result<(CompiledArtifact, Duration), PrepareError> {
	let blob = match prevalidate(&pvf.code()) {
		Err(err) => return Err(PrepareError::Prevalidation(format!("{:?}", err))),
		Ok(b) => b,
	};

	match prepare(blob, &pvf.executor_params()) {
		Ok(compiled_artifact) => Ok(CompiledArtifact::new(compiled_artifact)),
		Err(err) => Err(PrepareError::Preparation(format!("{:?}", err))),
	}
	.map(|artifact| (artifact, cpu_time_start.elapsed()))
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
