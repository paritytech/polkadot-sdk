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

//! Substrate fork-aware transaction pool implementation.

use super::{
	dropped_watcher::{MultiViewDroppedWatcherController, StreamOfDropped},
	import_notification_sink::MultiViewImportNotificationSink,
	metrics::MetricsLink as PrometheusMetrics,
	multi_view_listener::MultiViewListener,
	tx_mem_pool::{TxInMemPool, TxMemPool, TXMEMPOOL_TRANSACTION_LIMIT_MULTIPLIER},
	view::View,
	view_store::ViewStore,
};
use crate::{
	api::FullChainApi,
	common::log_xt::log_xt_trace,
	enactment_state::{EnactmentAction, EnactmentState},
	fork_aware_txpool::revalidation_worker,
	graph::{self, base_pool::Transaction, ExtrinsicFor, ExtrinsicHash, IsValidator, Options},
	PolledIterator, ReadyIteratorFor, LOG_TARGET,
};
use async_trait::async_trait;
use futures::{
	channel::oneshot,
	future::{self},
	prelude::*,
	FutureExt,
};
use parking_lot::Mutex;
use prometheus_endpoint::Registry as PrometheusRegistry;
use sc_transaction_pool_api::{
	error::{Error, IntoPoolError},
	ChainEvent, ImportNotificationStream, MaintainedTransactionPool, PoolFuture, PoolStatus,
	TransactionFor, TransactionPool, TransactionSource, TransactionStatusStreamFor, TxHash,
};
use sp_blockchain::{HashAndNumber, TreeRoute};
use sp_core::traits::SpawnEssentialNamed;
use sp_runtime::{
	generic::BlockId,
	traits::{Block as BlockT, NumberFor},
};
use std::{
	collections::{HashMap, HashSet},
	pin::Pin,
	sync::Arc,
	time::Instant,
};
use tokio::select;

/// Fork aware transaction pool task, that needs to be polled.
pub type ForkAwareTxPoolTask = Pin<Box<dyn Future<Output = ()> + Send>>;

/// A structure that maintains a collection of pollers associated with specific block hashes
/// (views).
struct ReadyPoll<T, Block>
where
	Block: BlockT,
{
	pollers: HashMap<Block::Hash, Vec<oneshot::Sender<T>>>,
}

impl<T, Block> ReadyPoll<T, Block>
where
	Block: BlockT,
{
	/// Creates a new `ReadyPoll` instance with an empty collection of pollers.
	fn new() -> Self {
		Self { pollers: Default::default() }
	}

	/// Adds a new poller for a specific block hash and returns the `Receiver` end of the created
	/// oneshot channel which will be used to deliver polled result.
	fn add(&mut self, at: <Block as BlockT>::Hash) -> oneshot::Receiver<T> {
		let (s, r) = oneshot::channel();
		self.pollers.entry(at).or_default().push(s);
		r
	}

	/// Triggers all pollers associated with a specific block by sending the polled result through
	/// each oneshot channel.
	///
	/// `ready_iterator` is a closure that generates the result data to be sent to the pollers.
	fn trigger(&mut self, at: Block::Hash, ready_iterator: impl Fn() -> T) {
		log::trace!(target: LOG_TARGET, "fatp::trigger {at:?} pending keys: {:?}", self.pollers.keys());
		let Some(pollers) = self.pollers.remove(&at) else { return };
		pollers.into_iter().for_each(|p| {
			log::debug!(target: LOG_TARGET, "trigger ready signal at block {}", at);
			let _ = p.send(ready_iterator());
		});
	}

	/// Removes pollers that have their oneshot channels cancelled.
	fn remove_cancelled(&mut self) {
		self.pollers.retain(|_, v| v.iter().any(|sender| !sender.is_canceled()));
	}
}

/// The fork-aware transaction pool.
///
/// It keeps track of every fork and provides the set of transactions that is valid for every fork.
pub struct ForkAwareTxPool<ChainApi, Block>
where
	Block: BlockT,
	ChainApi: graph::ChainApi<Block = Block> + 'static,
{
	/// The reference to the `ChainApi` provided by client/backend.
	api: Arc<ChainApi>,

	/// Intermediate buffer for the incoming transaction.
	mempool: Arc<TxMemPool<ChainApi, Block>>,

	/// The store for all the views.
	view_store: Arc<ViewStore<ChainApi, Block>>,

	/// Utility for managing pollers of `ready_at` future.
	ready_poll: Arc<Mutex<ReadyPoll<ReadyIteratorFor<ChainApi>, Block>>>,

	/// Prometheus's metrics endpoint.
	metrics: PrometheusMetrics,

	/// Util tracking best and finalized block.
	enactment_state: Arc<Mutex<EnactmentState<Block>>>,

	/// The channel allowing to send revalidation jobs to the background thread.
	revalidation_queue: Arc<revalidation_worker::RevalidationQueue<ChainApi, Block>>,

	/// Util providing an aggregated stream of transactions that were imported to ready queue in
	/// any view.
	import_notification_sink: MultiViewImportNotificationSink<Block::Hash, ExtrinsicHash<ChainApi>>,

	/// Externally provided pool options.
	options: Options,

	/// Is node the validator.
	is_validator: IsValidator,
}

impl<ChainApi, Block> ForkAwareTxPool<ChainApi, Block>
where
	Block: BlockT,
	ChainApi: graph::ChainApi<Block = Block> + 'static,
	<Block as BlockT>::Hash: Unpin,
{
	/// Create new fork aware transaction pool with provided shared instance of `ChainApi` intended
	/// for tests.
	pub fn new_test(
		pool_api: Arc<ChainApi>,
		best_block_hash: Block::Hash,
		finalized_hash: Block::Hash,
	) -> (Self, ForkAwareTxPoolTask) {
		Self::new_test_with_limits(
			pool_api,
			best_block_hash,
			finalized_hash,
			Options::default().ready,
			Options::default().future,
			usize::MAX,
		)
	}

	/// Create new fork aware transaction pool with given limits and with provided shared instance
	/// of `ChainApi` intended for tests.
	pub fn new_test_with_limits(
		pool_api: Arc<ChainApi>,
		best_block_hash: Block::Hash,
		finalized_hash: Block::Hash,
		ready_limits: crate::PoolLimit,
		future_limits: crate::PoolLimit,
		mempool_max_transactions_count: usize,
	) -> (Self, ForkAwareTxPoolTask) {
		let listener = Arc::from(MultiViewListener::new());
		let (import_notification_sink, import_notification_sink_task) =
			MultiViewImportNotificationSink::new_with_worker();

		let mempool = Arc::from(TxMemPool::new(
			pool_api.clone(),
			listener.clone(),
			Default::default(),
			mempool_max_transactions_count,
		));

		let (dropped_stream_controller, dropped_stream) =
			MultiViewDroppedWatcherController::<ChainApi>::new();
		let dropped_monitor_task = Self::dropped_monitor_task(
			dropped_stream,
			mempool.clone(),
			import_notification_sink.clone(),
		);

		let combined_tasks = async move {
			tokio::select! {
				_ = import_notification_sink_task => {},
				_ = dropped_monitor_task => {}
			}
		}
		.boxed();

		let options = Options { ready: ready_limits, future: future_limits, ..Default::default() };

		(
			Self {
				mempool,
				api: pool_api.clone(),
				view_store: Arc::new(ViewStore::new(pool_api, listener, dropped_stream_controller)),
				ready_poll: Arc::from(Mutex::from(ReadyPoll::new())),
				enactment_state: Arc::new(Mutex::new(EnactmentState::new(
					best_block_hash,
					finalized_hash,
				))),
				revalidation_queue: Arc::from(revalidation_worker::RevalidationQueue::new()),
				import_notification_sink,
				options,
				is_validator: false.into(),
				metrics: Default::default(),
			},
			combined_tasks,
		)
	}

	/// Monitors the stream of dropped transactions and removes them from the mempool.
	///
	/// This asynchronous task continuously listens for dropped transaction notifications provided
	/// within `dropped_stream` and ensures that these transactions are removed from the `mempool`
	/// and `import_notification_sink` instances.
	async fn dropped_monitor_task(
		mut dropped_stream: StreamOfDropped<ChainApi>,
		mempool: Arc<TxMemPool<ChainApi, Block>>,
		import_notification_sink: MultiViewImportNotificationSink<
			Block::Hash,
			ExtrinsicHash<ChainApi>,
		>,
	) {
		loop {
			let Some(dropped) = dropped_stream.next().await else {
				log::debug!(target: LOG_TARGET, "fatp::dropped_monitor_task: terminated...");
				break;
			};
			log::trace!(target: LOG_TARGET, "[{:?}] fatp::dropped notification, removing", dropped);
			mempool.remove_dropped_transactions(&[dropped]).await;
			import_notification_sink.clean_notified_items(&[dropped]);
		}
	}

	/// Creates new fork aware transaction pool with the background revalidation worker.
	///
	/// The txpool essential tasks (including a revalidation worker) are spawned using provided
	/// spawner.
	pub fn new_with_background_worker(
		options: Options,
		is_validator: IsValidator,
		pool_api: Arc<ChainApi>,
		prometheus: Option<&PrometheusRegistry>,
		spawner: impl SpawnEssentialNamed,
		best_block_hash: Block::Hash,
		finalized_hash: Block::Hash,
	) -> Self {
		let metrics = PrometheusMetrics::new(prometheus);
		let listener = Arc::from(MultiViewListener::new());
		let (revalidation_queue, revalidation_task) =
			revalidation_worker::RevalidationQueue::new_with_worker();

		let (import_notification_sink, import_notification_sink_task) =
			MultiViewImportNotificationSink::new_with_worker();

		let mempool = Arc::from(TxMemPool::new(
			pool_api.clone(),
			listener.clone(),
			metrics.clone(),
			TXMEMPOOL_TRANSACTION_LIMIT_MULTIPLIER * (options.ready.count + options.future.count),
		));

		let (dropped_stream_controller, dropped_stream) =
			MultiViewDroppedWatcherController::<ChainApi>::new();
		let dropped_monitor_task = Self::dropped_monitor_task(
			dropped_stream,
			mempool.clone(),
			import_notification_sink.clone(),
		);

		let combined_tasks = async move {
			tokio::select! {
				_ = revalidation_task => {},
				_ = import_notification_sink_task => {},
				_ = dropped_monitor_task => {}
			}
		}
		.boxed();
		spawner.spawn_essential("txpool-background", Some("transaction-pool"), combined_tasks);

		Self {
			mempool,
			api: pool_api.clone(),
			view_store: Arc::new(ViewStore::new(pool_api, listener, dropped_stream_controller)),
			ready_poll: Arc::from(Mutex::from(ReadyPoll::new())),
			enactment_state: Arc::new(Mutex::new(EnactmentState::new(
				best_block_hash,
				finalized_hash,
			))),
			revalidation_queue: Arc::from(revalidation_queue),
			import_notification_sink,
			options,
			metrics,
			is_validator,
		}
	}

	/// Get access to the underlying api
	pub fn api(&self) -> &ChainApi {
		&self.api
	}

	/// Provides a status for all views at the tips of the forks.
	pub fn status_all(&self) -> HashMap<Block::Hash, PoolStatus> {
		self.view_store.status()
	}

	/// Provides a number of views at the tips of the forks.
	pub fn active_views_count(&self) -> usize {
		self.view_store.active_views.read().len()
	}

	/// Provides a number of views at the tips of the forks.
	pub fn inactive_views_count(&self) -> usize {
		self.view_store.inactive_views.read().len()
	}

	/// Provides internal views statistics.
	///
	/// Provides block number, count of ready, count of future transactions for every view. It is
	/// suitable for printing log information.
	fn views_stats(&self) -> Vec<(NumberFor<Block>, usize, usize)> {
		self.view_store
			.active_views
			.read()
			.iter()
			.map(|v| (v.1.at.number, v.1.status().ready, v.1.status().future))
			.collect()
	}

	/// Checks if there is a view at the tip of the fork with given hash.
	pub fn has_view(&self, hash: &Block::Hash) -> bool {
		self.view_store.active_views.read().contains_key(hash)
	}

	/// Returns a number of unwatched and watched transactions in internal mempool.
	///
	/// Intended for use in unit tests.
	pub fn mempool_len(&self) -> (usize, usize) {
		self.mempool.unwatched_and_watched_count()
	}

	/// Returns a best-effort set of ready transactions for a given block, without executing full
	/// maintain process.
	///
	/// The method attempts to build a temporary view and create an iterator of ready transactions
	/// for a specific `at` hash. If a valid view is found, it collects and prunes
	/// transactions already included in the blocks and returns the valid set.
	///
	/// Pruning is just rebuilding the underlying transactions graph, no validations are executed,
	/// so this process shall be fast.
	pub fn ready_at_light(&self, at: Block::Hash) -> PolledIterator<ChainApi> {
		let start = Instant::now();
		let api = self.api.clone();
		log::trace!(target: LOG_TARGET, "fatp::ready_at_light {:?}", at);

		let Ok(block_number) = self.api.resolve_block_number(at) else {
			let empty: ReadyIteratorFor<ChainApi> = Box::new(std::iter::empty());
			return Box::pin(async { empty })
		};

		let best_result = {
			api.tree_route(self.enactment_state.lock().recent_finalized_block(), at).map(
				|tree_route| {
					if let Some((index, view)) =
						tree_route.enacted().iter().enumerate().rev().skip(1).find_map(|(i, b)| {
							self.view_store.get_view_at(b.hash, true).map(|(view, _)| (i, view))
						}) {
						let e = tree_route.enacted()[index..].to_vec();
						(TreeRoute::new(e, 0).ok(), Some(view))
					} else {
						(None, None)
					}
				},
			)
		};

		Box::pin(async move {
			if let Ok((Some(best_tree_route), Some(best_view))) = best_result {
				let tmp_view: View<ChainApi> = View::new_from_other(
					&best_view,
					&HashAndNumber { hash: at, number: block_number },
				);

				let mut all_extrinsics = vec![];

				for h in best_tree_route.enacted() {
					let extrinsics = api
						.block_body(h.hash)
						.await
						.unwrap_or_else(|e| {
							log::warn!(target: LOG_TARGET, "Compute ready light transactions: error request: {}", e);
							None
						})
						.unwrap_or_default()
						.into_iter()
						.map(|t| api.hash_and_length(&t).0);
					all_extrinsics.extend(extrinsics);
				}

				let before_count = tmp_view.pool.validated_pool().status().ready;
				let tags = tmp_view
					.pool
					.validated_pool()
					.extrinsics_tags(&all_extrinsics)
					.into_iter()
					.flatten()
					.flatten()
					.collect::<Vec<_>>();
				let _ = tmp_view.pool.validated_pool().prune_tags(tags);

				let after_count = tmp_view.pool.validated_pool().status().ready;
				log::debug!(target: LOG_TARGET,
					"fatp::ready_at_light {} from {} before: {} to be removed: {} after: {} took:{:?}",
					at,
					best_view.at.hash,
					before_count,
					all_extrinsics.len(),
					after_count,
					start.elapsed()
				);
				Box::new(tmp_view.pool.validated_pool().ready())
			} else {
				let empty: ReadyIteratorFor<ChainApi> = Box::new(std::iter::empty());
				log::debug!(target: LOG_TARGET, "fatp::ready_at_light {} -> empty, took:{:?}", at, start.elapsed());
				empty
			}
		})
	}

	/// Waits for the set of ready transactions for a given block up to a specified timeout.
	///
	/// This method combines two futures:
	/// - The `ready_at` future, which waits for the ready transactions resulting from the full
	/// maintenance process to be available.
	/// - The `ready_at_light` future, used as a fallback if the timeout expires before `ready_at`
	/// completes. This provides a best-effort, ready set of transactions as a result light
	/// maintain.
	///
	/// Returns a future resolving to a ready iterator of transactions.
	fn ready_at_with_timeout_internal(
		&self,
		at: Block::Hash,
		timeout: std::time::Duration,
	) -> PolledIterator<ChainApi> {
		log::debug!(target: LOG_TARGET, "fatp::ready_at_with_timeout at {:?} allowed delay: {:?}", at, timeout);

		let timeout = futures_timer::Delay::new(timeout);
		let (view_already_exists, ready_at) = self.ready_at_internal(at);

		if view_already_exists {
			return ready_at;
		}

		let maybe_ready = async move {
			select! {
				ready = ready_at => Some(ready),
				_ = timeout => {
					log::warn!(target: LOG_TARGET,
						"Timeout fired waiting for transaction pool at block: ({:?}). \
						Proceeding with production.",
						at,
					);
					None
				}
			}
		};

		let fall_back_ready = self.ready_at_light(at);
		Box::pin(async {
			let (maybe_ready, fall_back_ready) =
				futures::future::join(maybe_ready.boxed(), fall_back_ready.boxed()).await;
			maybe_ready.unwrap_or(fall_back_ready)
		})
	}

	fn ready_at_internal(&self, at: Block::Hash) -> (bool, PolledIterator<ChainApi>) {
		let mut ready_poll = self.ready_poll.lock();

		if let Some((view, inactive)) = self.view_store.get_view_at(at, true) {
			log::debug!(target: LOG_TARGET, "fatp::ready_at {at:?} (inactive:{inactive:?})");
			let iterator: ReadyIteratorFor<ChainApi> = Box::new(view.pool.validated_pool().ready());
			return (true, async move { iterator }.boxed());
		}

		let pending = ready_poll
			.add(at)
			.map(|received| {
				received.unwrap_or_else(|e| {
					log::warn!(target: LOG_TARGET, "Error receiving ready-set iterator: {:?}", e);
					Box::new(std::iter::empty())
				})
			})
			.boxed();
		log::debug!(target: LOG_TARGET,
			"fatp::ready_at {at:?} pending keys: {:?}",
			ready_poll.pollers.keys()
		);
		(false, pending)
	}
}

/// Converts the input view-to-statuses map into the output vector of statuses.
///
/// The result of importing a bunch of transactions into a single view is the vector of statuses.
/// Every item represents a status for single transaction. The input is the map that associates
/// hash-views with vectors indicating the statuses of transactions imports.
///
/// Import to multiple views result in two-dimensional array of statuses, which is provided as
/// input map.
///
/// This function converts the map into the vec of results, according to the following rules:
/// - for given transaction if at least one status is success, then output vector contains success,
/// - if given transaction status is error for every view, then output vector contains error.
///
/// The results for transactions are in the same order for every view. An output vector preserves
/// this order.
///
/// ```skip
/// in:
/// view  |   xt0 status | xt1 status | xt2 status
/// h1   -> [ Ok(xth0),    Ok(xth1),    Err       ]
/// h2   -> [ Ok(xth0),    Err,         Err       ]
/// h3   -> [ Ok(xth0),    Ok(xth1),    Err       ]
///
/// out:
/// [ Ok(xth0), Ok(xth1), Err ]
/// ```
fn reduce_multiview_result<H, E>(input: HashMap<H, Vec<Result<H, E>>>) -> Vec<Result<H, E>> {
	let mut values = input.values();
	let Some(first) = values.next() else {
		return Default::default();
	};
	let length = first.len();
	debug_assert!(values.all(|x| length == x.len()));

	input
		.into_values()
		.reduce(|mut agg_results, results| {
			agg_results.iter_mut().zip(results.into_iter()).for_each(|(agg_r, r)| {
				if agg_r.is_err() {
					*agg_r = r;
				}
			});
			agg_results
		})
		.unwrap_or_default()
}

impl<ChainApi, Block> TransactionPool for ForkAwareTxPool<ChainApi, Block>
where
	Block: BlockT,
	ChainApi: 'static + graph::ChainApi<Block = Block>,
	<Block as BlockT>::Hash: Unpin,
{
	type Block = ChainApi::Block;
	type Hash = ExtrinsicHash<ChainApi>;
	type InPoolTransaction = Transaction<ExtrinsicHash<ChainApi>, ExtrinsicFor<ChainApi>>;
	type Error = ChainApi::Error;

	/// Submits multiple transactions and returns a future resolving to the submission results.
	///
	/// Actual transactions submission process is delegated to the `ViewStore` internal instance.
	///
	/// The internal limits of the pool are checked. The results of submissions to individual views
	/// are reduced to single result. Refer to `reduce_multiview_result` for more details.
	fn submit_at(
		&self,
		_: <Self::Block as BlockT>::Hash,
		source: TransactionSource,
		xts: Vec<TransactionFor<Self>>,
	) -> PoolFuture<Vec<Result<TxHash<Self>, Self::Error>>, Self::Error> {
		let view_store = self.view_store.clone();
		log::debug!(target: LOG_TARGET, "fatp::submit_at count:{} views:{}", xts.len(), self.active_views_count());
		log_xt_trace!(target: LOG_TARGET, xts.iter().map(|xt| self.tx_hash(xt)), "[{:?}] fatp::submit_at");
		let xts = xts.into_iter().map(Arc::from).collect::<Vec<_>>();
		let mempool_result = self.mempool.extend_unwatched(source, &xts);

		if view_store.is_empty() {
			return future::ready(Ok(mempool_result)).boxed()
		}

		let (hashes, to_be_submitted): (Vec<TxHash<Self>>, Vec<ExtrinsicFor<ChainApi>>) =
			mempool_result
				.iter()
				.zip(xts)
				.filter_map(|(result, xt)| result.as_ref().ok().map(|xt_hash| (xt_hash, xt)))
				.unzip();

		self.metrics
			.report(|metrics| metrics.submitted_transactions.inc_by(to_be_submitted.len() as _));

		let mempool = self.mempool.clone();
		async move {
			let results_map = view_store.submit(source, to_be_submitted.into_iter(), hashes).await;
			let mut submission_results = reduce_multiview_result(results_map).into_iter();

			Ok(mempool_result
				.into_iter()
				.map(|result| {
					result.and_then(|xt_hash| {
						let result = submission_results
							.next()
							.expect("The number of Ok results in mempool is exactly the same as the size of to-views-submission result. qed.");
						result.or_else(|error| {
							let error = error.into_pool_error();
							match error {
								Ok(
									// The transaction is still in mempool it may get included into the view for the next block.
									Error::ImmediatelyDropped
								) => Ok(xt_hash),
								Ok(e) => {
									mempool.remove(xt_hash);
									Err(e.into())
								},
								Err(e) => Err(e),
							}
						})
					})
				})
				.collect::<Vec<_>>())
		}
		.boxed()
	}

	/// Submits a single transaction and returns a future resolving to the submission results.
	///
	/// Actual transaction submission process is delegated to the `submit_at` function.
	fn submit_one(
		&self,
		_at: <Self::Block as BlockT>::Hash,
		source: TransactionSource,
		xt: TransactionFor<Self>,
	) -> PoolFuture<TxHash<Self>, Self::Error> {
		log::trace!(target: LOG_TARGET, "[{:?}] fatp::submit_one views:{}", self.tx_hash(&xt), self.active_views_count());
		let result_future = self.submit_at(_at, source, vec![xt]);
		async move {
			let result = result_future.await;
			match result {
				Ok(mut v) =>
					v.pop().expect("There is exactly one element in result of submit_at. qed."),
				Err(e) => Err(e),
			}
		}
		.boxed()
	}

	/// Submits a transaction and starts to watch its progress in the pool, returning a stream of
	/// status updates.
	///
	/// Actual transaction submission process is delegated to the `ViewStore` internal instance.
	fn submit_and_watch(
		&self,
		at: <Self::Block as BlockT>::Hash,
		source: TransactionSource,
		xt: TransactionFor<Self>,
	) -> PoolFuture<Pin<Box<TransactionStatusStreamFor<Self>>>, Self::Error> {
		log::trace!(target: LOG_TARGET, "[{:?}] fatp::submit_and_watch views:{}", self.tx_hash(&xt), self.active_views_count());
		let xt = Arc::from(xt);
		let xt_hash = match self.mempool.push_watched(source, xt.clone()) {
			Ok(xt_hash) => xt_hash,
			Err(e) => return future::ready(Err(e)).boxed(),
		};

		self.metrics.report(|metrics| metrics.submitted_transactions.inc());

		let view_store = self.view_store.clone();
		let mempool = self.mempool.clone();
		async move {
			let result = view_store.submit_and_watch(at, source, xt).await;
			let result = result.or_else(|(e, maybe_watcher)| {
				let error = e.into_pool_error();
				match (error, maybe_watcher) {
					(
						Ok(
							// The transaction is still in mempool it may get included into the
							// view for the next block.
							Error::ImmediatelyDropped,
						),
						Some(watcher),
					) => Ok(watcher),
					(Ok(e), _) => {
						mempool.remove(xt_hash);
						Err(e.into())
					},
					(Err(e), _) => Err(e),
				}
			});
			result
		}
		.boxed()
	}

	/// Intended to remove transactions identified by the given hashes, and any dependent
	/// transactions, from the pool. In current implementation this function only outputs the error.
	/// Seems that API change is needed here to make this call reasonable.
	// todo [#5491]: api change? we need block hash here (assuming we need it at all - could be
	// useful for verification for debugging purposes).
	fn remove_invalid(&self, hashes: &[TxHash<Self>]) -> Vec<Arc<Self::InPoolTransaction>> {
		if !hashes.is_empty() {
			log::debug!(target: LOG_TARGET, "fatp::remove_invalid {}", hashes.len());
			log_xt_trace!(target:LOG_TARGET, hashes, "[{:?}] fatp::remove_invalid");
			self.metrics
				.report(|metrics| metrics.removed_invalid_txs.inc_by(hashes.len() as _));
		}
		Default::default()
	}

	// todo [#5491]: api change?
	// status(Hash) -> Option<PoolStatus>
	/// Returns the pool status which includes information like the number of ready and future
	/// transactions.
	///
	/// Currently the status for the most recently notified best block is returned (for which
	/// maintain process was accomplished).
	fn status(&self) -> PoolStatus {
		self.view_store
			.most_recent_view
			.read()
			.map(|hash| self.view_store.status()[&hash].clone())
			.unwrap_or(PoolStatus { ready: 0, ready_bytes: 0, future: 0, future_bytes: 0 })
	}

	/// Return an event stream of notifications when transactions are imported to the pool.
	///
	/// Consumers of this stream should use the `ready` method to actually get the
	/// pending transactions in the right order.
	fn import_notification_stream(&self) -> ImportNotificationStream<ExtrinsicHash<ChainApi>> {
		self.import_notification_sink.event_stream()
	}

	/// Returns the hash of a given transaction.
	fn hash_of(&self, xt: &TransactionFor<Self>) -> TxHash<Self> {
		self.api().hash_and_length(xt).0
	}

	/// Notifies the pool about the broadcasting status of transactions.
	fn on_broadcasted(&self, propagations: HashMap<TxHash<Self>, Vec<String>>) {
		self.view_store.listener.transactions_broadcasted(propagations);
	}

	/// Return specific ready transaction by hash, if there is one.
	///
	/// Currently the ready transaction is returned if it exists for the most recently notified best
	/// block (for which maintain process was accomplished).
	// todo [#5491]: api change: we probably should have at here?
	fn ready_transaction(&self, tx_hash: &TxHash<Self>) -> Option<Arc<Self::InPoolTransaction>> {
		let most_recent_view = self.view_store.most_recent_view.read();
		let result = most_recent_view
			.map(|block_hash| self.view_store.ready_transaction(block_hash, tx_hash))
			.flatten();
		log::trace!(
			target: LOG_TARGET,
			"[{tx_hash:?}] ready_transaction: {} {:?}",
			result.is_some(),
			most_recent_view
		);
		result
	}

	/// Returns an iterator for ready transactions at a specific block, ordered by priority.
	fn ready_at(&self, at: <Self::Block as BlockT>::Hash) -> PolledIterator<ChainApi> {
		let (_, result) = self.ready_at_internal(at);
		result
	}

	/// Returns an iterator for ready transactions, ordered by priority.
	///
	/// Currently the set of ready transactions is returned if it exists for the most recently
	/// notified best block (for which maintain process was accomplished).
	fn ready(&self) -> ReadyIteratorFor<ChainApi> {
		self.view_store.ready()
	}

	/// Returns a list of future transactions in the pool.
	///
	/// Currently the set of future transactions is returned if it exists for the most recently
	/// notified best block (for which maintain process was accomplished).
	fn futures(&self) -> Vec<Self::InPoolTransaction> {
		self.view_store.futures()
	}

	/// Returns a set of ready transactions at a given block within the specified timeout.
	///
	/// If the timeout expires before the maintain process is accomplished, a best-effort
	/// set of transactions is returned (refer to `ready_at_light`).
	fn ready_at_with_timeout(
		&self,
		at: <Self::Block as BlockT>::Hash,
		timeout: std::time::Duration,
	) -> PolledIterator<ChainApi> {
		self.ready_at_with_timeout_internal(at, timeout)
	}
}

impl<Block, Client> sc_transaction_pool_api::LocalTransactionPool
	for ForkAwareTxPool<FullChainApi<Client, Block>, Block>
where
	Block: BlockT,
	<Block as BlockT>::Hash: Unpin,
	Client: sp_api::ProvideRuntimeApi<Block>
		+ sc_client_api::BlockBackend<Block>
		+ sc_client_api::blockchain::HeaderBackend<Block>
		+ sp_runtime::traits::BlockIdTo<Block>
		+ sp_blockchain::HeaderMetadata<Block, Error = sp_blockchain::Error>,
	Client: Send + Sync + 'static,
	Client::Api: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>,
{
	type Block = Block;
	type Hash = ExtrinsicHash<FullChainApi<Client, Block>>;
	type Error = <FullChainApi<Client, Block> as graph::ChainApi>::Error;

	fn submit_local(
		&self,
		_at: Block::Hash,
		xt: sc_transaction_pool_api::LocalTransactionFor<Self>,
	) -> Result<Self::Hash, Self::Error> {
		log::debug!(target: LOG_TARGET, "fatp::submit_local views:{}", self.active_views_count());
		let xt = Arc::from(xt);
		let result = self
			.mempool
			.extend_unwatched(TransactionSource::Local, &[xt.clone()])
			.remove(0)?;

		self.view_store.submit_local(xt).or_else(|_| Ok(result))
	}
}

impl<ChainApi, Block> ForkAwareTxPool<ChainApi, Block>
where
	Block: BlockT,
	ChainApi: graph::ChainApi<Block = Block> + 'static,
	<Block as BlockT>::Hash: Unpin,
{
	/// Handles a new block notification.
	///
	/// It is responsible for handling a newly notified block. It executes some sanity checks, find
	/// the best view to clone from and executes the new view build procedure for the notified
	/// block.
	///
	/// If the view is correctly created, `ready_at` pollers for this block will be triggered.
	async fn handle_new_block(&self, tree_route: &TreeRoute<Block>) {
		let hash_and_number = match tree_route.last() {
			Some(hash_and_number) => hash_and_number,
			None => {
				log::warn!(
					target: LOG_TARGET,
					"Skipping ChainEvent - no last block in tree route {:?}",
					tree_route,
				);
				return
			},
		};

		if self.has_view(&hash_and_number.hash) {
			log::trace!(
				target: LOG_TARGET,
				"view already exists for block: {:?}",
				hash_and_number,
			);
			return
		}

		let best_view = self.view_store.find_best_view(tree_route);
		let new_view = self.build_new_view(best_view, hash_and_number, tree_route).await;

		if let Some(view) = new_view {
			{
				let view = view.clone();
				self.ready_poll.lock().trigger(hash_and_number.hash, move || {
					Box::from(view.pool.validated_pool().ready())
				});
			}

			View::start_background_revalidation(view, self.revalidation_queue.clone()).await;
		}
	}

	/// Builds a new view.
	///
	/// If `origin_view` is provided, the new view will be cloned from it. Otherwise an empty view
	/// will be created.
	///
	/// The new view will be updated with transactions from the tree_route and the mempool, all
	/// required events will be triggered, it will be inserted to the view store.
	///
	/// This method will also update multi-view listeners with newly created view.
	async fn build_new_view(
		&self,
		origin_view: Option<Arc<View<ChainApi>>>,
		at: &HashAndNumber<Block>,
		tree_route: &TreeRoute<Block>,
	) -> Option<Arc<View<ChainApi>>> {
		log::debug!(
			target: LOG_TARGET,
			"build_new_view: for: {:?} from: {:?} tree_route: {:?}",
			at,
			origin_view.as_ref().map(|v| v.at.clone()),
			tree_route
		);
		let mut view = if let Some(origin_view) = origin_view {
			let mut view = View::new_from_other(&origin_view, at);
			if !tree_route.retracted().is_empty() {
				view.pool.clear_recently_pruned();
			}
			view
		} else {
			log::debug!(target: LOG_TARGET, "creating non-cloned view: for: {at:?}");
			View::new(
				self.api.clone(),
				at.clone(),
				self.options.clone(),
				self.metrics.clone(),
				self.is_validator.clone(),
			)
		};

		// 1. Capture all import notification from the very beginning, so first register all
		//the listeners.
		self.import_notification_sink.add_view(
			view.at.hash,
			view.pool.validated_pool().import_notification_stream().boxed(),
		);

		self.view_store.dropped_stream_controller.add_view(
			view.at.hash,
			view.pool.validated_pool().create_dropped_by_limits_stream().boxed(),
		);

		let start = Instant::now();
		let watched_xts = self.register_listeners(&mut view).await;
		let duration = start.elapsed();
		log::debug!(target: LOG_TARGET, "register_listeners: at {at:?} took {duration:?}");

		// 2. Handle transactions from the tree route. Pruning transactions from the view first
		// will make some space for mempool transactions in case we are at the view's limits.
		let start = Instant::now();
		self.update_view_with_fork(&view, tree_route, at.clone()).await;
		let duration = start.elapsed();
		log::debug!(target: LOG_TARGET, "update_view_with_fork: at {at:?} took {duration:?}");

		// 3. Finally, submit transactions from the mempool.
		let start = Instant::now();
		self.update_view_with_mempool(&mut view, watched_xts).await;
		let duration = start.elapsed();
		log::debug!(target: LOG_TARGET, "update_view_with_mempool: at {at:?} took {duration:?}");

		let view = Arc::from(view);
		self.view_store.insert_new_view(view.clone(), tree_route).await;
		Some(view)
	}

	/// Returns the list of xts included in all block ancestors, including the block itself.
	///
	/// Example: for the following chain `F<-B1<-B2<-B3` xts from `F,B1,B2,B3` will be returned.
	async fn extrinsics_included_since_finalized(&self, at: Block::Hash) -> HashSet<TxHash<Self>> {
		let start = Instant::now();
		let recent_finalized_block = self.enactment_state.lock().recent_finalized_block();

		let Ok(tree_route) = self.api.tree_route(recent_finalized_block, at) else {
			return Default::default()
		};

		let api = self.api.clone();
		let mut all_extrinsics = HashSet::new();

		for h in tree_route.enacted().iter().rev() {
			api.block_body(h.hash)
				.await
				.unwrap_or_else(|e| {
					log::warn!(target: LOG_TARGET, "Compute ready light transactions: error request: {}", e);
					None
				})
				.unwrap_or_default()
				.into_iter()
				.map(|t| self.hash_of(&t))
				.for_each(|tx_hash| {
					all_extrinsics.insert(tx_hash);
				});
		}

		log::debug!(target: LOG_TARGET,
			"fatp::extrinsics_included_since_finalized {} from {} count: {} took:{:?}",
			at,
			recent_finalized_block,
			all_extrinsics.len(),
			start.elapsed()
		);
		all_extrinsics
	}

	/// For every watched transaction in the mempool registers a transaction listener in the view.
	///
	/// The transaction listener for a given view is also added to multi-view listener. This allows
	/// to track aggreagated progress of the transaction within the transaction pool.
	///
	/// Function returns a list of currently watched transactions in the mempool.
	async fn register_listeners(
		&self,
		view: &View<ChainApi>,
	) -> Vec<(ExtrinsicHash<ChainApi>, Arc<TxInMemPool<ChainApi, Block>>)> {
		log::debug!(
			target: LOG_TARGET,
			"register_listeners: {:?} xts:{:?} v:{}",
			view.at,
			self.mempool.unwatched_and_watched_count(),
			self.active_views_count()
		);

		//todo [#5495]: maybe we don't need to register listener in view? We could use
		// multi_view_listener.transaction_in_block
		let results = self
			.mempool
			.clone_watched()
			.into_iter()
			.map(|(tx_hash, tx)| {
				let watcher = view.create_watcher(tx_hash);
				let at = view.at.clone();
				async move {
					log::trace!(target: LOG_TARGET, "[{:?}] adding watcher {:?}", tx_hash, at.hash);
					self.view_store.listener.add_view_watcher_for_tx(
						tx_hash,
						at.hash,
						watcher.into_stream().boxed(),
					);
					(tx_hash, tx)
				}
			})
			.collect::<Vec<_>>();

		future::join_all(results).await
	}

	/// Updates the given view with the transaction from the internal mempol.
	///
	/// All transactions from the mempool (excluding those which are either already imported or
	/// already included in blocks since recently finalized block) are submitted to the
	/// view.
	///
	/// If there are no views, and mempool transaction is reported as invalid for the given view,
	/// the transaction is reported as invalid and removed from the mempool. This does not apply to
	/// stale and temporarily banned transactions.
	///
	/// As the listeners for watched transactions were registered at the very beginning of maintain
	/// procedure (`register_listeners`), this function accepts the list of watched transactions
	/// from the mempool for which listener was actually registered to avoid submit/maintain races.
	async fn update_view_with_mempool(
		&self,
		view: &View<ChainApi>,
		watched_xts: Vec<(ExtrinsicHash<ChainApi>, Arc<TxInMemPool<ChainApi, Block>>)>,
	) {
		log::debug!(
			target: LOG_TARGET,
			"update_view_with_mempool: {:?} xts:{:?} v:{}",
			view.at,
			self.mempool.unwatched_and_watched_count(),
			self.active_views_count()
		);
		let included_xts = self.extrinsics_included_since_finalized(view.at.hash).await;
		let xts = self.mempool.clone_unwatched();

		let mut all_submitted_count = 0;
		if !xts.is_empty() {
			let unwatched_count = xts.len();
			let mut buckets = HashMap::<TransactionSource, Vec<ExtrinsicFor<ChainApi>>>::default();
			xts.into_iter()
				.filter(|(hash, _)| !view.pool.validated_pool().pool.read().is_imported(hash))
				.filter(|(hash, _)| !included_xts.contains(&hash))
				.map(|(_, tx)| (tx.source(), tx.tx()))
				.for_each(|(source, tx)| buckets.entry(source).or_default().push(tx));

			for (source, xts) in buckets {
				all_submitted_count += xts.len();
				let _ = view.submit_many(source, xts).await;
			}
			log::debug!(target: LOG_TARGET, "update_view_with_mempool: at {:?} unwatched {}/{}", view.at.hash, all_submitted_count, unwatched_count);
		}

		let watched_submitted_count = watched_xts.len();

		let mut buckets = HashMap::<
			TransactionSource,
			Vec<(ExtrinsicHash<ChainApi>, ExtrinsicFor<ChainApi>)>,
		>::default();
		watched_xts
			.into_iter()
			.filter(|(hash, _)| !included_xts.contains(&hash))
			.map(|(tx_hash, tx)| (tx.source(), tx_hash, tx.tx()))
			.for_each(|(source, tx_hash, tx)| {
				buckets.entry(source).or_default().push((tx_hash, tx))
			});

		let mut watched_results = Vec::default();
		for (source, watched_xts) in buckets {
			let hashes = watched_xts.iter().map(|i| i.0).collect::<Vec<_>>();
			let results = view
				.submit_many(source, watched_xts.into_iter().map(|i| i.1))
				.await
				.into_iter()
				.zip(hashes)
				.map(|(result, tx_hash)| result.or_else(|_| Err(tx_hash)))
				.collect::<Vec<_>>();
			watched_results.extend(results);
		}

		log::debug!(target: LOG_TARGET, "update_view_with_mempool: at {:?} watched {}/{}", view.at.hash, watched_submitted_count, self.mempool_len().1);

		all_submitted_count += watched_submitted_count;
		let _ = all_submitted_count
			.try_into()
			.map(|v| self.metrics.report(|metrics| metrics.submitted_from_mempool_txs.inc_by(v)));

		// if there are no views yet, and a single newly created view is reporting error, just send
		// out the invalid event, and remove transaction.
		if self.view_store.is_empty() {
			for result in watched_results {
				match result {
					Err(tx_hash) => {
						self.view_store.listener.invalidate_transactions(&[tx_hash]);
						self.mempool.remove(tx_hash);
					},
					Ok(_) => {},
				}
			}
		}
	}

	/// Updates the view with the transactions from the given tree route.
	///
	/// Transactions from the retracted blocks are resubmitted to the given view. Tags for
	/// transactions included in blocks on enacted fork are pruned from the provided view.
	async fn update_view_with_fork(
		&self,
		view: &View<ChainApi>,
		tree_route: &TreeRoute<Block>,
		hash_and_number: HashAndNumber<Block>,
	) {
		log::debug!(target: LOG_TARGET, "update_view_with_fork tree_route: {:?} {tree_route:?}", view.at);
		let api = self.api.clone();

		// We keep track of everything we prune so that later we won't add
		// transactions with those hashes from the retracted blocks.
		let mut pruned_log = HashSet::<ExtrinsicHash<ChainApi>>::new();

		future::join_all(
			tree_route
				.enacted()
				.iter()
				.map(|h| crate::prune_known_txs_for_block(h, &*api, &view.pool)),
		)
		.await
		.into_iter()
		.for_each(|enacted_log| {
			pruned_log.extend(enacted_log);
		});

		//resubmit
		{
			let mut resubmit_transactions = Vec::new();

			for retracted in tree_route.retracted() {
				let hash = retracted.hash;

				let block_transactions = api
					.block_body(hash)
					.await
					.unwrap_or_else(|e| {
						log::warn!(target: LOG_TARGET, "Failed to fetch block body: {}", e);
						None
					})
					.unwrap_or_default()
					.into_iter();

				let mut resubmitted_to_report = 0;

				resubmit_transactions.extend(
					block_transactions
						.into_iter()
						.map(|tx| (self.hash_of(&tx), tx))
						.filter(|(tx_hash, _)| {
							let contains = pruned_log.contains(&tx_hash);

							// need to count all transactions, not just filtered, here
							resubmitted_to_report += 1;

							if !contains {
								log::trace!(
									target: LOG_TARGET,
									"[{:?}]: Resubmitting from retracted block {:?}",
									tx_hash,
									hash,
								);
							}
							!contains
						})
						.map(|(tx_hash, tx)| {
							//find arc if tx is known
							self.mempool.get_by_hash(tx_hash).unwrap_or_else(|| Arc::from(tx))
						}),
				);

				self.metrics.report(|metrics| {
					metrics.resubmitted_retracted_txs.inc_by(resubmitted_to_report)
				});
			}

			let _ = view
				.pool
				.resubmit_at(
					&hash_and_number,
					// These transactions are coming from retracted blocks, we should
					// simply consider them external.
					TransactionSource::External,
					resubmit_transactions,
				)
				.await;
		}
	}

	/// Executes the maintainance for the finalized event.
	///
	/// Performs a house-keeping required for finalized event. This includes:
	/// - executing the on finalized procedure for the view store,
	/// - purging finalized transactions from the mempool and triggering mempool revalidation,
	async fn handle_finalized(&self, finalized_hash: Block::Hash, tree_route: &[Block::Hash]) {
		let finalized_number = self.api.block_id_to_number(&BlockId::Hash(finalized_hash));
		log::debug!(target: LOG_TARGET, "handle_finalized {finalized_number:?} tree_route: {tree_route:?} views_count:{}", self.active_views_count());

		let finalized_xts = self.view_store.handle_finalized(finalized_hash, tree_route).await;

		self.mempool.purge_finalized_transactions(&finalized_xts).await;
		self.import_notification_sink.clean_notified_items(&finalized_xts);

		self.metrics
			.report(|metrics| metrics.finalized_txs.inc_by(finalized_xts.len() as _));

		if let Ok(Some(finalized_number)) = finalized_number {
			self.revalidation_queue
				.revalidate_mempool(
					self.mempool.clone(),
					HashAndNumber { hash: finalized_hash, number: finalized_number },
				)
				.await;
		} else {
			log::trace!(target: LOG_TARGET, "purge_transactions_later skipped, cannot find block number {finalized_number:?}");
		}

		self.ready_poll.lock().remove_cancelled();
		log::trace!(target: LOG_TARGET, "handle_finalized after views_count:{:?}", self.active_views_count());
	}

	/// Computes a hash of the provided transaction
	fn tx_hash(&self, xt: &TransactionFor<Self>) -> TxHash<Self> {
		self.api.hash_and_length(xt).0
	}
}

#[async_trait]
impl<ChainApi, Block> MaintainedTransactionPool for ForkAwareTxPool<ChainApi, Block>
where
	Block: BlockT,
	ChainApi: 'static + graph::ChainApi<Block = Block>,
	<Block as BlockT>::Hash: Unpin,
{
	/// Executes the maintainance for the given chain event.
	async fn maintain(&self, event: ChainEvent<Self::Block>) {
		let start = Instant::now();
		log::debug!(target: LOG_TARGET, "processing event: {event:?}");

		self.view_store.finish_background_revalidations().await;

		let prev_finalized_block = self.enactment_state.lock().recent_finalized_block();

		let compute_tree_route = |from, to| -> Result<TreeRoute<Block>, String> {
			match self.api.tree_route(from, to) {
				Ok(tree_route) => Ok(tree_route),
				Err(e) =>
					return Err(format!(
						"Error occurred while computing tree_route from {from:?} to {to:?}: {e}"
					)),
			}
		};
		let block_id_to_number =
			|hash| self.api.block_id_to_number(&BlockId::Hash(hash)).map_err(|e| format!("{}", e));

		let result =
			self.enactment_state
				.lock()
				.update(&event, &compute_tree_route, &block_id_to_number);

		match result {
			Err(msg) => {
				log::trace!(target: LOG_TARGET, "enactment_state::update error: {msg}");
				self.enactment_state.lock().force_update(&event);
			},
			Ok(EnactmentAction::Skip) => return,
			Ok(EnactmentAction::HandleFinalization) => {
				// todo [#5492]: in some cases handle_new_block is actually needed (new_num >
				// tips_of_forks) let hash = event.hash();
				// if !self.has_view(hash) {
				// 	if let Ok(tree_route) = compute_tree_route(prev_finalized_block, hash) {
				// 		self.handle_new_block(&tree_route).await;
				// 	}
				// }
			},
			Ok(EnactmentAction::HandleEnactment(tree_route)) => {
				if matches!(event, ChainEvent::Finalized { .. }) {
					self.view_store.handle_pre_finalized(event.hash()).await;
				};
				self.handle_new_block(&tree_route).await;
			},
		};

		match event {
			ChainEvent::NewBestBlock { .. } => {},
			ChainEvent::Finalized { hash, ref tree_route } => {
				self.handle_finalized(hash, tree_route).await;

				log::trace!(
					target: LOG_TARGET,
					"on-finalized enacted: {tree_route:?}, previously finalized: \
					{prev_finalized_block:?}",
				);
			},
		}

		let maintain_duration = start.elapsed();

		log::info!(
			target: LOG_TARGET,
			"maintain: txs:{:?} views:[{};{:?}] event:{event:?}  took:{:?}",
			self.mempool_len(),
			self.active_views_count(),
			self.views_stats(),
			maintain_duration
		);

		self.metrics.report(|metrics| {
			let (unwatched, watched) = self.mempool_len();
			let _ = (
				self.active_views_count().try_into().map(|v| metrics.active_views.set(v)),
				self.inactive_views_count().try_into().map(|v| metrics.inactive_views.set(v)),
				watched.try_into().map(|v| metrics.watched_txs.set(v)),
				unwatched.try_into().map(|v| metrics.unwatched_txs.set(v)),
			);
			metrics.maintain_duration.observe(maintain_duration.as_secs_f64());
		});
	}
}

impl<Block, Client> ForkAwareTxPool<FullChainApi<Client, Block>, Block>
where
	Block: BlockT,
	Client: sp_api::ProvideRuntimeApi<Block>
		+ sc_client_api::BlockBackend<Block>
		+ sc_client_api::blockchain::HeaderBackend<Block>
		+ sp_runtime::traits::BlockIdTo<Block>
		+ sc_client_api::ExecutorProvider<Block>
		+ sc_client_api::UsageProvider<Block>
		+ sp_blockchain::HeaderMetadata<Block, Error = sp_blockchain::Error>
		+ Send
		+ Sync
		+ 'static,
	Client::Api: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>,
	<Block as BlockT>::Hash: std::marker::Unpin,
{
	/// Create new fork aware transaction pool for a full node with the provided api.
	pub fn new_full(
		options: Options,
		is_validator: IsValidator,
		prometheus: Option<&PrometheusRegistry>,
		spawner: impl SpawnEssentialNamed,
		client: Arc<Client>,
	) -> Self {
		let pool_api = Arc::new(FullChainApi::new(client.clone(), prometheus, &spawner));
		let pool = Self::new_with_background_worker(
			options,
			is_validator,
			pool_api,
			prometheus,
			spawner,
			client.usage_info().chain.best_hash,
			client.usage_info().chain.finalized_hash,
		);

		pool
	}
}

#[cfg(test)]
mod reduce_multiview_result_tests {
	use super::*;
	use sp_core::H256;
	#[derive(Debug, PartialEq, Clone)]
	enum Error {
		Custom(u8),
	}

	#[test]
	fn empty() {
		sp_tracing::try_init_simple();
		let input = HashMap::default();
		let r = reduce_multiview_result::<H256, Error>(input);
		assert!(r.is_empty());
	}

	#[test]
	fn errors_only() {
		sp_tracing::try_init_simple();
		let v: Vec<(H256, Vec<Result<H256, Error>>)> = vec![
			(
				H256::repeat_byte(0x13),
				vec![
					Err(Error::Custom(10)),
					Err(Error::Custom(11)),
					Err(Error::Custom(12)),
					Err(Error::Custom(13)),
				],
			),
			(
				H256::repeat_byte(0x14),
				vec![
					Err(Error::Custom(20)),
					Err(Error::Custom(21)),
					Err(Error::Custom(22)),
					Err(Error::Custom(23)),
				],
			),
			(
				H256::repeat_byte(0x15),
				vec![
					Err(Error::Custom(30)),
					Err(Error::Custom(31)),
					Err(Error::Custom(32)),
					Err(Error::Custom(33)),
				],
			),
		];
		let input = HashMap::from_iter(v.clone());
		let r = reduce_multiview_result(input);

		//order in HashMap is random, the result shall be one of:
		assert!(r == v[0].1 || r == v[1].1 || r == v[2].1);
	}

	#[test]
	#[should_panic]
	#[cfg(debug_assertions)]
	fn invalid_lengths() {
		sp_tracing::try_init_simple();
		let v: Vec<(H256, Vec<Result<H256, Error>>)> = vec![
			(H256::repeat_byte(0x13), vec![Err(Error::Custom(12)), Err(Error::Custom(13))]),
			(H256::repeat_byte(0x14), vec![Err(Error::Custom(23))]),
		];
		let input = HashMap::from_iter(v);
		let _ = reduce_multiview_result(input);
	}

	#[test]
	fn only_hashes() {
		sp_tracing::try_init_simple();

		let v: Vec<(H256, Vec<Result<H256, Error>>)> = vec![
			(
				H256::repeat_byte(0x13),
				vec![Ok(H256::repeat_byte(0x13)), Ok(H256::repeat_byte(0x14))],
			),
			(
				H256::repeat_byte(0x14),
				vec![Ok(H256::repeat_byte(0x13)), Ok(H256::repeat_byte(0x14))],
			),
		];
		let input = HashMap::from_iter(v);
		let r = reduce_multiview_result(input);

		assert_eq!(r, vec![Ok(H256::repeat_byte(0x13)), Ok(H256::repeat_byte(0x14))]);
	}

	#[test]
	fn one_view() {
		sp_tracing::try_init_simple();
		let v: Vec<(H256, Vec<Result<H256, Error>>)> = vec![(
			H256::repeat_byte(0x13),
			vec![Ok(H256::repeat_byte(0x10)), Err(Error::Custom(11))],
		)];
		let input = HashMap::from_iter(v);
		let r = reduce_multiview_result(input);

		assert_eq!(r, vec![Ok(H256::repeat_byte(0x10)), Err(Error::Custom(11))]);
	}

	#[test]
	fn mix() {
		sp_tracing::try_init_simple();
		let v: Vec<(H256, Vec<Result<H256, Error>>)> = vec![
			(
				H256::repeat_byte(0x13),
				vec![
					Ok(H256::repeat_byte(0x10)),
					Err(Error::Custom(11)),
					Err(Error::Custom(12)),
					Err(Error::Custom(33)),
				],
			),
			(
				H256::repeat_byte(0x14),
				vec![
					Err(Error::Custom(20)),
					Ok(H256::repeat_byte(0x21)),
					Err(Error::Custom(22)),
					Err(Error::Custom(33)),
				],
			),
			(
				H256::repeat_byte(0x15),
				vec![
					Err(Error::Custom(30)),
					Err(Error::Custom(31)),
					Ok(H256::repeat_byte(0x32)),
					Err(Error::Custom(33)),
				],
			),
		];
		let input = HashMap::from_iter(v);
		let r = reduce_multiview_result(input);

		assert_eq!(
			r,
			vec![
				Ok(H256::repeat_byte(0x10)),
				Ok(H256::repeat_byte(0x21)),
				Ok(H256::repeat_byte(0x32)),
				Err(Error::Custom(33))
			]
		);
	}
}
