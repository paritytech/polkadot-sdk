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
//! 1. During node start-up, the artifacts cache is cleaned up. This means that all local artifacts
//!    stored on-disk are cleared, and we start with an empty [`Artifacts`] table.
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

use crate::{host::PrepareResultSender, LOG_TARGET};
use always_assert::always;
use polkadot_core_primitives::Hash;
use polkadot_node_core_pvf_common::{error::PrepareError, prepare::PrepareStats, pvf::PvfPrepData};
use polkadot_node_primitives::NODE_VERSION;
use polkadot_parachain_primitives::primitives::ValidationCodeHash;
use polkadot_primitives::ExecutorParamsHash;
use std::{
	collections::HashMap,
	path::{Path, PathBuf},
	str::FromStr as _,
	time::{Duration, SystemTime},
};

macro_rules! concat_const {
    ($($arg:tt),*) => {{
        // ensure inputs to be strings
        $(const _: &str = $arg;)*

        const LEN: usize = 0 $(+ $arg.len())*;

        const CAT: [u8; LEN] = {
            let mut cat = [0u8; LEN];
            // for turning off unused warning
            let mut _offset = 0;

            $({
                const BYTES: &[u8] = $arg.as_bytes();

                let mut i = 0;
                let len = BYTES.len();
                while i < len {
                    cat[_offset + i] = BYTES[i];
                    i += 1;
                }
                _offset += len;
            })*

            cat
        };

        // SAFETY: safe because x and y are guaranteed to be valid
        unsafe { std::str::from_utf8_unchecked(&CAT) }
    }}
}

const RUNTIME_PREFIX: &str = "wasmtime_";
const NODE_PREFIX: &str = "polkadot_v";
const ARTIFACT_PREFIX: &str = concat_const!(RUNTIME_PREFIX, NODE_PREFIX, NODE_VERSION);

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

	/// Returns the expected path to this artifact given the root of the cache.
	pub fn path(&self, cache_path: &Path) -> PathBuf {
		let file_name = format!(
			"{}{}{}_{:#x}_{:#x}",
			RUNTIME_PREFIX, NODE_PREFIX, NODE_VERSION, self.code_hash, self.executor_params_hash
		);
		cache_path.join(file_name)
	}

	/// Tries to recover the artifact id from the given file name.
	pub(crate) fn from_file_name(file_name: &str) -> Option<Self> {
		let file_name = file_name.strip_prefix(ARTIFACT_PREFIX)?.strip_prefix('_')?;

		// [ code hash | param hash ]
		let hashes: Vec<&str> = file_name.split('_').collect();

		if hashes.len() != 2 {
			return None
		}

		let (code_hash_str, executor_params_hash_str) = (hashes[0], hashes[1]);

		let code_hash = Hash::from_str(code_hash_str).ok()?.into();
		let executor_params_hash =
			ExecutorParamsHash::from_hash(Hash::from_str(executor_params_hash_str).ok()?);

		Some(Self { code_hash, executor_params_hash })
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
	pub(crate) fn new(artifact_id: ArtifactId, cache_path: &Path) -> Self {
		Self { path: artifact_id.path(cache_path), id: artifact_id }
	}
}

pub enum ArtifactState {
	/// The artifact is ready to be used by the executor.
	///
	/// That means that the artifact should be accessible through the path obtained by the artifact
	/// id (unless, it was removed externally).
	Prepared {
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
		waiting_for_response: Vec<PrepareResultSender>,
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
	pub(crate) fn new() -> Self {
		Self { inner: HashMap::new() }
	}

	#[cfg(test)]
	pub(crate) fn len(&self) -> usize {
		self.inner.len()
	}

	/// Create a table with valid artifacts and prune the invalid ones.
	pub async fn new_and_prune(cache_path: &Path) -> Self {
		let mut artifacts = Self { inner: HashMap::new() };
		Self::prune_and_insert(cache_path, &mut artifacts).await;
		artifacts
	}

	// FIXME eagr: extremely janky, please comment on the appropriate way of setting
	// * `last_time_needed` set as roughly around startup time
	// * `prepare_stats` set as Nones, since the metadata was lost
	async fn prune_and_insert(cache_path: impl AsRef<Path>, artifacts: &mut Artifacts) {
		fn is_stale(file_name: &str) -> bool {
			!file_name.starts_with(ARTIFACT_PREFIX)
		}

		fn insert_cache(artifacts: &mut Artifacts, id: ArtifactId) {
			let last_time_needed = SystemTime::now();
			let prepare_stats = Default::default();
			always!(artifacts
				.inner
				.insert(id, ArtifactState::Prepared { last_time_needed, prepare_stats })
				.is_none());
		}

		// Make sure that the cache path directory and all its parents are created.
		let cache_path = cache_path.as_ref();
		let _ = tokio::fs::create_dir_all(cache_path).await;

		if let Ok(mut dir) = tokio::fs::read_dir(cache_path).await {
			let mut prunes = vec![];

			loop {
				match dir.next_entry().await {
					Ok(None) => break,
					Ok(Some(entry)) => {
						let file_name = entry.file_name();
						if let Some(file_name) = file_name.to_str() {
							if is_stale(file_name) {
								prunes.push(tokio::fs::remove_file(cache_path.join(file_name)));
							} else if let Some(id) = ArtifactId::from_file_name(file_name) {
								insert_cache(artifacts, id);
							}
						}
					},
					Err(err) => gum::error!(
						target: LOG_TARGET,
						?err,
						"collecting stale artifacts",
					),
				}
			}

			futures::future::join_all(prunes).await;
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
		waiting_for_response: Vec<PrepareResultSender>,
	) {
		// See the precondition.
		always!(self
			.inner
			.insert(artifact_id, ArtifactState::Preparing { waiting_for_response, num_failures: 0 })
			.is_none());
	}

	/// Insert an artifact with the given ID as "prepared".
	///
	/// This function must be used only for brand-new artifacts and should never be used for
	/// replacing existing ones.
	#[cfg(test)]
	pub fn insert_prepared(
		&mut self,
		artifact_id: ArtifactId,
		last_time_needed: SystemTime,
		prepare_stats: PrepareStats,
	) {
		// See the precondition.
		always!(self
			.inner
			.insert(artifact_id, ArtifactState::Prepared { last_time_needed, prepare_stats })
			.is_none());
	}

	/// Remove and retrieve the artifacts from the table that are older than the supplied
	/// Time-To-Live.
	pub fn prune(&mut self, artifact_ttl: Duration) -> Vec<ArtifactId> {
		let now = SystemTime::now();

		let mut to_remove = vec![];
		for (k, v) in self.inner.iter() {
			if let ArtifactState::Prepared { last_time_needed, .. } = *v {
				if now
					.duration_since(last_time_needed)
					.map(|age| age > artifact_ttl)
					.unwrap_or(false)
				{
					to_remove.push(k.clone());
				}
			}
		}

		for artifact in &to_remove {
			self.inner.remove(artifact);
		}

		to_remove
	}
}

#[cfg(test)]
mod tests {
	use super::{ArtifactId, Artifacts, ARTIFACT_PREFIX, NODE_VERSION};
	use polkadot_primitives::ExecutorParamsHash;
	use sp_core::H256;
	use std::{
		fs,
		path::{Path, PathBuf},
		str::FromStr,
	};

	fn file_name(code_hash: &str, param_hash: &str) -> String {
		format!("wasmtime_polkadot_v{}_0x{}_0x{}", NODE_VERSION, code_hash, param_hash)
	}

	fn fake_artifact_path<D: AsRef<Path>>(dir: D, prefix: &str) -> PathBuf {
		let code_hash = "1234567890123456789012345678901234567890123456789012345678901234";
		let params_hash = "4321098765432109876543210987654321098765432109876543210987654321";
		let file_name = format!("{}_0x{}_0x{}", prefix, code_hash, params_hash);

		let mut path = dir.as_ref().to_path_buf();
		path.push(file_name);
		path
	}

	fn create_fake_artifact<D: AsRef<Path>>(dir: D, prefix: &str) {
		let path = fake_artifact_path(dir, prefix);
		fs::File::create(path).unwrap();
	}

	#[test]
	fn artifact_prefix() {
		assert_eq!(ARTIFACT_PREFIX, format!("wasmtime_polkadot_v{}", NODE_VERSION),)
	}

	#[test]
	fn from_file_name() {
		assert!(ArtifactId::from_file_name("").is_none());
		assert!(ArtifactId::from_file_name("junk").is_none());

		let file_name = file_name(
			"0022800000000000000000000000000000000000000000000000000000000000",
			"0033900000000000000000000000000000000000000000000000000000000000",
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
		let file_name = file_name(code_hash, params_hash);

		let code_hash = H256::from_str(code_hash).unwrap();
		let params_hash = H256::from_str(params_hash).unwrap();

		assert_eq!(
			ArtifactId::new(code_hash.into(), ExecutorParamsHash::from_hash(params_hash))
				.path(dir)
				.to_str(),
			Some(format!("/test/{}", file_name).as_str()),
		);
	}

	#[tokio::test]
	async fn remove_stale_cache_on_startup() {
		let cache_dir = crate::worker_intf::tmppath("test-cache").await.unwrap();

		fs::create_dir_all(&cache_dir).unwrap();

		// 3 invalid, 1 valid
		create_fake_artifact(&cache_dir, "");
		create_fake_artifact(&cache_dir, "wasmtime_polkadot_v");
		create_fake_artifact(&cache_dir, "wasmtime_polkadot_v1.0.0");
		create_fake_artifact(&cache_dir, ARTIFACT_PREFIX);

		assert_eq!(fs::read_dir(&cache_dir).unwrap().count(), 4);

		let artifacts = Artifacts::new_and_prune(&cache_dir).await;

		assert_eq!(fs::read_dir(&cache_dir).unwrap().count(), 1);
		assert_eq!(artifacts.len(), 1);

		fs::remove_dir_all(cache_dir).unwrap();
	}
}
