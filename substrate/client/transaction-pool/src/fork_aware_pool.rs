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

//todo:
#![allow(missing_docs)]
#![warn(unused_extern_crates)]
//todo:
#![allow(unused_imports)]
//todo:
#![allow(unused_variables)]
#![allow(dead_code)]

// todo: remove:
// This is cleaned copy of src/lib.rs.

use crate::graph;
pub use crate::{
	api::FullChainApi,
	enactment_state::{EnactmentAction, EnactmentState},
	graph::{
		base_pool::Limit as PoolLimit, watcher::Watcher, ChainApi, Options, Pool, Transaction,
		ValidatedTransaction,
	},
};
use async_trait::async_trait;
use futures::{
	channel::{
		mpsc::{channel, Sender},
		oneshot,
	},
	future::{self, ready},
	prelude::*,
};
use parking_lot::{Mutex, RwLock};
use sc_transaction_pool_api::error::{Error, IntoPoolError};
use sp_runtime::transaction_validity::InvalidTransaction;
use std::{
	collections::{HashMap, HashSet},
	pin::Pin,
	sync::{
		atomic::{AtomicBool, Ordering},
		Arc,
	},
};

use crate::graph::{ExtrinsicFor, ExtrinsicHash, IsValidator};
use futures::FutureExt;
use sc_transaction_pool_api::{
	error::Error as TxPoolError, ChainEvent, ImportNotificationStream, MaintainedTransactionPool,
	PoolFuture, PoolStatus, ReadyTransactions, TransactionFor, TransactionPool, TransactionSource,
	TransactionStatusStreamFor, TxHash,
};
use sp_core::traits::SpawnEssentialNamed;
use sp_runtime::{
	generic::BlockId,
	traits::{
		AtLeast32Bit, Block as BlockT, Extrinsic, Hash as HashT, Header as HeaderT, NumberFor, Zero,
	},
	transaction_validity::UnknownTransaction,
};
use std::time::Instant;

pub use import_notification_sink::ImportNotificationTask;
use import_notification_sink::MultiViewImportNotificationSink;
use multi_view_listener::MultiViewListener;
use sp_blockchain::{HashAndNumber, TreeRoute};
use sp_runtime::transaction_validity::TransactionValidityError;

mod import_notification_sink;
mod multi_view_listener;
mod view_revalidation;

pub(crate) const LOG_TARGET: &str = "txpool";

pub struct View<PoolApi: graph::ChainApi> {
	pool: graph::Pool<PoolApi>,
	at: HashAndNumber<PoolApi::Block>,
	accept_xts: AtomicBool,
}

impl<PoolApi> View<PoolApi>
where
	PoolApi: graph::ChainApi,
{
	fn new(api: Arc<PoolApi>, at: HashAndNumber<PoolApi::Block>) -> Self {
		//todo!!
		use crate::graph::base_pool::Limit;
		let options = graph::Options {
			ready: Limit { count: 100000, total_bytes: 200 * 1024 * 1024 },
			future: Limit { count: 100000, total_bytes: 200 * 1024 * 1024 },
			reject_future_transactions: false,
			ban_time: core::time::Duration::from_secs(60 * 30),
		};

		Self {
			pool: graph::Pool::new(options, true.into(), api),
			at,
			accept_xts: AtomicBool::new(true),
		}
	}

	fn new_from_other(&self, at: &HashAndNumber<PoolApi::Block>) -> Self {
		View { at: at.clone(), pool: self.pool.deep_clone(), accept_xts: AtomicBool::new(true) }
	}

	async fn finalize(&self, finalized: graph::BlockHash<PoolApi>) {
		log::debug!("View::finalize: {:?} {:?}", self.at, finalized);
		self.disable();
		let _ = self.pool.validated_pool().on_block_finalized(finalized).await;
	}

	fn accept_xts(&self) -> bool {
		self.accept_xts.load(Ordering::Relaxed)
		// true
	}

	fn disable(&self) {
		self.accept_xts.store(false, Ordering::Relaxed);
	}

	pub async fn submit_many(
		&self,
		source: TransactionSource,
		xts: impl IntoIterator<Item = ExtrinsicFor<PoolApi>>,
	) -> Vec<Result<ExtrinsicHash<PoolApi>, PoolApi::Error>> {
		self.pool.submit_at(&self.at, source, xts).await
	}

	/// Imports one unverified extrinsic to the pool
	pub async fn submit_one(
		&self,
		source: TransactionSource,
		xt: ExtrinsicFor<PoolApi>,
	) -> Result<ExtrinsicHash<PoolApi>, PoolApi::Error> {
		self.pool.submit_one(&self.at, source, xt).await
	}

	/// Import a single extrinsic and starts to watch its progress in the pool.
	pub async fn submit_and_watch(
		&self,
		source: TransactionSource,
		xt: ExtrinsicFor<PoolApi>,
	) -> Result<Watcher<ExtrinsicHash<PoolApi>, ExtrinsicHash<PoolApi>>, PoolApi::Error> {
		self.pool.submit_and_watch(&self.at, source, xt).await
	}

	pub fn status(&self) -> PoolStatus {
		self.pool.validated_pool().status()
	}

	pub fn create_watcher(
		&self,
		tx_hash: ExtrinsicHash<PoolApi>,
	) -> Watcher<ExtrinsicHash<PoolApi>, ExtrinsicHash<PoolApi>> {
		self.pool.validated_pool().create_watcher(tx_hash)
	}
}

//todo: better name: ViewStore?
pub struct ViewStore<PoolApi, Block>
where
	Block: BlockT,
	PoolApi: graph::ChainApi<Block = Block>,
{
	api: Arc<PoolApi>,
	views: RwLock<HashMap<Block::Hash, Arc<View<PoolApi>>>>,
	listener: Arc<MultiViewListener<PoolApi>>,
}

impl<PoolApi, Block> ViewStore<PoolApi, Block>
where
	Block: BlockT,
	PoolApi: graph::ChainApi<Block = Block> + 'static,
	<Block as BlockT>::Hash: Unpin,
{
	fn new(api: Arc<PoolApi>, listener: Arc<MultiViewListener<PoolApi>>) -> Self {
		Self { api, views: Default::default(), listener }
	}

	/// Imports a bunch of unverified extrinsics to every view
	pub async fn submit_at(
		&self,
		source: TransactionSource,
		xts: impl IntoIterator<Item = Block::Extrinsic> + Clone,
	) -> HashMap<Block::Hash, Vec<Result<ExtrinsicHash<PoolApi>, PoolApi::Error>>> {
		let results = {
			let s = Instant::now();
			let views = self.views.read();
			log::debug!("submit_one: read: took:{:?}", s.elapsed());
			let futs = views
				.iter()
				.filter(|v| v.1.accept_xts())
				.map(|(hash, view)| {
					let view = view.clone();
					//todo: remove this clone (Arc?)
					let xts = xts.clone();
					async move {
						let s = Instant::now();
						let r = (view.at.hash, view.submit_many(source, xts.clone()).await);
						log::debug!("submit_one: submit_at: took:{:?}", s.elapsed());
						r
					}
				})
				.collect::<Vec<_>>();
			futs
		};
		let results = futures::future::join_all(results).await;

		HashMap::<_, _>::from_iter(results.into_iter())
	}

	/// Imports one unverified extrinsic to every view
	pub async fn submit_one(
		&self,
		source: TransactionSource,
		xt: Block::Extrinsic,
	) -> HashMap<Block::Hash, Result<ExtrinsicHash<PoolApi>, PoolApi::Error>> {
		let mut output = HashMap::new();
		let mut result = self.submit_at(source, std::iter::once(xt)).await;
		result.iter_mut().for_each(|(hash, result)| {
			output.insert(
				*hash,
				result
					.pop()
					.expect("for one transaction there shall be exactly one result. qed"),
			);
		});
		output
	}

	/// Import a single extrinsic and starts to watch its progress in the pool.
	pub async fn submit_and_watch(
		&self,
		at: Block::Hash,
		source: TransactionSource,
		xt: Block::Extrinsic,
	) -> Result<multi_view_listener::TxStatusStream<PoolApi>, PoolApi::Error> {
		let tx_hash = self.api.hash_and_length(&xt).0;
		let external_watcher = self.listener.create_external_watcher_for_tx(tx_hash).await;
		let results = {
			let views = self.views.read();
			let futs = views
				.iter()
				.filter(|v| v.1.accept_xts())
				.map(|(hash, view)| {
					let view = view.clone();
					let xt = xt.clone();

					async move {
						let result = view.submit_and_watch(source, xt).await;
						if let Ok(watcher) = result {
							self.listener
								.add_view_watcher_for_tx(
									tx_hash,
									view.at.hash,
									watcher.into_stream().boxed(),
								)
								.await;
							Ok(())
						} else {
							Err(result.unwrap_err())
						}
					}
				})
				.collect::<Vec<_>>();
			futs
		};
		let maybe_watchers = futures::future::join_all(results).await;
		log::trace!("[{:?}] submit_and_watch: maybe_watchers: {}", tx_hash, maybe_watchers.len());

		//todo: maybe try_fold + ControlFlow ?
		let maybe_error = maybe_watchers.into_iter().reduce(|mut r, v| {
			if r.is_err() && v.is_ok() {
				r = v;
			}
			r
		});
		if let Some(Err(err)) = maybe_error {
			log::debug!("[{:?}] submit_and_watch: err: {}", tx_hash, err);
			return Err(err);
		};

		Ok(external_watcher.unwrap())
	}

	pub fn status(&self) -> HashMap<Block::Hash, PoolStatus> {
		self.views.read().iter().map(|(h, v)| (*h, v.status())).collect()
	}

	pub fn is_empty(&self) -> bool {
		self.views.read().is_empty()
	}

	/// Finds the best existing view to clone from along the path.
	/// Allows to include all the transactions from the imported blocks (that are on the retracted
	/// path) without additional validation. Tip of retracted fork is usually most recent block
	/// processed by txpool.
	///
	/// ```text
	/// Tree route from R1 to E2.
	///   <- R3 <- R2 <- R1
	///  /
	/// C
	///  \-> E1 -> E2
	/// ```
	/// ```text
	/// Search path is:
	/// [R1, R2, R3, C, E1]
	/// ```
	fn find_best_view(&self, tree_route: &TreeRoute<Block>) -> Option<Arc<View<PoolApi>>> {
		let views = self.views.read();
		let best_view = {
			tree_route
				.retracted()
				.iter()
				.chain(std::iter::once(tree_route.common_block()))
				.chain(tree_route.enacted().iter())
				.find(|block| views.contains_key(&block.hash))
		};
		best_view.map(|h| {
			views.get(&h.hash).expect("hash was just found in the map's keys. qed").clone()
		})
	}

	// todo: API change? ready at block?
	fn ready(&self, at: Block::Hash) -> Option<super::ReadyIteratorFor<PoolApi>> {
		let maybe_ready = self.views.read().get(&at).map(|v| v.pool.validated_pool().ready());
		let Some(ready) = maybe_ready else { return None };
		Some(Box::new(ready))
	}

	// todo: API change? futures at block?
	fn futures(
		&self,
		at: Block::Hash,
	) -> Option<Vec<graph::base_pool::Transaction<ExtrinsicHash<PoolApi>, Block::Extrinsic>>> {
		self.views
			.read()
			.get(&at)
			.map(|v| v.pool.validated_pool().pool.read().futures().cloned().collect())
	}

	async fn finalize_route(&self, finalized_hash: Block::Hash, tree_route: &[Block::Hash]) {
		log::debug!(target: LOG_TARGET, "finalize_route finalized_hash:{finalized_hash:?} tree_route: {tree_route:?}");
		let mut no_view_blocks = vec![];
		for hash in tree_route.iter().chain(std::iter::once(&finalized_hash)) {
			let finalized_view = { self.views.read().get(&hash).map(|v| v.clone()) };
			log::trace!(target: LOG_TARGET, "finalize_route block_hash:{hash:?} {no_view_blocks:?} fv:{:#?}", finalized_view.is_some());
			if let Some(finalized_view) = finalized_view {
				for h in no_view_blocks.iter().chain(std::iter::once(hash)) {
					finalized_view.finalize(*h).await;
				}
				no_view_blocks.clear();
			} else {
				no_view_blocks.push(*hash);
			}
		}
	}

	fn ready_transaction(
		&self,
		at: Block::Hash,
		tx_hash: &ExtrinsicHash<PoolApi>,
	) -> Option<Arc<graph::base_pool::Transaction<ExtrinsicHash<PoolApi>, Block::Extrinsic>>> {
		self.views
			.read()
			.get(&at)
			.map(|v| v.pool.validated_pool().ready_by_hash(tx_hash))
			.flatten()
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
		let Some(pollers) = self.pollers.remove(&at) else { return };
		pollers.into_iter().for_each(|p| {
			log::debug!(target: LOG_TARGET, "Sending ready signal at block {}", at);
			let _ = p.send(ready_iterator());
		});
	}
}

////////////////////////////////////////////////////////////////////////////////

pub struct TxInMemPool<Block>
where
	Block: BlockT,
{
	watched: bool,
	tx: Block::Extrinsic,
	source: TransactionSource,
	//todo: add listener? for updating view with invalid transaction?
}

pub struct TxMemPool<PoolApi, Block>
where
	Block: BlockT,
	PoolApi: graph::ChainApi<Block = Block> + 'static,
{
	api: Arc<PoolApi>,
	xts: RwLock<Vec<Block::Extrinsic>>,
	watched_xts: RwLock<Vec<Block::Extrinsic>>,
	listener: Arc<MultiViewListener<PoolApi>>,
	// todo:
	// xts2: HashMap<graph::ExtrinsicHash<PoolApi>, TxInMemPool<PoolApi, Block>>,
}

impl<PoolApi, Block> TxMemPool<PoolApi, Block>
where
	Block: BlockT,
	PoolApi: graph::ChainApi<Block = Block> + 'static,
	<Block as BlockT>::Hash: Unpin,
{
	fn new(api: Arc<PoolApi>, listener: Arc<MultiViewListener<PoolApi>>) -> Self {
		Self { api, xts: Default::default(), watched_xts: Default::default(), listener }
	}

	fn watched_xts(&self) -> impl Iterator<Item = Block::Extrinsic> {
		self.watched_xts.read().clone().into_iter()
	}

	fn len(&self) -> (usize, usize) {
		(self.xts.read().len(), self.watched_xts.read().len())
	}

	fn push_unwatched(&self, xt: Block::Extrinsic) {
		self.xts.write().push(xt)
	}

	fn extend_unwatched(&self, xts: Vec<Block::Extrinsic>) {
		self.xts.write().extend(xts)
	}

	fn push_watched(&self, xt: Block::Extrinsic) {
		self.watched_xts.write().push(xt)
	}

	fn clone_unwatched(&self) -> Vec<Block::Extrinsic> {
		self.xts.read().clone()
	}

	fn remove_watched(&self, xt: &Block::Extrinsic) {
		self.watched_xts.write().retain(|t| t != xt);
	}

	//returns vec of invalid hashes
	async fn validate_array(
		&self,
		xts: impl Iterator<Item = Block::Extrinsic>,
		finalized_block: Block::Hash,
		round: u32,
	) -> Vec<Block::Hash> {
		let xts = xts.map(|xt| (self.api.hash_and_length(&xt).0, xt)).collect::<Vec<_>>();
		let count = xts.len();

		// 	todo: source
		let source = TransactionSource::External;

		let start = Instant::now();
		let validation_results = futures::future::join_all(xts.into_iter().map(|(xt_hash, xt)| {
			self.api
				.validate_transaction(finalized_block, source, xt)
				.map(move |validation_result| (xt_hash, validation_result))
		}))
		.await;

		let duration = start.elapsed();

		let mut invalid_hashes = Vec::new();

		for (xt_hash, validation_result) in validation_results {
			match validation_result {
				Ok(Ok(_)) |
				Ok(Err(TransactionValidityError::Invalid(InvalidTransaction::Future))) => {},
				Err(_) |
				Ok(Err(TransactionValidityError::Unknown(_))) |
				Ok(Err(TransactionValidityError::Invalid(_))) => {
					log::debug!(
						target: LOG_TARGET,
						"[{:?}]: Purging: invalid: {:?}",
						xt_hash,
						validation_result,
					);
					invalid_hashes.push(xt_hash);
				},
			}
		}

		log::info!(
			"purge_transactions {round}: at {finalized_block:?} count:{count:?} purged:{:?} took {duration:?}", invalid_hashes.len()
		);

		invalid_hashes
	}

	async fn purge_transactions(&self, finalized_block: Block::Hash) {
		let invalid_hashes = self
			.validate_array(self.clone_unwatched().into_iter(), finalized_block, 0)
			.await;

		self.xts
			.write()
			.retain(|xt| !invalid_hashes.contains(&self.api.hash_and_length(xt).0));
		self.listener.invalidate_transactions(invalid_hashes).await;

		let invalid_hashes = self.validate_array(self.watched_xts(), finalized_block, 1).await;
		self.watched_xts
			.write()
			.retain(|xt| !invalid_hashes.contains(&self.api.hash_and_length(xt).0));
		self.listener.invalidate_transactions(invalid_hashes).await;
	}
}

////////////////////////////////////////////////////////////////////////////////

pub struct ForkAwareTxPool<PoolApi, Block>
where
	Block: BlockT,
	PoolApi: graph::ChainApi<Block = Block> + 'static,
	<Block as BlockT>::Hash: Unpin,
{
	api: Arc<PoolApi>,
	mempool: Arc<TxMemPool<PoolApi, Block>>,

	// todo: is ViewManager strucy really needed? (no)
	view_store: Arc<ViewStore<PoolApi, Block>>,
	// todo: is ReadyPoll struct really needed? (no)
	ready_poll: Arc<Mutex<ReadyPoll<super::ReadyIteratorFor<PoolApi>, Block>>>,
	// current tree? (somehow similar to enactment state?)
	// todo: metrics
	enactment_state: Arc<Mutex<EnactmentState<Block>>>,
	revalidation_queue: Arc<view_revalidation::RevalidationQueue<PoolApi, Block>>,

	most_recent_view: RwLock<Option<Block::Hash>>,
	import_notification_sink:
		MultiViewImportNotificationSink<Block::Hash, graph::ExtrinsicHash<PoolApi>>,
	// todo: this are coming from ValidatedPool, some of them maybe needed here
	// is_validator: IsValidator,
	// options: Options,
	// rotator: PoolRotator<ExtrinsicHash<B>>,
}

impl<PoolApi, Block> ForkAwareTxPool<PoolApi, Block>
where
	Block: BlockT,
	PoolApi: graph::ChainApi<Block = Block> + 'static,
	<Block as BlockT>::Hash: Unpin,
{
	/// Create new fork aware transaction pool with provided api, for tests.
	pub fn new_test(
		pool_api: Arc<PoolApi>,
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
				most_recent_view: RwLock::from(None),
				import_notification_sink,
			},
			import_notification_sink_task,
		)
	}

	pub fn new_with_background_queue(
		options: graph::Options,
		is_validator: IsValidator,
		pool_api: Arc<PoolApi>,
		// todo: prometheus: Option<&PrometheusRegistry>,
		spawner: impl SpawnEssentialNamed,
		best_block_number: NumberFor<Block>,
		best_block_hash: Block::Hash,
		finalized_hash: Block::Hash,
	) -> Self {
		let listener = Arc::from(MultiViewListener::new());
		let (revalidation_queue, revalidation_task) =
			view_revalidation::RevalidationQueue::new_with_worker();

		let (import_notification_sink, import_notification_sink_task) =
			MultiViewImportNotificationSink::new_with_worker();

		//todo: error handling?
		//todo: is it a really god idea? (revalidation_task may be quite heavy)
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
			most_recent_view: RwLock::from(None),
			import_notification_sink,
		}
	}

	/// Get access to the underlying api
	pub fn api(&self) -> &PoolApi {
		&self.api
	}

	//todo: this should be new TransactionPool API?
	pub fn status_all(&self) -> HashMap<Block::Hash, PoolStatus> {
		self.view_store.status()
	}

	//todo:naming? maybe just views()
	pub fn views_len(&self) -> usize {
		self.view_store.views.read().len()
	}

	pub fn views_accpeting_len(&self) -> usize {
		self.view_store
			.views
			.read()
			.iter()
			.filter(|v| v.1.accept_xts())
			.collect::<Vec<_>>()
			.len()
	}

	pub fn views_numbers(&self) -> Vec<(NumberFor<Block>, usize, usize, bool)> {
		self.view_store
			.views
			.read()
			.iter()
			.map(|v| (v.1.at.number, v.1.status().ready, v.1.status().future, v.1.accept_xts()))
			.collect()
	}

	pub fn has_view(&self, hash: Block::Hash) -> bool {
		self.view_store.views.read().get(&hash).is_some()
	}

	pub fn mempool_len(&self) -> (usize, usize) {
		self.mempool.len()
	}
}

//todo: naming + better doc!
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
	for i in 0..length {
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
		let r = reduce_multiview_result(&mut input);
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

impl<PoolApi, Block> TransactionPool for ForkAwareTxPool<PoolApi, Block>
where
	Block: BlockT,
	PoolApi: 'static + graph::ChainApi<Block = Block>,
	<Block as BlockT>::Hash: Unpin,
{
	type Block = PoolApi::Block;
	type Hash = graph::ExtrinsicHash<PoolApi>;
	type InPoolTransaction = graph::base_pool::Transaction<TxHash<Self>, TransactionFor<Self>>;
	type Error = PoolApi::Error;

	fn submit_at(
		&self,
		_: <Self::Block as BlockT>::Hash,
		source: TransactionSource,
		xts: Vec<TransactionFor<Self>>,
	) -> PoolFuture<Vec<Result<TxHash<Self>, Self::Error>>, Self::Error> {
		let view_store = self.view_store.clone();
		self.mempool.extend_unwatched(xts.clone());
		let xts = xts.clone();

		if view_store.is_empty() {
			return future::ready(Ok(xts
				.iter()
				.map(|xt| {
					//todo: error or ok if no views?
					// Err(TxPoolError::UnknownTransaction(UnknownTransaction::CannotLookup).into())
					Ok(self.api.hash_and_length(xt).0)
				})
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
		let start = Instant::now();
		// todo:
		// self.metrics.report(|metrics| metrics.submitted_transactions.inc());
		let views = self.view_store.clone();
		self.mempool.push_unwatched(xt.clone());

		if views.is_empty() {
			//todo: error or ok if no views?
			return future::ready(Ok(self.api.hash_and_length(&xt).0)).boxed()
			// return future::ready(Err(TxPoolError::UnknownTransaction(
			// 	UnknownTransaction::CannotLookup,
			// )
			// .into()))
			// .boxed()
		}

		let tx_hash = self.hash_of(&xt);
		let view_count = self.views_len();
		async move {
			let s = Instant::now();
			let results = views.submit_one(source, xt).await;
			let d = s.elapsed();
			let results = results
				.into_values()
				.reduce(|mut r, v| {
					if r.is_err() && v.is_ok() {
						r = v;
					}
					r
				})
				.expect("there is at least one entry in input");

			let duration = start.elapsed();

			log::debug!("[{tx_hash:?}] submit_one: views:{view_count} took {duration:?} {d:?}");

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
		let view_store = self.view_store.clone();
		self.mempool.push_watched(xt.clone());

		// todo:
		// self.metrics.report(|metrics| metrics.submitted_transactions.inc());

		async move {
			let result = view_store.submit_and_watch(at, source, xt).await;
			match result {
				Ok(watcher) => Ok(watcher),
				Err(err) => Err(err),
			}
			// let watcher = result?;
			// let watcher = views.submit_and_watch(at, source, xt).await?;
			// watcher
		}
		.boxed()
	}

	// todo: api change? we need block hash here (assuming we need it at all).
	fn remove_invalid(&self, hashes: &[TxHash<Self>]) -> Vec<Arc<Self::InPoolTransaction>> {
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
	fn status(&self) -> PoolStatus {
		self.most_recent_view
			.read()
			.map(|hash| self.view_store.status()[&hash].clone())
			.unwrap_or(PoolStatus { ready: 0, ready_bytes: 0, future: 0, future_bytes: 0 })
	}

	/// Return an event stream of notifications for when transactions are imported to the pool.
	///
	/// Consumers of this stream should use the `ready` method to actually get the
	/// pending transactions in the right order.
	fn import_notification_stream(&self) -> ImportNotificationStream<ExtrinsicHash<PoolApi>> {
		futures::executor::block_on(self.import_notification_sink.event_stream())
	}

	fn hash_of(&self, xt: &TransactionFor<Self>) -> TxHash<Self> {
		self.api().hash_and_length(xt).0
	}

	fn on_broadcasted(&self, propagations: HashMap<TxHash<Self>, Vec<String>>) {
		// self.pool.validated_pool().on_broadcasted(propagations)
		// unimplemented!()
	}

	// todo: api change?
	fn ready_transaction(&self, tx_hash: &TxHash<Self>) -> Option<Arc<Self::InPoolTransaction>> {
		// unimplemented!()
		let result = self
			.most_recent_view
			.read()
			.map(|block_hash| self.view_store.ready_transaction(block_hash, tx_hash))
			.flatten();
		log::debug!(
			"{tx_hash:?} ready_transaction: {} {:?}",
			result.is_some(),
			self.most_recent_view.read()
		);
		result
	}

	// todo: API change? ready at hash (not number)?
	fn ready_at(&self, at: <Self::Block as BlockT>::Hash) -> super::PolledIterator<PoolApi> {
		if let Some(view) = self.view_store.views.read().get(&at) {
			let iterator: super::ReadyIteratorFor<PoolApi> =
				Box::new(view.pool.validated_pool().ready());
			return async move { iterator }.boxed();
		}

		self.ready_poll
			.lock()
			.add(at)
			.map(|received| {
				received.unwrap_or_else(|e| {
					log::warn!("Error receiving pending set: {:?}", e);
					Box::new(std::iter::empty())
				})
			})
			.boxed()
	}

	fn ready(&self, at: <Self::Block as BlockT>::Hash) -> Option<super::ReadyIteratorFor<PoolApi>> {
		self.view_store.ready(at)
	}

	fn futures(&self, at: <Self::Block as BlockT>::Hash) -> Option<Vec<Self::InPoolTransaction>> {
		self.view_store.futures(at)
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
		at: Block::Hash,
		xt: sc_transaction_pool_api::LocalTransactionFor<Self>,
	) -> Result<Self::Hash, Self::Error> {
		unimplemented!();
	}
}

impl<PoolApi, Block> ForkAwareTxPool<PoolApi, Block>
where
	Block: BlockT,
	PoolApi: graph::ChainApi<Block = Block> + 'static,
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

		if self.view_store.views.read().contains_key(&hash_and_number.hash) {
			log::debug!(
				target: LOG_TARGET,
				"view already exists for block: {:?}",
				hash_and_number,
			);
			return
		}

		let best_view = self.view_store.find_best_view(tree_route);
		let new_view = if let Some(best_view) = best_view {
			self.build_cloned_view(best_view, hash_and_number, tree_route).await
		} else {
			self.create_new_view_at(hash_and_number, tree_route).await
		};

		if let Some(view) = new_view {
			self.most_recent_view.write().replace(view.at.hash);
			self.revalidation_queue.revalidate_later(view).await;
		}
	}

	pub async fn create_new_view_at(
		&self,
		at: &HashAndNumber<Block>,
		tree_route: &TreeRoute<Block>,
	) -> Option<Arc<View<PoolApi>>> {
		//todo: handle errors during creation (log?)

		if self.view_store.views.read().contains_key(&at.hash) {
			return None;
		}

		log::info!("create_new_view_at: {at:?}");

		let mut view = View::new(self.api.clone(), at.clone());

		//we need to capture all import notifiication from the very beginning
		self.import_notification_sink
			.add_view(view.at.hash, view.pool.validated_pool().import_notification_stream().boxed())
			.await;

		// we need to install listeners first
		let start = Instant::now();
		self.update_view(&mut view).await;
		let duration = start.elapsed();
		log::info!("update_view_pool: at {at:?} took {duration:?}");

		let start = Instant::now();
		self.update_view_with_fork(&mut view, tree_route, at.clone()).await;
		let duration = start.elapsed();
		log::info!("update_view_fork: at {at:?} took {duration:?}");

		let view = Arc::new(view);
		self.view_store.views.write().insert(at.hash, view.clone());

		{
			let view = view.clone();
			self.ready_poll
				.lock()
				.trigger(at.hash, move || Box::from(view.pool.validated_pool().ready()));
		}

		Some(view)
	}

	async fn build_cloned_view(
		&self,
		origin_view: Arc<View<PoolApi>>,
		at: &HashAndNumber<Block>,
		tree_route: &TreeRoute<Block>,
	) -> Option<Arc<View<PoolApi>>> {
		log::info!(
			"build_cloned_view: for: {:?} from: {:?} tree_route: {:?}",
			at.hash,
			origin_view.at.hash,
			tree_route
		);
		let new_block_hash = at.hash;
		let mut view = View::new_from_other(&origin_view, at);

		//we need to capture all import notifiication from the very beginning
		self.import_notification_sink
			.add_view(view.at.hash, view.pool.validated_pool().import_notification_stream().boxed())
			.await;

		self.update_view(&mut view).await;
		self.update_view_with_fork(&mut view, tree_route, at.clone()).await;
		let view = Arc::from(view);
		self.view_store.views.write().insert(new_block_hash, view.clone());

		{
			let view = view.clone();
			self.ready_poll
				.lock()
				.trigger(new_block_hash, move || Box::from(view.pool.validated_pool().ready()));
		}

		Some(view)
	}

	async fn update_view(&self, view: &mut View<PoolApi>) {
		log::debug!(
			"update_view: {:?} xts:{:?} v:{}",
			view.at,
			self.mempool.len(),
			self.views_len()
		);
		//todo: source?
		let source = TransactionSource::External;

		//todo this clone is not neccessary, try to use iterators
		let xts = self.mempool.clone_unwatched();

		//todo: internal checked banned: not required any more?
		let _ = view.submit_many(source, xts).await;
		let view = Arc::from(view);

		let results = self
			.mempool
			.watched_xts()
			.map(|t| {
				let view = view.clone();
				async move {
					let tx_hash = self.hash_of(&t);
					let result = view.submit_and_watch(source, t.clone()).await;
					let result = result.map_or_else(
						|error| {
							let error = error.into_pool_error();
							log::trace!(
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
									log::error!("[{:?}] txpool: update_view: somehing went wrong: {error:?}", tx_hash);
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
						log::trace!("[{:?}] adding watcher {:?}", tx_hash, view.at.hash);
						self.view_store
							.listener
							.add_view_watcher_for_tx(
								tx_hash,
								view.at.hash,
								watcher.into_stream().boxed(),
							)
							.await;
						Ok(())
					} else {
						result.map(|_| ())
					}
				}
			})
			.collect::<Vec<_>>();

		let results = future::join_all(results).await;

		// if there are no views yet, and a single newly created view is reporting error, just send
		// out the invalid event, and remove transaction.
		if self.view_store.is_empty() {
			for result in results {
				match result {
					Err((Error::TemporarilyBanned | Error::AlreadyImported(_), ..)) => {},
					Err((Error::InvalidTransaction(_), tx_hash, tx)) => {
						self.view_store.listener.invalidate_transactions(vec![tx_hash]).await;
						self.mempool.remove_watched(&tx);
					},

					_ => {},
				}
			}
		}
	}

	//copied from handle_enactment
	//todo: move to ViewManager
	async fn update_view_with_fork(
		&self,
		view: &mut View<PoolApi>,
		tree_route: &TreeRoute<Block>,
		hash_and_number: HashAndNumber<Block>,
	) {
		log::debug!(target: LOG_TARGET, "update_view_with_fork tree_route: {:?} {tree_route:?}", view.at);
		let api = self.api.clone();

		// We keep track of everything we prune so that later we won't add
		// transactions with those hashes from the retracted blocks.
		let mut pruned_log = HashSet::<ExtrinsicHash<PoolApi>>::new();

		future::join_all(
			tree_route
				.enacted()
				.iter()
				// .chain(std::iter::once(&hash_and_number))
				.map(|h| super::prune_known_txs_for_block(h, &*api, &view.pool)),
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
						log::warn!("Failed to fetch block body: {}", e);
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

			let x = view
				.pool
				.resubmit_at(
					&hash_and_number,
					// These transactions are coming from retracted blocks, we should
					// simply consider them external.
					TransactionSource::External,
					resubmit_transactions,
				)
				.await;
			log::trace!("retracted resubmit: {:#?}", x);
		}
	}

	async fn handle_finalized(&self, finalized_hash: Block::Hash, tree_route: &[Block::Hash]) {
		let finalized_number = self.api.block_id_to_number(&BlockId::Hash(finalized_hash));
		log::info!(target: LOG_TARGET, "handle_finalized {finalized_number:?} tree_route: {tree_route:?}");

		self.view_store.finalize_route(finalized_hash, tree_route).await;
		log::debug!(target: LOG_TARGET, "handle_finalized b:{:?}", self.views_len());
		{
			//clean up older then finalized
			let mut views = self.view_store.views.write();
			views.retain(|hash, v| match finalized_number {
				Err(_) | Ok(None) => *hash == finalized_hash,
				Ok(Some(n)) if v.at.number == n => *hash == finalized_hash,
				Ok(Some(n)) => v.at.number > n,
			})
		}

		self.revalidation_queue
			.purge_transactions_later(self.mempool.clone(), finalized_hash)
			.await;
		log::debug!(target: LOG_TARGET, "handle_finalized a:{:?}", self.views_len());
	}
}

#[async_trait]
impl<PoolApi, Block> MaintainedTransactionPool for ForkAwareTxPool<PoolApi, Block>
where
	Block: BlockT,
	PoolApi: 'static + graph::ChainApi<Block = Block>,
	<Block as BlockT>::Hash: Unpin,
{
	async fn maintain(&self, event: ChainEvent<Self::Block>) {
		let start = Instant::now();
		// log::info!(
		// 	"maintain: txs:{:?} views:[{};{:?}] event:{event:?}",
		// 	self.mempool_len(),
		// 	self.views_len(),
		// 	self.views_numbers(),
		// );
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
				let hash = event.hash();
				if !self.has_view(hash) {
					if let Ok(tree_route) = compute_tree_route(prev_finalized_block, hash) {
						self.handle_new_block(&tree_route).await;
					}
				}
			},
			Ok(EnactmentAction::HandleEnactment(tree_route)) =>
				self.handle_new_block(&tree_route).await,
		};

		use sp_runtime::traits::CheckedSub;
		match event {
			ChainEvent::NewBestBlock { hash, .. } => {},
			ChainEvent::Finalized { hash, tree_route } => {
				self.handle_finalized(hash, &*tree_route).await;

				log::trace!(
					target: LOG_TARGET,
					"on-finalized enacted: {tree_route:?}, previously finalized: \
					{prev_finalized_block:?}",
				);
			},
		}

		log::info!(
			"maintain: txs:{:?} views:[{};{:?}] event:{event:?}  took:{:?}",
			self.mempool_len(),
			self.views_len(),
			self.views_numbers(),
			start.elapsed()
		);

		()
	}
}

/// Inform the transaction pool about imported and finalized blocks.
pub async fn notification_future<Client, Pool, Block>(client: Arc<Client>, txpool: Arc<Pool>)
where
	Block: BlockT,
	Client: sc_client_api::BlockchainEvents<Block>,
	Pool: MaintainedTransactionPool<Block = Block>,
{
	let import_stream = client
		.import_notification_stream()
		.filter_map(|n| ready(n.try_into().ok()))
		.fuse();
	let finality_stream = client.finality_notification_stream().map(Into::into).fuse();

	futures::stream::select(import_stream, finality_stream)
		.for_each(|evt| txpool.maintain(evt))
		.await
}
