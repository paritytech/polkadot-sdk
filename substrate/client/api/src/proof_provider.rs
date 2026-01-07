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

//! Proof utilities
use crate::{CompactProof, StorageProof};
use sp_runtime::traits::Block as BlockT;
use sp_state_machine::{KeyValueStates, KeyValueStorageLevel};
use sp_storage::ChildInfo;

/// Options for a single key in read proof requests (RFC-0009).
#[derive(Debug, Clone)]
pub struct KeyOptions {
	/// The storage key to read.
	pub key: Vec<u8>,
	/// If true, only include the hash of the value in the proof, not the value itself.
	/// Only effective for state_version=1 with values > 32 bytes.
	pub skip_value: bool,
	/// If true, include all descendant keys under this prefix in the proof.
	pub include_descendants: bool,
}

/// Parameters for read proof requests on the main trie (RFC-0009).
#[derive(Debug, Clone)]
pub struct ReadProofParams<Hash> {
	/// Block hash to read state from.
	pub block: Hash,
	/// Keys to read with their individual options.
	pub keys: Vec<KeyOptions>,
	/// Lower bound for returned keys (pagination). Keys <= this value are excluded.
	pub only_keys_after: Option<Vec<u8>>,
	/// If true, ignore the last 4 bits (nibble) of `only_keys_after`.
	pub only_keys_after_ignore_last_nibble: bool,
	/// Maximum response size in bytes.
	pub size_limit: usize,
}

/// Parameters for read proof requests on a child trie (RFC-0009).
#[derive(Debug, Clone)]
pub struct ReadChildProofParams<Hash> {
	/// Block hash to read state from.
	pub block: Hash,
	/// The child trie to read from.
	pub child_info: ChildInfo,
	/// Keys to read with their individual options.
	pub keys: Vec<KeyOptions>,
	/// Lower bound for returned keys (pagination). Keys <= this value are excluded.
	pub only_keys_after: Option<Vec<u8>>,
	/// If true, ignore the last 4 bits (nibble) of `only_keys_after`.
	pub only_keys_after_ignore_last_nibble: bool,
	/// Maximum response size in bytes.
	pub size_limit: usize,
}

/// Interface for providing block proving utilities.
pub trait ProofProvider<Block: BlockT> {
	/// Reads storage values from the main trie at a given block, returning read proof.
	///
	/// Supports advanced features (RFC-0009):
	/// - `skip_value`: Include only the hash of values, not the values themselves
	/// - `include_descendants`: Include all keys under a prefix
	/// - `only_keys_after`: Pagination support
	/// - `size_limit`: Maximum response size
	///
	/// Returns the proof and the number of keys included.
	fn read_proof(
		&self,
		params: ReadProofParams<Block::Hash>,
	) -> sp_blockchain::Result<(StorageProof, u32)>;

	/// Reads storage values from a child trie at a given block, returning read proof.
	///
	/// Supports advanced features (RFC-0009):
	/// - `skip_value`: Include only the hash of values, not the values themselves
	/// - `include_descendants`: Include all keys under a prefix
	/// - `only_keys_after`: Pagination support
	/// - `size_limit`: Maximum response size
	///
	/// Returns the proof and the number of keys included.
	fn read_child_proof(
		&self,
		params: ReadChildProofParams<Block::Hash>,
	) -> sp_blockchain::Result<(StorageProof, u32)>;

	/// Execute a call to a contract on top of state in a block of given hash
	/// AND returning execution proof.
	///
	/// No changes are made.
	fn execution_proof(
		&self,
		hash: Block::Hash,
		method: &str,
		call_data: &[u8],
	) -> sp_blockchain::Result<(Vec<u8>, StorageProof)>;

	/// Given a `Hash` iterate over all storage values starting at `start_keys`.
	/// Last `start_keys` element contains last accessed key value.
	/// With multiple `start_keys`, first `start_keys` element is
	/// the current storage key of of the last accessed child trie.
	/// at last level the value to start at exclusively.
	/// Proofs is build until size limit is reached and always include at
	/// least one key following `start_keys`.
	/// Returns combined proof and the numbers of collected keys.
	fn read_proof_collection(
		&self,
		hash: Block::Hash,
		start_keys: &[Vec<u8>],
		size_limit: usize,
	) -> sp_blockchain::Result<(CompactProof, u32)>;

	/// Given a `Hash` iterate over all storage values starting at `start_key`.
	/// Returns collected keys and values.
	/// Returns the collected keys values content of the top trie followed by the
	/// collected keys values of child tries.
	/// Only child tries with their root part of the collected content or
	/// related to `start_key` are attached.
	/// For each collected state a boolean indicates if state reach
	/// end.
	fn storage_collection(
		&self,
		hash: Block::Hash,
		start_key: &[Vec<u8>],
		size_limit: usize,
	) -> sp_blockchain::Result<Vec<(KeyValueStorageLevel, bool)>>;

	/// Verify read storage proof for a set of keys.
	/// Returns collected key-value pairs and the nested state
	/// depth of current iteration or 0 if completed.
	fn verify_range_proof(
		&self,
		root: Block::Hash,
		proof: CompactProof,
		start_keys: &[Vec<u8>],
	) -> sp_blockchain::Result<(KeyValueStates, usize)>;
}
