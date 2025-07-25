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
	archive::{
		error::{Error, Infallible},
		types::MethodResult,
	},
	common::events::{
		ArchiveStorageDiffEvent, ArchiveStorageDiffItem, ArchiveStorageEvent, StorageQuery,
	},
};
use jsonrpsee::proc_macros::rpc;

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
	#[method(name = "archive_v1_body")]
	fn archive_v1_body(&self, hash: Hash) -> Result<Option<Vec<String>>, Infallible>;

	/// Get the chain's genesis hash.
	///
	/// Returns a string containing the hexadecimal-encoded hash of the genesis block of the chain.
	///
	/// # Unstable
	///
	/// This method is unstable and subject to change in the future.
	#[method(name = "archive_v1_genesisHash")]
	fn archive_v1_genesis_hash(&self) -> Result<String, Infallible>;

	/// Get the block's header.
	///
	/// Returns a string containing the hexadecimal-encoded SCALE-codec encoding header of the
	/// block.
	///
	/// # Unstable
	///
	/// This method is unstable and subject to change in the future.
	#[method(name = "archive_v1_header")]
	fn archive_v1_header(&self, hash: Hash) -> Result<Option<String>, Infallible>;

	/// Get the height of the current finalized block.
	///
	/// Returns an integer height of the current finalized block of the chain.
	///
	/// # Unstable
	///
	/// This method is unstable and subject to change in the future.
	#[method(name = "archive_v1_finalizedHeight")]
	fn archive_v1_finalized_height(&self) -> Result<u64, Infallible>;

	/// Get the hashes of blocks from the given height.
	///
	/// Returns an array (possibly empty) of strings containing an hexadecimal-encoded hash of a
	/// block header.
	///
	/// # Unstable
	///
	/// This method is unstable and subject to change in the future.
	#[method(name = "archive_v1_hashByHeight")]
	fn archive_v1_hash_by_height(&self, height: u64) -> Result<Vec<String>, Error>;

	/// Call into the Runtime API at a specified block's state.
	///
	/// # Unstable
	///
	/// This method is unstable and subject to change in the future.
	#[method(name = "archive_v1_call")]
	fn archive_v1_call(
		&self,
		hash: Hash,
		function: String,
		call_parameters: String,
	) -> Result<MethodResult, Error>;

	/// Returns storage entries at a specific block's state.
	///
	/// # Unstable
	///
	/// This method is unstable and subject to change in the future.
	#[subscription(
		name = "archive_v1_storage" => "archive_v1_storageEvent",
		unsubscribe = "archive_v1_stopStorage",
		item = ArchiveStorageEvent,
	)]
	fn archive_v1_storage(
		&self,
		hash: Hash,
		items: Vec<StorageQuery<String>>,
		child_trie: Option<String>,
	);

	/// Returns the storage difference between two blocks.
	///
	/// # Unstable
	///
	/// This method is unstable and can change in minor or patch releases.
	#[subscription(
		name = "archive_v1_storageDiff" => "archive_v1_storageDiffEvent",
		unsubscribe = "archive_v1_storageDiff_stopStorageDiff",
		item = ArchiveStorageDiffEvent,
	)]
	fn archive_v1_storage_diff(
		&self,
		hash: Hash,
		items: Vec<ArchiveStorageDiffItem<String>>,
		previous_hash: Option<Hash>,
	);
}
