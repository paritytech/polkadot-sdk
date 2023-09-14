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
}
