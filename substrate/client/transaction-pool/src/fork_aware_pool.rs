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
	channel::oneshot,
	future::{self, ready},
	prelude::*,
};
use parking_lot::{Mutex, RwLock};
use sc_transaction_pool_api::error::{Error, IntoPoolError};
use sp_runtime::transaction_validity::InvalidTransaction;
use std::{
	collections::{HashMap, HashSet},
	pin::Pin,
	sync::Arc,
};

use crate::graph::{ExtrinsicHash, IsValidator};
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

use multi_view_listener::MultiViewListener;
use sp_blockchain::{HashAndNumber, TreeRoute};

mod multi_view_listener;
mod view_revalidation;

pub(crate) const LOG_TARGET: &str = "txpool";

//todo: View probably needs a hash? parent hash? number?
pub struct View<PoolApi: graph::ChainApi> {
	pool: graph::Pool<PoolApi>,
	at: HashAndNumber<PoolApi::Block>,
}

impl<PoolApi> View<PoolApi>
where
	PoolApi: graph::ChainApi,
{
	fn new(api: Arc<PoolApi>, at: HashAndNumber<PoolApi::Block>) -> Self {
		Self { pool: graph::Pool::new(Default::default(), true.into(), api), at }
	}

	async fn finalize(&self, finalized: graph::BlockHash<PoolApi>) {
		log::info!("View::finalize: {:?} {:?}", self.at, finalized);
		let _ = self.pool.validated_pool().on_block_finalized(finalized).await;
	}
}

//todo: better name: ViewStore?
pub struct ViewManager<PoolApi, Block>
where
	Block: BlockT,
	PoolApi: graph::ChainApi<Block = Block>,
{
	api: Arc<PoolApi>,
	views: RwLock<HashMap<Block::Hash, Arc<View<PoolApi>>>>,
	listener: MultiViewListener<PoolApi>,
}

#[derive(Debug)]
pub enum ViewCreationError {
	AlreadyExists,
	Unknown,
	BlockIdConversion,
}

impl<PoolApi, Block> ViewManager<PoolApi, Block>
where
	Block: BlockT,
	PoolApi: graph::ChainApi<Block = Block> + 'static,
	<Block as BlockT>::Hash: Unpin,
{
	fn new(api: Arc<PoolApi>, finalized_hash: Block::Hash) -> Self {
		// let number = api
		// 	.resolve_block_number(finalized_hash)
		// 	.map_err(|_| ViewCreationError::BlockIdConversion) //?
		// 	.unwrap();
		// let at = HashAndNumber { hash: finalized_hash, number };
		// let view = Arc::new(View::new(api.clone(), at.clone()));
		// let views = RwLock::from(HashMap::from([(finalized_hash, view)]));
		let views = Default::default();

		Self { api, views, listener: MultiViewListener::new() }
	}

	fn create_new_empty_view_at(&self, hash: Block::Hash) {
		//todo: error handling
	}

	// shall be called on block import
	// todo: shall be moved to ForkAwareTxPool
	// pub async fn create_new_view_at(
	// 	&self,
	// 	hash: Block::Hash,
	// 	xts: Arc<RwLock<Vec<Block::Extrinsic>>>,
	// ) -> Result<Arc<View<PoolApi>>, ViewCreationError> {
	// 	if self.views.read().contains_key(&hash) {
	// 		return Err(ViewCreationError::AlreadyExists)
	// 	}
	//
	// 	log::info!("create_new_view_at: {hash:?}");
	//
	// 	let number = self
	// 		.api
	// 		.resolve_block_number(hash)
	// 		.map_err(|_| ViewCreationError::BlockIdConversion)?;
	// 	let at = HashAndNumber { hash, number };
	// 	let view = Arc::new(View::new(self.api.clone(), at.clone()));
	//
	// 	//todo: lock or clone?
	// 	//todo: source?
	// 	let source = TransactionSource::External;
	//
	// 	//todo: internal checked banned: not required any more?
	// 	let xts = xts.read().clone();
	// 	let _ = view.pool.submit_at(&at, source, xts).await;
	// 	self.views.write().insert(hash, view.clone());
	//
	// 	// brute force: just revalidate all xts against block
	// 	// target: find parent, extract all provided tags on enacted path and recompute graph
	//
	// 	Ok(view)
	// }

	/// Imports a bunch of unverified extrinsics to every view
	pub async fn submit_at(
		&self,
		source: TransactionSource,
		xts: impl IntoIterator<Item = Block::Extrinsic> + Clone,
	) -> HashMap<Block::Hash, Vec<Result<ExtrinsicHash<PoolApi>, PoolApi::Error>>> {
		let futs = {
			let g = self.views.read();
			let futs = g
				.iter()
				.map(|(hash, view)| {
					let view = view.clone();
					//todo: remove this clone (Arc?)
					let xts = xts.clone();
					async move {
						(view.at.hash, view.pool.submit_at(&view.at, source, xts.clone()).await)
					}
				})
				.collect::<Vec<_>>();
			futs
		};
		let results = futures::future::join_all(futs).await;

		HashMap::<_, _>::from_iter(results.into_iter())
	}

	/// Imports one unverified extrinsic to every view
	pub async fn submit_one(
		&self,
		source: TransactionSource,
		xt: Block::Extrinsic,
	) -> HashMap<Block::Hash, Result<ExtrinsicHash<PoolApi>, PoolApi::Error>> {
		let futs = {
			let g = self.views.read();
			let futs = g
				.iter()
				.map(|(hash, view)| {
					let view = view.clone();
					let xt = xt.clone();

					async move {
						(view.at.hash, view.pool.submit_one(&view.at, source, xt.clone()).await)
					}
				})
				.collect::<Vec<_>>();
			futs
		};
		let results = futures::future::join_all(futs).await;

		HashMap::<_, _>::from_iter(results.into_iter())
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
		let futs = {
			let g = self.views.read();
			let futs = g
				.iter()
				.map(|(hash, view)| {
					let view = view.clone();
					let xt = xt.clone();

					async move {
						let result = view.pool.submit_and_watch(&view.at, source, xt.clone()).await;
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
		let maybe_watchers = futures::future::join_all(futs).await;
		log::info!("submit_and_watch: maybe_watchers: {}", maybe_watchers.len());

		let maybe_error = maybe_watchers.into_iter().reduce(|mut r, v| {
			if r.is_err() && v.is_ok() {
				r = v;
			}
			r
		});
		if let Some(Err(err)) = maybe_error {
			return Err(err);
		};

		Ok(external_watcher.unwrap())
	}

	pub fn status(&self) -> HashMap<Block::Hash, PoolStatus> {
		self.views
			.read()
			.iter()
			.map(|(h, v)| (*h, v.pool.validated_pool().status()))
			.collect()
	}

	pub fn is_empty(&self) -> bool {
		self.views.read().is_empty()
	}

	fn find_best_view(&self, tree_route: &TreeRoute<Block>) -> Option<Arc<View<PoolApi>>> {
		let views = self.views.read();
		let best_view = {
			tree_route
				.enacted()
				.iter()
				.rev()
				.chain(std::iter::once(tree_route.common_block()))
				.chain(tree_route.retracted().iter().rev())
				.rev()
				.find(|i| views.contains_key(&i.hash))
		};
		best_view.map(|h| views.get(&h.hash).expect("best_hash is an existing key.qed").clone())
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
	) -> Option<Vec<graph::base_pool::Transaction<graph::ExtrinsicHash<PoolApi>, Block::Extrinsic>>>
	{
		// let pool = self.pool.validated_pool().pool.read();
		// pool.futures().cloned().collect::<Vec<_>>()
		self.views
			.read()
			.get(&at)
			.map(|v| v.pool.validated_pool().pool.read().futures().cloned().collect())
	}

	async fn finalize_route(&self, finalized_hash: Block::Hash, tree_route: &[Block::Hash]) {
		log::info!(target: LOG_TARGET, "finalize_route {finalized_hash:?} tree_route: {tree_route:?}");
		let mut no_view_blocks = vec![];
		for hash in tree_route.iter().chain(std::iter::once(&finalized_hash)) {
			let finalized_view = { self.views.read().get(&hash).map(|v| v.clone()) };
			log::info!(target: LOG_TARGET, "finalize_route --> {hash:?} {no_view_blocks:?} fv:{:#?}", finalized_view.is_some());
			if let Some(finalized_view) = finalized_view {
				for h in no_view_blocks.iter().chain(std::iter::once(hash)) {
					log::info!(target: LOG_TARGET, "finalize_route --> {h:?}");
					finalized_view.finalize(*h).await;
				}
				no_view_blocks.clear();
			} else {
				log::info!(target: LOG_TARGET, "finalize_route --> push {hash:?} {no_view_blocks:?}");
				no_view_blocks.push(*hash);
			}
		}
		// let finalized_view = { self.views.read().get(&finalized_hash).map(|v| v.clone()) };
		//
		// let Some(finalized_view) = finalized_view else {
		// 	log::warn!(
		// 		target: LOG_TARGET,
		// 		"Error occurred while attempting to notify watchers about finalization {}",
		// 		finalized_hash
		// 	);
		// 	return;
		// };
		//
		// for hash in tree_route.iter().chain(std::iter::once(&finalized_hash)) {
		// 	finalized_view.finalize(*hash).await;
		// }
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

pub struct ForkAwareTxPool<PoolApi, Block>
where
	Block: BlockT,
	PoolApi: graph::ChainApi<Block = Block> + 'static,
	<Block as BlockT>::Hash: Unpin,
{
	api: Arc<PoolApi>,
	xts: Arc<RwLock<Vec<Block::Extrinsic>>>,
	watched_xts: Arc<RwLock<Vec<Block::Extrinsic>>>,

	// todo: is ViewManager strucy really needed? (no)
	views: Arc<ViewManager<PoolApi, Block>>,
	// todo: is ReadyPoll struct really needed? (no)
	ready_poll: Arc<Mutex<ReadyPoll<super::ReadyIteratorFor<PoolApi>, Block>>>,
	// current tree? (somehow similar to enactment state?)
	// todo: metrics
	enactment_state: Arc<Mutex<EnactmentState<Block>>>,
	revalidation_queue: Arc<view_revalidation::RevalidationQueue<PoolApi>>,
	// todo: this are coming from ValidatedPool, some of them maybe needed here
	// is_validator: IsValidator,
	// options: Options,
	// listener: RwLock<Listener<ExtrinsicHash<B>, B>>,
	// import_notification_sinks: Mutex<Vec<Sender<ExtrinsicHash<B>>>>,
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
	) -> Self {
		Self {
			api: pool_api.clone(),
			xts: Default::default(),
			watched_xts: Default::default(),
			views: Arc::new(ViewManager::new(pool_api, finalized_hash)),
			ready_poll: Arc::from(Mutex::from(ReadyPoll::new())),
			enactment_state: Arc::new(Mutex::new(EnactmentState::new(
				best_block_hash,
				finalized_hash,
			))),
			revalidation_queue: Arc::from(view_revalidation::RevalidationQueue::new()),
		}
	}

	/// Get access to the underlying api
	pub fn api(&self) -> &PoolApi {
		&self.api
	}

	//todo: this should be new TransactionPool API?
	pub fn status_all(&self) -> HashMap<Block::Hash, PoolStatus> {
		self.views.status()
	}

	//todo:naming? maybe just views()
	pub fn views_len(&self) -> usize {
		self.views.views.read().len()
	}

	pub fn has_view(&self, hash: Block::Hash) -> bool {
		self.views.views.read().get(&hash).is_some()
	}
}

//todo: naming + doc!
fn xxxx<H, E>(input: &mut HashMap<H, Vec<Result<H, E>>>) -> Vec<Result<H, E>> {
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
mod xxxx_test {
	use super::*;
	use sp_core::H256;
	#[derive(Debug, PartialEq)]
	enum Error {
		Custom(u8),
	}

	#[test]
	fn empty() {
		sp_tracing::try_init_simple();
		let mut input = HashMap::default();
		let r = xxxx::<H256, Error>(&mut input);
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
		let mut input = HashMap::from_iter(v);
		let r = xxxx(&mut input);

		let x: Option<(u8, usize)> = r.into_iter().fold(None, |h, e| match (h, e) {
			(None, Err(Error::Custom(n))) => Some((n, 1)),
			(Some((h, count)), Err(Error::Custom(n))) => {
				assert_eq!(h + 1, n);
				Some((n, count + 1))
			},
			_ => panic!(),
		});
		assert_eq!(x.unwrap().0 % 10, 3);
		assert_eq!(x.unwrap().1, 4);
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
		let r = xxxx(&mut input);
	}

	#[test]
	fn only_hashes() {
		sp_tracing::try_init_simple();

		let v: Vec<(H256, Vec<Result<H256, Error>>)> = vec![
			(
				H256::repeat_byte(0x13),
				vec![Ok(H256::repeat_byte(0x13)), Ok(H256::repeat_byte(0x13))],
			),
			(
				H256::repeat_byte(0x14),
				vec![Ok(H256::repeat_byte(0x13)), Ok(H256::repeat_byte(0x13))],
			),
		];
		let mut input = HashMap::from_iter(v);
		let r = xxxx(&mut input);

		assert_eq!(r, vec![Ok(H256::repeat_byte(0x13)), Ok(H256::repeat_byte(0x13))]);
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
		let r = xxxx(&mut input);

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
		let views = self.views.clone();
		self.xts.write().extend(xts.clone());
		let xts = xts.clone();

		if views.is_empty() {
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
			let mut results_map = views.submit_at(source, xts).await;
			Ok(xxxx(&mut results_map))
		}
		.boxed()
	}

	fn submit_one(
		&self,
		_: <Self::Block as BlockT>::Hash,
		source: TransactionSource,
		xt: TransactionFor<Self>,
	) -> PoolFuture<TxHash<Self>, Self::Error> {
		// todo:
		// self.metrics.report(|metrics| metrics.submitted_transactions.inc());
		let views = self.views.clone();
		self.xts.write().push(xt.clone());

		if views.is_empty() {
			//todo: error or ok if no views?
			return future::ready(Ok(self.api.hash_and_length(&xt).0)).boxed()
			// return future::ready(Err(TxPoolError::UnknownTransaction(
			// 	UnknownTransaction::CannotLookup,
			// )
			// .into()))
			// .boxed()
		}

		async move {
			let results = views.submit_one(source, xt).await;
			results
				.into_values()
				.reduce(|mut r, v| {
					if r.is_err() && v.is_ok() {
						r = v;
					}
					r
				})
				.expect("there is at least one entry in input")
		}
		.boxed()
	}

	fn submit_and_watch(
		&self,
		at: <Self::Block as BlockT>::Hash,
		source: TransactionSource,
		xt: TransactionFor<Self>,
	) -> PoolFuture<Pin<Box<TransactionStatusStreamFor<Self>>>, Self::Error> {
		let views = self.views.clone();
		self.watched_xts.write().push(xt.clone());

		// todo:
		// self.metrics.report(|metrics| metrics.submitted_transactions.inc());

		async move {
			let result = views.submit_and_watch(at, source, xt).await;
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

		unimplemented!()
	}

	fn status(&self) -> PoolStatus {
		// self.pool.validated_pool().status()
		unimplemented!()
	}

	fn import_notification_stream(&self) -> ImportNotificationStream<TxHash<Self>> {
		// self.pool.validated_pool().import_notification_stream()
		unimplemented!()
	}

	fn hash_of(&self, xt: &TransactionFor<Self>) -> TxHash<Self> {
		self.api().hash_and_length(xt).0
	}

	fn on_broadcasted(&self, propagations: HashMap<TxHash<Self>, Vec<String>>) {
		// self.pool.validated_pool().on_broadcasted(propagations)
		unimplemented!()
	}

	// todo: api change?
	fn ready_transaction(&self, hash: &TxHash<Self>) -> Option<Arc<Self::InPoolTransaction>> {
		// self.pool.validated_pool().ready_by_hash(hash)
		unimplemented!()
	}

	// todo: API change? ready at hash (not number)?
	fn ready_at(&self, at: <Self::Block as BlockT>::Hash) -> super::PolledIterator<PoolApi> {
		if let Some(view) = self.views.views.read().get(&at) {
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
		self.views.ready(at)
	}

	fn futures(&self, at: <Self::Block as BlockT>::Hash) -> Option<Vec<Self::InPoolTransaction>> {
		self.views.futures(at)
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

		if self.views.views.read().contains_key(&hash_and_number.hash) {
			log::debug!(
				target: LOG_TARGET,
				"view already exists for block: {:?}",
				hash_and_number,
			);
			return
		}

		let best_view = self.views.find_best_view(tree_route);
		let new_view = if let Some(best_view) = best_view {
			self.build_cloned_view(best_view, hash_and_number, tree_route).await
		} else {
			self.create_new_view_at(hash_and_number, tree_route).await
		};

		if let Some(view) = new_view {
			self.revalidation_queue.revalidate_later(view).await;
		}
	}

	pub async fn create_new_view_at(
		&self,
		at: &HashAndNumber<Block>,
		tree_route: &TreeRoute<Block>,
	) -> Option<Arc<View<PoolApi>>> {
		//todo: handle errors during creation (log?)

		if self.views.views.read().contains_key(&at.hash) {
			return None;
		}

		log::info!("create_new_view_at: {at:?}");

		let mut view = View::new(self.api.clone(), at.clone());

		self.update_view(&mut view).await;
		self.update_view_with_fork(&mut view, tree_route, at.clone()).await;

		let view = Arc::new(view);
		self.views.views.write().insert(at.hash, view.clone());

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
		log::info!("build_cloned_view: {:?}", at.hash);
		let new_block_hash = at.hash;
		let mut view = View { at: at.clone(), pool: origin_view.pool.deep_clone() };

		//todo: this cloning probably has some flaws. It is possible that tx should be watched, but
		//was removed from original view (e.g. runtime upgrade)
		//so we need to have watched transactions in FAPool, question is how and when remove them.
		let futs = origin_view
			.pool
			.validated_pool()
			.watched_transactions()
			.iter()
			.map(|tx_hash| {
				let watcher = view.pool.validated_pool().create_watcher(*tx_hash);
				self.views.listener.add_view_watcher_for_tx(
					*tx_hash,
					at.hash,
					watcher.into_stream().boxed(),
				)
			})
			.collect::<Vec<_>>();

		future::join_all(futs).await;

		self.update_view_with_fork(&mut view, tree_route, at.clone()).await;
		self.update_view(&mut view).await;
		let view = Arc::from(view);
		self.views.views.write().insert(new_block_hash, view.clone());

		{
			let view = view.clone();
			self.ready_poll
				.lock()
				.trigger(new_block_hash, move || Box::from(view.pool.validated_pool().ready()));
		}

		Some(view)
	}

	async fn update_view(&self, view: &mut View<PoolApi>) {
		log::info!(
			"update_view: {:?} xts:{}/{} v:{}",
			view.at,
			self.xts.read().len(),
			self.watched_xts.read().len(),
			self.views_len()
		);
		//todo: source?
		let source = TransactionSource::External;
		let xts = self.xts.read().clone();
		//todo: internal checked banned: not required any more?
		let _ = view.pool.submit_at(&view.at, source, xts).await;
		let view = Arc::from(view);

		let futs = {
			let watched_xts = self.watched_xts.read();
			let futs = watched_xts
				.iter()
				.map(|t| {
					let view = view.clone();
					let t = t.clone();
					async move {
						let tx_hash = self.hash_of(&t);
						let result = view.pool.submit_and_watch(&view.at, source, t.clone()).await;
						let watcher = result.map_or_else(
							|error| {
								let error = error.into_pool_error();
								match error {
									// We need to install listener for stale xt: in case of
									// transaction being already included in the block we want to
									// send inblock + finalization event.
									Ok(Error::InvalidTransaction(InvalidTransaction::Stale)) =>
										Some(view.pool.validated_pool().create_watcher(tx_hash)),
									//ignore
									Ok(Error::TemporarilyBanned | Error::AlreadyImported(_)) =>
										None,
									//todo: panic while testing
									_ => {
										panic!(
											"txpool: update_view: somehing went wrong: {error:?}"
										);
									},
								}
							},
							Into::into,
						);

						if let Some(watcher) = watcher {
							self.views
								.listener
								.add_view_watcher_for_tx(
									tx_hash,
									view.at.hash,
									watcher.into_stream().boxed(),
								)
								.await;
						}
						()
					}
				})
				.collect::<Vec<_>>();
			futs
		};
		future::join_all(futs).await;
	}

	//copied from handle_enactment
	//todo: move to ViewManager
	async fn update_view_with_fork(
		&self,
		view: &mut View<PoolApi>,
		tree_route: &TreeRoute<Block>,
		hash_and_number: HashAndNumber<Block>,
	) {
		log::info!(target: LOG_TARGET, "update_view tree_route: {tree_route:?}");
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
					let tx_hash = view.pool.hash_of(tx);
					let contains = pruned_log.contains(&tx_hash);

					// need to count all transactions, not just filtered, here
					resubmitted_to_report += 1;

					if !contains {
						log::debug!(
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

			view.pool
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

		// if !self.views.views.read().contains_key(&finalized_hash) {
		// 	if tree_route.is_empty() {
		// 		log::info!("Creating new view for finalized block: {}", finalized_hash);
		// 		self.create_new_view_at(finalized_hash).await;
		// 	} else {
		// 		//convert &[Hash] to TreeRoute
		// 		let tree_route = self.api.tree_route(tree_route[0],
		// tree_route[tree_route.len()-1]).expect( 			"Tree route between currently and recently
		// finalized blocks must exist. qed", 		);
		// 		self.handle_new_block(finalized_hash, &tree_route).await;
		// 	}
		// }

		self.views.finalize_route(finalized_hash, tree_route).await;
		log::info!(target: LOG_TARGET, "handle_finalized b:{:?}", self.views_len());
		{
			//clean up older then finalized
			let mut views = self.views.views.write();
			views.retain(|hash, v| match finalized_number {
				Err(_) | Ok(None) => *hash == finalized_hash,
				Ok(Some(n)) if v.at.number == n => *hash == finalized_hash,
				Ok(Some(n)) => v.at.number > n,
			})
		}
		log::info!(target: LOG_TARGET, "handle_finalized a:{:?}", self.views_len());
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
		log::info!("maintain: {event:#?}");
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
	unimplemented!();
}
