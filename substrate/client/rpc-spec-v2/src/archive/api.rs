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

//! API trait of the archive methods.

use crate::{
	common::events::{ArchiveStorageResult, PaginatedStorageQuery},
	MethodResult,
};
use jsonrpsee::{core::RpcResult, proc_macros::rpc};

#[rpc(client, server)]
pub trait ArchiveApi<Hash> {
	/// Retrieves the body (list of transactions) of a given block hash.
	///
	/// Returns an array of strings containing the hexadecimal-encoded SCALE-codec-encoded
	/// transactions in that block. If no block with that hash is found, null.
	///
	/// # Unstable
	///
	/// This method is unstable and subject to change in the future.
	#[method(name = "archive_unstable_body")]
	fn archive_unstable_body(&self, hash: Hash) -> RpcResult<Option<Vec<String>>>;

	/// Get the chain's genesis hash.
	///
	/// Returns a string containing the hexadecimal-encoded hash of the genesis block of the chain.
	///
	/// # Unstable
	///
	/// This method is unstable and subject to change in the future.
	#[method(name = "archive_unstable_genesisHash")]
	fn archive_unstable_genesis_hash(&self) -> RpcResult<String>;

	/// Get the block's header.
	///
	/// Returns a string containing the hexadecimal-encoded SCALE-codec encoding header of the
	/// block.
	///
	/// # Unstable
	///
	/// This method is unstable and subject to change in the future.
	#[method(name = "archive_unstable_header")]
	fn archive_unstable_header(&self, hash: Hash) -> RpcResult<Option<String>>;

	/// Get the height of the current finalized block.
	///
	/// Returns an integer height of the current finalized block of the chain.
	///
	/// # Unstable
	///
	/// This method is unstable and subject to change in the future.
	#[method(name = "archive_unstable_finalizedHeight")]
	fn archive_unstable_finalized_height(&self) -> RpcResult<u64>;

	/// Get the hashes of blocks from the given height.
	///
	/// Returns an array (possibly empty) of strings containing an hexadecimal-encoded hash of a
	/// block header.
	///
	/// # Unstable
	///
	/// This method is unstable and subject to change in the future.
	#[method(name = "archive_unstable_hashByHeight")]
	fn archive_unstable_hash_by_height(&self, height: u64) -> RpcResult<Vec<String>>;

	/// Call into the Runtime API at a specified block's state.
	///
	/// # Unstable
	///
	/// This method is unstable and subject to change in the future.
	#[method(name = "archive_unstable_call")]
	fn archive_unstable_call(
		&self,
		hash: Hash,
		function: String,
		call_parameters: String,
	) -> RpcResult<MethodResult>;

	/// Returns storage entries at a specific block's state.
	///
	/// # Unstable
	///
	/// This method is unstable and subject to change in the future.
	#[method(name = "archive_unstable_storage", blocking)]
	fn archive_unstable_storage(
		&self,
		hash: Hash,
		items: Vec<PaginatedStorageQuery<String>>,
		child_trie: Option<String>,
	) -> RpcResult<ArchiveStorageResult>;
}
