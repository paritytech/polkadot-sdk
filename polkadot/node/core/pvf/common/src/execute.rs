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

use crate::error::InternalValidationError;
use parity_scale_codec::{Decode, Encode};
use polkadot_parachain_primitives::primitives::ValidationResult;
use polkadot_primitives::ExecutorParams;
use std::time::Duration;

/// The payload of the one-time handshake that is done when a worker process is created. Carries
/// data from the host to the worker.
#[derive(Encode, Decode)]
pub struct Handshake {
	/// The executor parameters.
	pub executor_params: ExecutorParams,
}

/// The response from the execution worker.
#[derive(Debug, Encode, Decode)]
pub enum WorkerResponse {
	/// The job completed successfully.
	Ok {
		/// The result of parachain validation.
		result_descriptor: ValidationResult,
		/// The amount of CPU time taken by the job.
		duration: Duration,
	},
	/// The candidate is invalid.
	InvalidCandidate(String),
	/// The job timed out.
	JobTimedOut,
	/// The job process has died. We must kill the worker just in case.
	///
	/// We cannot treat this as an internal error because malicious code may have killed the job.
	/// We still retry it, because in the non-malicious case it is likely spurious.
	JobDied { err: String, job_pid: i32 },
	/// An unexpected error occurred in the job process, e.g. failing to spawn a thread, panic,
	/// etc.
	///
	/// Because malicious code can cause a job error, we must not treat it as an internal error. We
	/// still retry it, because in the non-malicious case it is likely spurious.
	JobError(String),

	/// Some internal error occurred.
	InternalError(InternalValidationError),
}

/// The result of a job on the execution worker.
pub type JobResult = Result<JobResponse, JobError>;

/// The successful response from a job on the execution worker.
#[derive(Debug, Encode, Decode)]
pub enum JobResponse {
	Ok {
		/// The result of parachain validation.
		result_descriptor: ValidationResult,
	},
	/// The candidate is invalid.
	InvalidCandidate(String),
}

impl JobResponse {
	/// Creates an invalid response from a context `ctx` and a message `msg` (which can be empty).
	pub fn format_invalid(ctx: &'static str, msg: &str) -> Self {
		if msg.is_empty() {
			Self::InvalidCandidate(ctx.to_string())
		} else {
			Self::InvalidCandidate(format!("{}: {}", ctx, msg))
		}
	}
}

/// An unexpected error occurred in the execution job process. Because this comes from the job,
/// which executes untrusted code, this error must likewise be treated as untrusted. That is, we
/// cannot raise an internal error based on this.
#[derive(thiserror::Error, Debug, Encode, Decode)]
pub enum JobError {
	#[error("The job timed out")]
	TimedOut,
	#[error("An unexpected panic has occurred in the execution job: {0}")]
	Panic(String),
	/// Some error occurred when interfacing with the kernel.
	#[error("Error interfacing with the kernel: {0}")]
	Kernel(String),
	#[error("Could not spawn the requested thread: {0}")]
	CouldNotSpawnThread(String),
	#[error("An error occurred in the CPU time monitor thread: {0}")]
	CpuTimeMonitorThread(String),
}
