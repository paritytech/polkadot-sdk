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

#![warn(unused_extern_crates)]
#![allow(dead_code)]

use crate::{
	api::FullChainApi,
	enactment_state::{EnactmentAction, EnactmentState},
	graph,
	graph::Options,
	log_xt_debug,
};
use async_trait::async_trait;
use futures::{
	channel::oneshot,
	future::{self},
	prelude::*,
};
use parking_lot::Mutex;
use sc_transaction_pool_api::error::{Error, IntoPoolError};
use sp_runtime::transaction_validity::InvalidTransaction;
use std::{
	collections::{HashMap, HashSet},
	pin::Pin,
	sync::Arc,
};
use tokio::select;

use crate::graph::{ExtrinsicHash, IsValidator};
use futures::FutureExt;
use sc_transaction_pool_api::{
	ChainEvent, ImportNotificationStream, MaintainedTransactionPool, PoolFuture, PoolStatus,
	TransactionFor, TransactionPool, TransactionSource, TransactionStatusStreamFor, TxHash,
};
use sp_core::traits::SpawnEssentialNamed;
use sp_runtime::{
	generic::BlockId,
	traits::{Block as BlockT, Extrinsic, NumberFor},
	transaction_validity::UnknownTransaction,
};
use std::time::Instant;

pub use super::import_notification_sink::ImportNotificationTask;
use super::{
	import_notification_sink::MultiViewImportNotificationSink,
	multi_view_listener::MultiViewListener,
};
use crate::{fork_aware_txpool::view_revalidation, PolledIterator, ReadyIteratorFor, LOG_TARGET};
use prometheus_endpoint::Registry as PrometheusRegistry;
use sp_blockchain::{HashAndNumber, TreeRoute};

pub type FullPool<Block, Client> = ForkAwareTxPool<FullChainApi<Client, Block>, Block>;
use super::{txmempool::TxMemPool, view::View, view_store::ViewStore};

impl<Block, Client> FullPool<Block, Client>
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
	/// Create new basic transaction pool for a full node with the provided api.
	pub fn new_full(
		options: graph::Options,
		is_validator: IsValidator,
		prometheus: Option<&PrometheusRegistry>,
		spawner: impl SpawnEssentialNamed,
		client: Arc<Client>,
	) -> Arc<Self> {
		let pool_api = Arc::new(FullChainApi::new(client.clone(), prometheus, &spawner));
		let pool = Arc::new(Self::new_with_background_queue(
			options,
			is_validator,
			pool_api,
			//todo: add prometheus,
			spawner,
			client.usage_info().chain.best_number,
			client.usage_info().chain.best_hash,
			client.usage_info().chain.finalized_hash,
		));

		pool
	}
}

////////////////////////////////////////////////////////////////////////////////

struct ReadyPoll<T, Block>
where
	Block: BlockT,
{
	pollers: HashMap<<Block as BlockT>::Hash, Vec<oneshot::Sender<T>>>,
}

impl<T, Block> ReadyPoll<T, Block>
where
	Block: BlockT,
{
	fn new() -> Self {
		Self { pollers: Default::default() }
	}

	fn add(&mut self, at: <Block as BlockT>::Hash) -> oneshot::Receiver<T> {
		let (s, r) = oneshot::channel();
		self.pollers.entry(at).or_default().push(s);
		r
	}

	fn trigger(&mut self, at: <Block as BlockT>::Hash, ready_iterator: impl Fn() -> T) {
		log::info!( target: LOG_TARGET,
			"fatp::trigger {at:?} pending keys: {:?}",
			self.pollers.keys());
		let Some(pollers) = self.pollers.remove(&at) else { return };
		pollers.into_iter().for_each(|p| {
			log::info!(target: LOG_TARGET, "trigger ready signal at block {}", at);
			let _ = p.send(ready_iterator());
		});
	}
}

////////////////////////////////////////////////////////////////////////////////

/// The fork-aware transaction pool.
///
/// It keeps track of every fork and provides the set of transactions that is valid for every fork.
pub struct ForkAwareTxPool<ChainApi, Block>
where
	Block: BlockT,
	ChainApi: graph::ChainApi<Block = Block> + 'static,
	<Block as BlockT>::Hash: Unpin,
{
	api: Arc<ChainApi>,
	mempool: Arc<TxMemPool<ChainApi, Block>>,

	view_store: Arc<ViewStore<ChainApi, Block>>,
	ready_poll: Arc<Mutex<ReadyPoll<ReadyIteratorFor<ChainApi>, Block>>>,
	// todo: metrics
	enactment_state: Arc<Mutex<EnactmentState<Block>>>,
	revalidation_queue: Arc<view_revalidation::RevalidationQueue<ChainApi, Block>>,

	import_notification_sink:
		MultiViewImportNotificationSink<Block::Hash, graph::ExtrinsicHash<ChainApi>>,
	options: Options,
	// todo: this are coming from ValidatedPool, some of them maybe needed here
	// is_validator: IsValidator,
	// rotator: PoolRotator<ExtrinsicHash<B>>,
}

impl<ChainApi, Block> ForkAwareTxPool<ChainApi, Block>
where
	Block: BlockT,
	ChainApi: graph::ChainApi<Block = Block> + 'static,
	<Block as BlockT>::Hash: Unpin,
{
	/// Create new fork aware transaction pool with provided api, for tests.
	pub fn new_test(
		pool_api: Arc<ChainApi>,
		best_block_hash: Block::Hash,
		finalized_hash: Block::Hash,
	) -> (Self, ImportNotificationTask) {
		let listener = Arc::from(MultiViewListener::new());
		let (import_notification_sink, import_notification_sink_task) =
			MultiViewImportNotificationSink::new_with_worker();

		(
			Self {
				mempool: Arc::from(TxMemPool::new(pool_api.clone(), listener.clone())),
				api: pool_api.clone(),
				view_store: Arc::new(ViewStore::new(pool_api, listener)),
				ready_poll: Arc::from(Mutex::from(ReadyPoll::new())),
				enactment_state: Arc::new(Mutex::new(EnactmentState::new(
					best_block_hash,
					finalized_hash,
				))),
				revalidation_queue: Arc::from(view_revalidation::RevalidationQueue::new()),
				import_notification_sink,
				options: graph::Options::default(),
			},
			import_notification_sink_task,
		)
	}

	/// Create new fork aware transaction pool with provided api.
	///
	/// The txpool essential tasks are spawned using provided spawner.
	pub fn new_with_background_queue(
		options: graph::Options,
		_is_validator: IsValidator,
		pool_api: Arc<ChainApi>,
		// todo: prometheus: Option<&PrometheusRegistry>,
		spawner: impl SpawnEssentialNamed,
		_best_block_number: NumberFor<Block>,
		best_block_hash: Block::Hash,
		finalized_hash: Block::Hash,
	) -> Self {
		let listener = Arc::from(MultiViewListener::new());
		let (revalidation_queue, revalidation_task) =
			view_revalidation::RevalidationQueue::new_with_worker();

		let (import_notification_sink, import_notification_sink_task) =
			MultiViewImportNotificationSink::new_with_worker();

		//todo: error handling?
		//todo: is it a really good idea? (revalidation_task may be quite heavy)
		let combined_tasks = async move {
			tokio::select! {
				_ = revalidation_task => {},
				_ = import_notification_sink_task => {},
			}
		}
		.boxed();
		spawner.spawn_essential("txpool-background", Some("transaction-pool"), combined_tasks);

		Self {
			mempool: Arc::from(TxMemPool::new(pool_api.clone(), listener.clone())),
			api: pool_api.clone(),
			view_store: Arc::new(ViewStore::new(pool_api, listener)),
			ready_poll: Arc::from(Mutex::from(ReadyPoll::new())),
			enactment_state: Arc::new(Mutex::new(EnactmentState::new(
				best_block_hash,
				finalized_hash,
			))),
			revalidation_queue: Arc::from(revalidation_queue),
			import_notification_sink,
			options,
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
	pub fn views_len(&self) -> usize {
		self.view_store.views.read().len()
	}

	/// Provides internal views statistics.
	///
	/// Provides block number, count of ready, count of future transactions for every view. It is
	/// suitable for printing log information.
	fn views_numbers(&self) -> Vec<(NumberFor<Block>, usize, usize)> {
		self.view_store
			.views
			.read()
			.iter()
			.map(|v| (v.1.at.number, v.1.status().ready, v.1.status().future))
			.collect()
	}

	/// Checks if there is a view at the tip of the fork with given hash.
	pub fn has_view(&self, hash: &Block::Hash) -> bool {
		self.view_store.views.read().contains_key(hash)
	}

	/// Returns numbder of unwatched and watched transactions in internal mempool.
	///
	/// Intended for use in unit tests.
	pub fn mempool_len(&self) -> (usize, usize) {
		self.mempool.len()
	}

	/// Returns best effort set of ready transactions for given block, without executing full
	/// maintain process
	//todo: better naming?
	fn ready_light(&self, at: Block::Hash) -> PolledIterator<ChainApi> {
		let start = Instant::now();
		log::info!( target: LOG_TARGET, "fatp::ready_light {:?}", at);

		let Ok(block_number) = self.api.resolve_block_number(at) else {
			let empty: ReadyIteratorFor<ChainApi> = Box::new(std::iter::empty());
			return Box::pin(async { empty })
		};

		let (best_view, best_tree_route) = {
			let views = self.view_store.views.read();
			let retracted_views = self.view_store.retracted_views.read();
			let mut best_tree_route = None;
			let mut best_view = None;
			let mut best_enacted_len = usize::MAX;
			for v in views.values().chain(retracted_views.values()) {
				let tree_route = self.api.tree_route(v.at.hash, at);
				if let Ok(tree_route) = tree_route {
					log::info!( target: LOG_TARGET, "fatp::ready_light {} tree_route from: {} e:{} r:{}", at,v.at.hash,tree_route.enacted().len(), tree_route.retracted().len());
					if tree_route.retracted().is_empty() &&
						tree_route.enacted().len() < best_enacted_len
					{
						best_enacted_len = tree_route.enacted().len();
						best_view = Some(v.clone());
						best_tree_route = Some(tree_route);
					}
				}
			}
			(best_view, best_tree_route)
		};

		let api = self.api.clone();

		Box::pin(async move {
			if let (Some(best_tree_route), Some(best_view)) = (best_tree_route, best_view) {
				let tmp_view = View::new_from_other(
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

				let before_count = tmp_view.pool.validated_pool().ready().count();
				// tmp_view.pool.validated_pool().remove_invalid(&all_extrinsics[..]);

				// todo: do we need to send ready event? (that requires registering all listener for
				// every txs). But maybe we only use transactions already imported to ready. No new
				// tags appear so future txs shall not promoted.
				// ++ todo: add light_prune?
				let in_pool_tags = tmp_view.pool.validated_pool().extrinsics_tags(&all_extrinsics);

				let mut tags = Vec::new();
				for i in in_pool_tags {
					match i {
						// reuse the tags for extrinsics that were found in the pool
						Some(t) => tags.extend(t),
						None => {},
					}
				}
				let _ = tmp_view.pool.validated_pool().prune_tags(tags);
				// --

				let after_count = tmp_view.pool.validated_pool().ready().count();
				log::info!( target: LOG_TARGET,
					"fatp::ready_light {} from {} before: {} removed: {} after: {} took:{:?}",
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
				log::info!( target: LOG_TARGET, "fatp::ready_light {} -> empty, took:{:?}", at, start.elapsed());
				empty
			}
		})
	}

	fn ready_at_with_timeout_internal(
		&self,
		at: Block::Hash,
		timeout: std::time::Duration,
	) -> PolledIterator<ChainApi> {
		log::debug!(target: LOG_TARGET, "fatp::ready_at_with_timeout at {:?} allowed delay: {:?}", at, timeout);

		let timeout = futures_timer::Delay::new(timeout);
		let fall_back_ready = self.ready_light(at).map(|ready| Some(ready));
		let ready_at = self.ready_at(at);

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

		Box::pin(async {
			let results = futures::future::join(maybe_ready.boxed(), fall_back_ready.boxed()).await;
			if let Some(ready) = results.0 {
				ready
			} else {
				results.1.expect("Fallback value is always Some. qed")
			}
		})
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
fn reduce_multiview_result<H, E>(input: &mut HashMap<H, Vec<Result<H, E>>>) -> Vec<Result<H, E>> {
	let mut values = input.values();
	let Some(first) = values.next() else {
		return Default::default();
	};
	let length = first.len();
	assert!(values.all(|x| length == x.len()));

	let mut output = Vec::with_capacity(length);
	for _ in 0..length {
		let ith_results = input
			.values_mut()
			.map(|values_for_view| values_for_view.pop().expect(""))
			.reduce(|mut r, v| {
				if r.is_err() && v.is_ok() {
					r = v;
				}
				r
			});

		output.push(ith_results.expect("views contain at least one entry. qed."));
	}
	output.into_iter().rev().collect()
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
		let mut input = HashMap::default();
		let r = reduce_multiview_result::<H256, Error>(&mut input);
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
		let mut input = HashMap::from_iter(v.clone());
		let r = reduce_multiview_result(&mut input);

		//order in HashMap is random, the result shall be one of:
		assert!(r == v[0].1 || r == v[1].1 || r == v[2].1);
	}

	#[test]
	#[should_panic]
	fn invalid_lengths() {
		sp_tracing::try_init_simple();
		let v: Vec<(H256, Vec<Result<H256, Error>>)> = vec![
			(H256::repeat_byte(0x13), vec![Err(Error::Custom(12)), Err(Error::Custom(13))]),
			(H256::repeat_byte(0x14), vec![Err(Error::Custom(23))]),
		];
		let mut input = HashMap::from_iter(v);
		let _ = reduce_multiview_result(&mut input);
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
		let mut input = HashMap::from_iter(v);
		let r = reduce_multiview_result(&mut input);

		assert_eq!(r, vec![Ok(H256::repeat_byte(0x13)), Ok(H256::repeat_byte(0x14))]);
	}

	#[test]
	fn one_view() {
		sp_tracing::try_init_simple();
		let v: Vec<(H256, Vec<Result<H256, Error>>)> = vec![(
			H256::repeat_byte(0x13),
			vec![Ok(H256::repeat_byte(0x10)), Err(Error::Custom(11))],
		)];
		let mut input = HashMap::from_iter(v);
		let r = reduce_multiview_result(&mut input);

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
		let mut input = HashMap::from_iter(v);
		let r = reduce_multiview_result(&mut input);

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

impl<ChainApi, Block> TransactionPool for ForkAwareTxPool<ChainApi, Block>
where
	Block: BlockT,
	ChainApi: 'static + graph::ChainApi<Block = Block>,
	<Block as BlockT>::Hash: Unpin,
{
	type Block = ChainApi::Block;
	type Hash = graph::ExtrinsicHash<ChainApi>;
	type InPoolTransaction = graph::base_pool::Transaction<TxHash<Self>, TransactionFor<Self>>;
	type Error = ChainApi::Error;

	fn submit_at(
		&self,
		_: <Self::Block as BlockT>::Hash,
		source: TransactionSource,
		xts: Vec<TransactionFor<Self>>,
	) -> PoolFuture<Vec<Result<TxHash<Self>, Self::Error>>, Self::Error> {
		let view_store = self.view_store.clone();
		log::info!(target: LOG_TARGET, "fatp::submit_at count:{} views:{}", xts.len(), self.views_len());
		log_xt_debug!(target: LOG_TARGET, xts.iter().map(|xt| self.tx_hash(xt)), "[{:?}] fatp::submit_at");
		self.mempool.extend_unwatched(xts.clone());
		let xts = xts.clone();

		if view_store.is_empty() {
			return future::ready(Ok(xts
				.iter()
				.map(|xt| Ok(self.api.hash_and_length(xt).0))
				.collect()))
			.boxed()
		}

		// todo:
		// self.metrics
		// 	.report(|metrics| metrics.submitted_transactions.inc_by(xts.len() as u64));

		async move {
			let mut results_map = view_store.submit_at(source, xts).await;
			Ok(reduce_multiview_result(&mut results_map))
		}
		.boxed()
	}

	fn submit_one(
		&self,
		_: <Self::Block as BlockT>::Hash,
		source: TransactionSource,
		xt: TransactionFor<Self>,
	) -> PoolFuture<TxHash<Self>, Self::Error> {
		log::debug!(target: LOG_TARGET, "[{:?}] fatp::submit_one views:{}", self.tx_hash(&xt), self.views_len());
		// todo:
		// self.metrics.report(|metrics| metrics.submitted_transactions.inc());
		self.mempool.push_unwatched(xt.clone());

		// assume that transaction may be valid, will be validated later.
		let view_store = self.view_store.clone();
		if view_store.is_empty() {
			return future::ready(Ok(self.api.hash_and_length(&xt).0)).boxed()
		}

		async move {
			let results = view_store.submit_one(source, xt).await;
			let results = results
				.into_values()
				.reduce(|mut r, v| {
					if r.is_err() && v.is_ok() {
						r = v;
					}
					r
				})
				.expect("there is at least one entry in input");
			results
		}
		.boxed()
	}

	fn submit_and_watch(
		&self,
		at: <Self::Block as BlockT>::Hash,
		source: TransactionSource,
		xt: TransactionFor<Self>,
	) -> PoolFuture<Pin<Box<TransactionStatusStreamFor<Self>>>, Self::Error> {
		log::debug!(target: LOG_TARGET, "[{:?}] fatp::submit_and_watch views:{}", self.tx_hash(&xt), self.views_len());
		self.mempool.push_watched(xt.clone());

		// todo:
		// self.metrics.report(|metrics| metrics.submitted_transactions.inc());

		let view_store = self.view_store.clone();
		async move { view_store.submit_and_watch(at, source, xt).await }.boxed()
	}

	// todo: api change? we need block hash here (assuming we need it at all).
	fn remove_invalid(&self, hashes: &[TxHash<Self>]) -> Vec<Arc<Self::InPoolTransaction>> {
		log_xt_debug!(target:LOG_TARGET, hashes, "[{:?}] fatp::remove_invalid");

		//revlidate check: what hash shall be used here?
		// for tx in ready {
		// 	let validation_result = self
		// 		.api
		// 		.validate_transaction(block_hash, TransactionSource::External, tx.data.clone())
		// 		.await;
		// 	log::debug!(target:LOG_TARGET, "[{:?}] is ready in view {:?} validation result {:?}",
		// tx.hash, block_hash, validation_result); }

		//todo:
		// let removed = self.pool.validated_pool().remove_invalid(hashes);
		// removed

		//todo:
		// self.metrics
		// 	.report(|metrics| metrics.validations_invalid.inc_by(removed.len() as u64));

		// todo: what to do here?
		// unimplemented!()
		Default::default()
	}

	// todo: probably API change to:
	// status(Hash) -> Option<PoolStatus>
	//
	// todo: move to ViewStore
	fn status(&self) -> PoolStatus {
		self.view_store
			.most_recent_view
			.read()
			.map(|hash| self.view_store.status()[&hash].clone())
			.unwrap_or(PoolStatus { ready: 0, ready_bytes: 0, future: 0, future_bytes: 0 })
	}

	/// Return an event stream of notifications for when transactions are imported to the pool.
	///
	/// Consumers of this stream should use the `ready` method to actually get the
	/// pending transactions in the right order.
	fn import_notification_stream(&self) -> ImportNotificationStream<ExtrinsicHash<ChainApi>> {
		//todo: can we do better, block_on smells like problems...
		futures::executor::block_on(self.import_notification_sink.event_stream())
	}

	fn hash_of(&self, xt: &TransactionFor<Self>) -> TxHash<Self> {
		self.api().hash_and_length(xt).0
	}

	fn on_broadcasted(&self, propagations: HashMap<TxHash<Self>, Vec<String>>) {
		self.view_store.listener.transactions_broadcasted(propagations);
	}

	// todo: api change: we probably should have at here?
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

	fn ready_at(&self, at: <Self::Block as BlockT>::Hash) -> PolledIterator<ChainApi> {
		if let Some((view, retracted)) = self.view_store.get_view_at(at, true) {
			log::info!( target: LOG_TARGET, "fatp::ready_at {:?} (retracted:{:?})", at, retracted);
			let iterator: ReadyIteratorFor<ChainApi> = Box::new(view.pool.validated_pool().ready());
			return async move { iterator }.boxed();
		}

		let pending = self
			.ready_poll
			.lock()
			.add(at)
			.map(|received| {
				received.unwrap_or_else(|e| {
					log::warn!(target: LOG_TARGET, "Error receiving ready-set iterator: {:?}", e);
					Box::new(std::iter::empty())
				})
			})
			.boxed();
		log::info!( target: LOG_TARGET,
			"fatp::ready_at {at:?} pending keys: {:?}",
			self.ready_poll.lock().pollers.keys());
		pending
	}

	fn ready(&self) -> ReadyIteratorFor<ChainApi> {
		self.view_store.ready()
	}

	fn futures(&self) -> Vec<Self::InPoolTransaction> {
		self.view_store.futures()
	}

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
	type Hash = graph::ExtrinsicHash<FullChainApi<Client, Block>>;
	type Error = <FullChainApi<Client, Block> as graph::ChainApi>::Error;

	fn submit_local(
		&self,
		_at: Block::Hash,
		_xt: sc_transaction_pool_api::LocalTransactionFor<Self>,
	) -> Result<Self::Hash, Self::Error> {
		//todo
		//looks like view_store / view needs non async submit_local method ?.
		unimplemented!();
	}
}

impl<ChainApi, Block> ForkAwareTxPool<ChainApi, Block>
where
	Block: BlockT,
	ChainApi: graph::ChainApi<Block = Block> + 'static,
	<Block as BlockT>::Hash: Unpin,
{
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
			log::debug!(
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

	async fn build_new_view(
		&self,
		origin_view: Option<Arc<View<ChainApi>>>,
		at: &HashAndNumber<Block>,
		tree_route: &TreeRoute<Block>,
	) -> Option<Arc<View<ChainApi>>> {
		log::info!(
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
			View::new(self.api.clone(), at.clone(), self.options.clone())
		};

		//we need to capture all import notifiication from the very beginning
		self.import_notification_sink
			.add_view(view.at.hash, view.pool.validated_pool().import_notification_stream().boxed())
			.await;

		let start = Instant::now();
		self.update_view(&mut view).await;
		let duration = start.elapsed();
		log::info!(target: LOG_TARGET, "update_view_pool: at {at:?} took {duration:?}");

		let start = Instant::now();
		self.update_view_with_fork(&view, tree_route, at.clone()).await;
		let duration = start.elapsed();
		log::info!(target: LOG_TARGET, "update_view_fork: at {at:?} took {duration:?}");

		let view = Arc::from(view);
		self.view_store.insert_new_view(view.clone(), tree_route).await;
		Some(view)
	}

	async fn extrinsics_included_since_finalized(&self, at: Block::Hash) -> HashSet<TxHash<Self>> {
		let start = Instant::now();
		let recent_finalized_block = self.enactment_state.lock().recent_finalized_block();

		let Ok(tree_route) = self.api.tree_route(recent_finalized_block, at) else {
			return Default::default()
		};

		let api = self.api.clone();
		let mut all_extrinsics = HashSet::new();

		for h in tree_route.enacted().iter().rev().skip(1) {
			api.block_body(h.hash)
				.await
				.unwrap_or_else(|e| {
					log::warn!(target: LOG_TARGET, "Compute ready light transactions: error request: {}", e);
					None
				})
				.unwrap_or_default()
				.into_iter()
				.map(|t| api.hash_and_length(&t).0)
				.for_each(|tx_hash| {
					all_extrinsics.insert(tx_hash);
				});
		}

		log::info!( target: LOG_TARGET,
			"fatp::extrinsics_included_since_finalized {} from {} count: {} took:{:?}",
			at,
			recent_finalized_block,
			all_extrinsics.len(),
			start.elapsed()
		);
		all_extrinsics
	}

	//todo: maybe move to ViewManager
	async fn update_view(&self, view: &View<ChainApi>) {
		log::debug!(
			target: LOG_TARGET,
			"update_view: {:?} xts:{:?} v:{}",
			view.at,
			self.mempool.len(),
			self.views_len()
		);
		//todo: source?
		let source = TransactionSource::External;

		//this could be collected/cached in view
		let included_xts = self.extrinsics_included_since_finalized(view.at.hash).await;

		//todo this clone is not neccessary, try to use iterators
		let xts = self.mempool.clone_unwatched();

		if !xts.is_empty() {
			let unwatched_count = xts.len();
			let xts = xts
				.into_iter()
				.filter(|tx| {
					!view.pool.validated_pool().pool.read().is_imported(&self.hash_of(&tx))
				})
				.filter(|tx| !included_xts.contains(&self.hash_of(&tx)))
				.collect::<Vec<_>>();
			let filtered_count = xts.len();
			//todo: internal checked banned: not required any more?
			let _ = view.submit_many(source, xts).await;
			log::info!(target: LOG_TARGET, "update_view_pool: at {:?} unwatched {}/{}", view.at.hash, filtered_count, unwatched_count);
		}
		let view = Arc::from(view);

		let included_xts = Arc::from(included_xts);
		//todo: hash computed multiple times
		//todo: maybe we don't need to register listener in view? We could use
		// multi_view_listner.transcation_in_block

		let results = self
			.mempool
			.watched_xts()
			.map(|t| {
				let view = view.clone();
				let included_xts = included_xts.clone();
				async move {
					let tx_hash = self.hash_of(&t);
					let result = if view.pool.validated_pool().pool.read().is_imported(&tx_hash) || included_xts.contains(&tx_hash)
					{
						Err(Error::AlreadyImported(Box::new(tx_hash)).into())
					} else {
						view.submit_and_watch(source, t.clone()).await
					};
					let result = result.map_or_else(
						|error| {
							let error = error.into_pool_error();
							log::trace!(
								target: LOG_TARGET,
								"[{:?}] update_view: submit_and_watch result: {:?} {:?}",
								tx_hash,
								view.at.hash,
								error,
							);
							match error {
								// We need to install listener for stale xt: in case of
								// transaction being already included in the block we want to
								// send inblock + finalization event.
								// The same applies for TemporarilyBanned / AlreadyImported. We
								// need to create listener.
								Ok(
									Error::InvalidTransaction(InvalidTransaction::Stale) |
									Error::TemporarilyBanned |
									Error::AlreadyImported(_),
								) => Ok(view.create_watcher(tx_hash)),
								//ignore
								Ok(
									//todo: shall be: Error::InvalidTransaction(_)
									Error::InvalidTransaction(InvalidTransaction::Custom(_)),
								) => Err((error.expect("already in Ok arm. qed."), tx_hash, t)),
								//todo: panic while testing
								_ => {
									// Err(crate::error::Error::RuntimeApi(_)) => {
									//todo:
									//Err(RuntimeApi("Api called for an unknown Block: State
									// already discarded for
									// 0x881b8b0e32780e99c1dfb353f6850cdd8271e05b551f0f29d3e12dd09520efda"
									// ))',
									log::error!(target: LOG_TARGET, "[{:?}] txpool: update_view: somehing went wrong: {error:?}", tx_hash);
									Err((
										Error::UnknownTransaction(UnknownTransaction::CannotLookup),
										tx_hash,
										t,
									))
								},
								// _ => {
								// 	panic!("[{:?}] txpool: update_view: somehing went wrong:
								// {error:?}", tx_hash); },
							}
						},
						|x| Ok(x),
					);

					if let Ok(watcher) = result {
						log::trace!(target: LOG_TARGET, "[{:?}] adding watcher {:?}", tx_hash, view.at.hash);
						self.view_store
							.listener
							.add_view_watcher_for_tx(
								tx_hash,
								view.at.hash,
								watcher.into_stream().boxed(),
							);
						Ok(())
					} else {
						result.map(|_| ())
					}
				}
			})
			.collect::<Vec<_>>();

		let results = future::join_all(results).await;

		log::info!(target: LOG_TARGET, "update_view_pool: at {:?} watched {}/{}", view.at.hash, results.len(), self.mempool_len().1);

		// if there are no views yet, and a single newly created view is reporting error, just send
		// out the invalid event, and remove transaction.
		if self.view_store.is_empty() {
			for result in results {
				match result {
					Err((Error::TemporarilyBanned | Error::AlreadyImported(_), ..)) => {},
					Err((Error::InvalidTransaction(_), tx_hash, tx)) => {
						self.view_store.listener.invalidate_transactions(vec![tx_hash]);
						self.mempool.remove_watched(&tx);
					},

					_ => {},
				}
			}
		}
	}

	//todo: maybe move to ViewManager
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
				// .chain(std::iter::once(&hash_and_number))
				.map(|h| crate::prune_known_txs_for_block(h, &*api, &view.pool)),
		)
		.await
		.into_iter()
		.for_each(|enacted_log| {
			pruned_log.extend(enacted_log);
		});

		// todo: metrics (does pruned makes sense?)
		// self.metrics
		// 	.report(|metrics| metrics.block_transactions_pruned.inc_by(pruned_log.len() as u64));

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
					.into_iter()
					.filter(|tx| tx.is_signed().unwrap_or(true));

				let mut resubmitted_to_report = 0;

				resubmit_transactions.extend(block_transactions.into_iter().filter(|tx| {
					let tx_hash = self.api.hash_and_length(tx).0;
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
				}));

				// todo: metrics (does resubmit makes sense?)
				// self.metrics.report(|metrics| {
				// 	metrics.block_transactions_resubmitted.inc_by(resubmitted_to_report)
				// });
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

	async fn handle_finalized(&self, finalized_hash: Block::Hash, tree_route: &[Block::Hash]) {
		let finalized_number = self.api.block_id_to_number(&BlockId::Hash(finalized_hash));
		log::info!(target: LOG_TARGET, "handle_finalized {finalized_number:?} tree_route: {tree_route:?}");

		let finalized_xts = self.view_store.handle_finalized(finalized_hash, tree_route).await;
		log::debug!(target: LOG_TARGET, "handle_finalized b:{:?}", self.views_len());

		self.mempool.purge_finalized_transactions(&finalized_xts).await;
		self.import_notification_sink.clean_filter(&finalized_xts).await;

		if let Ok(Some(finalized_number)) = finalized_number {
			self.revalidation_queue
				.purge_transactions_later(
					self.mempool.clone(),
					HashAndNumber { hash: finalized_hash, number: finalized_number },
				)
				.await;
		} else {
			log::debug!(target: LOG_TARGET, "purge_transactions_later skipped, cannot find block number {finalized_number:?}");
		}

		//todo:
		//delete old keys in ReadyPoll.pollers (little memleak possible)
		log::debug!(target: LOG_TARGET, "handle_finalized a:{:?}", self.views_len());
	}

	fn tx_hash(&self, xt: &TransactionFor<Self>) -> TxHash<Self> {
		self.api.hash_and_length(xt).0
	}

	async fn verify(&self) {
		log::info!(target:LOG_TARGET, "fatp::verify++");

		let views_ready_txs = {
			let views = self.view_store.views.read();

			views
				.values()
				.map(|view| {
					let ready = view.pool.validated_pool().ready();
					let future = view.pool.validated_pool().futures();
					(view.at.hash, ready.collect::<Vec<_>>(), future)
				})
				.collect::<Vec<_>>()
		};

		for view in views_ready_txs {
			let block_hash = view.0;
			let ready = view.1;
			for tx in ready {
				let validation_result = self
					.api
					.validate_transaction(block_hash, TransactionSource::External, tx.data.clone())
					.await;
				log::debug!(target:LOG_TARGET, "[{:?}] is ready in view {:?} validation result {:?}", tx.hash, block_hash, validation_result);
			}
			let future = view.2;
			for tx in future {
				let validation_result = self
					.api
					.validate_transaction(block_hash, TransactionSource::External, tx.1.clone())
					.await;
				log::debug!(target:LOG_TARGET, "[{:?}] is future in view {:?} validation result {:?}", tx.0, block_hash, validation_result);
			}
		}
		log::info!(target:LOG_TARGET, "fatp::verify--");
	}
}

#[async_trait]
impl<ChainApi, Block> MaintainedTransactionPool for ForkAwareTxPool<ChainApi, Block>
where
	Block: BlockT,
	ChainApi: 'static + graph::ChainApi<Block = Block>,
	<Block as BlockT>::Hash: Unpin,
{
	async fn maintain(&self, event: ChainEvent<Self::Block>) {
		let start = Instant::now();

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
				log::debug!(target: LOG_TARGET, "enactment_state::update error: {msg}");
				self.enactment_state.lock().force_update(&event);
			},
			Ok(EnactmentAction::Skip) => return,
			Ok(EnactmentAction::HandleFinalization) => {
				// todo: in some cases handle_new_block is actually needed (new_num > tips_of_forks)
				// let hash = event.hash();
				// if !self.has_view(hash) {
				// 	if let Ok(tree_route) = compute_tree_route(prev_finalized_block, hash) {
				// 		self.handle_new_block(&tree_route).await;
				// 	}
				// }
			},
			Ok(EnactmentAction::HandleEnactment(tree_route)) =>
				self.handle_new_block(&tree_route).await,
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

		log::info!(
			target: LOG_TARGET,
			"maintain: txs:{:?} views:[{};{:?}] event:{event:?}  took:{:?}",
			self.mempool_len(),
			self.views_len(),
			self.views_numbers(),
			start.elapsed()
		);

		// self.verify().await;

		()
	}
}
