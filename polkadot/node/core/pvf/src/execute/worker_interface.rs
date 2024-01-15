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

//! Host interface to the execute worker.

use crate::{
	artifacts::ArtifactPathId,
	worker_interface::{
		clear_worker_dir_path, framed_recv, framed_send, spawn_with_program_path, IdleWorker,
		SpawnErr, WorkerDir, WorkerHandle, JOB_TIMEOUT_WALL_CLOCK_FACTOR,
	},
	LOG_TARGET,
};
use futures::FutureExt;
use futures_timer::Delay;
use parity_scale_codec::{Decode, Encode};
use polkadot_node_core_pvf_common::{
	error::InternalValidationError,
	execute::{Handshake, WorkerResponse},
	worker_dir, SecurityStatus,
};
use polkadot_parachain_primitives::primitives::ValidationResult;
use polkadot_primitives::ExecutorParams;
use std::{path::Path, time::Duration};
use tokio::{io, net::UnixStream};

/// Spawns a new worker with the given program path that acts as the worker and the spawn timeout.
///
/// Sends a handshake message to the worker as soon as it is spawned.
pub async fn spawn(
	program_path: &Path,
	cache_path: &Path,
	executor_params: ExecutorParams,
	spawn_timeout: Duration,
	node_version: Option<&str>,
	security_status: SecurityStatus,
) -> Result<(IdleWorker, WorkerHandle), SpawnErr> {
	let mut extra_args = vec!["execute-worker"];
	if let Some(node_version) = node_version {
		extra_args.extend_from_slice(&["--node-impl-version", node_version]);
	}

	let (mut idle_worker, worker_handle) = spawn_with_program_path(
		"execute",
		program_path,
		cache_path,
		&extra_args,
		spawn_timeout,
		security_status,
	)
	.await?;
	send_execute_handshake(&mut idle_worker.stream, Handshake { executor_params })
		.await
		.map_err(|error| {
			let err = SpawnErr::Handshake { err: error.to_string() };
			gum::warn!(
				target: LOG_TARGET,
				worker_pid = %idle_worker.pid,
				%err
			);
			err
		})?;
	Ok((idle_worker, worker_handle))
}

/// Outcome of PVF execution.
///
/// If the idle worker token is not returned, it means the worker must be terminated.
pub enum Outcome {
	/// PVF execution completed successfully and the result is returned. The worker is ready for
	/// another job.
	Ok { result_descriptor: ValidationResult, duration: Duration, idle_worker: IdleWorker },
	/// The candidate validation failed. It may be for example because the wasm execution triggered
	/// a trap. Errors related to the preparation process are not expected to be encountered by the
	/// execution workers.
	InvalidCandidate { err: String, idle_worker: IdleWorker },
	/// The execution time exceeded the hard limit. The worker is terminated.
	HardTimeout,
	/// An I/O error happened during communication with the worker. This may mean that the worker
	/// process already died. The token is not returned in any case.
	WorkerIntfErr,
	/// The job process has died. We must kill the worker just in case.
	///
	/// We cannot treat this as an internal error because malicious code may have caused this.
	JobDied { err: String },
	/// An unexpected error occurred in the job process.
	///
	/// Because malicious code can cause a job error, we must not treat it as an internal error.
	JobError { err: String },

	/// An internal error happened during the validation. Such an error is most likely related to
	/// some transient glitch.
	///
	/// Should only ever be used for errors independent of the candidate and PVF. Therefore it may
	/// be a problem with the worker, so we terminate it.
	InternalError { err: InternalValidationError },
}

/// Given the idle token of a worker and parameters of work, communicates with the worker and
/// returns the outcome.
///
/// NOTE: Not returning the idle worker token in `Outcome` will trigger the child process being
/// killed, if it's still alive.
pub async fn start_work(
	worker: IdleWorker,
	artifact: ArtifactPathId,
	execution_timeout: Duration,
	validation_params: Vec<u8>,
) -> Outcome {
	let IdleWorker { mut stream, pid, worker_dir } = worker;

	gum::debug!(
		target: LOG_TARGET,
		worker_pid = %pid,
		?worker_dir,
		validation_code_hash = ?artifact.id.code_hash,
		"starting execute for {}",
		artifact.path.display(),
	);

	with_worker_dir_setup(worker_dir, pid, &artifact.path, |worker_dir| async move {
		if let Err(error) = send_request(&mut stream, &validation_params, execution_timeout).await {
			gum::warn!(
				target: LOG_TARGET,
				worker_pid = %pid,
				validation_code_hash = ?artifact.id.code_hash,
				?error,
				"failed to send an execute request",
			);
			return Outcome::WorkerIntfErr
		}

		// We use a generous timeout here. This is in addition to the one in the child process, in
		// case the child stalls. We have a wall clock timeout here in the host, but a CPU timeout
		// in the child. We want to use CPU time because it varies less than wall clock time under
		// load, but the CPU resources of the child can only be measured from the parent after the
		// child process terminates.
		let timeout = execution_timeout * JOB_TIMEOUT_WALL_CLOCK_FACTOR;
		let response = futures::select! {
			response = recv_response(&mut stream).fuse() => {
				match response {
					Ok(response) =>
						handle_response(
							response,
							pid,
							execution_timeout,
						)
							.await,
					Err(error) => {
						gum::warn!(
							target: LOG_TARGET,
							worker_pid = %pid,
							validation_code_hash = ?artifact.id.code_hash,
							?error,
							"failed to recv an execute response",
						);

						return Outcome::WorkerIntfErr
					},
				}
			},
			_ = Delay::new(timeout).fuse() => {
				gum::warn!(
					target: LOG_TARGET,
					worker_pid = %pid,
					validation_code_hash = ?artifact.id.code_hash,
					"execution worker exceeded lenient timeout for execution, child worker likely stalled",
				);
				WorkerResponse::JobTimedOut
			},
		};

		match response {
			WorkerResponse::Ok { result_descriptor, duration } => Outcome::Ok {
				result_descriptor,
				duration,
				idle_worker: IdleWorker { stream, pid, worker_dir },
			},
			WorkerResponse::InvalidCandidate(err) => Outcome::InvalidCandidate {
				err,
				idle_worker: IdleWorker { stream, pid, worker_dir },
			},
			WorkerResponse::JobTimedOut => Outcome::HardTimeout,
			WorkerResponse::JobDied { err, job_pid: _ } => Outcome::JobDied { err },
			WorkerResponse::JobError(err) => Outcome::JobError { err },

			WorkerResponse::InternalError(err) => Outcome::InternalError { err },
		}
	})
	.await
}

/// Handles the case where we successfully received response bytes on the host from the child.
///
/// Here we know the artifact exists, but is still located in a temporary file which will be cleared
/// by [`with_worker_dir_setup`].
async fn handle_response(
	response: WorkerResponse,
	worker_pid: u32,
	execution_timeout: Duration,
) -> WorkerResponse {
	if let WorkerResponse::Ok { duration, .. } = response {
		if duration > execution_timeout {
			// The job didn't complete within the timeout.
			gum::warn!(
				target: LOG_TARGET,
				worker_pid,
				"execute job took {}ms cpu time, exceeded execution timeout {}ms.",
				duration.as_millis(),
				execution_timeout.as_millis(),
			);

			// Return a timeout error.
			return WorkerResponse::JobTimedOut
		}
	}

	response
}

/// Create a temporary file for an artifact in the worker cache, execute the given future/closure
/// passing the file path in, and clean up the worker cache.
///
/// Failure to clean up the worker cache results in an error - leaving any files here could be a
/// security issue, and we should shut down the worker. This should be very rare.
async fn with_worker_dir_setup<F, Fut>(
	worker_dir: WorkerDir,
	pid: u32,
	artifact_path: &Path,
	f: F,
) -> Outcome
where
	Fut: futures::Future<Output = Outcome>,
	F: FnOnce(WorkerDir) -> Fut,
{
	// Cheaply create a hard link to the artifact. The artifact is always at a known location in the
	// worker cache, and the child can't access any other artifacts or gain any information from the
	// original filename.
	let link_path = worker_dir::execute_artifact(worker_dir.path());
	if let Err(err) = tokio::fs::hard_link(artifact_path, link_path).await {
		gum::warn!(
			target: LOG_TARGET,
			worker_pid = %pid,
			?worker_dir,
			"failed to clear worker cache after the job: {:?}",
			err,
		);
		return Outcome::InternalError {
			err: InternalValidationError::CouldNotCreateLink(format!("{:?}", err)),
		}
	}

	let worker_dir_path = worker_dir.path().to_owned();
	let outcome = f(worker_dir).await;

	// Try to clear the worker dir.
	if let Err(err) = clear_worker_dir_path(&worker_dir_path) {
		gum::warn!(
			target: LOG_TARGET,
			worker_pid = %pid,
			?worker_dir_path,
			"failed to clear worker cache after the job: {:?}",
			err,
		);
		return Outcome::InternalError {
			err: InternalValidationError::CouldNotClearWorkerDir {
				err: format!("{:?}", err),
				path: worker_dir_path.to_str().map(String::from),
			},
		}
	}

	outcome
}

/// Sends a handshake with information specific to the execute worker.
async fn send_execute_handshake(stream: &mut UnixStream, handshake: Handshake) -> io::Result<()> {
	framed_send(stream, &handshake.encode()).await
}

async fn send_request(
	stream: &mut UnixStream,
	validation_params: &[u8],
	execution_timeout: Duration,
) -> io::Result<()> {
	framed_send(stream, validation_params).await?;
	framed_send(stream, &execution_timeout.encode()).await
}

async fn recv_response(stream: &mut UnixStream) -> io::Result<WorkerResponse> {
	let response_bytes = framed_recv(stream).await?;
	WorkerResponse::decode(&mut response_bytes.as_slice()).map_err(|e| {
		io::Error::new(
			io::ErrorKind::Other,
			format!("execute pvf recv_response: decode error: {:?}", e),
		)
	})
}
