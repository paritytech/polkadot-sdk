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
use sc_client_api::{CompactProof, KeyValueStates, ProofProviderHashDb};
use sc_consensus::ImportedState;
use smallvec::SmallVec;
use sp_core::storage::well_known_keys;
use sp_runtime::traits::HashingFor;
use sp_runtime::{
	traits::{Block as BlockT, Header, NumberFor},
	Justifications,
};
use std::{collections::HashMap, fmt, sync::Arc};
use trie_db::node::Node;
use trie_db::node::NodeHandle;
use trie_db::node::Value;
use trie_db::NibbleSlice;
use trie_db::NibbleVec;
use trie_db::NodeCodec;
use trie_db::TrieLayout;

// error[E0658]: associated type defaults are unstable
type HashDbEmplaceBatch<B> = Vec<(NibbleVec, <B as BlockT>::Hash, Vec<u8>)>;

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

type Layout<B> = sp_state_machine::LayoutV0<HashingFor<B>>;
type LayoutCodec<B> = <Layout<B> as TrieLayout>::Codec;
type ProofDb<B> = sp_state_machine::MemoryDB<HashingFor<B>>;

fn as_hash<B: BlockT>(slice: &[u8]) -> B::Hash {
	let mut hash = B::Hash::default();
	assert_eq!(slice.len(), hash.as_ref().len());
	hash.as_mut().copy_from_slice(slice);
	hash
}

struct LevelNode<B: BlockT> {
	hash: B::Hash,
	encoded: Vec<u8>,
	prefix_nibbles: usize,
	branches: SmallVec<[(u8, B::Hash); 16]>,
}

enum LevelState<B: BlockT> {
	Root(B::Hash),
	ChildHash(NibbleVec, B::Hash),
	ValueHash(B::Hash),
	Value,
	Branch(B::Hash),
	Pop,
	End,
}

struct Level<B: BlockT> {
	allow_child: bool,
	state: LevelState<B>,
	stack: Vec<LevelNode<B>>,
	prefix: NibbleVec,
}
impl<B: BlockT> Level<B> {
	fn new(allow_child: bool, prefix: NibbleVec, root: B::Hash) -> Self {
		Self { allow_child, state: LevelState::Root(root), stack: vec![], prefix }
	}
	// ChildHash | ValueHash | Value | Branch | Pop -> Branch | End
	fn next_branch(&mut self) {
		let is_value = matches!(
			self.state,
			LevelState::ChildHash(_, _) | LevelState::ValueHash(_) | LevelState::Value
		);
		let is_branch = matches!(self.state, LevelState::Branch(_) | LevelState::Pop);
		assert!(is_value || is_branch);
		if is_branch {
			self.prefix.pop().unwrap();
		}
		self.state = if let Some((i, hash)) = self.stack.last_mut().unwrap().branches.pop() {
			self.prefix.push(i);
			LevelState::Branch(hash)
		} else {
			let node = self.stack.last().unwrap();
			self.prefix.drop_lasts(node.prefix_nibbles);
			LevelState::End
		};
	}
	// Root | Branch -> ChildHash | ValueHash | Value
	fn push(&mut self, encoded: Vec<u8>) {
		let hash = match &self.state {
			LevelState::Root(hash) | LevelState::Branch(hash) => *hash,
			_ => panic!(),
		};
		let node = LayoutCodec::<B>::decode(&encoded).unwrap();
		let (prefix, value, branches) = match &node {
			Node::Empty => (None, None, None),
			Node::Leaf(prefix, value) => (Some(prefix), Some(value), None),
			Node::Extension(_, _) => panic!(),
			Node::Branch(branches, value) => (None, value.as_ref(), Some(branches)),
			Node::NibbledBranch(prefix, branches, value) => {
				(Some(prefix), value.as_ref(), Some(branches))
			},
		};
		let prefix_nibbles = if let Some(prefix) = prefix {
			self.prefix.append_partial(prefix.right());
			prefix.len()
		} else {
			0
		};
		let branches = branches
			.map(|branches| {
				branches
					.iter()
					.enumerate()
					.rev()
					.filter_map(|(i, branch)| {
						if let Some(NodeHandle::Hash(hash)) = branch {
							Some((i as u8, as_hash::<B>(hash)))
						} else {
							None
						}
					})
					.collect()
			})
			.unwrap_or_default();
		self.state = if let Some(value) = value {
			let key = self.prefix.as_prefix().0;
			if self.allow_child && well_known_keys::is_child_storage_key(key) {
				let value = if let Value::Inline(value) = value {
					value
				} else {
					panic!();
				};
				assert!(well_known_keys::is_default_child_storage_key(key));
				let prefix = NibbleVec::from(NibbleSlice::new(
					&key[well_known_keys::DEFAULT_CHILD_STORAGE_KEY_PREFIX.len()..],
				));
				LevelState::ChildHash(prefix, as_hash::<B>(value))
			} else {
				match value {
					Value::Inline(_) => LevelState::Value,
					Value::Node(hash) => LevelState::ValueHash(as_hash::<B>(hash)),
				}
			}
		} else {
			LevelState::Value
		};
		self.stack.push(LevelNode { hash, encoded, prefix_nibbles, branches });
	}
	// End -> Pop -> Branch | End
	fn pop(&mut self) {
		assert!(matches!(self.state, LevelState::End));
		let node = self.stack.pop().unwrap();
		if !self.stack.is_empty() {
			self.state = LevelState::Pop;
			self.next_branch();
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
	levels: SmallVec<[Level<B>; 2]>,
	null_hash: B::Hash,
}

impl<B, Client> StateSync<B, Client>
where
	B: BlockT,
	Client: ProofProviderHashDb<B> + Send + Sync + 'static,
{
	///  Create a new instance.
	pub fn new(
		client: Arc<Client>,
		target_header: B::Header,
		target_body: Option<Vec<B::Extrinsic>>,
		target_justifications: Option<Justifications>,
		skip_proof: bool,
	) -> Self {
		let mut sync = Self {
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
			levels: SmallVec::default(),
			null_hash: LayoutCodec::<B>::hashed_null_node(),
		};
		if !skip_proof {
			sync.levels
				.push(Level::new(true, NibbleVec::new(), sync.metadata.target_root()));
		}
		sync
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

	fn _process_state_verified(&mut self, values: KeyValueStates) {
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

	fn on_proof_response(&mut self, proof: &ProofDb<B>) -> bool {
		use hash_db::HashDBRef;
		let is_known =
			|client: &Arc<Client>, null_hash: &B::Hash, prefix: &NibbleVec, hash: &B::Hash| {
				hash == null_hash || client.hash_db_contains(prefix, hash)
			};
		let mut batch: HashDbEmplaceBatch<B> = vec![];
		'outer: while let Some(level) = self.levels.last_mut() {
			if let LevelState::Root(hash) = &level.state {
				if let Some(value) = proof.get(hash, level.prefix.as_prefix()) {
					level.push(value);
				} else {
					break 'outer;
				}
			}
			while let Some(node) = level.stack.last_mut() {
				match &mut level.state {
					LevelState::Root(_) => panic!(),
					LevelState::ChildHash(prefix, hash) => {
						if !is_known(&self.client, &self.null_hash, prefix, hash) {
							let child_level = Level::new(false, std::mem::take(prefix), *hash);
							level.next_branch();
							self.levels.push(child_level);
						} else {
							level.next_branch();
						}
						continue 'outer;
					},
					LevelState::ValueHash(hash) => {
						if !is_known(&self.client, &self.null_hash, &level.prefix, hash) {
							if let Some(value) = proof.get(hash, level.prefix.as_prefix()) {
								batch.push((level.prefix.clone(), *hash, value));
							} else {
								break 'outer;
							}
						}
						level.next_branch();
					},
					LevelState::Value => {
						level.next_branch();
					},
					LevelState::Branch(hash) => {
						if !is_known(&self.client, &self.null_hash, &level.prefix, hash) {
							if let Some(value) = proof.get(hash, level.prefix.as_prefix()) {
								level.push(value);
								continue;
							} else {
								break 'outer;
							}
						} else {
							level.next_branch();
						}
					},
					LevelState::Pop => panic!(),
					LevelState::End => {
						batch.push((
							level.prefix.clone(),
							node.hash,
							std::mem::take(&mut node.encoded),
						));
						level.pop();
					},
				}
			}
			self.levels.pop();
		}
		self.client.hash_db_emplace_batch(batch).unwrap();
		self.levels.is_empty()
	}
}

impl<B, Client> StateSyncProvider<B> for StateSync<B, Client>
where
	B: BlockT,
	Client: ProofProviderHashDb<B> + Send + Sync + 'static,
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
		let complete = if !self.metadata.skip_proof {
			debug!(target: LOG_TARGET, "Importing state from {} trie nodes", response.proof.len());
			let proof_size = response.proof.len() as u64;
			let proof = match CompactProof::decode(&mut response.proof.as_ref()) {
				Ok(proof) => proof,
				Err(e) => {
					debug!(target: LOG_TARGET, "Error decoding proof: {:?}", e);
					return ImportResult::BadResponse
				},
			};

			let mut proof_db = ProofDb::<B>::new(&[]);
			if let Err(e) = sp_trie::decode_compact::<Layout<B>, _, _>(
				&mut proof_db,
				proof.iter_compact_encoded_nodes(),
				Some(&self.metadata.target_root()),
			) {
				debug!(
					target: LOG_TARGET,
					"StateResponse proof decode error: {}",
					e,
				);
				return ImportResult::BadResponse;
			}
			self.metadata.imported_bytes += proof_size;
			let complete = self.on_proof_response(&proof_db);
			self.metadata.last_key =
				self.levels.iter().map(|level| level.prefix.inner().to_vec()).collect();
			complete
		} else {
			self.process_state_unverified(response)
		};
		if complete {
			self.metadata.complete = true;
			let target_hash = self.metadata.target_hash();
			ImportResult::Import(
				target_hash,
				self.metadata.target_header.clone(),
				ImportedState {
					block: target_hash,
					state: std::mem::take(&mut self.state).into(),
					written: !self.metadata.skip_proof,
				},
				self.metadata.target_body.clone(),
				self.metadata.target_justifications.clone(),
			)
		} else {
			ImportResult::Continue
		}
	}

	/// Produce next state request.
	fn next_request(&self) -> StateRequest {
		self.metadata.next_request()
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
