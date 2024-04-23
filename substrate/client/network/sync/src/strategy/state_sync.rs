// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! State sync support.

use crate::{
	schema::v1::{StateEntry, StateRequest, StateResponse},
	LOG_TARGET,
};
use codec::{Decode, Encode};
use log::debug;
use sc_client_api::{CompactProof, ProofProvider};
use sc_consensus::ImportedState;
use smallvec::SmallVec;
use sp_core::storage::well_known_keys;
use sp_runtime::{
	traits::{Block as BlockT, Header, NumberFor},
	Justifications,
};
use std::{collections::HashMap, fmt, sync::Arc};

/// Generic state sync provider. Used for mocking in tests.
pub trait StateSyncProvider<B: BlockT>: Send + Sync {
	/// Validate and import a state response.
	fn import(&mut self, response: StateResponse) -> ImportResult<B>;
	/// Produce next state request.
	fn next_request(&self) -> StateRequest;
	/// Check if the state is complete.
	fn is_complete(&self) -> bool;
	/// Returns target block number.
	fn target_number(&self) -> NumberFor<B>;
	/// Returns target block hash.
	fn target_hash(&self) -> B::Hash;
	/// Returns state sync estimated progress.
	fn progress(&self) -> StateSyncProgress;
}

// Reported state sync phase.
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum StateSyncPhase {
	// State download in progress.
	DownloadingState,
	// Download is complete, state is being imported.
	ImportingState,
}

impl fmt::Display for StateSyncPhase {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self {
			Self::DownloadingState => write!(f, "Downloading state"),
			Self::ImportingState => write!(f, "Importing state"),
		}
	}
}

/// Reported state download progress.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct StateSyncProgress {
	/// Estimated download percentage.
	pub percentage: u32,
	/// Total state size in bytes downloaded so far.
	pub size: u64,
	/// Current state sync phase.
	pub phase: StateSyncPhase,
}

/// Import state chunk result.
pub enum ImportResult<B: BlockT> {
	/// State is complete and ready for import.
	Import(B::Hash, B::Header, ImportedState<B>, Option<Vec<B::Extrinsic>>, Option<Justifications>),
	/// Continue downloading.
	Continue,
	/// Bad state chunk.
	BadResponse,
}

/// State sync state machine. Accumulates partial state data until it
/// is ready to be imported.
pub struct StateSync<B: BlockT, Client> {
	target_block: B::Hash,
	target_header: B::Header,
	target_root: B::Hash,
	target_body: Option<Vec<B::Extrinsic>>,
	target_justifications: Option<Justifications>,
	last_key: SmallVec<[Vec<u8>; 2]>,
	state: HashMap<Vec<u8>, (Vec<(Vec<u8>, Vec<u8>)>, Vec<Vec<u8>>)>,
	complete: bool,
	client: Arc<Client>,
	imported_bytes: u64,
	skip_proof: bool,
}

impl<B, Client> StateSync<B, Client>
where
	B: BlockT,
	Client: ProofProvider<B> + Send + Sync + 'static,
{
	///  Create a new instance.
	pub fn new(
		client: Arc<Client>,
		target_header: B::Header,
		target_body: Option<Vec<B::Extrinsic>>,
		target_justifications: Option<Justifications>,
		skip_proof: bool,
	) -> Self {
		Self {
			client,
			target_block: target_header.hash(),
			target_root: *target_header.state_root(),
			target_header,
			target_body,
			target_justifications,
			last_key: SmallVec::default(),
			state: HashMap::default(),
			complete: false,
			imported_bytes: 0,
			skip_proof,
		}
	}
}

impl<B, Client> StateSyncProvider<B> for StateSync<B, Client>
where
	B: BlockT,
	Client: ProofProvider<B> + Send + Sync + 'static,
{
	///  Validate and import a state response.
	fn import(&mut self, response: StateResponse) -> ImportResult<B> {
		if response.entries.is_empty() && response.proof.is_empty() {
			debug!(target: LOG_TARGET, "Bad state response");
			return ImportResult::BadResponse
		}
		if !self.skip_proof && response.proof.is_empty() {
			debug!(target: LOG_TARGET, "Missing proof");
			return ImportResult::BadResponse
		}
		let complete = if !self.skip_proof {
			debug!(target: LOG_TARGET, "Importing state from {} trie nodes", response.proof.len());
			let proof_size = response.proof.len() as u64;
			let proof = match CompactProof::decode(&mut response.proof.as_ref()) {
				Ok(proof) => proof,
				Err(e) => {
					debug!(target: LOG_TARGET, "Error decoding proof: {:?}", e);
					return ImportResult::BadResponse
				},
			};
			let (values, completed) = match self.client.verify_range_proof(
				self.target_root,
				proof,
				self.last_key.as_slice(),
			) {
				Err(e) => {
					debug!(
						target: LOG_TARGET,
						"StateResponse failed proof verification: {}",
						e,
					);
					return ImportResult::BadResponse
				},
				Ok(values) => values,
			};
			debug!(target: LOG_TARGET, "Imported with {} keys", values.len());

			let complete = completed == 0;
			if !complete && !values.update_last_key(completed, &mut self.last_key) {
				debug!(target: LOG_TARGET, "Error updating key cursor, depth: {}", completed);
			};

			for values in values.0 {
				let key_values = if values.state_root.is_empty() {
					// Read child trie roots.
					values
						.key_values
						.into_iter()
						.filter(|key_value| {
							if well_known_keys::is_child_storage_key(key_value.0.as_slice()) {
								self.state
									.entry(key_value.1.clone())
									.or_default()
									.1
									.push(key_value.0.clone());
								false
							} else {
								true
							}
						})
						.collect()
				} else {
					values.key_values
				};
				let entry = self.state.entry(values.state_root).or_default();
				if entry.0.len() > 0 && entry.1.len() > 1 {
					// Already imported child_trie with same root.
					// Warning this will not work with parallel download.
				} else if entry.0.is_empty() {
					for (key, _value) in key_values.iter() {
						self.imported_bytes += key.len() as u64;
					}

					entry.0 = key_values;
				} else {
					for (key, value) in key_values {
						self.imported_bytes += key.len() as u64;
						entry.0.push((key, value))
					}
				}
			}
			self.imported_bytes += proof_size;
			complete
		} else {
			let mut complete = true;
			// if the trie is a child trie and one of its parent trie is empty,
			// the parent cursor stays valid.
			// Empty parent trie content only happens when all the response content
			// is part of a single child trie.
			if self.last_key.len() == 2 && response.entries[0].entries.is_empty() {
				// Do not remove the parent trie position.
				self.last_key.pop();
			} else {
				self.last_key.clear();
			}
			for state in response.entries {
				debug!(
					target: LOG_TARGET,
					"Importing state from {:?} to {:?}",
					state.entries.last().map(|e| sp_core::hexdisplay::HexDisplay::from(&e.key)),
					state.entries.first().map(|e| sp_core::hexdisplay::HexDisplay::from(&e.key)),
				);

				if !state.complete {
					if let Some(e) = state.entries.last() {
						self.last_key.push(e.key.clone());
					}
					complete = false;
				}
				let is_top = state.state_root.is_empty();
				let entry = self.state.entry(state.state_root).or_default();
				if entry.0.len() > 0 && entry.1.len() > 1 {
					// Already imported child trie with same root.
				} else {
					let mut child_roots = Vec::new();
					for StateEntry { key, value } in state.entries {
						// Skip all child key root (will be recalculated on import).
						if is_top && well_known_keys::is_child_storage_key(key.as_slice()) {
							child_roots.push((value, key));
						} else {
							self.imported_bytes += key.len() as u64;
							entry.0.push((key, value))
						}
					}
					for (root, storage_key) in child_roots {
						self.state.entry(root).or_default().1.push(storage_key);
					}
				}
			}
			complete
		};
		if complete {
			self.complete = true;
			ImportResult::Import(
				self.target_block,
				self.target_header.clone(),
				ImportedState {
					block: self.target_block,
					state: std::mem::take(&mut self.state).into(),
				},
				self.target_body.clone(),
				self.target_justifications.clone(),
			)
		} else {
			ImportResult::Continue
		}
	}

	/// Produce next state request.
	fn next_request(&self) -> StateRequest {
		StateRequest {
			block: self.target_block.encode(),
			start: self.last_key.clone().into_vec(),
			no_proof: self.skip_proof,
		}
	}

	/// Check if the state is complete.
	fn is_complete(&self) -> bool {
		self.complete
	}

	/// Returns target block number.
	fn target_number(&self) -> NumberFor<B> {
		*self.target_header.number()
	}

	/// Returns target block hash.
	fn target_hash(&self) -> B::Hash {
		self.target_block
	}

	/// Returns state sync estimated progress.
	fn progress(&self) -> StateSyncProgress {
		let cursor = *self.last_key.get(0).and_then(|last| last.get(0)).unwrap_or(&0u8);
		let percent_done = cursor as u32 * 100 / 256;
		StateSyncProgress {
			percentage: percent_done,
			size: self.imported_bytes,
			phase: if self.complete {
				StateSyncPhase::ImportingState
			} else {
				StateSyncPhase::DownloadingState
			},
		}
	}
}
