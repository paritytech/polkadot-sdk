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

//! API implementation for submitting transactions.

use crate::{
	transaction::{
		api::TransactionApiServer,
		error::Error,
		event::{TransactionBlock, TransactionDropped, TransactionError, TransactionEvent},
	},
	SubscriptionTaskExecutor,
};

use codec::Decode;
use futures::{StreamExt, TryFutureExt};
use jsonrpsee::{core::async_trait, PendingSubscriptionSink};
use parking_lot::Mutex;
use prometheus_endpoint::{
	register, CounterVec, HistogramOpts, HistogramVec, Opts, PrometheusError, Registry, U64,
};
use sc_rpc::utils::{RingBuffer, Subscription};
use sc_transaction_pool_api::{
	error::IntoPoolError, BlockHash, TransactionFor, TransactionPool, TransactionSource,
	TransactionStatus,
};
use sp_blockchain::HeaderBackend;
use sp_core::Bytes;
use sp_runtime::traits::Block as BlockT;
use std::{sync::Arc, time::Instant};

pub(crate) const LOG_TARGET: &str = "rpc-spec-v2";

/// Histogram time buckets in microseconds.
const HISTOGRAM_BUCKETS: [f64; 11] = [
	5.0,
	25.0,
	100.0,
	500.0,
	1_000.0,
	2_500.0,
	10_000.0,
	25_000.0,
	100_000.0,
	1_000_000.0,
	10_000_000.0,
];

/// RPC layer metrics for transaction pool.
#[derive(Clone)]
struct Metrics {
	/// Counter for transaction status.
	status: CounterVec<U64>,

	/// Histogram for transaction execution time in each event.
	execution_time: HistogramVec,
}

struct ExecutionState {
	/// The time when the transaction entered this state.
	started_at: Instant,
	/// The initial state.
	initial_state: &'static str,
}

impl ExecutionState {
	/// Creates a new [`ExecutionState`].
	fn new() -> Self {
		Self { started_at: Instant::now(), initial_state: LABEL_INITIAL }
	}

	/// Advance the state of the transaction.
	fn advance_state(&mut self, state: &'static str) {
		self.initial_state = state;
		self.started_at = Instant::now();
	}
}

const LABEL_SUBMITTED: &str = "submitted";
const LABEL_FINALIZED: &str = "finalized";
const LABEL_DROPPED: &str = "dropped";
const LABEL_INVALID: &str = "invalid";

const LABEL_INITIAL: &str = "initial";
const LABEL_VALIDATED: &str = "validated";
const LABEL_IN_BLOCK: &str = "in_block";
const LABEL_RETRACTED: &str = "retracted";

impl Metrics {
	/// Creates a new [`Metrics`] instance.
	fn new(registry: &Registry) -> Result<Self, PrometheusError> {
		let status = register(
			CounterVec::new(
				Opts::new("rpc_transaction_status", "Number of transactions by status"),
				&["state"],
			)?,
			registry,
		)?;

		let execution_time = register(
			HistogramVec::new(
				HistogramOpts::new(
					"rpc_transaction_execution_time",
					"Transaction execution time in each event",
				)
				.buckets(HISTOGRAM_BUCKETS.to_vec()),
				&["initial_state", "final_state"],
			)?,
			registry,
		)?;

		Ok(Metrics { status, execution_time })
	}

	/// Record the execution time of a transaction state.
	///
	/// This represents how long it took for the transaction to move to the next state.
	fn publish_and_advance_state(&self, state: &mut ExecutionState, final_state: &'static str) {
		let elapsed = state.started_at.elapsed().as_micros() as f64;
		self.execution_time
			.with_label_values(&[state.initial_state, final_state])
			.observe(elapsed);

		state.advance_state(final_state);
	}
}

/// An API for transaction RPC calls.
pub struct Transaction<Pool, Client> {
	/// Substrate client.
	client: Arc<Client>,
	/// Transactions pool.
	pool: Arc<Pool>,
	/// Executor to spawn subscriptions.
	executor: SubscriptionTaskExecutor,
	/// Metrics for transactions.
	metrics: Option<Metrics>,
}

impl<Pool, Client> Transaction<Pool, Client> {
	/// Creates a new [`Transaction`].
	pub fn new(
		client: Arc<Client>,
		pool: Arc<Pool>,
		executor: SubscriptionTaskExecutor,
		registry: Option<&Registry>,
	) -> Result<Self, PrometheusError> {
		let metrics =
			if let Some(registry) = registry { Some(Metrics::new(registry)?) } else { None };

		Ok(Transaction { client, pool, executor, metrics })
	}
}

/// Currently we treat all RPC transactions as externals.
///
/// Possibly in the future we could allow opt-in for special treatment
/// of such transactions, so that the block authors can inject
/// some unique transactions via RPC and have them included in the pool.
const TX_SOURCE: TransactionSource = TransactionSource::External;

#[async_trait]
impl<Pool, Client> TransactionApiServer<BlockHash<Pool>> for Transaction<Pool, Client>
where
	Pool: TransactionPool + Sync + Send + 'static,
	Pool::Hash: Unpin,
	<Pool::Block as BlockT>::Hash: Unpin,
	Client: HeaderBackend<Pool::Block> + Send + Sync + 'static,
{
	fn submit_and_watch(&self, pending: PendingSubscriptionSink, xt: Bytes) {
		let client = self.client.clone();
		let pool = self.pool.clone();
		let metrics = self.metrics.clone();

		let fut = async move {
			let metrics = &metrics;
			if let Some(metrics) = metrics {
				metrics.status.with_label_values(&[LABEL_SUBMITTED]).inc();
			}

			let decoded_extrinsic = match TransactionFor::<Pool>::decode(&mut &xt[..]) {
				Ok(decoded_extrinsic) => decoded_extrinsic,
				Err(e) => {
					log::debug!(target: LOG_TARGET, "Extrinsic bytes cannot be decoded: {:?}", e);

					let Ok(sink) = pending.accept().await.map(Subscription::from) else { return };

					if let Some(metrics) = metrics {
						metrics.status.with_label_values(&[LABEL_INVALID]).inc();
					}

					// The transaction is invalid.
					let _ = sink
						.send(&TransactionEvent::Invalid::<BlockHash<Pool>>(TransactionError {
							error: "Extrinsic bytes cannot be decoded".into(),
						}))
						.await;
					return
				},
			};

			let best_block_hash = client.info().best_hash;

			let submit = pool
				.submit_and_watch(best_block_hash, TX_SOURCE, decoded_extrinsic)
				.map_err(|e| {
					e.into_pool_error()
						.map(Error::from)
						.unwrap_or_else(|e| Error::Verification(Box::new(e)))
				});

			let Ok(sink) = pending.accept().await.map(Subscription::from) else {
				return;
			};

			let execution_state = Arc::new(Mutex::new(ExecutionState::new()));

			match submit.await {
				Ok(stream) => {
					let stream = stream
						.filter_map(|event| {
							let execution_state = execution_state.clone();
							async move { handle_event(event, metrics, execution_state) }
						})
						.boxed();

					// If the subscription is too slow older events will be overwritten.
					sink.pipe_from_stream(stream, RingBuffer::new(3)).await;
				},
				Err(err) => {
					// We have not created an `Watcher` for the tx. Make sure the
					// error is still propagated as an event.
					let event: TransactionEvent<<Pool::Block as BlockT>::Hash> = err.into();
					_ = sink.send(&event).await;
				},
			};
		};

		sc_rpc::utils::spawn_subscription_task(&self.executor, fut);
	}
}

// const LABEL_INITIAL: &str = "initial";
// const LABEL_VALIDATED: &str = "validated";

/// Handle events generated by the transaction-pool and convert them
/// to the new API expected state.
#[inline]
fn handle_event<Hash: Clone, BlockHash: Clone>(
	event: TransactionStatus<Hash, BlockHash>,
	metrics: &Option<Metrics>,
	execution_state: Arc<Mutex<ExecutionState>>,
) -> Option<TransactionEvent<BlockHash>> {
	let mut execution_state = execution_state.lock();
	match event {
		TransactionStatus::Ready | TransactionStatus::Future => {
			if let Some(metrics) = metrics {
				metrics.publish_and_advance_state(&mut execution_state, LABEL_VALIDATED);
			}

			Some(TransactionEvent::<BlockHash>::Validated)
		},
		TransactionStatus::InBlock((hash, index)) => {
			if let Some(metrics) = metrics {
				metrics.publish_and_advance_state(&mut execution_state, LABEL_IN_BLOCK);
			}

			Some(TransactionEvent::BestChainBlockIncluded(Some(TransactionBlock { hash, index })))
		},
		TransactionStatus::Retracted(_) => {
			if let Some(metrics) = metrics {
				metrics.publish_and_advance_state(&mut execution_state, LABEL_RETRACTED);
			}

			Some(TransactionEvent::BestChainBlockIncluded(None))
		},
		TransactionStatus::FinalityTimeout(_) => {
			if let Some(metrics) = metrics {
				metrics.status.with_label_values(&[LABEL_DROPPED]).inc();
				metrics.publish_and_advance_state(&mut execution_state, LABEL_DROPPED);
			}

			Some(TransactionEvent::Dropped(TransactionDropped {
				error: "Maximum number of finality watchers has been reached".into(),
			}))
		},
		TransactionStatus::Finalized((hash, index)) => {
			if let Some(metrics) = metrics {
				metrics.status.with_label_values(&[LABEL_FINALIZED]).inc();
				metrics.publish_and_advance_state(&mut execution_state, LABEL_FINALIZED);
			}

			Some(TransactionEvent::Finalized(TransactionBlock { hash, index }))
		},
		TransactionStatus::Usurped(_) => {
			if let Some(metrics) = metrics {
				metrics.status.with_label_values(&[LABEL_INVALID]).inc();
				metrics.publish_and_advance_state(&mut execution_state, LABEL_INVALID);
			}

			Some(TransactionEvent::Invalid(TransactionError {
				error: "Extrinsic was rendered invalid by another extrinsic".into(),
			}))
		},
		TransactionStatus::Dropped => {
			if let Some(metrics) = metrics {
				metrics.status.with_label_values(&[LABEL_DROPPED]).inc();
				metrics.publish_and_advance_state(&mut execution_state, LABEL_DROPPED);
			}

			Some(TransactionEvent::Dropped(TransactionDropped {
				error: "Extrinsic dropped from the pool due to exceeding limits".into(),
			}))
		},
		TransactionStatus::Invalid => {
			if let Some(metrics) = metrics {
				metrics.status.with_label_values(&[LABEL_INVALID]).inc();
				metrics.publish_and_advance_state(&mut execution_state, LABEL_INVALID);
			}

			Some(TransactionEvent::Invalid(TransactionError {
				error: "Extrinsic marked as invalid".into(),
			}))
		},
		// These are the events that are not supported by the new API.
		TransactionStatus::Broadcast(_) => None,
	}
}
