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
	schema::v1::{KeyValueStateEntry, StateEntry, StateRequest, StateResponse},
	LOG_TARGET,
};
use codec::{Decode, Encode};
use log::debug;
use sc_client_api::{CompactProof, KeyValueStates, ProofProvider};
use sc_consensus::ImportedState;
use smallvec::SmallVec;
use sp_core::storage::well_known_keys;
use sp_runtime::{
	traits::{Block as BlockT, HashingFor, Header, NumberFor},
	Justifications,
};
use sp_trie::PrefixedMemoryDB;
use sp_trie::MemoryDB;
use sp_trie::ClientProof;
use sp_trie::CLIENT_PROOF;
use sp_trie::TrieNodeChild;
use sp_trie::get_trie_node_children;
use hash_db::HashDB;
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
	Import {
		hash: B::Hash,
		header: B::Header,
		partial_state: Option<PrefixedMemoryDB<HashingFor<B>>>,
		state: ImportedState<B>,
		body: Option<Vec<B::Extrinsic>>,
		justifications: Option<Justifications>,
	},
	/// Continue downloading.
	Continue {
		partial_state: Option<PrefixedMemoryDB<HashingFor<B>>>,
	},
	/// Bad state chunk.
	BadResponse,
}

impl<B: BlockT> ImportResult<B> {
	pub fn take_partial_state(&mut self) -> Option<PrefixedMemoryDB<HashingFor<B>>> {
		match self {
			ImportResult::Import { partial_state, .. } | ImportResult::Continue { partial_state } => partial_state.take(),
			ImportResult::BadResponse => None,
		}
	}
}

struct StateSyncMetadata<B: BlockT> {
	last_key: SmallVec<[Vec<u8>; 2]>,
	target_header: B::Header,
	target_body: Option<Vec<B::Extrinsic>>,
	target_justifications: Option<Justifications>,
	complete: bool,
	imported_bytes: u64,
	skip_proof: bool,
}

impl<B: BlockT> StateSyncMetadata<B> {
	fn target_hash(&self) -> B::Hash {
		self.target_header.hash()
	}

	/// Returns target block number.
	fn target_number(&self) -> NumberFor<B> {
		*self.target_header.number()
	}

	fn target_root(&self) -> B::Hash {
		*self.target_header.state_root()
	}

	fn next_request(&self) -> StateRequest {
		StateRequest {
			block: self.target_hash().encode(),
			start: self.last_key.clone().into_vec(),
			no_proof: self.skip_proof,
		}
	}

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

enum Tree<B: BlockT> {
	Known {
		hash: B::Hash,
		children: Vec<Tree<B>>,
	},
	Unknown(TrieNodeChild<B::Hash>),
}
impl<B: BlockT> Tree<B> {
	fn complete(&self) -> bool {
		match self {
			Self::Known { children, .. } if children.is_empty() => true,
			_ => false,
		}
	}

	fn node_hash(&self) -> &B::Hash {
		match self {
			Self::Known { hash, .. } | Self::Unknown(TrieNodeChild { hash, .. }) => hash,
		}
	}

	fn request(&self) -> ClientProof<B::Hash> {
		ClientProof {
			hash: *self.node_hash(),
			children: if let Self::Known { children, .. } = self {
				children.iter().map(|child| child.request()).collect()
			} else {
				vec![]
			},
		}
	}
}

type Paths<B> = HashMap<<B as BlockT>::Hash, Vec<Vec<<B as BlockT>::Hash>>>;

fn fill<B: BlockT>(
	tree: &mut Tree<B>,
	paths: &mut Paths<B>,
	db_out: &mut PrefixedMemoryDB::<HashingFor<B>>,
	db_in: &MemoryDB::<HashingFor<B>>,
	path: &mut Vec<B::Hash>,
	depth: usize,
) {
	match tree {
		Tree::Known { children, .. } => {
			let child_hash = path.get(depth).unwrap();
			let child_index = children.iter().position(|child_tree| child_tree.node_hash() == child_hash).unwrap();
			let child_tree = children.get_mut(child_index).unwrap();
			fill(child_tree, paths, db_out, db_in, path, depth + 1);
			if child_tree.complete() {
				children.remove(child_index);
			}
		},
		Tree::Unknown(node) => {
			if let Some(encoded) = db_in.get(&node.hash, node.prefix.as_prefix()) {
				let children = if !node.has_children() {
					vec![]
				} else {
					get_trie_node_children::<HashingFor<B>>(&node.prefix, &encoded)
						.unwrap()
						.into_iter()
						.filter_map(|child_node| {
							let mut child_tree = Tree::Unknown(child_node);
							path.push(*child_tree.node_hash());
							fill(&mut child_tree, paths, db_out, db_in, path, depth + 1);
							path.pop();
							if child_tree.complete() {
								None
							} else {
								Some(child_tree)
							}
						})
						.collect()
				};
				db_out.emplace(node.hash, node.prefix.as_prefix(), encoded);
				*tree = Tree::Known { hash: node.hash, children };
			} else {
				paths.entry(node.hash).or_default().push(path.clone());
			}
		}
	}
}

/// State sync state machine.
///
/// Accumulates partial state data until it is ready to be imported.
pub struct StateSync<B: BlockT, Client> {
	metadata: StateSyncMetadata<B>,
	state: HashMap<Vec<u8>, (Vec<(Vec<u8>, Vec<u8>)>, Vec<Vec<u8>>)>,
	client: Arc<Client>,
	tree: Tree<B>,
	paths: Paths<B>,
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
		let state_root = *target_header.state_root();
		Self {
			client,
			metadata: StateSyncMetadata {
				last_key: SmallVec::default(),
				target_header,
				target_body,
				target_justifications,
				complete: false,
				imported_bytes: 0,
				skip_proof,
			},
			state: HashMap::default(),
			tree: Tree::Unknown(TrieNodeChild::root(state_root)),
			paths: Paths::<B>::from_iter([(state_root, vec![])]),
		}
	}

	fn process_state_key_values(
		&mut self,
		state_root: Vec<u8>,
		key_values: impl IntoIterator<Item = (Vec<u8>, Vec<u8>)>,
	) {
		let is_top = state_root.is_empty();

		let entry = self.state.entry(state_root).or_default();

		if entry.0.len() > 0 && entry.1.len() > 1 {
			// Already imported child_trie with same root.
			// Warning this will not work with parallel download.
			return;
		}

		let mut child_storage_roots = Vec::new();

		for (key, value) in key_values {
			// Skip all child key root (will be recalculated on import)
			if is_top && well_known_keys::is_child_storage_key(key.as_slice()) {
				child_storage_roots.push((value, key));
			} else {
				self.metadata.imported_bytes += key.len() as u64;
				entry.0.push((key, value));
			}
		}

		for (root, storage_key) in child_storage_roots {
			self.state.entry(root).or_default().1.push(storage_key);
		}
	}

	fn process_state_verified(&mut self, values: KeyValueStates) {
		for values in values.0 {
			self.process_state_key_values(values.state_root, values.key_values);
		}
	}

	fn process_state_unverified(&mut self, response: StateResponse) -> bool {
		let mut complete = true;
		// if the trie is a child trie and one of its parent trie is empty,
		// the parent cursor stays valid.
		// Empty parent trie content only happens when all the response content
		// is part of a single child trie.
		if self.metadata.last_key.len() == 2 && response.entries[0].entries.is_empty() {
			// Do not remove the parent trie position.
			self.metadata.last_key.pop();
		} else {
			self.metadata.last_key.clear();
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
					self.metadata.last_key.push(e.key.clone());
				}
				complete = false;
			}

			let KeyValueStateEntry { state_root, entries, complete: _ } = state;
			self.process_state_key_values(
				state_root,
				entries.into_iter().map(|StateEntry { key, value }| (key, value)),
			);
		}
		complete
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
		if !self.metadata.skip_proof && response.proof.is_empty() {
			debug!(target: LOG_TARGET, "Missing proof");
			return ImportResult::BadResponse
		}
		let (complete, partial_state) = if !self.metadata.skip_proof {
			debug!(target: LOG_TARGET, "Importing state from {} trie nodes", response.proof.len());
			let proof_size = response.proof.len() as u64;
			let proof = match CompactProof::decode(&mut response.proof.as_ref()) {
				Ok(proof) => proof,
				Err(e) => {
					debug!(target: LOG_TARGET, "Error decoding proof: {:?}", e);
					return ImportResult::BadResponse
				},
			};

			let mut partial_state = PrefixedMemoryDB::<HashingFor<B>>::default();
			for proof in &proof.encoded_nodes {
				let proof = match CompactProof::decode(&mut &proof[..]) {
					Ok(proof) => proof,
					Err(e) => {
						debug!(target: LOG_TARGET, "Error decoding proof: {:?}", e);
						return ImportResult::BadResponse;
					},
				};
				let mut db = MemoryDB::<HashingFor<B>>::default();
				let root = match sp_trie::decode_compact::<sp_state_machine::LayoutV0<HashingFor<B>>, _, _>(
					&mut db,
					proof.iter_compact_encoded_nodes(),
					None,
				) {
					Ok(root) => root,
					Err(e) => {
						debug!(target: LOG_TARGET, "Error decoding proof: {:?}", e);
						return ImportResult::BadResponse;
					},
				};
				if let Some(paths) = self.paths.remove(&root) {
					for mut path in paths {
						fill(&mut self.tree, &mut self.paths, &mut partial_state, &db, &mut path, 0);
					}
				}
			}
			debug!(target: LOG_TARGET, "Imported with ??? nodes");

			self.metadata.imported_bytes += proof_size;
			(self.tree.complete(), Some(partial_state))
		} else {
			(self.process_state_unverified(response), None)
		};
		if complete {
			self.metadata.complete = true;
			let target_hash = self.metadata.target_hash();
			let state = if partial_state.is_none() {
				ImportedState::KeyValues { block: target_hash, state: std::mem::take(&mut self.state).into() }
			} else {
				ImportedState::Proof
			};
			ImportResult::Import {
				hash: target_hash,
				header: self.metadata.target_header.clone(),
				partial_state,
				state,
				body: self.metadata.target_body.clone(),
				justifications: self.metadata.target_justifications.clone(),
			}
		} else {
			ImportResult::Continue {
				partial_state,
			}
		}
	}

	/// Produce next state request.
	fn next_request(&self) -> StateRequest {
		StateRequest {
			block: self.target_hash().encode(),
			start: vec![CLIENT_PROOF.to_vec(), self.tree.request().encode()],
			no_proof: false,
		}
	}

	/// Check if the state is complete.
	fn is_complete(&self) -> bool {
		self.metadata.complete
	}

	/// Returns target block number.
	fn target_number(&self) -> NumberFor<B> {
		self.metadata.target_number()
	}

	/// Returns target block hash.
	fn target_hash(&self) -> B::Hash {
		self.metadata.target_hash()
	}

	/// Returns state sync estimated progress.
	fn progress(&self) -> StateSyncProgress {
		self.metadata.progress()
	}
}
