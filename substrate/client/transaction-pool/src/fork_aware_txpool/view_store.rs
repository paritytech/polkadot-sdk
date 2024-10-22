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

//! Transaction pool view store. Basically block hash to view map with some utility methods.

use super::{
	multi_view_listener::{MultiViewListener, TxStatusStream},
	view::View,
};
use crate::{
	fork_aware_txpool::dropped_watcher::MultiViewDroppedWatcherController,
	graph,
	graph::{base_pool::Transaction, ExtrinsicFor, ExtrinsicHash, TransactionFor},
	ReadyIteratorFor, LOG_TARGET,
};
use futures::prelude::*;
use itertools::Itertools;
use parking_lot::RwLock;
use sc_transaction_pool_api::{error::Error as PoolError, PoolStatus, TransactionSource};
use sp_blockchain::TreeRoute;
use sp_runtime::{generic::BlockId, traits::Block as BlockT};
use std::{collections::HashMap, sync::Arc, time::Instant};

/// The helper structure encapsulates all the views.
pub(super) struct ViewStore<ChainApi, Block>
where
	Block: BlockT,
	ChainApi: graph::ChainApi<Block = Block>,
{
	/// The blockchain api.
	pub(super) api: Arc<ChainApi>,
	/// Active views at tips of the forks.
	///
	/// Active views are updated with incoming transactions.
	pub(super) active_views: RwLock<HashMap<Block::Hash, Arc<View<ChainApi>>>>,
	/// Inactive views at intermediary blocks that are no longer tips of the forks.
	///
	/// Inactive views are not updated with incoming transactions, while they can still be used to
	/// build new blocks upon them.
	pub(super) inactive_views: RwLock<HashMap<Block::Hash, Arc<View<ChainApi>>>>,
	/// Listener for controlling external watchers of transactions.
	///
	/// Provides a side-channel allowing to send per-transaction state changes notification.
	pub(super) listener: Arc<MultiViewListener<ChainApi>>,
	/// Most recent block processed by tx-pool. Used in the API functions that were not changed to
	/// add `at` parameter.
	pub(super) most_recent_view: RwLock<Option<Block::Hash>>,
	/// The controller of multi view dropped stream.
	pub(super) dropped_stream_controller: MultiViewDroppedWatcherController<ChainApi>,
}

impl<ChainApi, Block> ViewStore<ChainApi, Block>
where
	Block: BlockT,
	ChainApi: graph::ChainApi<Block = Block> + 'static,
	<Block as BlockT>::Hash: Unpin,
{
	/// Creates a new empty view store.
	pub(super) fn new(
		api: Arc<ChainApi>,
		listener: Arc<MultiViewListener<ChainApi>>,
		dropped_stream_controller: MultiViewDroppedWatcherController<ChainApi>,
	) -> Self {
		Self {
			api,
			active_views: Default::default(),
			inactive_views: Default::default(),
			listener,
			most_recent_view: RwLock::from(None),
			dropped_stream_controller,
		}
	}

	/// Imports a bunch of unverified extrinsics to every active view.
	pub(super) async fn submit(
		&self,
		source: TransactionSource,
		xts: impl IntoIterator<Item = ExtrinsicFor<ChainApi>> + Clone,
		xts_hashes: impl IntoIterator<Item = ExtrinsicHash<ChainApi>> + Clone,
	) -> HashMap<Block::Hash, Vec<Result<ExtrinsicHash<ChainApi>, ChainApi::Error>>> {
		let submit_futures = {
			let active_views = self.active_views.read();
			active_views
				.iter()
				.map(|(_, view)| {
					let view = view.clone();
					let xts = xts.clone();
					self.dropped_stream_controller
						.add_initial_views(xts_hashes.clone(), view.at.hash);
					async move { (view.at.hash, view.submit_many(source, xts.clone()).await) }
				})
				.collect::<Vec<_>>()
		};
		let results = futures::future::join_all(submit_futures).await;

		HashMap::<_, _>::from_iter(results.into_iter())
	}

	/// Synchronously imports single unverified extrinsics into every active view.
	pub(super) fn submit_local(
		&self,
		xt: ExtrinsicFor<ChainApi>,
	) -> Result<ExtrinsicHash<ChainApi>, ChainApi::Error> {
		let active_views = self
			.active_views
			.read()
			.iter()
			.map(|(_, view)| view.clone())
			.collect::<Vec<_>>();

		let tx_hash = self.api.hash_and_length(&xt).0;

		let result = active_views
			.iter()
			.map(|view| {
				self.dropped_stream_controller
					.add_initial_views(std::iter::once(tx_hash), view.at.hash);
				view.submit_local(xt.clone())
			})
			.find_or_first(Result::is_ok);

		if let Some(Err(err)) = result {
			log::trace!(target: LOG_TARGET, "[{:?}] submit_local: err: {}", tx_hash, err);
			return Err(err)
		};

		Ok(tx_hash)
	}

	/// Import a single extrinsic and starts to watch its progress in the pool.
	///
	/// The extrinsic is imported to every view, and the individual streams providing the progress
	/// of this transaction within every view are added to the multi view listener.
	///
	/// The external stream of aggregated/processed events provided by the `MultiViewListener`
	/// instance is returned.
	pub(super) async fn submit_and_watch(
		&self,
		_at: Block::Hash,
		source: TransactionSource,
		xt: ExtrinsicFor<ChainApi>,
	) -> Result<TxStatusStream<ChainApi>, (ChainApi::Error, Option<TxStatusStream<ChainApi>>)> {
		let tx_hash = self.api.hash_and_length(&xt).0;
		let Some(external_watcher) = self.listener.create_external_watcher_for_tx(tx_hash) else {
			return Err((PoolError::AlreadyImported(Box::new(tx_hash)).into(), None))
		};
		let submit_and_watch_futures = {
			let active_views = self.active_views.read();
			active_views
				.iter()
				.map(|(_, view)| {
					let view = view.clone();
					let xt = xt.clone();
					self.dropped_stream_controller
						.add_initial_views(std::iter::once(tx_hash), view.at.hash);
					async move {
						match view.submit_and_watch(source, xt).await {
							Ok(watcher) => {
								self.listener.add_view_watcher_for_tx(
									tx_hash,
									view.at.hash,
									watcher.into_stream().boxed(),
								);
								Ok(())
							},
							Err(e) => Err(e),
						}
					}
				})
				.collect::<Vec<_>>()
		};
		let maybe_error = futures::future::join_all(submit_and_watch_futures)
			.await
			.into_iter()
			.find_or_first(Result::is_ok);

		if let Some(Err(err)) = maybe_error {
			log::trace!(target: LOG_TARGET, "[{:?}] submit_and_watch: err: {}", tx_hash, err);
			return Err((err, Some(external_watcher)));
		};

		Ok(external_watcher)
	}

	/// Returns the pool status for every active view.
	pub(super) fn status(&self) -> HashMap<Block::Hash, PoolStatus> {
		self.active_views.read().iter().map(|(h, v)| (*h, v.status())).collect()
	}

	/// Returns true if there are no active views.
	pub(super) fn is_empty(&self) -> bool {
		self.active_views.read().is_empty() && self.inactive_views.read().is_empty()
	}

	/// Finds the best existing active view to clone from along the path.
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
	/// [E1, C, R3, R2, R1]
	/// ```
	pub(super) fn find_best_view(
		&self,
		tree_route: &TreeRoute<Block>,
	) -> Option<Arc<View<ChainApi>>> {
		let active_views = self.active_views.read();
		let best_view = {
			tree_route
				.retracted()
				.iter()
				.chain(std::iter::once(tree_route.common_block()))
				.chain(tree_route.enacted().iter())
				.rev()
				.find(|block| active_views.contains_key(&block.hash))
		};
		best_view.map(|h| {
			active_views
				.get(&h.hash)
				.expect("hash was just found in the map's keys. qed")
				.clone()
		})
	}

	/// Returns an iterator for ready transactions for the most recently notified best block.
	///
	/// The iterator for future transactions is returned if the most recently notified best block,
	/// for which maintain process was accomplished, exists.
	pub(super) fn ready(&self) -> ReadyIteratorFor<ChainApi> {
		let ready_iterator = self
			.most_recent_view
			.read()
			.map(|at| self.get_view_at(at, true))
			.flatten()
			.map(|(v, _)| v.pool.validated_pool().ready());

		if let Some(ready_iterator) = ready_iterator {
			return Box::new(ready_iterator)
		} else {
			return Box::new(std::iter::empty())
		}
	}

	/// Returns a list of future transactions for the most recently notified best block.
	///
	/// The set of future transactions is returned if the most recently notified best block, for
	/// which maintain process was accomplished, exists.
	pub(super) fn futures(
		&self,
	) -> Vec<Transaction<ExtrinsicHash<ChainApi>, ExtrinsicFor<ChainApi>>> {
		self.most_recent_view
			.read()
			.map(|at| self.get_view_at(at, true))
			.flatten()
			.map(|(v, _)| v.pool.validated_pool().pool.read().futures().cloned().collect())
			.unwrap_or_default()
	}

	/// Collects all the transactions included in the blocks on the provided `tree_route` and
	/// triggers finalization event for them.
	///
	/// The finalization event is sent using side-channel of the multi view `listener`.
	///
	/// Returns the list of finalized transactions hashes.
	pub(super) async fn finalize_route(
		&self,
		finalized_hash: Block::Hash,
		tree_route: &[Block::Hash],
	) -> Vec<ExtrinsicHash<ChainApi>> {
		log::trace!(target: LOG_TARGET, "finalize_route finalized_hash:{finalized_hash:?} tree_route: {tree_route:?}");

		let mut finalized_transactions = Vec::new();

		for block in tree_route.iter().chain(std::iter::once(&finalized_hash)) {
			let extrinsics = self
				.api
				.block_body(*block)
				.await
				.unwrap_or_else(|e| {
					log::warn!(target: LOG_TARGET, "Finalize route: error request: {}", e);
					None
				})
				.unwrap_or_default()
				.iter()
				.map(|e| self.api.hash_and_length(&e).0)
				.collect::<Vec<_>>();

			extrinsics
				.iter()
				.enumerate()
				.for_each(|(i, tx_hash)| self.listener.finalize_transaction(*tx_hash, *block, i));

			finalized_transactions.extend(extrinsics);
		}

		finalized_transactions
	}

	/// Return specific ready transaction by hash, if there is one.
	///
	/// Currently the ready transaction is returned if it exists for the most recently notified best
	/// block (for which maintain process was accomplished).
	pub(super) fn ready_transaction(
		&self,
		at: Block::Hash,
		tx_hash: &ExtrinsicHash<ChainApi>,
	) -> Option<TransactionFor<ChainApi>> {
		self.active_views
			.read()
			.get(&at)
			.and_then(|v| v.pool.validated_pool().ready_by_hash(tx_hash))
	}

	/// Inserts new view into the view store.
	///
	/// All the views associated with the blocks which are on enacted path (including common
	/// ancestor) will be:
	/// - moved to the inactive views set (`inactive_views`),
	/// - removed from the multi view listeners.
	///
	/// The `most_recent_view` is update with the reference to the newly inserted view.
	pub(super) async fn insert_new_view(
		&self,
		view: Arc<View<ChainApi>>,
		tree_route: &TreeRoute<Block>,
	) {
		//note: most_recent_view must be synced with changes in in/active_views.
		{
			let mut most_recent_view_lock = self.most_recent_view.write();
			let mut active_views = self.active_views.write();
			let mut inactive_views = self.inactive_views.write();

			std::iter::once(tree_route.common_block())
				.chain(tree_route.enacted().iter())
				.map(|block| block.hash)
				.for_each(|hash| {
					active_views.remove(&hash).map(|view| {
						inactive_views.insert(hash, view);
					});
				});
			active_views.insert(view.at.hash, view.clone());
			most_recent_view_lock.replace(view.at.hash);
		};
		log::trace!(target:LOG_TARGET,"insert_new_view: inactive_views: {:?}", self.inactive_views.read().keys());
	}

	/// Returns an optional reference to the view at given hash.
	///
	/// If `allow_retracted` flag is set, inactive views are also searched.
	///
	/// If the view at provided hash does not exist `None` is returned.
	pub(super) fn get_view_at(
		&self,
		at: Block::Hash,
		allow_inactive: bool,
	) -> Option<(Arc<View<ChainApi>>, bool)> {
		if let Some(view) = self.active_views.read().get(&at) {
			return Some((view.clone(), false));
		}
		if allow_inactive {
			if let Some(view) = self.inactive_views.read().get(&at) {
				return Some((view.clone(), true))
			}
		};
		None
	}

	/// The pre-finalization event handle for the view store.
	///
	/// This function removes the references to the views that will be removed during finalization
	/// from the dropped stream controller. This will allow for correct dispatching of `Dropped`
	/// events.
	pub(crate) async fn handle_pre_finalized(&self, finalized_hash: Block::Hash) {
		let finalized_number = self.api.block_id_to_number(&BlockId::Hash(finalized_hash));
		let mut removed_views = vec![];

		{
			self.active_views
				.read()
				.iter()
				.filter(|(hash, v)| !match finalized_number {
					Err(_) | Ok(None) => **hash == finalized_hash,
					Ok(Some(n)) if v.at.number == n => **hash == finalized_hash,
					Ok(Some(n)) => v.at.number > n,
				})
				.map(|(_, v)| removed_views.push(v.at.hash))
				.for_each(drop);
		}

		{
			self.inactive_views
				.read()
				.iter()
				.filter(|(_, v)| !match finalized_number {
					Err(_) | Ok(None) => false,
					Ok(Some(n)) => v.at.number >= n,
				})
				.map(|(_, v)| removed_views.push(v.at.hash))
				.for_each(drop);
		}

		log::trace!(target:LOG_TARGET,"handle_pre_finalized: removed_views: {:?}", removed_views);

		removed_views.iter().for_each(|view| {
			self.dropped_stream_controller.remove_view(*view);
		});
	}

	/// The finalization event handle for the view store.
	///
	/// Views that have associated block number less than finalized block number are removed from
	/// both active and inactive set.
	///
	/// Note: the views with the associated number greater than finalized block number on the forks
	/// that are not finalized will stay in the view store. They will be removed in the future, once
	/// new finalized blocks will be notified. This is to avoid scanning for common ancestors.
	///
	/// All watched transactions in the blocks from the tree_route will be notified with `Finalized`
	/// event.
	///
	/// Returns the list of hashes of all finalized transactions along the provided `tree_route`.
	pub(crate) async fn handle_finalized(
		&self,
		finalized_hash: Block::Hash,
		tree_route: &[Block::Hash],
	) -> Vec<ExtrinsicHash<ChainApi>> {
		let finalized_xts = self.finalize_route(finalized_hash, tree_route).await;
		let finalized_number = self.api.block_id_to_number(&BlockId::Hash(finalized_hash));

		//clean up older then finalized
		{
			let mut active_views = self.active_views.write();
			active_views.retain(|hash, v| match finalized_number {
				Err(_) | Ok(None) => *hash == finalized_hash,
				Ok(Some(n)) if v.at.number == n => *hash == finalized_hash,
				Ok(Some(n)) => v.at.number > n,
			});
		}

		{
			let mut inactive_views = self.inactive_views.write();
			inactive_views.retain(|_, v| match finalized_number {
				Err(_) | Ok(None) => false,
				Ok(Some(n)) => v.at.number >= n,
			});

			log::trace!(target:LOG_TARGET,"handle_finalized: inactive_views: {:?}", inactive_views.keys());
		}

		self.listener.remove_view(finalized_hash);
		self.listener.remove_stale_controllers();
		self.dropped_stream_controller.remove_finalized_txs(finalized_xts.clone());

		finalized_xts
	}

	/// Terminates all the ongoing background views revalidations triggered at the end of maintain
	/// process.
	///
	/// Refer to [*View revalidation*](../index.html#view-revalidation) for more details.
	pub(crate) async fn finish_background_revalidations(&self) {
		let start = Instant::now();
		let finish_revalidation_futures = {
			let active_views = self.active_views.read();
			active_views
				.iter()
				.map(|(_, view)| {
					let view = view.clone();
					async move { view.finish_revalidation().await }
				})
				.collect::<Vec<_>>()
		};
		futures::future::join_all(finish_revalidation_futures).await;
		log::trace!(target:LOG_TARGET,"finish_background_revalidations took {:?}", start.elapsed());
	}
}
