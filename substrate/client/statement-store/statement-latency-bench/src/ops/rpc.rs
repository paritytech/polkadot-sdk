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

//! Abstraction over the two statement-store RPC methods used by the
//! `statement-ops-bench` subcommands.
//!
//! Production code uses [`WsClientRpc`] (jsonrpsee `WsClient`); unit tests use
//! [`MockRpc`] (in-memory, no live node).

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use codec::Encode;
use futures::{Stream, StreamExt};
use jsonrpsee::{
	core::client::{ClientT, Subscription, SubscriptionClientT},
	rpc_params,
	ws_client::{WsClient, WsClientBuilder},
};
use sp_core::Bytes;
use sp_statement_store::{Statement, StatementEvent, SubmitResult, TopicFilter};
use std::{pin::Pin, sync::Arc};

#[cfg(test)]
use std::time::Duration;

/// Stream type returned by [`StatementRpc::subscribe_topic`].
pub type EventStream = Pin<Box<dyn Stream<Item = Result<StatementEvent>> + Send>>;

/// Abstraction over the two statement-store RPC methods used by the bench.
///
/// Concrete impls:
/// * [`WsClientRpc`] — production, wraps a jsonrpsee `WsClient`.
/// * [`MockRpc`]     — tests, records submissions and serves canned subscriptions.
#[async_trait]
pub trait StatementRpc: Send + Sync {
	/// Submit a (already-built) statement to the node and return the node's
	/// [`SubmitResult`].
	async fn submit_statement(&self, statement: &Statement) -> Result<SubmitResult>;

	/// Open a statement subscription with the given filter.
	async fn subscribe_topic(&self, filter: TopicFilter) -> Result<EventStream>;
}

/// Production impl backed by a jsonrpsee `WsClient`.
pub struct WsClientRpc {
	endpoint: String,
	client: Arc<WsClient>,
}

impl WsClientRpc {
	/// Connect to `endpoint` and wrap the resulting client.
	pub async fn connect(endpoint: &str) -> Result<Self> {
		let client = WsClientBuilder::default()
			.max_concurrent_requests(10_000)
			.build(endpoint)
			.await
			.with_context(|| format!("Failed to connect to {endpoint}"))?;
		Ok(Self { endpoint: endpoint.to_string(), client: Arc::new(client) })
	}

	#[allow(dead_code)]
	pub fn endpoint(&self) -> &str {
		&self.endpoint
	}
}

#[async_trait]
impl StatementRpc for WsClientRpc {
	async fn submit_statement(&self, statement: &Statement) -> Result<SubmitResult> {
		let encoded: Bytes = statement.encode().into();
		self.client
			.request("statement_submit", rpc_params![encoded])
			.await
			.map_err(|e| anyhow!("statement_submit failed on {}: {e}", self.endpoint))
	}

	async fn subscribe_topic(&self, filter: TopicFilter) -> Result<EventStream> {
		let sub: Subscription<StatementEvent> = self
			.client
			.subscribe(
				"statement_subscribeStatement",
				rpc_params![filter],
				"statement_unsubscribeStatement",
			)
			.await
			.map_err(|e| {
				anyhow!("statement_subscribeStatement failed on {}: {e}", self.endpoint)
			})?;

		let endpoint = self.endpoint.clone();
		let stream = sub.map(move |item| match item {
			Ok(event) => Ok(event),
			Err(e) => Err(anyhow!("Subscription error on {endpoint}: {e}")),
		});
		Ok(Box::pin(stream))
	}
}

// ---------------------------------------------------------------------------
// Mock impl for tests
// ---------------------------------------------------------------------------

#[cfg(test)]
pub use mock::MockRpc;

#[cfg(test)]
mod mock {
	use super::*;
	use std::{collections::VecDeque, sync::Mutex};

	/// Mode controlling how the next `subscribe_topic` call behaves.
	#[derive(Debug, Clone)]
	pub(crate) enum SubscribePlan {
		/// Yield this pre-built sequence of events, then end the stream.
		Events(Vec<Result<StatementEvent, String>>),
		/// Yield this pre-built sequence of events, then keep the stream open
		/// without delivering anything else (for live-stream timeout tests).
		EventsThenPending(Vec<Result<StatementEvent, String>>),
		/// Yield no events and never terminate (for timeout tests).
		Pending,
		/// Return an error from `subscribe_topic` itself.
		Error(String),
	}

	#[derive(Default)]
	struct MockState {
		submitted: Vec<Statement>,
		next_submit_results: VecDeque<Result<SubmitResult, String>>,
		default_submit_result: Option<SubmitResult>,
		submit_delay: Duration,
		subscribe_plans: VecDeque<SubscribePlan>,
		subscribe_delay: Duration,
		// Used to verify pair-isolation in tests.
		captured_filters: Vec<TopicFilter>,
	}

	/// Cheap-to-clone, thread-safe mock of [`StatementRpc`].
	#[derive(Clone, Default)]
	pub struct MockRpc {
		state: Arc<Mutex<MockState>>,
	}

	impl MockRpc {
		pub fn new() -> Self {
			Self::default()
		}

		// ---- configuration --------------------------------------------------

		pub fn set_default_submit_result(&self, r: SubmitResult) {
			self.state.lock().unwrap().default_submit_result = Some(r);
		}

		pub fn set_submit_delay(&self, d: Duration) {
			self.state.lock().unwrap().submit_delay = d;
		}

		pub fn push_submit_result(&self, r: Result<SubmitResult, String>) {
			self.state.lock().unwrap().next_submit_results.push_back(r);
		}

		#[allow(dead_code)]
		pub fn set_subscribe_delay(&self, d: Duration) {
			self.state.lock().unwrap().subscribe_delay = d;
		}

		pub fn push_subscribe_events(&self, events: Vec<Result<StatementEvent, String>>) {
			self.state
				.lock()
				.unwrap()
				.subscribe_plans
				.push_back(SubscribePlan::Events(events));
		}

		pub fn push_subscribe_events_then_pending(
			&self,
			events: Vec<Result<StatementEvent, String>>,
		) {
			self.state
				.lock()
				.unwrap()
				.subscribe_plans
				.push_back(SubscribePlan::EventsThenPending(events));
		}

		pub fn push_subscribe_pending(&self) {
			self.state.lock().unwrap().subscribe_plans.push_back(SubscribePlan::Pending);
		}

		pub fn push_subscribe_error(&self, msg: &str) {
			self.state
				.lock()
				.unwrap()
				.subscribe_plans
				.push_back(SubscribePlan::Error(msg.to_string()));
		}

		// ---- inspection -----------------------------------------------------

		pub fn submitted(&self) -> Vec<Statement> {
			self.state.lock().unwrap().submitted.clone()
		}

		pub fn submit_count(&self) -> usize {
			self.state.lock().unwrap().submitted.len()
		}

		pub fn captured_filters(&self) -> Vec<TopicFilter> {
			self.state.lock().unwrap().captured_filters.clone()
		}
	}

	#[async_trait]
	impl StatementRpc for MockRpc {
		async fn submit_statement(&self, statement: &Statement) -> Result<SubmitResult> {
			let (delay, popped, default) = {
				let mut state = self.state.lock().unwrap();
				let delay = state.submit_delay;
				let popped = state.next_submit_results.pop_front();
				let default = state.default_submit_result.clone();
				state.submitted.push(statement.clone());
				(delay, popped, default)
			};
			if !delay.is_zero() {
				tokio::time::sleep(delay).await;
			}
			match popped {
				Some(Ok(r)) => Ok(r),
				Some(Err(msg)) => Err(anyhow!("MockRpc::submit_statement error: {msg}")),
				None => Ok(default.unwrap_or(SubmitResult::New)),
			}
		}

		async fn subscribe_topic(&self, filter: TopicFilter) -> Result<EventStream> {
			let (delay, plan) = {
				let mut state = self.state.lock().unwrap();
				state.captured_filters.push(filter);
				let delay = state.subscribe_delay;
				let plan =
					state.subscribe_plans.pop_front().unwrap_or(SubscribePlan::Events(vec![]));
				(delay, plan)
			};
			if !delay.is_zero() {
				tokio::time::sleep(delay).await;
			}
			match plan {
				SubscribePlan::Error(msg) => Err(anyhow!("MockRpc::subscribe_topic error: {msg}")),
				SubscribePlan::Pending => Ok(Box::pin(futures::stream::pending())),
				SubscribePlan::Events(events) => {
					let mapped = mapped_events(events);
					Ok(Box::pin(futures::stream::iter(mapped)))
				},
				SubscribePlan::EventsThenPending(events) => {
					let mapped = mapped_events(events);
					let initial = futures::stream::iter(mapped);
					let pending = futures::stream::pending();
					Ok(Box::pin(futures::StreamExt::chain(initial, pending)))
				},
			}
		}
	}

	fn mapped_events(
		events: Vec<Result<StatementEvent, String>>,
	) -> impl Iterator<Item = Result<StatementEvent>> {
		events.into_iter().map(|r| match r {
			Ok(ev) => Ok(ev),
			Err(msg) => Err(anyhow!("MockRpc subscription event error: {msg}")),
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_core::ConstU32;
	use sp_statement_store::{Topic, TopicFilter};

	fn empty_filter() -> TopicFilter {
		let topics: sp_core::bounded_vec::BoundedVec<Topic, ConstU32<128>> = Default::default();
		TopicFilter::MatchAny(topics)
	}

	fn ev_empty() -> StatementEvent {
		StatementEvent::NewStatements { statements: vec![], remaining: Some(0) }
	}

	fn build_dummy_statement() -> Statement {
		let kp = sc_statement_store::test_utils::get_keypair(0);
		crate::ops::common::build_statement(&kp, [1u8; 32], [2u8; 32], 1_000, 0, vec![0u8; 8])
	}

	#[tokio::test]
	async fn mock_submit_records_and_returns_default() {
		let m = MockRpc::new();
		let s = build_dummy_statement();
		let r = m.submit_statement(&s).await.unwrap();
		assert!(matches!(r, SubmitResult::New));
		assert_eq!(m.submit_count(), 1);
	}

	#[tokio::test]
	async fn mock_submit_pops_queued_results_in_order() {
		let m = MockRpc::new();
		m.push_submit_result(Ok(SubmitResult::New));
		m.push_submit_result(Ok(SubmitResult::Known));
		let s = build_dummy_statement();
		let r1 = m.submit_statement(&s).await.unwrap();
		let r2 = m.submit_statement(&s).await.unwrap();
		assert!(matches!(r1, SubmitResult::New));
		assert!(matches!(r2, SubmitResult::Known));
	}

	#[tokio::test]
	async fn mock_submit_returns_error_when_queued() {
		let m = MockRpc::new();
		m.push_submit_result(Err("boom".into()));
		let s = build_dummy_statement();
		assert!(m.submit_statement(&s).await.is_err());
	}

	#[tokio::test]
	async fn mock_subscribe_returns_queued_events_in_order() {
		let m = MockRpc::new();
		m.push_subscribe_events(vec![Ok(ev_empty())]);
		let mut stream = m.subscribe_topic(empty_filter()).await.unwrap();
		let first = stream.next().await.unwrap().unwrap();
		match first {
			StatementEvent::NewStatements { remaining, .. } => assert_eq!(remaining, Some(0)),
		}
		assert!(stream.next().await.is_none());
	}

	#[tokio::test]
	async fn mock_subscribe_default_yields_empty_stream() {
		let m = MockRpc::new();
		let mut stream = m.subscribe_topic(empty_filter()).await.unwrap();
		assert!(stream.next().await.is_none());
	}

	#[tokio::test]
	async fn mock_subscribe_pending_never_yields() {
		let m = MockRpc::new();
		m.push_subscribe_pending();
		let mut stream = m.subscribe_topic(empty_filter()).await.unwrap();
		let poll = tokio::time::timeout(Duration::from_millis(20), stream.next()).await;
		assert!(poll.is_err(), "pending subscription should not yield");
	}

	#[tokio::test]
	async fn mock_subscribe_error_propagates() {
		let m = MockRpc::new();
		m.push_subscribe_error("no good");
		assert!(m.subscribe_topic(empty_filter()).await.is_err());
	}

	#[tokio::test]
	async fn mock_subscribe_records_filters() {
		let m = MockRpc::new();
		let f1 = empty_filter();
		let f2 = empty_filter();
		let _ = m.subscribe_topic(f1).await.unwrap();
		let _ = m.subscribe_topic(f2).await.unwrap();
		assert_eq!(m.captured_filters().len(), 2);
	}
}
