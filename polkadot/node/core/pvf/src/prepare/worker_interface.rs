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

//! Host interface to the prepare worker.

use crate::{
	artifacts::generate_artifact_path,
	metrics::Metrics,
	worker_interface::{
		clear_worker_dir_path, framed_recv, framed_send, spawn_with_program_path, IdleWorker,
		SpawnErr, WorkerDir, WorkerHandle, JOB_TIMEOUT_WALL_CLOCK_FACTOR,
	},
	LOG_TARGET,
};
use parity_scale_codec::{Decode, Encode};
use polkadot_node_core_pvf_common::{
	error::{PrepareError, PrepareResult, PrepareWorkerResult},
	prepare::{PrepareStats, PrepareSuccess, PrepareWorkerSuccess},
	pvf::PvfPrepData,
	worker_dir, SecurityStatus,
};

use sp_core::hexdisplay::HexDisplay;
use std::{
	path::{Path, PathBuf},
	time::Duration,
};
use tokio::{io, net::UnixStream};

/// Spawns a new worker with the given program path that acts as the worker and the spawn timeout.
///
/// Sends a handshake message to the worker as soon as it is spawned.
pub async fn spawn(
	program_path: &Path,
	cache_path: &Path,
	spawn_timeout: Duration,
	node_version: Option<&str>,
	security_status: SecurityStatus,
) -> Result<(IdleWorker, WorkerHandle), SpawnErr> {
	let mut extra_args = vec!["prepare-worker"];
	if let Some(node_version) = node_version {
		extra_args.extend_from_slice(&["--node-impl-version", node_version]);
	}

	spawn_with_program_path(
		"prepare",
		program_path,
		cache_path,
		&extra_args,
		spawn_timeout,
		security_status,
	)
	.await
}

/// Outcome of PVF preparation.
///
/// If the idle worker token is not returned, it means the worker must be terminated.
pub enum Outcome {
	/// The worker has finished the work assigned to it.
	Concluded { worker: IdleWorker, result: PrepareResult },
	/// The host tried to reach the worker but failed. This is most likely because the worked was
	/// killed by the system.
	Unreachable,
	/// The temporary file for the artifact could not be created at the given cache path.
	CreateTmpFileErr { worker: IdleWorker, err: String },
	/// The response from the worker is received, but the tmp file cannot be renamed (moved) to the
	/// final destination location.
	RenameTmpFile {
		worker: IdleWorker,
		result: PrepareWorkerResult,
		err: String,
		// Unfortunately `PathBuf` doesn't implement `Encode`/`Decode`, so we do a fallible
		// conversion to `Option<String>`.
		src: Option<String>,
		dest: Option<String>,
	},
	/// The worker cache could not be cleared for the given reason.
	ClearWorkerDir { err: String },
	/// The worker failed to finish the job until the given deadline.
	///
	/// The worker is no longer usable and should be killed.
	TimedOut,
	/// An IO error occurred while receiving the result from the worker process.
	///
	/// This doesn't return an idle worker instance, thus this worker is no longer usable.
	IoErr(String),
	/// The worker ran out of memory and is aborting. The worker should be ripped.
	OutOfMemory,
	/// The preparation job process died, due to OOM, a seccomp violation, or some other factor.
	///
	/// The worker might still be usable, but we kill it just in case.
	JobDied { err: String, job_pid: i32 },
}

/// Given the idle token of a worker and parameters of work, communicates with the worker and
/// returns the outcome.
///
/// NOTE: Returning the `TimedOut`, `IoErr` or `Unreachable` outcomes will trigger the child process
/// being killed.
pub async fn start_work(
	metrics: &Metrics,
	worker: IdleWorker,
	pvf: PvfPrepData,
	cache_path: PathBuf,
) -> Outcome {
	let IdleWorker { stream, pid, worker_dir } = worker;

	gum::debug!(
		target: LOG_TARGET,
		worker_pid = %pid,
		?worker_dir,
		"starting prepare for {:?}",
		pvf,
	);

	with_worker_dir_setup(
		worker_dir,
		stream,
		pid,
		|tmp_artifact_file, mut stream, worker_dir| async move {
			let preparation_timeout = pvf.prep_timeout();

			if let Err(err) = send_request(&mut stream, &pvf).await {
				gum::warn!(
					target: LOG_TARGET,
					worker_pid = %pid,
					"failed to send a prepare request: {:?}",
					err,
				);
				return Outcome::Unreachable
			}

			// Wait for the result from the worker, keeping in mind that there may be a timeout, the
			// worker may get killed, or something along these lines. In that case we should
			// propagate the error to the pool.
			//
			// We use a generous timeout here. This is in addition to the one in the child process,
			// in case the child stalls. We have a wall clock timeout here in the host, but a CPU
			// timeout in the child. We want to use CPU time because it varies less than wall clock
			// time under load, but the CPU resources of the child can only be measured from the
			// parent after the child process terminates.
			let timeout = preparation_timeout * JOB_TIMEOUT_WALL_CLOCK_FACTOR;
			let result = tokio::time::timeout(timeout, recv_response(&mut stream, pid)).await;

			match result {
				// Received bytes from worker within the time limit.
				Ok(Ok(prepare_worker_result)) =>
					handle_response(
						metrics,
						IdleWorker { stream, pid, worker_dir },
						prepare_worker_result,
						pid,
						tmp_artifact_file,
						&cache_path,
						preparation_timeout,
					)
					.await,
				Ok(Err(err)) => {
					// Communication error within the time limit.
					gum::warn!(
						target: LOG_TARGET,
						worker_pid = %pid,
						"failed to recv a prepare response: {}",
						err,
					);
					Outcome::IoErr(err.to_string())
				},
				Err(_) => {
					// Timed out here on the host.
					gum::warn!(
						target: LOG_TARGET,
						worker_pid = %pid,
						"did not recv a prepare response within the time limit",
					);
					Outcome::TimedOut
				},
			}
		},
	)
	.await
}

/// Handles the case where we successfully received response bytes on the host from the child.
///
/// Here we know the artifact exists, but is still located in a temporary file which will be cleared
/// by [`with_worker_dir_setup`].
async fn handle_response(
	metrics: &Metrics,
	worker: IdleWorker,
	result: PrepareWorkerResult,
	worker_pid: u32,
	tmp_file: PathBuf,
	cache_path: &Path,
	preparation_timeout: Duration,
) -> Outcome {
	// TODO: Add `checksum` to `ArtifactPathId`. See:
	//       https://github.com/paritytech/polkadot-sdk/issues/2399
	let PrepareWorkerSuccess {
		checksum: _,
		stats: PrepareStats { cpu_time_elapsed, memory_stats },
	} = match result.clone() {
		Ok(result) => result,
		// Timed out on the child. This should already be logged by the child.
		Err(PrepareError::TimedOut) => return Outcome::TimedOut,
		Err(PrepareError::JobDied { err, job_pid }) => return Outcome::JobDied { err, job_pid },
		Err(PrepareError::OutOfMemory) => return Outcome::OutOfMemory,
		Err(err) => return Outcome::Concluded { worker, result: Err(err) },
	};

	if cpu_time_elapsed > preparation_timeout {
		// The job didn't complete within the timeout.
		gum::warn!(
			target: LOG_TARGET,
			%worker_pid,
			"prepare job took {}ms cpu time, exceeded preparation timeout {}ms. Clearing WIP artifact {}",
			cpu_time_elapsed.as_millis(),
			preparation_timeout.as_millis(),
			tmp_file.display(),
		);
		return Outcome::TimedOut
	}

	// The file name should uniquely identify the artifact even across restarts. In case the cache
	// for some reason is not cleared correctly, we cannot
	// accidentally execute an artifact compiled under a different wasmtime version, host
	// environment, etc.
	let artifact_path = generate_artifact_path(cache_path);

	gum::debug!(
		target: LOG_TARGET,
		%worker_pid,
		"promoting WIP artifact {} to {}",
		tmp_file.display(),
		artifact_path.display(),
	);

	let outcome = match tokio::fs::rename(&tmp_file, &artifact_path).await {
		Ok(()) => Outcome::Concluded {
			worker,
			result: Ok(PrepareSuccess {
				path: artifact_path,
				stats: PrepareStats { cpu_time_elapsed, memory_stats: memory_stats.clone() },
			}),
		},
		Err(err) => {
			gum::warn!(
				target: LOG_TARGET,
				%worker_pid,
				"failed to rename the artifact from {} to {}: {:?}",
				tmp_file.display(),
				artifact_path.display(),
				err,
			);
			Outcome::RenameTmpFile {
				worker,
				result,
				err: format!("{:?}", err),
				src: tmp_file.to_str().map(String::from),
				dest: artifact_path.to_str().map(String::from),
			}
		},
	};

	// If there were no errors up until now, log the memory stats for a successful preparation, if
	// available.
	metrics.observe_preparation_memory_metrics(memory_stats);

	outcome
}

/// Create a temporary file for an artifact in the worker cache, execute the given future/closure
/// passing the file path in, and clean up the worker cache.
///
/// Failure to clean up the worker cache results in an error - leaving any files here could be a
/// security issue, and we should shut down the worker. This should be very rare.
async fn with_worker_dir_setup<F, Fut>(
	worker_dir: WorkerDir,
	stream: UnixStream,
	pid: u32,
	f: F,
) -> Outcome
where
	Fut: futures::Future<Output = Outcome>,
	F: FnOnce(PathBuf, UnixStream, WorkerDir) -> Fut,
{
	// Create the tmp file here so that the child doesn't need any file creation rights. This will
	// be cleared at the end of this function.
	let tmp_file = worker_dir::prepare_tmp_artifact(worker_dir.path());
	if let Err(err) = tokio::fs::File::create(&tmp_file).await {
		gum::warn!(
			target: LOG_TARGET,
			worker_pid = %pid,
			?worker_dir,
			"failed to create a temp file for the artifact: {:?}",
			err,
		);
		return Outcome::CreateTmpFileErr {
			worker: IdleWorker { stream, pid, worker_dir },
			err: format!("{:?}", err),
		}
	};

	let worker_dir_path = worker_dir.path().to_owned();
	let outcome = f(tmp_file, stream, worker_dir).await;

	// Try to clear the worker dir.
	if let Err(err) = clear_worker_dir_path(&worker_dir_path) {
		gum::warn!(
			target: LOG_TARGET,
			worker_pid = %pid,
			?worker_dir_path,
			"failed to clear worker cache after the job: {:?}",
			err,
		);
		return Outcome::ClearWorkerDir { err: format!("{:?}", err) }
	}

	outcome
}

async fn send_request(stream: &mut UnixStream, pvf: &PvfPrepData) -> io::Result<()> {
	framed_send(stream, &pvf.encode()).await?;
	Ok(())
}

async fn recv_response(stream: &mut UnixStream, pid: u32) -> io::Result<PrepareWorkerResult> {
	let result = framed_recv(stream).await?;
	let result = PrepareWorkerResult::decode(&mut &result[..]).map_err(|e| {
		// We received invalid bytes from the worker.
		let bound_bytes = &result[..result.len().min(4)];
		gum::warn!(
			target: LOG_TARGET,
			worker_pid = %pid,
			"received unexpected response from the prepare worker: {}",
			HexDisplay::from(&bound_bytes),
		);
		io::Error::new(
			io::ErrorKind::Other,
			format!("prepare pvf recv_response: failed to decode result: {:?}", e),
		)
	})?;
	Ok(result)
}
