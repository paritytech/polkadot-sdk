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

/// Filter for querying statements with different topics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TopicFilter {
	/// Matches all topics.
	Any,
	/// Matches only statements including all of the given topics.
	/// Bytes are expected to be a 32-byte topic.
	MatchAll(Vec<Bytes>),
}

/// Filter for querying statements with different decryption key identifiers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DecryptionKeyIdFilter {
	/// Matches any statement regardless of their decryption key identifier.
	Any,
	/// Match only statements without a decryption key identifier.
	NoDecryptionKey,
	/// Match only statements with the provided decryption key identifier.
	/// Bytes are expected to be a 32-byte key identifier.
	Matches(Bytes),
}

/// Filter for querying statements with different submitters.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SubmitterFilter {
	/// Matches any statement regardless of their owner identifier.
	Any,
	/// Match only statements with the provided owner identifier.
	/// Bytes are expected to be a 32-byte owner identifier.
	Matches(Bytes),
}

/// Cursor for paginated statement queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PageCursor {
	// No more pages
	End,
	// Cursor to fetch the next page, opaque to the client.
	NextPage(Bytes),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetStatementsResponse {
	/// SCALE-encoded statements matching the query.
	pub encoded_statements: Vec<Bytes>,
	/// Cursor for the next page, or `End` if there are no further pages.
	pub next_page: PageCursor,
}

/// Substrate statement RPC API
#[rpc(client, server)]
pub trait StatementApi2 {
	/// Return statements that match the provided filters, with pagination.
	///
	/// # Parameters
	///
	/// - `topic_filter` — Which topics to match. Use `TopicFilter::Any` to match all topics, or
	///   `TopicFilter::MatchAll(vec)` to match statements that include all provided topics.
	/// - `key_filter` — Filter by decryption key identifier. Use `DecryptionKeyIdFilter::Any` to
	///   ignore the decryption key, `NoDecryptionKey` to select statements without a decryption
	///   key, or `Matches(id)` to select statements with the given key id.
	/// - `submitter_filter` — Filter by statement submitter. Use `SubmitterFilter::Any` to match
	///   any owner identifier or `SubmitterFilter::Matches(owner)` to restrict to a specific owner
	///   identifier.
	/// - `next_page` — Optional pagination cursor. Pass `None` to request the first page. When a
	///   previous response contained a `PageCursor::NextPage(bytes)`, pass that wrapped in
	///   `Some(...)` to fetch the next page. The server will return `PageCursor::End` when there
	///   are no further pages.
	/// - `limit` — Optional maximum number of statements to return in this page. The server may
	///   enforce a maximum cap; if more results exist the response will include a `next_page`
	///   cursor.
	///
	/// # Returns
	///
	/// Returns `RpcResult<GetStatementsResponse>` on success.
	/// - `GetStatementsResponse.encoded_statements` contains a Vec of SCALE-encoded statements as
	/// `Bytes`.
	/// - `GetStatementsResponse.next_page` indicates whether more pages are available (an
	/// opaque cursor) or `End` if the result set is exhausted.
	#[method(name = "statement_getStatements")]
	fn get_statements(
		&self,
		topic_filter: TopicFilter,
		key_filter: DecryptionKeyIdFilter,
		submitter_filter: SubmitterFilter,
		next_page: Option<PageCursor>,
		limit: Option<u32>,
	) -> RpcResult<GetStatementsResponse>;

	/// Subscribe to new statements that match the provided filters.
	///
	/// # Parameters
	///
	/// See `get_statements` for parameter descriptions.
	///
	/// # Returns
	///
	/// Returns a stream of SCALE-encoded statements as `Bytes`.
	#[subscription(
		name = "statement_subscribeStatement" => "statement_statement",
		unsubscribe = "statement_unsubscribeStatement",
		item = Bytes,
		with_extensions,
	)]
	fn subscribe_statement(
		&self,
		topic_filter: TopicFilter,
		key_filter: DecryptionKeyIdFilter,
		submitter_filter: SubmitterFilter,
	);

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
#[deprecated(since = "0.0.0", note = "Please use StatementApi2 instead")]
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
