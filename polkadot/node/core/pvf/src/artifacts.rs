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
//! 1. During node start-up, we will check the cached artifacts, if any. The stale and corrupted
//!    ones are pruned. The valid ones are registered in the [`Artifacts`] table.
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

use crate::{host::PrecheckResultSender, LOG_TARGET};
use always_assert::always;
use polkadot_core_primitives::Hash;
use polkadot_node_core_pvf_common::{
	error::PrepareError, prepare::PrepareStats, pvf::PvfPrepData, RUNTIME_VERSION,
};
use polkadot_node_primitives::NODE_VERSION;
use polkadot_parachain_primitives::primitives::ValidationCodeHash;
use polkadot_primitives::ExecutorParamsHash;
use std::{
	collections::HashMap,
	io,
	path::{Path, PathBuf},
	str::FromStr as _,
	time::{Duration, SystemTime},
};

const RUNTIME_PREFIX: &str = "wasmtime_v";
const NODE_PREFIX: &str = "polkadot_v";

fn artifact_prefix() -> String {
	format!("{}{}_{}{}", RUNTIME_PREFIX, RUNTIME_VERSION, NODE_PREFIX, NODE_VERSION)
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

	/// Returns the canonical path to the concluded artifact.
	pub(crate) fn path(&self, cache_path: &Path, checksum: &str) -> PathBuf {
		let file_name = format!(
			"{}_{:#x}_{:#x}_0x{}",
			artifact_prefix(),
			self.code_hash,
			self.executor_params_hash,
			checksum
		);
		cache_path.join(file_name)
	}

	/// Tries to recover the artifact id from the given file name.
	/// Return `None` if the given file name is invalid.
	/// VALID_NAME := <PREFIX> _ <CODE_HASH> _ <PARAM_HASH> _ <CHECKSUM>
	fn from_file_name(file_name: &str) -> Option<Self> {
		let file_name = file_name.strip_prefix(&artifact_prefix())?.strip_prefix('_')?;
		let parts: Vec<&str> = file_name.split('_').collect();

		if let [code_hash, param_hash, _checksum] = parts[..] {
			let code_hash = Hash::from_str(code_hash).ok()?.into();
			let executor_params_hash =
				ExecutorParamsHash::from_hash(Hash::from_str(param_hash).ok()?);
			return Some(Self { code_hash, executor_params_hash })
		}

		None
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
	pub(crate) fn len(&self) -> usize {
		self.inner.len()
	}

	/// Create an empty table and populate it with valid artifacts as [`ArtifactState::Prepared`],
	/// if any. The existing caches will be checked by their file name to determine whether they are
	/// valid, e.g., matching the current node version. The ones deemed invalid will be pruned.
	///
	/// Create the cache directory on-disk if it doesn't exist.
	pub async fn new_and_prune(cache_path: &Path) -> Self {
		let mut artifacts = Self { inner: HashMap::new() };
		let _ = artifacts.insert_and_prune(cache_path).await.map_err(|err| {
			gum::error!(
				target: LOG_TARGET,
				"could not initialize artifacts cache: {err}",
			)
		});
		artifacts
	}

	async fn insert_and_prune(&mut self, cache_path: &Path) -> Result<(), String> {
		async fn is_corrupted(path: &Path) -> bool {
			let checksum = match tokio::fs::read(path).await {
				Ok(bytes) => blake3::hash(&bytes),
				Err(err) => {
					// just remove the file if we cannot read it
					gum::warn!(
						target: LOG_TARGET,
						?err,
						"unable to read artifact {:?} when checking integrity, removing...",
						path,
					);
					return true
				},
			};

			if let Some(file_name) = path.file_name() {
				if let Some(file_name) = file_name.to_str() {
					return !file_name.ends_with(checksum.to_hex().as_str())
				}
			}
			true
		}

		// Insert the entry into the artifacts table if it is valid.
		// Otherwise, prune it.
		async fn insert_or_prune(
			artifacts: &mut Artifacts,
			entry: &tokio::fs::DirEntry,
			cache_path: &Path,
		) -> Result<(), String> {
			let file_type = entry.file_type().await;
			let file_name = entry.file_name();

			match file_type {
				Ok(file_type) =>
					if !file_type.is_file() {
						return Ok(())
					},
				Err(err) => return Err(format!("unable to get file type for {file_name:?}: {err}")),
			}

			if let Some(file_name) = file_name.to_str() {
				let id = ArtifactId::from_file_name(file_name);
				let path = cache_path.join(file_name);

				if id.is_none() || is_corrupted(&path).await {
					let _ = tokio::fs::remove_file(&path).await;
					return Err(format!("invalid artifact {path:?}, file deleted"))
				}

				let id = id.expect("checked is_none() above; qed");
				gum::debug!(
					target: LOG_TARGET,
					"reusing existing {:?} for node version v{}",
					&path,
					NODE_VERSION,
				);
				artifacts.insert_prepared(id, path, SystemTime::now(), Default::default());

				Ok(())
			} else {
				Err(format!("non-Unicode file name {file_name:?} found in {cache_path:?}"))
			}
		}

		// Make sure that the cache path directory and all its parents are created.
		if let Err(err) = tokio::fs::create_dir_all(cache_path).await {
			if err.kind() != io::ErrorKind::AlreadyExists {
				return Err(format!("failed to create dir {cache_path:?}: {err}"))
			}
		}

		let mut dir = tokio::fs::read_dir(cache_path)
			.await
			.map_err(|err| format!("failed to read dir {cache_path:?}: {err}"))?;

		loop {
			match dir.next_entry().await {
				Ok(Some(entry)) =>
					if let Err(err) = insert_or_prune(self, &entry, cache_path).await {
						gum::warn!(
							target: LOG_TARGET,
							?cache_path,
							"could not insert entry {:?} into the artifact cache: {}",
							entry,
							err,
						)
					},
				Ok(None) => return Ok(()),
				Err(err) =>
					return Err(format!("error processing artifacts in {cache_path:?}: {err}")),
			}
		}
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
	use super::{artifact_prefix as prefix, ArtifactId, Artifacts, NODE_VERSION, RUNTIME_VERSION};
	use polkadot_primitives::ExecutorParamsHash;
	use rand::Rng;
	use sp_core::H256;
	use std::{
		fs,
		io::Write,
		path::{Path, PathBuf},
		str::FromStr,
	};

	fn rand_hash(len: usize) -> String {
		let mut rng = rand::thread_rng();
		let hex: Vec<_> = "0123456789abcdef".chars().collect();
		(0..len).map(|_| hex[rng.gen_range(0..hex.len())]).collect()
	}

	fn file_name(code_hash: &str, param_hash: &str, checksum: &str) -> String {
		format!("{}_0x{}_0x{}_0x{}", prefix(), code_hash, param_hash, checksum)
	}

	fn create_artifact(
		dir: impl AsRef<Path>,
		prefix: &str,
		code_hash: impl AsRef<str>,
		params_hash: impl AsRef<str>,
	) -> (PathBuf, String) {
		fn artifact_path_without_checksum(
			dir: impl AsRef<Path>,
			prefix: &str,
			code_hash: impl AsRef<str>,
			params_hash: impl AsRef<str>,
		) -> PathBuf {
			let mut path = dir.as_ref().to_path_buf();
			let file_name =
				format!("{}_0x{}_0x{}", prefix, code_hash.as_ref(), params_hash.as_ref(),);
			path.push(file_name);
			path
		}

		let (code_hash, params_hash) = (code_hash.as_ref(), params_hash.as_ref());
		let path = artifact_path_without_checksum(dir, prefix, code_hash, params_hash);
		let mut file = fs::File::create(&path).unwrap();

		let content = format!("{}{}", code_hash, params_hash).into_bytes();
		file.write_all(&content).unwrap();
		let checksum = blake3::hash(&content).to_hex().to_string();

		(path, checksum)
	}

	fn create_rand_artifact(dir: impl AsRef<Path>, prefix: &str) -> (PathBuf, String) {
		create_artifact(dir, prefix, rand_hash(64), rand_hash(64))
	}

	fn concluded_path(path: impl AsRef<Path>, checksum: &str) -> PathBuf {
		let path = path.as_ref();
		let mut file_name = path.file_name().unwrap().to_os_string();
		file_name.push("_0x");
		file_name.push(checksum);
		path.with_file_name(file_name)
	}

	#[test]
	fn artifact_prefix() {
		assert_eq!(prefix(), format!("wasmtime_v{}_polkadot_v{}", RUNTIME_VERSION, NODE_VERSION));
	}

	#[test]
	fn from_file_name() {
		assert!(ArtifactId::from_file_name("").is_none());
		assert!(ArtifactId::from_file_name("junk").is_none());

		let file_name = file_name(
			"0022800000000000000000000000000000000000000000000000000000000000",
			"0033900000000000000000000000000000000000000000000000000000000000",
			"00000000000000000000000000000000",
		);

		assert_eq!(
			ArtifactId::from_file_name(&file_name),
			Some(ArtifactId::new(
				hex_literal::hex![
					"0022800000000000000000000000000000000000000000000000000000000000"
				]
				.into(),
				ExecutorParamsHash::from_hash(sp_core::H256(hex_literal::hex![
					"0033900000000000000000000000000000000000000000000000000000000000"
				])),
			)),
		);
	}

	#[test]
	fn path() {
		let dir = Path::new("/test");
		let code_hash = "1234567890123456789012345678901234567890123456789012345678901234";
		let params_hash = "4321098765432109876543210987654321098765432109876543210987654321";
		let checksum = "34567890123456789012345678901234";
		let file_name = file_name(code_hash, params_hash, checksum);

		let code_hash = H256::from_str(code_hash).unwrap();
		let params_hash = H256::from_str(params_hash).unwrap();
		let path = ArtifactId::new(code_hash.into(), ExecutorParamsHash::from_hash(params_hash))
			.path(dir, checksum);

		assert_eq!(path.to_str().unwrap(), format!("/test/{}", file_name));
	}

	#[tokio::test]
	async fn remove_stale_cache_on_startup() {
		let cache_dir = tempfile::Builder::new().prefix("test-cache-").tempdir().unwrap();

		// invalid prefix
		create_rand_artifact(&cache_dir, "");
		create_rand_artifact(&cache_dir, "wasmtime_polkadot_v");
		create_rand_artifact(&cache_dir, "wasmtime_v8.0.0_polkadot_v1.0.0");

		let prefix = prefix();

		// no checksum
		create_rand_artifact(&cache_dir, &prefix);

		// invalid hashes
		let (path, checksum) = create_artifact(&cache_dir, &prefix, "000", "000001");
		let new_path = concluded_path(&path, &checksum);
		fs::rename(&path, &new_path).unwrap();

		// checksum tampered
		let (path, checksum) = create_rand_artifact(&cache_dir, &prefix);
		let new_path = concluded_path(&path, checksum.chars().rev().collect::<String>().as_str());
		fs::rename(&path, &new_path).unwrap();

		// valid
		let (path, checksum) = create_rand_artifact(&cache_dir, &prefix);
		let new_path = concluded_path(&path, &checksum);
		fs::rename(&path, &new_path).unwrap();

		assert_eq!(fs::read_dir(&cache_dir).unwrap().count(), 7);

		let artifacts = Artifacts::new_and_prune(cache_dir.path()).await;

		assert_eq!(fs::read_dir(&cache_dir).unwrap().count(), 1);
		assert_eq!(artifacts.len(), 1);

		fs::remove_dir_all(cache_dir).unwrap();
	}
}
