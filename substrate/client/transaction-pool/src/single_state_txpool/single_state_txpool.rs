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

//! Substrate transaction pool implementation.

use super::{metrics::MetricsLink as PrometheusMetrics, revalidation};
pub use crate::{
	api::FullChainApi,
	graph::{ChainApi, ValidatedTransaction},
};
use crate::{
	common::{
		enactment_state::{EnactmentAction, EnactmentState},
		error,
		log_xt::log_xt_trace,
	},
	graph,
	graph::{ExtrinsicHash, IsValidator},
	PolledIterator, ReadyIteratorFor, LOG_TARGET,
};
use async_trait::async_trait;
use futures::{channel::oneshot, future, prelude::*, Future, FutureExt};
use parking_lot::Mutex;
use prometheus_endpoint::Registry as PrometheusRegistry;
use sc_transaction_pool_api::{
	error::Error as TxPoolError, ChainEvent, ImportNotificationStream, MaintainedTransactionPool,
	PoolFuture, PoolStatus, TransactionFor, TransactionPool, TransactionSource,
	TransactionStatusStreamFor, TxHash,
};
use sp_blockchain::{HashAndNumber, TreeRoute};
use sp_core::traits::SpawnEssentialNamed;
use sp_runtime::{
	generic::BlockId,
	traits::{AtLeast32Bit, Block as BlockT, Header as HeaderT, NumberFor, Zero},
};
use std::{
	collections::{HashMap, HashSet},
	pin::Pin,
	sync::Arc,
	time::Instant,
};
use tokio::select;

/// Basic implementation of transaction pool that can be customized by providing PoolApi.
pub struct BasicPool<PoolApi, Block>
where
	Block: BlockT,
	PoolApi: graph::ChainApi<Block = Block>,
{
	pool: Arc<graph::Pool<PoolApi>>,
	api: Arc<PoolApi>,
	revalidation_strategy: Arc<Mutex<RevalidationStrategy<NumberFor<Block>>>>,
	revalidation_queue: Arc<revalidation::RevalidationQueue<PoolApi>>,
	ready_poll: Arc<Mutex<ReadyPoll<ReadyIteratorFor<PoolApi>, Block>>>,
	metrics: PrometheusMetrics,
	enactment_state: Arc<Mutex<EnactmentState<Block>>>,
}

struct ReadyPoll<T, Block: BlockT> {
	updated_at: NumberFor<Block>,
	pollers: Vec<(NumberFor<Block>, oneshot::Sender<T>)>,
}

impl<T, Block: BlockT> Default for ReadyPoll<T, Block> {
	fn default() -> Self {
		Self { updated_at: NumberFor::<Block>::zero(), pollers: Default::default() }
	}
}

impl<T, Block: BlockT> ReadyPoll<T, Block> {
	fn new(best_block_number: NumberFor<Block>) -> Self {
		Self { updated_at: best_block_number, pollers: Default::default() }
	}

	fn trigger(&mut self, number: NumberFor<Block>, iterator_factory: impl Fn() -> T) {
		self.updated_at = number;

		let mut idx = 0;
		while idx < self.pollers.len() {
			if self.pollers[idx].0 <= number {
				let poller_sender = self.pollers.swap_remove(idx);
				log::trace!(target: LOG_TARGET, "Sending ready signal at block {}", number);
				let _ = poller_sender.1.send(iterator_factory());
			} else {
				idx += 1;
			}
		}
	}

	fn add(&mut self, number: NumberFor<Block>) -> oneshot::Receiver<T> {
		let (sender, receiver) = oneshot::channel();
		self.pollers.push((number, sender));
		receiver
	}

	fn updated_at(&self) -> NumberFor<Block> {
		self.updated_at
	}
}

/// Type of revalidation.
pub enum RevalidationType {
	/// Light revalidation type.
	///
	/// During maintenance, transaction pool makes periodic revalidation
	/// of all transactions depending on number of blocks or time passed.
	/// Also this kind of revalidation does not resubmit transactions from
	/// retracted blocks, since it is too expensive.
	Light,

	/// Full revalidation type.
	///
	/// During maintenance, transaction pool revalidates some fixed amount of
	/// transactions from the pool of valid transactions.
	Full,
}

impl<PoolApi, Block> BasicPool<PoolApi, Block>
where
	Block: BlockT,
	PoolApi: graph::ChainApi<Block = Block> + 'static,
{
	/// Create new basic transaction pool with provided api, for tests.
	pub fn new_test(
		pool_api: Arc<PoolApi>,
		best_block_hash: Block::Hash,
		finalized_hash: Block::Hash,
		options: graph::Options,
	) -> (Self, Pin<Box<dyn Future<Output = ()> + Send>>) {
		let pool = Arc::new(graph::Pool::new(options, true.into(), pool_api.clone()));
		let (revalidation_queue, background_task) = revalidation::RevalidationQueue::new_background(
			pool_api.clone(),
			pool.clone(),
			finalized_hash,
		);
		(
			Self {
				api: pool_api,
				pool,
				revalidation_queue: Arc::new(revalidation_queue),
				revalidation_strategy: Arc::new(Mutex::new(RevalidationStrategy::Always)),
				ready_poll: Default::default(),
				metrics: Default::default(),
				enactment_state: Arc::new(Mutex::new(EnactmentState::new(
					best_block_hash,
					finalized_hash,
				))),
			},
			background_task,
		)
	}

	/// Create new basic transaction pool with provided api and custom
	/// revalidation type.
	pub fn with_revalidation_type(
		options: graph::Options,
		is_validator: IsValidator,
		pool_api: Arc<PoolApi>,
		prometheus: Option<&PrometheusRegistry>,
		revalidation_type: RevalidationType,
		spawner: impl SpawnEssentialNamed,
		best_block_number: NumberFor<Block>,
		best_block_hash: Block::Hash,
		finalized_hash: Block::Hash,
	) -> Self {
		let pool = Arc::new(graph::Pool::new(options, is_validator, pool_api.clone()));
		let (revalidation_queue, background_task) = match revalidation_type {
			RevalidationType::Light =>
				(revalidation::RevalidationQueue::new(pool_api.clone(), pool.clone()), None),
			RevalidationType::Full => {
				let (queue, background) = revalidation::RevalidationQueue::new_background(
					pool_api.clone(),
					pool.clone(),
					finalized_hash,
				);
				(queue, Some(background))
			},
		};

		if let Some(background_task) = background_task {
			spawner.spawn_essential("txpool-background", Some("transaction-pool"), background_task);
		}

		Self {
			api: pool_api,
			pool,
			revalidation_queue: Arc::new(revalidation_queue),
			revalidation_strategy: Arc::new(Mutex::new(match revalidation_type {
				RevalidationType::Light =>
					RevalidationStrategy::Light(RevalidationStatus::NotScheduled),
				RevalidationType::Full => RevalidationStrategy::Always,
			})),
			ready_poll: Arc::new(Mutex::new(ReadyPoll::new(best_block_number))),
			metrics: PrometheusMetrics::new(prometheus),
			enactment_state: Arc::new(Mutex::new(EnactmentState::new(
				best_block_hash,
				finalized_hash,
			))),
		}
	}

	/// Gets shared reference to the underlying pool.
	pub fn pool(&self) -> &Arc<graph::Pool<PoolApi>> {
		&self.pool
	}

	/// Get access to the underlying api
	pub fn api(&self) -> &PoolApi {
		&self.api
	}

	fn ready_at_with_timeout_internal(
		&self,
		at: Block::Hash,
		timeout: std::time::Duration,
	) -> PolledIterator<PoolApi> {
		let timeout = futures_timer::Delay::new(timeout);
		let ready_maintained = self.ready_at(at);
		let ready_current = self.ready();

		let ready = async {
			select! {
				ready = ready_maintained => ready,
				_ = timeout => ready_current
			}
		};

		Box::pin(ready)
	}
}

impl<PoolApi, Block> TransactionPool for BasicPool<PoolApi, Block>
where
	Block: BlockT,
	PoolApi: 'static + graph::ChainApi<Block = Block>,
{
	type Block = PoolApi::Block;
	type Hash = graph::ExtrinsicHash<PoolApi>;
	type InPoolTransaction =
		graph::base_pool::Transaction<graph::ExtrinsicHash<PoolApi>, graph::ExtrinsicFor<PoolApi>>;
	type Error = PoolApi::Error;

	fn submit_at(
		&self,
		at: <Self::Block as BlockT>::Hash,
		source: TransactionSource,
		xts: Vec<TransactionFor<Self>>,
	) -> PoolFuture<Vec<Result<TxHash<Self>, Self::Error>>, Self::Error> {
		let pool = self.pool.clone();
		let xts = xts.into_iter().map(Arc::from).collect::<Vec<_>>();

		self.metrics
			.report(|metrics| metrics.submitted_transactions.inc_by(xts.len() as u64));

		let number = self.api.resolve_block_number(at);
		async move {
			let at = HashAndNumber { hash: at, number: number? };
			Ok(pool.submit_at(&at, source, xts).await)
		}
		.boxed()
	}

	fn submit_one(
		&self,
		at: <Self::Block as BlockT>::Hash,
		source: TransactionSource,
		xt: TransactionFor<Self>,
	) -> PoolFuture<TxHash<Self>, Self::Error> {
		let pool = self.pool.clone();
		let xt = Arc::from(xt);

		self.metrics.report(|metrics| metrics.submitted_transactions.inc());

		let number = self.api.resolve_block_number(at);
		async move {
			let at = HashAndNumber { hash: at, number: number? };
			pool.submit_one(&at, source, xt).await
		}
		.boxed()
	}

	fn submit_and_watch(
		&self,
		at: <Self::Block as BlockT>::Hash,
		source: TransactionSource,
		xt: TransactionFor<Self>,
	) -> PoolFuture<Pin<Box<TransactionStatusStreamFor<Self>>>, Self::Error> {
		let pool = self.pool.clone();
		let xt = Arc::from(xt);

		self.metrics.report(|metrics| metrics.submitted_transactions.inc());

		let number = self.api.resolve_block_number(at);

		async move {
			let at = HashAndNumber { hash: at, number: number? };
			let watcher = pool.submit_and_watch(&at, source, xt).await?;

			Ok(watcher.into_stream().boxed())
		}
		.boxed()
	}

	fn remove_invalid(&self, hashes: &[TxHash<Self>]) -> Vec<Arc<Self::InPoolTransaction>> {
		let removed = self.pool.validated_pool().remove_invalid(hashes);
		self.metrics
			.report(|metrics| metrics.validations_invalid.inc_by(removed.len() as u64));
		removed
	}

	fn status(&self) -> PoolStatus {
		self.pool.validated_pool().status()
	}

	fn import_notification_stream(&self) -> ImportNotificationStream<TxHash<Self>> {
		self.pool.validated_pool().import_notification_stream()
	}

	fn hash_of(&self, xt: &TransactionFor<Self>) -> TxHash<Self> {
		self.pool.hash_of(xt)
	}

	fn on_broadcasted(&self, propagations: HashMap<TxHash<Self>, Vec<String>>) {
		self.pool.validated_pool().on_broadcasted(propagations)
	}

	fn ready_transaction(&self, hash: &TxHash<Self>) -> Option<Arc<Self::InPoolTransaction>> {
		self.pool.validated_pool().ready_by_hash(hash)
	}

	fn ready_at(&self, at: <Self::Block as BlockT>::Hash) -> PolledIterator<PoolApi> {
		let Ok(at) = self.api.resolve_block_number(at) else {
			return async { Box::new(std::iter::empty()) as Box<_> }.boxed()
		};

		let status = self.status();
		// If there are no transactions in the pool, it is fine to return early.
		//
		// There could be transaction being added because of some re-org happening at the relevant
		// block, but this is relative unlikely.
		if status.ready == 0 && status.future == 0 {
			return async { Box::new(std::iter::empty()) as Box<_> }.boxed()
		}

		if self.ready_poll.lock().updated_at() >= at {
			log::trace!(target: LOG_TARGET, "Transaction pool already processed block  #{}", at);
			let iterator: ReadyIteratorFor<PoolApi> = Box::new(self.pool.validated_pool().ready());
			return async move { iterator }.boxed()
		}

		self.ready_poll
			.lock()
			.add(at)
			.map(|received| {
				received.unwrap_or_else(|e| {
					log::warn!(target: LOG_TARGET, "Error receiving pending set: {:?}", e);
					Box::new(std::iter::empty())
				})
			})
			.boxed()
	}

	fn ready(&self) -> ReadyIteratorFor<PoolApi> {
		Box::new(self.pool.validated_pool().ready())
	}

	fn futures(&self) -> Vec<Self::InPoolTransaction> {
		let pool = self.pool.validated_pool().pool.read();
		pool.futures().cloned().collect::<Vec<_>>()
	}

	fn ready_at_with_timeout(
		&self,
		at: <Self::Block as BlockT>::Hash,
		timeout: std::time::Duration,
	) -> PolledIterator<PoolApi> {
		self.ready_at_with_timeout_internal(at, timeout)
	}
}

impl<Block, Client> BasicPool<FullChainApi<Client, Block>, Block>
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
{
	/// Create new basic transaction pool for a full node with the provided api.
	pub fn new_full(
		options: graph::Options,
		is_validator: IsValidator,
		prometheus: Option<&PrometheusRegistry>,
		spawner: impl SpawnEssentialNamed,
		client: Arc<Client>,
	) -> Self {
		let pool_api = Arc::new(FullChainApi::new(client.clone(), prometheus, &spawner));
		let pool = Self::with_revalidation_type(
			options,
			is_validator,
			pool_api,
			prometheus,
			RevalidationType::Full,
			spawner,
			client.usage_info().chain.best_number,
			client.usage_info().chain.best_hash,
			client.usage_info().chain.finalized_hash,
		);

		pool
	}
}

impl<Block, Client> sc_transaction_pool_api::LocalTransactionPool
	for BasicPool<FullChainApi<Client, Block>, Block>
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
		use sp_runtime::{
			traits::SaturatedConversion, transaction_validity::TransactionValidityError,
		};

		let validity = self
			.api
			.validate_transaction_blocking(at, TransactionSource::Local, Arc::from(xt.clone()))?
			.map_err(|e| {
				Self::Error::Pool(match e {
					TransactionValidityError::Invalid(i) => TxPoolError::InvalidTransaction(i),
					TransactionValidityError::Unknown(u) => TxPoolError::UnknownTransaction(u),
				})
			})?;

		let (hash, bytes) = self.pool.validated_pool().api().hash_and_length(&xt);
		let block_number = self
			.api
			.block_id_to_number(&BlockId::hash(at))?
			.ok_or_else(|| error::Error::BlockIdConversion(format!("{:?}", at)))?;

		let validated = ValidatedTransaction::valid_at(
			block_number.saturated_into::<u64>(),
			hash,
			TransactionSource::Local,
			Arc::from(xt),
			bytes,
			validity,
		);

		self.pool.validated_pool().submit(vec![validated]).remove(0)
	}
}

#[cfg_attr(test, derive(Debug))]
enum RevalidationStatus<N> {
	/// The revalidation has never been completed.
	NotScheduled,
	/// The revalidation is scheduled.
	Scheduled(Option<Instant>, Option<N>),
	/// The revalidation is in progress.
	InProgress,
}

enum RevalidationStrategy<N> {
	Always,
	Light(RevalidationStatus<N>),
}

struct RevalidationAction {
	revalidate: bool,
	resubmit: bool,
}

impl<N: Clone + Copy + AtLeast32Bit> RevalidationStrategy<N> {
	pub fn clear(&mut self) {
		if let Self::Light(status) = self {
			status.clear()
		}
	}

	pub fn next(
		&mut self,
		block: N,
		revalidate_time_period: Option<std::time::Duration>,
		revalidate_block_period: Option<N>,
	) -> RevalidationAction {
		match self {
			Self::Light(status) => RevalidationAction {
				revalidate: status.next_required(
					block,
					revalidate_time_period,
					revalidate_block_period,
				),
				resubmit: false,
			},
			Self::Always => RevalidationAction { revalidate: true, resubmit: true },
		}
	}
}

impl<N: Clone + Copy + AtLeast32Bit> RevalidationStatus<N> {
	/// Called when revalidation is completed.
	pub fn clear(&mut self) {
		*self = Self::NotScheduled;
	}

	/// Returns true if revalidation is required.
	pub fn next_required(
		&mut self,
		block: N,
		revalidate_time_period: Option<std::time::Duration>,
		revalidate_block_period: Option<N>,
	) -> bool {
		match *self {
			Self::NotScheduled => {
				*self = Self::Scheduled(
					revalidate_time_period.map(|period| Instant::now() + period),
					revalidate_block_period.map(|period| block + period),
				);
				false
			},
			Self::Scheduled(revalidate_at_time, revalidate_at_block) => {
				let is_required =
					revalidate_at_time.map(|at| Instant::now() >= at).unwrap_or(false) ||
						revalidate_at_block.map(|at| block >= at).unwrap_or(false);
				if is_required {
					*self = Self::InProgress;
				}
				is_required
			},
			Self::InProgress => false,
		}
	}
}

/// Prune the known txs for the given block.
pub async fn prune_known_txs_for_block<Block: BlockT, Api: graph::ChainApi<Block = Block>>(
	at: &HashAndNumber<Block>,
	api: &Api,
	pool: &graph::Pool<Api>,
) -> Vec<ExtrinsicHash<Api>> {
	let extrinsics = api
		.block_body(at.hash)
		.await
		.unwrap_or_else(|e| {
			log::warn!(target: LOG_TARGET, "Prune known transactions: error request: {}", e);
			None
		})
		.unwrap_or_default();

	let hashes = extrinsics.iter().map(|tx| pool.hash_of(tx)).collect::<Vec<_>>();

	let header = match api.block_header(at.hash) {
		Ok(Some(h)) => h,
		Ok(None) => {
			log::trace!(target: LOG_TARGET, "Could not find header for {:?}.", at.hash);
			return hashes
		},
		Err(e) => {
			log::trace!(target: LOG_TARGET, "Error retrieving header for {:?}: {}", at.hash, e);
			return hashes
		},
	};

	log_xt_trace!(target: LOG_TARGET, &hashes, "[{:?}] Pruning transaction.");

	pool.prune(at, *header.parent_hash(), &extrinsics).await;
	hashes
}

impl<PoolApi, Block> BasicPool<PoolApi, Block>
where
	Block: BlockT,
	PoolApi: 'static + graph::ChainApi<Block = Block>,
{
	/// Handles enactment and retraction of blocks, prunes stale transactions
	/// (that have already been enacted) and resubmits transactions that were
	/// retracted.
	async fn handle_enactment(&self, tree_route: TreeRoute<Block>) {
		log::trace!(target: LOG_TARGET, "handle_enactment tree_route: {tree_route:?}");
		let pool = self.pool.clone();
		let api = self.api.clone();

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

		let next_action = self.revalidation_strategy.lock().next(
			hash_and_number.number,
			Some(std::time::Duration::from_secs(60)),
			Some(20u32.into()),
		);

		// We keep track of everything we prune so that later we won't add
		// transactions with those hashes from the retracted blocks.
		let mut pruned_log = HashSet::<ExtrinsicHash<PoolApi>>::new();

		// If there is a tree route, we use this to prune known tx based on the enacted
		// blocks. Before pruning enacted transactions, we inform the listeners about
		// retracted blocks and their transactions. This order is important, because
		// if we enact and retract the same transaction at the same time, we want to
		// send first the retract and then the prune event.
		for retracted in tree_route.retracted() {
			// notify txs awaiting finality that it has been retracted
			pool.validated_pool().on_block_retracted(retracted.hash);
		}

		future::join_all(
			tree_route.enacted().iter().map(|h| prune_known_txs_for_block(h, &*api, &*pool)),
		)
		.await
		.into_iter()
		.for_each(|enacted_log| {
			pruned_log.extend(enacted_log);
		});

		self.metrics
			.report(|metrics| metrics.block_transactions_pruned.inc_by(pruned_log.len() as u64));

		if next_action.resubmit {
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
					//todo: arctx - we need to get ref from somewhere
					block_transactions.into_iter().map(Arc::from).filter(|tx| {
						let tx_hash = pool.hash_of(tx);
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
					}),
				);

				self.metrics.report(|metrics| {
					metrics.block_transactions_resubmitted.inc_by(resubmitted_to_report)
				});
			}

			pool.resubmit_at(
				&hash_and_number,
				// These transactions are coming from retracted blocks, we should
				// simply consider them external.
				TransactionSource::External,
				resubmit_transactions,
			)
			.await;
		}

		let extra_pool = pool.clone();
		// After #5200 lands, this arguably might be moved to the
		// handler of "all blocks notification".
		self.ready_poll
			.lock()
			.trigger(hash_and_number.number, move || Box::new(extra_pool.validated_pool().ready()));

		if next_action.revalidate {
			let hashes = pool.validated_pool().ready().map(|tx| tx.hash).collect();
			self.revalidation_queue.revalidate_later(hash_and_number.hash, hashes).await;

			self.revalidation_strategy.lock().clear();
		}
	}
}

#[async_trait]
impl<PoolApi, Block> MaintainedTransactionPool for BasicPool<PoolApi, Block>
where
	Block: BlockT,
	PoolApi: 'static + graph::ChainApi<Block = Block>,
{
	async fn maintain(&self, event: ChainEvent<Self::Block>) {
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
				log::trace!(target: LOG_TARGET, "{msg}");
				self.enactment_state.lock().force_update(&event);
			},
			Ok(EnactmentAction::Skip) => return,
			Ok(EnactmentAction::HandleFinalization) => {},
			Ok(EnactmentAction::HandleEnactment(tree_route)) => {
				self.handle_enactment(tree_route).await;
			},
		};

		if let ChainEvent::Finalized { hash, tree_route } = event {
			log::trace!(
				target: LOG_TARGET,
				"on-finalized enacted: {tree_route:?}, previously finalized: \
				{prev_finalized_block:?}",
			);

			for hash in tree_route.iter().chain(std::iter::once(&hash)) {
				if let Err(e) = self.pool.validated_pool().on_block_finalized(*hash).await {
					log::warn!(
						target: LOG_TARGET,
						"Error occurred while attempting to notify watchers about finalization {}: {}",
						hash, e
					)
				}
			}
		}
	}
}
