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

#![allow(non_snake_case)]

//! API trait of the chain head.
use crate::{
	chain_head::{
		error::Error,
		event::{FollowEvent, MethodResponse},
	},
	common::events::StorageQuery,
};
use jsonrpsee::{proc_macros::rpc, server::ResponsePayload};
pub use sp_rpc::list::ListOrValue;

#[rpc(client, server)]
pub trait ChainHeadApi<Hash> {
	/// Track the state of the head of the chain: the finalized, non-finalized, and best blocks.
	///
	/// # Unstable
	///
	/// This method is unstable and subject to change in the future.
	#[subscription(
		name = "chainHead_unstable_follow" => "chainHead_unstable_followEvent",
		unsubscribe = "chainHead_unstable_unfollow",
		item = FollowEvent<Hash>,
	)]
	fn chain_head_unstable_follow(&self, with_runtime: bool);

	/// Retrieves the body (list of transactions) of a pinned block.
	///
	/// This method should be seen as a complement to `chainHead_unstable_follow`,
	/// allowing the JSON-RPC client to retrieve more information about a block
	/// that has been reported.
	///
	/// Use `archive_unstable_body` if instead you want to retrieve the body of an arbitrary block.
	///
	/// # Unstable
	///
	/// This method is unstable and subject to change in the future.
	#[method(name = "chainHead_unstable_body", raw_method)]
	async fn chain_head_unstable_body(
		&self,
		follow_subscription: String,
		hash: Hash,
	) -> ResponsePayload<'static, MethodResponse>;

	/// Retrieves the header of a pinned block.
	///
	/// This method should be seen as a complement to `chainHead_unstable_follow`,
	/// allowing the JSON-RPC client to retrieve more information about a block
	/// that has been reported.
	///
	/// Use `archive_unstable_header` if instead you want to retrieve the header of an arbitrary
	/// block.
	///
	/// # Unstable
	///
	/// This method is unstable and subject to change in the future.
	#[method(name = "chainHead_unstable_header", raw_method)]
	async fn chain_head_unstable_header(
		&self,
		follow_subscription: String,
		hash: Hash,
	) -> Result<Option<String>, Error>;

	/// Returns storage entries at a specific block's state.
	///
	/// # Unstable
	///
	/// This method is unstable and subject to change in the future.
	#[method(name = "chainHead_unstable_storage", raw_method)]
	async fn chain_head_unstable_storage(
		&self,
		follow_subscription: String,
		hash: Hash,
		items: Vec<StorageQuery<String>>,
		child_trie: Option<String>,
	) -> ResponsePayload<'static, MethodResponse>;

	/// Call into the Runtime API at a specified block's state.
	///
	/// # Unstable
	///
	/// This method is unstable and subject to change in the future.
	#[method(name = "chainHead_unstable_call", raw_method)]
	async fn chain_head_unstable_call(
		&self,
		follow_subscription: String,
		hash: Hash,
		function: String,
		call_parameters: String,
	) -> ResponsePayload<'static, MethodResponse>;

	/// Unpin a block or multiple blocks reported by the `follow` method.
	///
	/// Ongoing operations that require the provided block
	/// will continue normally.
	///
	/// When this method returns an error, it is guaranteed that no blocks have been unpinned.
	///
	/// # Unstable
	///
	/// This method is unstable and subject to change in the future.
	#[method(name = "chainHead_unstable_unpin", raw_method)]
	async fn chain_head_unstable_unpin(
		&self,
		follow_subscription: String,
		hash_or_hashes: ListOrValue<Hash>,
	) -> Result<(), Error>;

	/// Resumes a storage fetch started with `chainHead_storage` after it has generated an
	/// `operationWaitingForContinue` event.
	///
	/// # Unstable
	///
	/// This method is unstable and subject to change in the future.
	#[method(name = "chainHead_unstable_continue", raw_method)]
	async fn chain_head_unstable_continue(
		&self,
		follow_subscription: String,
		operation_id: String,
	) -> Result<(), Error>;

	/// Stops an operation started with chainHead_unstable_body, chainHead_unstable_call, or
	/// chainHead_unstable_storage. If the operation was still in progress, this interrupts it. If
	/// the operation was already finished, this call has no effect.
	///
	/// # Unstable
	///
	/// This method is unstable and subject to change in the future.
	#[method(name = "chainHead_unstable_stopOperation", raw_method)]
	async fn chain_head_unstable_stop_operation(
		&self,
		follow_subscription: String,
		operation_id: String,
	) -> Result<(), Error>;
}
