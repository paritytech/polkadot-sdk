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

//! PVF artifacts (final compiled code blobs).
//!
//! # Lifecycle of an artifact
//!
//! 1. During node start-up, we prune all the cached artifacts, if any.
//!
//! 2. In order to be executed, a PVF should be prepared first. This means that artifacts should
//!    have an [`ArtifactState::Prepared`] entry for that artifact in the table. If not, the
//!    preparation process kicks in. The execution request is stashed until after the preparation is
//!    done, and the artifact state in the host is set to [`ArtifactState::Preparing`]. Preparation
//!    goes through the preparation queue and the pool.
//!
//!    1. If the artifact is already being processed, we add another execution request to the
//!       existing preparation job, without starting a new one.
//!
//!    2. Note that if the state is [`ArtifactState::FailedToProcess`], we usually do not retry
//!       preparation, though we may under certain conditions.
//!
//! 3. The pool gets an available worker and instructs it to work on the given PVF. The worker
//!    starts compilation. When the worker finishes successfully, it writes the serialized artifact
//!    into a temporary file and notifies the host that it's done. The host atomically moves
//!    (renames) the temporary file to the destination filename of the artifact.
//!
//! 4. If the worker concluded successfully or returned an error, then the pool notifies the queue.
//!    In both cases, the queue reports to the host that the result is ready.
//!
//! 5. The host will react by changing the artifact state to either [`ArtifactState::Prepared`] or
//!    [`ArtifactState::FailedToProcess`] for the PVF in question. On success, the
//!    `last_time_needed` will be set to the current time. It will also dispatch the pending
//!    execution requests.
//!
//! 6. On success, the execution request will come through the execution queue and ultimately be
//!    processed by an execution worker. When this worker receives the request, it will read the
//!    requested artifact. If it doesn't exist it reports an internal error. A request for execution
//!    will bump the `last_time_needed` to the current time.
//!
//! 7. There is a separate process for pruning the prepared artifacts whose `last_time_needed` is
//!    older by a predefined parameter. This process is run very rarely (say, once a day). Once the
//!    artifact is expired it is removed from disk eagerly atomically.

use crate::{host::PrecheckResultSender, worker_interface::WORKER_DIR_PREFIX};
use always_assert::always;
use polkadot_node_core_pvf_common::{error::PrepareError, prepare::PrepareStats, pvf::PvfPrepData};
use polkadot_parachain_primitives::primitives::ValidationCodeHash;
use polkadot_primitives::ExecutorParamsHash;
use std::{
	collections::HashMap,
	fs,
	path::{Path, PathBuf},
	time::{Duration, SystemTime},
};

/// The extension to use for cached artifacts.
const ARTIFACT_EXTENSION: &str = "pvf";

/// The prefix that artifacts used to start with under the old naming scheme.
const ARTIFACT_OLD_PREFIX: &str = "wasmtime_";

pub fn generate_artifact_path(cache_path: &Path) -> PathBuf {
	let file_name = {
		use array_bytes::Hex;
		use rand::RngCore;
		let mut bytes = [0u8; 64];
		rand::thread_rng().fill_bytes(&mut bytes);
		bytes.hex("0x")
	};
	let mut artifact_path = cache_path.join(file_name);
	artifact_path.set_extension(ARTIFACT_EXTENSION);
	artifact_path
}

/// Identifier of an artifact. Encodes a code hash of the PVF and a hash of executor parameter set.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArtifactId {
	pub(crate) code_hash: ValidationCodeHash,
	pub(crate) executor_params_hash: ExecutorParamsHash,
}

impl ArtifactId {
	/// Creates a new artifact ID with the given hash.
	pub fn new(code_hash: ValidationCodeHash, executor_params_hash: ExecutorParamsHash) -> Self {
		Self { code_hash, executor_params_hash }
	}

	/// Returns an artifact ID that corresponds to the PVF with given executor params.
	pub fn from_pvf_prep_data(pvf: &PvfPrepData) -> Self {
		Self::new(pvf.code_hash(), pvf.executor_params().hash())
	}
}

/// A bundle of the artifact ID and the path.
///
/// Rationale for having this is two-fold:
///
/// - While we can derive the artifact path from the artifact id, it makes sense to carry it around
/// sometimes to avoid extra work.
/// - At the same time, carrying only path limiting the ability for logging.
#[derive(Debug, Clone)]
pub struct ArtifactPathId {
	pub(crate) id: ArtifactId,
	pub(crate) path: PathBuf,
}

impl ArtifactPathId {
	pub(crate) fn new(artifact_id: ArtifactId, path: &Path) -> Self {
		Self { id: artifact_id, path: path.to_owned() }
	}
}

#[derive(Debug)]
pub enum ArtifactState {
	/// The artifact is ready to be used by the executor.
	///
	/// That means that the artifact should be accessible through the path obtained by the artifact
	/// id (unless, it was removed externally).
	Prepared {
		/// The path of the compiled artifact.
		path: PathBuf,
		/// The time when the artifact was last needed.
		///
		/// This is updated when we get the heads up for this artifact or when we just discover
		/// this file.
		last_time_needed: SystemTime,
		/// Stats produced by successful preparation.
		prepare_stats: PrepareStats,
	},
	/// A task to prepare this artifact is scheduled.
	Preparing {
		/// List of result senders that are waiting for a response.
		waiting_for_response: Vec<PrecheckResultSender>,
		/// The number of times this artifact has failed to prepare.
		num_failures: u32,
	},
	/// The code couldn't be compiled due to an error. Such artifacts
	/// never reach the executor and stay in the host's memory.
	FailedToProcess {
		/// Keep track of the last time that processing this artifact failed.
		last_time_failed: SystemTime,
		/// The number of times this artifact has failed to prepare.
		num_failures: u32,
		/// The last error encountered for preparation.
		error: PrepareError,
	},
}

/// A container of all known artifact ids and their states.
pub struct Artifacts {
	inner: HashMap<ArtifactId, ArtifactState>,
}

impl Artifacts {
	#[cfg(test)]
	pub(crate) fn empty() -> Self {
		Self { inner: HashMap::new() }
	}

	#[cfg(test)]
	fn len(&self) -> usize {
		self.inner.len()
	}

	/// Create an empty table and the cache directory on-disk if it doesn't exist.
	pub async fn new(cache_path: &Path) -> Self {
		// Make sure that the cache path directory and all its parents are created.
		let _ = tokio::fs::create_dir_all(cache_path).await;

		// Delete any leftover artifacts and worker dirs from previous runs. We don't delete the
		// entire cache directory in case the user made a mistake and set it to e.g. their home
		// directory. This is a best-effort to do clean-up, so ignore any errors.
		for entry in fs::read_dir(cache_path).into_iter().flatten().flatten() {
			let path = entry.path();
			let Some(file_name) = path.file_name().and_then(|f| f.to_str()) else { continue };
			if path.is_dir() && file_name.starts_with(WORKER_DIR_PREFIX) {
				let _ = fs::remove_dir_all(path);
			} else if path.extension().map_or(false, |ext| ext == ARTIFACT_EXTENSION) ||
				file_name.starts_with(ARTIFACT_OLD_PREFIX)
			{
				let _ = fs::remove_file(path);
			}
		}

		Self { inner: HashMap::new() }
	}

	/// Returns the state of the given artifact by its ID.
	pub fn artifact_state_mut(&mut self, artifact_id: &ArtifactId) -> Option<&mut ArtifactState> {
		self.inner.get_mut(artifact_id)
	}

	/// Inform the table about the artifact with the given ID. The state will be set to "preparing".
	///
	/// This function must be used only for brand-new artifacts and should never be used for
	/// replacing existing ones.
	pub fn insert_preparing(
		&mut self,
		artifact_id: ArtifactId,
		waiting_for_response: Vec<PrecheckResultSender>,
	) {
		// See the precondition.
		always!(self
			.inner
			.insert(artifact_id, ArtifactState::Preparing { waiting_for_response, num_failures: 0 })
			.is_none());
	}

	/// Insert an artifact with the given ID as "prepared".
	///
	/// This function should only be used to build the artifact table at startup with valid
	/// artifact caches.
	#[cfg(test)]
	pub(crate) fn insert_prepared(
		&mut self,
		artifact_id: ArtifactId,
		path: PathBuf,
		last_time_needed: SystemTime,
		prepare_stats: PrepareStats,
	) {
		// See the precondition.
		always!(self
			.inner
			.insert(artifact_id, ArtifactState::Prepared { path, last_time_needed, prepare_stats })
			.is_none());
	}

	/// Remove artifacts older than the given TTL and return id and path of the removed ones.
	pub fn prune(&mut self, artifact_ttl: Duration) -> Vec<(ArtifactId, PathBuf)> {
		let now = SystemTime::now();

		let mut to_remove = vec![];
		for (k, v) in self.inner.iter() {
			if let ArtifactState::Prepared { last_time_needed, ref path, .. } = *v {
				if now
					.duration_since(last_time_needed)
					.map(|age| age > artifact_ttl)
					.unwrap_or(false)
				{
					to_remove.push((k.clone(), path.clone()));
				}
			}
		}

		for artifact in &to_remove {
			self.inner.remove(&artifact.0);
		}

		to_remove
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test]
	async fn cache_cleared_on_startup() {
		let tempdir = tempfile::tempdir().unwrap();
		let cache_path = tempdir.path();

		// These should be cleared.
		fs::write(cache_path.join("abcd.pvf"), "test").unwrap();
		fs::write(cache_path.join("wasmtime_..."), "test").unwrap();
		fs::create_dir(cache_path.join("worker-dir-prepare-test")).unwrap();

		// These should not be touched.
		fs::write(cache_path.join("abcd.pvfartifact"), "test").unwrap();
		fs::write(cache_path.join("polkadot_..."), "test").unwrap();
		fs::create_dir(cache_path.join("worker-prepare-test")).unwrap();

		let artifacts = Artifacts::new(cache_path).await;

		let entries: Vec<String> = fs::read_dir(&cache_path)
			.unwrap()
			.map(|entry| entry.unwrap().file_name().into_string().unwrap())
			.collect();
		assert_eq!(entries.len(), 3);
		assert!(entries.contains(&String::from("abcd.pvfartifact")));
		assert!(entries.contains(&String::from("polkadot_...")));
		assert!(entries.contains(&String::from("worker-prepare-test")));
		assert_eq!(artifacts.len(), 0);
	}
}
