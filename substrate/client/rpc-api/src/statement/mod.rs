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

pub mod error;

/// Result of submitting a statement to the store.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum StatementSubmitResult {
	/// Statement was accepted as new.
	New,
	/// Statement was already known.
	Known,
	/// Statement was already known but has expired.
	KnownExpired,
	/// Statement was rejected because the store is full or priority is too low.
	Rejected(RejectionReason),
	/// Statement failed validation.
	Invalid(InvalidReason),
}

/// Reason why a statement failed validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "camelCase")]
pub enum InvalidReason {
	/// Statement was not accompanied by a cryptographic proof.
	NoProof,
	/// Cryptographic proof validation failed.
	BadProof,
	/// Statement encoding exceeds the maximum allowed size for network propagation.
	EncodingTooLarge {
		/// The size of the submitted statement encoding.
		submitted_size: usize,
		/// The maximum allowed size.
		max_size: usize,
	},
}

/// Reason why a statement was rejected from the store.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "camelCase")]
pub enum RejectionReason {
	/// Statement data exceeds the maximum allowed size for the account.
	DataTooLarge {
		/// The size of the submitted statement data.
		submitted_size: usize,
		/// Still available data size for the account.
		available_size: usize,
	},
	/// Attempting to replace a channel message with lower or equal priority.
	ChannelPriorityTooLow {
		/// The priority of the submitted statement.
		submitted_priority: u32,
		/// The minimum priority needed to replace the existing channel message.
		min_priority: u32,
	},
	/// Account has reached its statement limit and submitted priority is too low to evict existing
	/// statements.
	AccountFull {
		/// The priority of the submitted statement.
		submitted_priority: u32,
		/// The minimum priority needed to evict an existing statement.
		min_priority: u32,
	},
	/// The global statement store is full and cannot accept new statements.
	StoreFull,
}

/// Substrate statement RPC API
#[rpc(client, server)]
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
	fn submit(&self, encoded: Bytes) -> RpcResult<StatementSubmitResult>;

	/// Remove a statement from the store.
	#[method(name = "statement_remove")]
	fn remove(&self, statement_hash: [u8; 32]) -> RpcResult<()>;
}
