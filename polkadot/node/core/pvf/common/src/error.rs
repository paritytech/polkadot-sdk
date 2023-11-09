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

use crate::prepare::PrepareStats;
use parity_scale_codec::{Decode, Encode};
use std::fmt;

/// Result of PVF preparation performed by the validation host. Contains stats about the preparation
/// if successful
pub type PrepareResult = Result<PrepareStats, PrepareError>;

/// An error that occurred during the prepare part of the PVF pipeline.
// Codec indexes are intended to stabilize pre-encoded payloads (see `OOM_PAYLOAD` below)
#[derive(Debug, Clone, Encode, Decode)]
pub enum PrepareError {
	/// During the prevalidation stage of preparation an issue was found with the PVF.
	#[codec(index = 0)]
	Prevalidation(String),
	/// Compilation failed for the given PVF.
	#[codec(index = 1)]
	Preparation(String),
	/// Instantiation of the WASM module instance failed.
	#[codec(index = 2)]
	RuntimeConstruction(String),
	/// An unexpected panic has occurred in the preparation worker.
	#[codec(index = 3)]
	Panic(String),
	/// Failed to prepare the PVF due to the time limit.
	#[codec(index = 4)]
	TimedOut,
	/// An IO error occurred. This state is reported by either the validation host or by the
	/// worker.
	#[codec(index = 5)]
	IoErr(String),
	/// The temporary file for the artifact could not be created at the given cache path. This
	/// state is reported by the validation host (not by the worker).
	#[codec(index = 6)]
	CreateTmpFileErr(String),
	/// The response from the worker is received, but the file cannot be renamed (moved) to the
	/// final destination location. This state is reported by the validation host (not by the
	/// worker).
	#[codec(index = 7)]
	RenameTmpFileErr {
		err: String,
		// Unfortunately `PathBuf` doesn't implement `Encode`/`Decode`, so we do a fallible
		// conversion to `Option<String>`.
		src: Option<String>,
		dest: Option<String>,
	},
	/// Memory limit reached
	#[codec(index = 8)]
	OutOfMemory,
	/// The response from the worker is received, but the worker cache could not be cleared. The
	/// worker has to be killed to avoid jobs having access to data from other jobs. This state is
	/// reported by the validation host (not by the worker).
	#[codec(index = 9)]
	ClearWorkerDir(String),
}

/// Pre-encoded length-prefixed `PrepareResult::Err(PrepareError::OutOfMemory)`
pub const OOM_PAYLOAD: &[u8] = b"\x02\x00\x00\x00\x00\x00\x00\x00\x01\x08";

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
			Prevalidation(_) | Preparation(_) | Panic(_) | OutOfMemory => true,
			TimedOut |
			IoErr(_) |
			CreateTmpFileErr(_) |
			RenameTmpFileErr { .. } |
			ClearWorkerDir(_) => false,
			// Can occur due to issues with the PVF, but also due to local errors.
			RuntimeConstruction(_) => false,
		}
	}
}

impl fmt::Display for PrepareError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		use PrepareError::*;
		match self {
			Prevalidation(err) => write!(f, "prevalidation: {}", err),
			Preparation(err) => write!(f, "preparation: {}", err),
			RuntimeConstruction(err) => write!(f, "runtime construction: {}", err),
			Panic(err) => write!(f, "panic: {}", err),
			TimedOut => write!(f, "prepare: timeout"),
			IoErr(err) => write!(f, "prepare: io error while receiving response: {}", err),
			CreateTmpFileErr(err) => write!(f, "prepare: error creating tmp file: {}", err),
			RenameTmpFileErr { err, src, dest } =>
				write!(f, "prepare: error renaming tmp file ({:?} -> {:?}): {}", src, dest, err),
			OutOfMemory => write!(f, "prepare: out of memory"),
			ClearWorkerDir(err) => write!(f, "prepare: error clearing worker cache: {}", err),
		}
	}
}

/// Some internal error occurred.
///
/// Should only ever be used for validation errors independent of the candidate and PVF, or for
/// errors we ruled out during pre-checking (so preparation errors are fine).
#[derive(Debug, Clone, Encode, Decode)]
pub enum InternalValidationError {
	/// Some communication error occurred with the host.
	HostCommunication(String),
	/// Host could not create a hard link to the artifact path.
	CouldNotCreateLink(String),
	/// Could not find or open compiled artifact file.
	CouldNotOpenFile(String),
	/// Host could not clear the worker cache after a job.
	CouldNotClearWorkerDir {
		err: String,
		// Unfortunately `PathBuf` doesn't implement `Encode`/`Decode`, so we do a fallible
		// conversion to `Option<String>`.
		path: Option<String>,
	},
	/// An error occurred in the CPU time monitor thread. Should be totally unrelated to
	/// validation.
	CpuTimeMonitorThread(String),
	/// Some non-deterministic preparation error occurred.
	NonDeterministicPrepareError(PrepareError),
}

impl fmt::Display for InternalValidationError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		use InternalValidationError::*;
		match self {
			HostCommunication(err) =>
				write!(f, "validation: some communication error occurred with the host: {}", err),
			CouldNotCreateLink(err) => write!(
				f,
				"validation: host could not create a hard link to the artifact path: {}",
				err
			),
			CouldNotOpenFile(err) =>
				write!(f, "validation: could not find or open compiled artifact file: {}", err),
			CouldNotClearWorkerDir { err, path } => write!(
				f,
				"validation: host could not clear the worker cache ({:?}) after a job: {}",
				path, err
			),
			CpuTimeMonitorThread(err) =>
				write!(f, "validation: an error occurred in the CPU time monitor thread: {}", err),
			NonDeterministicPrepareError(err) => write!(f, "validation: prepare: {}", err),
		}
	}
}

#[test]
fn pre_encoded_payloads() {
	let oom_enc = PrepareResult::Err(PrepareError::OutOfMemory).encode();
	let mut oom_payload = oom_enc.len().to_le_bytes().to_vec();
	oom_payload.extend(oom_enc);
	assert_eq!(oom_payload, OOM_PAYLOAD);
}
