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
use sp_core::Bytes;
use sp_statement_store::{SubmitResult, TopicFilter};

pub mod error;

/// Substrate statement RPC API
#[rpc(client, server)]
pub trait StatementApi {
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
