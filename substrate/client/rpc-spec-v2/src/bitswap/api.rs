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

use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use serde::{Deserialize, Serialize};

/// Per-event payload emitted by [`BitswapApiServer::bitswap_unstable_stream`].
///
/// Wire shape is a tagged object distinguished by the `event` field, per the spec:
/// `{ "event": "streamItem", "cid": "...", "value": "0x..." }`,
/// `{ "event": "streamItemError", "cid": "...", "code": ..., "message": "..." }`,
/// `{ "event": "streamDone" }`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum StreamEvent {
	/// Successful retrieval of a single CID.
	StreamItem {
		/// The input CID string, returned verbatim.
		cid: String,
		/// Hex-encoded chunk data, always starting with `0x...`.
		value: String,
	},
	/// Failed retrieval of a single CID. The stream continues with the remaining CIDs.
	StreamItemError {
		/// The input CID string, returned verbatim.
		cid: String,
		/// JSON-RPC error code identifying the retry category.
		code: i32,
		/// Human-readable diagnostic message. Implementation-specific; do not rely on in business
		/// logic.
		message: String,
	},
	/// End-of-stream marker. Emitted exactly once after a per-CID event has been emitted for every
	/// input CID. Not emitted on client cancellation or disconnect.
	StreamDone,
}

#[rpc(client, server)]
pub trait BitswapApi {
	/// Retrieve indexed transaction data by CID.
	///
	/// Accepts a CIDv1 (base32 multibase-encoded string), extracts the 32-byte hash
	/// digest, looks up the indexed transaction, and returns hex-encoded data.
	///
	/// `bitswap_v1_get` is kept as an alias during the transition to the `unstable_` naming and
	/// will be removed once clients migrate.
	#[method(name = "bitswap_unstable_get", aliases = ["bitswap_v1_get"])]
	fn bitswap_unstable_get(&self, cid: String) -> RpcResult<String>;

	/// Stream chunks as they become available.
	///
	/// Emits one [`StreamEvent::StreamItem`] / [`StreamEvent::StreamItemError`] event per input
	/// CID, in arrival order (the order in which each CID resolves) — **not** input order.
	/// Clients correlate each event with its request via the embedded `cid`. After the last
	/// per-CID event, a single [`StreamEvent::StreamDone`] marks end-of-stream. No
	/// `streamDone` is emitted if the client cancels via the unsubscribe method or disconnects.
	///
	/// Top-level rejections (no events emitted, no subscription opened):
	/// `-32801 TooManyCids`, `-32802 EmptyCids`, `-32803 DuplicateCids`.
	#[subscription(
		name = "bitswap_unstable_stream" => "bitswap_unstable_streamEvent",
		unsubscribe = "bitswap_unstable_unstream",
		item = StreamEvent,
	)]
	fn bitswap_unstable_stream(&self, cids: Vec<String>);
}
