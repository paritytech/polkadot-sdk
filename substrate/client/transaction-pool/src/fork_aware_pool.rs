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
use std::{
	collections::{HashMap, HashSet},
	pin::Pin,
	sync::Arc,
};

use crate::graph::{ExtrinsicHash, IsValidator};
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
};
use std::time::Instant;

use sp_blockchain::{HashAndNumber, TreeRoute};

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
}

pub struct ViewManager<PoolApi, Block>
where
	Block: BlockT,
	PoolApi: graph::ChainApi<Block = Block>,
{
	api: Arc<PoolApi>,
	views: RwLock<HashMap<Block::Hash, Arc<View<PoolApi>>>>,
}

pub enum ViewCreationError {
	AlreadyExists,
	Unknown,
	BlockIdConversion,
}

impl<PoolApi, Block> ViewManager<PoolApi, Block>
where
	Block: BlockT,
	PoolApi: graph::ChainApi<Block = Block>,
{
	fn new(api: Arc<PoolApi>) -> Self {
		Self { api, views: Default::default() }
	}

	// shall be called on block import
	pub async fn create_new_view_at(
		&self,
		event: ChainEvent<Block>,
		xts: Arc<RwLock<Vec<Block::Extrinsic>>>,
	) -> Result<(), ViewCreationError> {
		let hash = match event {
			ChainEvent::Finalized { hash, .. } | ChainEvent::NewBestBlock { hash, .. } => hash,
		};
		if self.views.read().contains_key(&hash) {
			return Err(ViewCreationError::AlreadyExists)
		}

		let number = self
			.api
			.resolve_block_number(hash)
			.map_err(|_| ViewCreationError::BlockIdConversion)?;
		let at = HashAndNumber { hash, number };
		let view = Arc::new(View::new(self.api.clone(), at.clone()));

		//todo: lock or clone?
		//todo: source?
		let source = TransactionSource::External;

		//todo: internal checked banned: not required any more?
		let xts = xts.read().clone();
		let _ = view.pool.submit_at(&at, source, xts).await;
		self.views.write().insert(hash, view);

		// brute force: just revalidate all xts against block
		// target: find parent, extract all provided tags on enacted path and recompute graph

		Ok(())
	}

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
	) -> Result<Watcher<ExtrinsicHash<PoolApi>, ExtrinsicHash<PoolApi>>, PoolApi::Error> {
		unimplemented!()
	}

	pub fn status(&self) -> HashMap<Block::Hash, PoolStatus> {
		self.views
			.read()
			.iter()
			.map(|(h, v)| (*h, v.pool.validated_pool().status()))
			.collect()
	}
}

////////////////////////////////////////////////////////////////////////////////

pub struct ForkAwareTxPool<PoolApi, Block>
where
	Block: BlockT,
	PoolApi: graph::ChainApi<Block = Block>,
{
	api: Arc<PoolApi>,
	xts: Arc<RwLock<Vec<Block::Extrinsic>>>,
	views: Arc<ViewManager<PoolApi, Block>>,
	// todo:
	// map: hash -> view
	// ready_poll: Arc<Mutex<ReadyPoll<ReadyIteratorFor<PoolApi>, Block>>>,
	// current tree? (somehow similar to enactment state?)
	// todo: metrics

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
			views: Arc::new(ViewManager::new(pool_api)),
		}
	}

	/// Get access to the underlying api
	pub fn api(&self) -> &PoolApi {
		&self.api
	}

	pub fn status_all(&self) -> HashMap<Block::Hash, PoolStatus> {
		self.views.status()
	}
}

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
		self.xts.write().push(xt.clone());

		// todo:
		// self.metrics.report(|metrics| metrics.submitted_transactions.inc());

		async move {
			let watcher = views.submit_and_watch(at, source, xt).await?;

			Ok(watcher.into_stream().boxed())
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
	fn ready_at(
		&self,
		at: NumberFor<Self::Block>,
	) -> Pin<
		Box<
			dyn Future<
					Output = Box<dyn ReadyTransactions<Item = Arc<Self::InPoolTransaction>> + Send>,
				> + Send,
		>,
	> {
		// -> PolledIterator<PoolApi>
		unimplemented!()
	}

	// todo: API change? ready at block?
	fn ready(&self) -> Box<dyn ReadyTransactions<Item = Arc<Self::InPoolTransaction>> + Send> {
		//originally it was: -> ReadyIteratorFor<PoolApi>
		// Box::new(self.pool.validated_pool().ready())
		unimplemented!()
	}

	// todo: API change? futures at block?
	fn futures(&self) -> Vec<Self::InPoolTransaction> {
		// let pool = self.pool.validated_pool().pool.read();
		// pool.futures().cloned().collect::<Vec<_>>()
		unimplemented!()
	}
}

impl<Block, Client> sc_transaction_pool_api::LocalTransactionPool
	for ForkAwareTxPool<FullChainApi<Client, Block>, Block>
where
	Block: BlockT,
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

#[async_trait]
impl<PoolApi, Block> MaintainedTransactionPool for ForkAwareTxPool<PoolApi, Block>
where
	Block: BlockT,
	PoolApi: 'static + graph::ChainApi<Block = Block>,
{
	async fn maintain(&self, event: ChainEvent<Self::Block>) {
		//todo: print error?
		let _ = self.views.create_new_view_at(event, self.xts.clone()).await;
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
