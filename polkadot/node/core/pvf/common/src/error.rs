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

use crate::prepare::{PrepareSuccess, PrepareWorkerSuccess};
use parity_scale_codec::{Decode, Encode};
pub use sc_executor_common::error::Error as ExecuteError;

/// Result of PVF preparation from a worker, with checksum of the compiled PVF and stats of the
/// preparation if successful.
pub type PrepareWorkerResult = Result<PrepareWorkerSuccess, PrepareError>;

/// Result of PVF preparation propagated all the way back to the host, with path to the concluded
/// artifact and stats of the preparation if successful.
pub type PrepareResult = Result<PrepareSuccess, PrepareError>;

/// Result of prechecking PVF performed by the validation host. Contains stats about the preparation
/// if successful.
pub type PrecheckResult = Result<(), PrepareError>;

/// An error that occurred during the prepare part of the PVF pipeline.
// Codec indexes are intended to stabilize pre-encoded payloads (see `OOM_PAYLOAD`)
#[derive(thiserror::Error, Debug, Clone, Encode, Decode)]
pub enum PrepareError {
	/// During the prevalidation stage of preparation an issue was found with the PVF.
	#[codec(index = 0)]
	#[error("prepare: prevalidation error: {0}")]
	Prevalidation(String),
	/// Compilation failed for the given PVF.
	#[codec(index = 1)]
	#[error("prepare: preparation error: {0}")]
	Preparation(String),
	/// Instantiation of the WASM module instance failed.
	#[codec(index = 2)]
	#[error("prepare: runtime construction: {0}")]
	RuntimeConstruction(String),
	/// An unexpected error has occurred in the preparation job.
	#[codec(index = 3)]
	#[error("prepare: job error: {0}")]
	JobError(String),
	/// Failed to prepare the PVF due to the time limit.
	#[codec(index = 4)]
	#[error("prepare: timeout")]
	TimedOut,
	/// An IO error occurred. This state is reported by either the validation host or by the
	/// worker.
	#[codec(index = 5)]
	#[error("prepare: io error while receiving response: {0}")]
	IoErr(String),
	/// The temporary file for the artifact could not be created at the given cache path. This
	/// state is reported by the validation host (not by the worker).
	#[codec(index = 6)]
	#[error("prepare: error creating tmp file: {0}")]
	CreateTmpFile(String),
	/// The response from the worker is received, but the file cannot be renamed (moved) to the
	/// final destination location. This state is reported by the validation host (not by the
	/// worker).
	#[codec(index = 7)]
	#[error("prepare: error renaming tmp file ({src:?} -> {dest:?}): {err}")]
	RenameTmpFile {
		err: String,
		// Unfortunately `PathBuf` doesn't implement `Encode`/`Decode`, so we do a fallible
		// conversion to `Option<String>`.
		src: Option<String>,
		dest: Option<String>,
	},
	/// Memory limit reached
	#[codec(index = 8)]
	#[error("prepare: out of memory")]
	OutOfMemory,
	/// The response from the worker is received, but the worker cache could not be cleared. The
	/// worker has to be killed to avoid jobs having access to data from other jobs. This state is
	/// reported by the validation host (not by the worker).
	#[codec(index = 9)]
	#[error("prepare: error clearing worker cache: {0}")]
	ClearWorkerDir(String),
	/// The preparation job process died, due to OOM, a seccomp violation, or some other factor.
	#[codec(index = 10)]
	#[error("prepare: prepare job with pid {job_pid} died: {err}")]
	JobDied { err: String, job_pid: i32 },
	/// Some error occurred when interfacing with the kernel.
	#[codec(index = 11)]
	#[error("prepare: error interfacing with the kernel: {0}")]
	Kernel(String),
}

impl PrepareError {
	/// Returns whether this is a deterministic error, i.e. one that should trigger reliably. Those
	/// errors depend on the PVF itself and the sc-executor/wasmtime logic.
	///
	/// Non-deterministic errors can happen spuriously. Typically, they occur due to resource
	/// starvation, e.g. under heavy load or memory pressure. Those errors are typically transient
	/// but may persist e.g. if the node is run by overwhelmingly underpowered machine.
	pub fn is_deterministic(&self) -> bool {
		use PrepareError::*;
		match self {
			Prevalidation(_) | Preparation(_) | JobError(_) | OutOfMemory => true,
			IoErr(_) |
			JobDied { .. } |
			CreateTmpFile(_) |
			RenameTmpFile { .. } |
			ClearWorkerDir(_) |
			Kernel(_) => false,
			// Can occur due to issues with the PVF, but also due to factors like local load.
			TimedOut => false,
			// Can occur due to issues with the PVF, but also due to local errors.
			RuntimeConstruction(_) => false,
		}
	}
}

/// Some internal error occurred.
///
/// Should only ever be used for validation errors independent of the candidate and PVF, or for
/// errors we ruled out during pre-checking (so preparation errors are fine).
#[derive(thiserror::Error, Debug, Clone, Encode, Decode)]
pub enum InternalValidationError {
	/// Some communication error occurred with the host.
	#[error("validation: some communication error occurred with the host: {0}")]
	HostCommunication(String),
	/// Host could not create a hard link to the artifact path.
	#[error("validation: host could not create a hard link to the artifact path: {0}")]
	CouldNotCreateLink(String),
	/// Could not find or open compiled artifact file.
	#[error("validation: could not find or open compiled artifact file: {0}")]
	CouldNotOpenFile(String),
	/// Host could not clear the worker cache after a job.
	#[error("validation: host could not clear the worker cache ({path:?}) after a job: {err}")]
	CouldNotClearWorkerDir {
		err: String,
		// Unfortunately `PathBuf` doesn't implement `Encode`/`Decode`, so we do a fallible
		// conversion to `Option<String>`.
		path: Option<String>,
	},
	/// Some error occurred when interfacing with the kernel.
	#[error("validation: error interfacing with the kernel: {0}")]
	Kernel(String),
	/// Some non-deterministic preparation error occurred.
	#[error("validation: prepare: {0}")]
	NonDeterministicPrepareError(PrepareError),
}
