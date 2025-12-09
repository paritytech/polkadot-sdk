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

//! Substrate Statement Store RPC API.

use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use serde::{Deserialize, Serialize};
use sp_core::Bytes;
use sp_statement_store::SubmitResult;

pub mod error;

/// Filter for subscribing to statements with different topics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TopicFilter {
	/// Matches all topics.
	Any,
	/// Matches only statements including all of the given topics.
	/// Bytes are expected to be a 32-byte topic. Up to `4` topics can be provided.
	MatchAll(Vec<Bytes>),
	/// Matches statements including any of the given topics.
	/// Bytes are expected to be a 32-byte topic. Up to `128` topics can be provided.
	MatchAny(Vec<Bytes>),
}

/// Substrate statement RPC API
#[rpc(client, server)]
pub trait StatementApi2 {
	/// Subscribe to new statements that match the provided filters.
	///
	/// # Parameters
	///
	/// - `topic_filter` — Which topics to match. Use `TopicFilter::Any` to match all topics,
	///   `TopicFilter::MatchAll(vec)` to match statements that include all provided topics, or
	///   `TopicFilter::MatchAny(vec)` to match statements that include any of the provided topics.
	///
	/// # Returns
	///
	/// Returns a stream of SCALE-encoded statements as `Bytes`.
	/// When a subscription is initiated the endpoint will immediately return the matching
	/// statements already in the store. Subsequent matching statements will be pushed to the client
	/// as they are added to the store.
	#[subscription(
		name = "statement_subscribeStatement" => "statement_statement",
		unsubscribe = "statement_unsubscribeStatement",
		item = Bytes,
		with_extensions,
	)]
	fn subscribe_statement(&self, topic_filter: TopicFilter);

	/// Submit a SCALE-encoded statement.
	///
	/// See `Statement` definition for more details.
	///
	/// Returns `SubmitResult` indicating success or failure reason.
	#[method(name = "statement_submit")]
	fn submit(&self, encoded: Bytes) -> RpcResult<SubmitResult>;
}

/// Substrate statement RPC API
#[rpc(client, server)]
#[deprecated(since = "0.0.0", note = "Please use StatementApi2 instead, will be removed soon.")]
pub trait StatementApi {
	/// Return all statements, SCALE-encoded.
	#[method(name = "statement_dump", with_extensions)]
	fn dump(&self) -> RpcResult<Vec<Bytes>>;

	/// Return the data of all known statements which include all topics and have no `DecryptionKey`
	/// field.
	///
	/// To get the statement, and not just the data, use `statement_broadcastsStatement`.
	#[method(name = "statement_broadcasts")]
	fn broadcasts(&self, match_all_topics: Vec<[u8; 32]>) -> RpcResult<Vec<Bytes>>;

	/// Return the data of all known statements whose decryption key is identified as `dest` (this
	/// will generally be the public key or a hash thereof for symmetric ciphers, or a hash of the
	/// private key for symmetric ciphers).
	///
	/// To get the statement, and not just the data, use `statement_postedStatement`.
	#[method(name = "statement_posted")]
	fn posted(&self, match_all_topics: Vec<[u8; 32]>, dest: [u8; 32]) -> RpcResult<Vec<Bytes>>;

	/// Return the decrypted data of all known statements whose decryption key is identified as
	/// `dest`. The key must be available to the client.
	///
	/// To get the statement, and not just the data, use `statement_postedClearStatement`.
	#[method(name = "statement_postedClear")]
	fn posted_clear(
		&self,
		match_all_topics: Vec<[u8; 32]>,
		dest: [u8; 32],
	) -> RpcResult<Vec<Bytes>>;

	/// Return all known statements which include all topics and have no `DecryptionKey`
	/// field.
	///
	/// This returns the SCALE-encoded statements not just the data as in rpc
	/// `statement_broadcasts`.
	#[method(name = "statement_broadcastsStatement")]
	fn broadcasts_stmt(&self, match_all_topics: Vec<[u8; 32]>) -> RpcResult<Vec<Bytes>>;

	/// Return all known statements whose decryption key is identified as `dest` (this
	/// will generally be the public key or a hash thereof for symmetric ciphers, or a hash of the
	/// private key for symmetric ciphers).
	///
	/// This returns the SCALE-encoded statements not just the data as in rpc `statement_posted`.
	#[method(name = "statement_postedStatement")]
	fn posted_stmt(&self, match_all_topics: Vec<[u8; 32]>, dest: [u8; 32])
		-> RpcResult<Vec<Bytes>>;

	/// Return the statement and the decrypted data of all known statements whose decryption key is
	/// identified as `dest`. The key must be available to the client.
	///
	/// This returns for each statement: the SCALE-encoded statement concatenated to the decrypted
	/// data. Not just the data as in rpc `statement_postedClear`.
	#[method(name = "statement_postedClearStatement")]
	fn posted_clear_stmt(
		&self,
		match_all_topics: Vec<[u8; 32]>,
		dest: [u8; 32],
	) -> RpcResult<Vec<Bytes>>;

	/// Submit a pre-encoded statement.
	#[method(name = "statement_submit")]
	fn submit(&self, encoded: Bytes) -> RpcResult<SubmitResult>;

	/// Remove a statement from the store.
	#[method(name = "statement_remove")]
	fn remove(&self, statement_hash: [u8; 32]) -> RpcResult<()>;
}
