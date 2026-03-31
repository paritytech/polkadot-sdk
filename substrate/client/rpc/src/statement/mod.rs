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
use futures::FutureExt;
use jsonrpsee::{
	core::{async_trait, RpcResult},
	Extensions, PendingSubscriptionSink,
};
/// Re-export the API for backward compatibility.
pub use sc_rpc_api::statement::{error::Error, StatementApiServer};
use sp_core::Bytes;
use sp_statement_store::{
	OptimizedTopicFilter, StatementEvent, StatementSource, SubmitResult, TopicFilter,
};
use std::sync::Arc;
const LOG_TARGET: &str = "statement-store-rpc";
// The maximum size of a chunk of statements to send in a single JSON response. This is needed to
// avoid hitting the maximum JSON size limit in the RPC response. Each statement is SCALE-encoded
// and then hex-encoded in the JSON response, so the size of the JSON response is approximately 2x.
// This value is chosen to be large enough to send a reasonable number of statements in a single
// chunk, but small enough to avoid hitting the JSON size limit.
const MAX_CHUNK_BYTES_LIMIT: usize = 4 * 1024 * 1024;

use crate::{
	utils::{spawn_subscription_task, BoundedVecDeque, PendingSubscription},
	SubscriptionTaskExecutor,
};

mod metrics;

#[cfg(test)]
mod tests;

/// Send existing statements in chunks over the subscription channel.
///
/// Splits the statements into chunks that fit within [`MAX_CHUNK_BYTES_LIMIT`] to avoid
/// exceeding the RPC max response size, then sends each chunk as a
/// [`StatementEvent::NewStatements`].
async fn send_in_chunks(
	existing_statements: Vec<Vec<u8>>,
	subscription_sender: async_channel::Sender<StatementEvent>,
) {
	let mut iter = existing_statements.into_iter().peekable();
	loop {
		let mut chunk = Vec::<Bytes>::new();
		let mut chunk_json_size = 0usize;
		while let Some(statement) = iter.peek() {
			// Each SCALE-encoded byte becomes 2 hex chars in the JSON response
			let json_size_estimate = statement.len() * 2;
			// If a single statement exceeds the max chunk size, skip it but continue sending the
			// rest of the statements. This would never happen in practice because the statement
			// store should reject statements that are too large, but we add this check to be safe.
			if json_size_estimate > MAX_CHUNK_BYTES_LIMIT {
				iter.next();
				continue;
			}
			if chunk_json_size + json_size_estimate > MAX_CHUNK_BYTES_LIMIT {
				break;
			}
			let Some(statement) = iter.next() else { break };
			chunk_json_size += json_size_estimate;
			chunk.push(statement.into());
		}
		if chunk.is_empty() {
			break;
		}
		let remaining = iter.len();
		if let Err(e) = subscription_sender
			.send(StatementEvent::NewStatements {
				statements: chunk,
				remaining: Some(remaining as u32),
			})
			.await
		{
			log::warn!(
				target: LOG_TARGET,
				"Failed to send existing statement in subscription: {:?}", e
			);
			break;
		}
	}
}

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
	metrics: metrics::MetricsLink,
}

impl StatementStore {
	/// Create new instance of Offchain API.
	pub fn new(
		store: Arc<dyn StatementStoreApi>,
		executor: SubscriptionTaskExecutor,
		prometheus_registry: Option<&prometheus_endpoint::Registry>,
	) -> Self {
		StatementStore {
			store,
			executor,
			metrics: metrics::MetricsLink::new(prometheus_registry),
		}
	}
}

#[async_trait]
impl StatementApiServer for StatementStore {
	fn submit(&self, encoded: Bytes) -> RpcResult<SubmitResult> {
		let _timer = self
			.metrics
			.0
			.as_ref()
			.as_ref()
			.map(|m| m.submit_duration_seconds.start_timer());

		let statement = match Decode::decode(&mut &*encoded) {
			Ok(s) => s,
			Err(e) => {
				self.metrics.report(|m| {
					m.submit_calls_total.with_label_values(&["decode_error"]).inc();
				});
				return Err(
					Error::StatementStore(format!("Error decoding statement: {:?}", e)).into()
				);
			},
		};

		let result = self.store.submit(statement, StatementSource::Local);

		let label = match &result {
			SubmitResult::New => "new",
			SubmitResult::Known => "known",
			SubmitResult::KnownExpired => "known_expired",
			SubmitResult::Rejected(_) => "rejected",
			SubmitResult::Invalid(_) => "invalid",
			SubmitResult::InternalError(_) => "internal_error",
		};
		self.metrics.report(|m| {
			m.submit_calls_total.with_label_values(&[label]).inc();
		});

		match result {
			SubmitResult::InternalError(e) => Err(Error::StatementStore(e.to_string()).into()),
			// We return the result as is but `KnownExpired` should not happen. Expired statements
			// submitted with `StatementSource::Rpc` should be renewed.
			result => Ok(result),
		}
	}

	fn subscribe_statement(
		&self,
		pending: PendingSubscriptionSink,
		_ext: &Extensions,
		topic_filter: TopicFilter,
	) {
		let optimized_topic_filter: OptimizedTopicFilter = topic_filter.into();

		let (existing_statements, subscription_sender, subscription_stream) =
			match self.store.subscribe_statement(optimized_topic_filter) {
				Ok(res) => {
					self.metrics.report(|m| {
						m.subscribe_calls_total.with_label_values(&["ok"]).inc();
						m.active_subscriptions.inc();
					});
					res
				},
				Err(err) => {
					self.metrics.report(|m| {
						m.subscribe_calls_total.with_label_values(&["error"]).inc();
					});
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

		let metrics = self.metrics.clone();
		let subscription_fut = async move {
			PendingSubscription::from(pending)
				.pipe_from_stream(subscription_stream, BoundedVecDeque::new(128))
				.await;
			metrics.report(|m| {
				m.active_subscriptions.dec();
			});
		};

		spawn_subscription_task(&self.executor, subscription_fut);

		self.executor.spawn(
			"statement-store-rpc-send",
			Some("rpc"),
			send_in_chunks(existing_statements, subscription_sender).boxed(),
		)
	}
}
