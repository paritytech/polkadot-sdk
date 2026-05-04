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

//! API trait for the bitswap RPC methods.

use crate::bitswap::error::Error;
use jsonrpsee::{core::RpcResult, proc_macros::rpc, types::ErrorObject};
use serde::{Deserialize, Serialize};

/// Per-CID outcome.
///
/// On success, a `0x`-prefixed hex string carrying the chunk data (same encoding as
/// [`bitswap_v1_get`](BitswapApiServer::bitswap_v1_get)). On failure, a JSON-RPC error code (one of
/// the [four `bitswap_v1_get`
/// categories](https://github.com/paritytech/json-rpc-interface-spec/blob/main/src/api/bitswap_v1_get.md#error-categories))
/// and a human-readable diagnostic message.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BlockResult {
	/// Hex-encoded chunk data.
	Ok(String),
	/// JSON-RPC error.
	Err {
		/// Error code identifying the retry category.
		code: i32,
		/// Human-readable diagnostic message.
		message: String,
	},
}

impl From<Error> for BlockResult {
	fn from(e: Error) -> Self {
		let obj = ErrorObject::from(e);
		BlockResult::Err { code: obj.code(), message: obj.message().to_string() }
	}
}

#[rpc(client, server)]
pub trait BitswapApi {
	/// Retrieve indexed transaction data by CID.
	///
	/// Accepts a CIDv1 (base32 multibase-encoded string), extracts the 32-byte hash
	/// digest, looks up the indexed transaction, and returns hex-encoded data.
	#[method(name = "bitswap_v1_get")]
	fn bitswap_v1_get(&self, cid: String) -> RpcResult<String>;

	/// Retrieve multiple chunks by CID in a single call.
	///
	/// Returns one [`BlockResult`] per input CID, in input order. Per-CID errors are
	/// embedded in the returned vec; only whole-call failures (`MajorSyncing`,
	/// `TooManyCids`) raise a top-level JSON-RPC error.
	#[method(name = "bitswap_v1_getMany")]
	fn bitswap_v1_get_many(&self, cids: Vec<String>) -> RpcResult<Vec<(String, BlockResult)>>;

	/// Stream chunks as they become available.
	///
	/// Emits one `(cid, BlockResult)` event per input CID, in input order. Same
	/// top-level rejection rules as `bitswap_v1_getMany`.
	#[subscription(
		name = "bitswap_v1_stream" => "bitswap_v1_streamEvent",
		unsubscribe = "bitswap_v1_unstream",
		item = (String, BlockResult),
	)]
	fn bitswap_v1_stream(&self, cids: Vec<String>);
}
