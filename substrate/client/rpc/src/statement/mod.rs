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

//! Substrate statement store API.

use codec::Decode;
use jsonrpsee::{
	core::{async_trait, RpcResult},
	Extensions, PendingSubscriptionSink,
};
/// Re-export the API for backward compatibility.
pub use sc_rpc_api::statement::{error::Error, StatementApiServer};
use sp_core::Bytes;
use sp_statement_store::{StatementSource, SubmitResult, TopicFilter};
use std::sync::Arc;

use crate::{
	utils::{spawn_subscription_task, BoundedVecDeque, PendingSubscription},
	SubscriptionTaskExecutor,
};

#[cfg(test)]
mod tests;

/// Trait alias for statement store API required by the RPC.
pub trait StatementStoreApi:
	sp_statement_store::StatementStore + sc_statement_store::StatementStoreSubscriptionApi
{
}
impl<T> StatementStoreApi for T where
	T: sp_statement_store::StatementStore + sc_statement_store::StatementStoreSubscriptionApi
{
}
/// Statement store API
pub struct StatementStore {
	store: Arc<dyn StatementStoreApi>,
	executor: SubscriptionTaskExecutor,
}

impl StatementStore {
	/// Create new instance of Offchain API.
	pub fn new(store: Arc<dyn StatementStoreApi>, executor: SubscriptionTaskExecutor) -> Self {
		StatementStore { store, executor }
	}
}

#[async_trait]
impl StatementApiServer for StatementStore {
	fn submit(&self, encoded: Bytes) -> RpcResult<SubmitResult> {
		let statement = Decode::decode(&mut &*encoded)
			.map_err(|e| Error::StatementStore(format!("Error decoding statement: {:?}", e)))?;
		match self.store.submit(statement, StatementSource::Local) {
			SubmitResult::InternalError(e) => Err(Error::StatementStore(e.to_string()).into()),
			// We return the result as is but `KnownExpired` should not happen. Expired statements
			// submitted with `StatementSource::Rpc` should be renewed.
			result => Ok(result),
		}
	}

	fn subscribe_statement(
		&self,
		pending: PendingSubscriptionSink,
		ext: &Extensions,
		topic_filter: TopicFilter,
	) {
		let checked_topic_filter = match topic_filter.try_into() {
			Ok(filter) => filter,
			Err(e) => {
				spawn_subscription_task(
					&self.executor,
					pending.reject(Error::StatementStore(format!(
						"Error parsing topic filter: {:?}",
						e
					))),
				);
				return;
			},
		};

		let (existing_statements, subscription_sender, subscription_stream) =
			match self.store.subscribe_statement(checked_topic_filter) {
				Ok(res) => res,
				Err(err) => {
					spawn_subscription_task(
						&self.executor,
						pending.reject(Error::StatementStore(format!(
							"Error collecting existing statements: {:?}",
							err
						))),
					);
					return;
				},
			};

		spawn_subscription_task(&self.executor, async {
			PendingSubscription::from(pending)
				.pipe_from_stream(subscription_stream, BoundedVecDeque::new(2048 * 2048))
				.await;
		});

		// Send existing statements before returning, to make sure we did not miss any statements.
		for statement in existing_statements {
			// Channel size is chosen to be large enough to always fit existing statements.
			if let Err(e) = subscription_sender.try_send(statement.into()) {
				log::warn!(
					target: "statement_store_rpc",
					"Failed to send existing statement in subscription: {:?}", e
				);
				break;
			}
		}
	}
}
