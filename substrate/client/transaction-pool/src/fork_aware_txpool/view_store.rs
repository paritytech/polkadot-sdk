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
	graph::{
		self,
		base_pool::{TimedTransactionSource, Transaction},
		BaseSubmitOutcome, ExtrinsicFor, ExtrinsicHash, TransactionFor, ValidatedPoolSubmitOutcome,
	},
	ReadyIteratorFor, LOG_TARGET,
};
use futures::prelude::*;
use itertools::Itertools;
use parking_lot::RwLock;
use sc_transaction_pool_api::{error::Error as PoolError, PoolStatus};
use sp_blockchain::TreeRoute;
use sp_runtime::{generic::BlockId, traits::Block as BlockT};
use std::{
	collections::{hash_map::Entry, HashMap, HashSet},
	sync::Arc,
	time::Instant,
};

/// Helper struct to maintain the context for pending transaction submission, executed for
/// newly inserted views.
#[derive(Clone)]
struct PendingTxSubmission<ChainApi>
where
	ChainApi: graph::ChainApi,
{
	/// New transaction replacing the old one.
	xt: ExtrinsicFor<ChainApi>,
	/// Source of the transaction.
	source: TimedTransactionSource,
	/// Inidicates if transaction is watched.
	watched: bool,
}

/// Helper type representing the callback allowing to trigger per-transaction events on
/// `ValidatedPool`'s listener.
type RemovalListener<ChainApi> =
	Arc<dyn Fn(&mut crate::graph::Listener<ChainApi>, ExtrinsicHash<ChainApi>) + Send + Sync>;

/// Helper struct to maintain the context for pending transaction removal, executed for
/// newly inserted views.
struct PendingTxRemoval<ChainApi>
where
	ChainApi: graph::ChainApi,
{
	/// Hash of the transaction that will be removed,
	xt_hash: ExtrinsicHash<ChainApi>,
	/// Action that shall be executed on underlying `ValidatedPool`'s listener.
	listener_action: RemovalListener<ChainApi>,
}

/// This enum represents an action that should be executed on the newly built
/// view before this view is inserted into the view store.
enum PreInsertAction<ChainApi>
where
	ChainApi: graph::ChainApi,
{
	/// Represents the action of submitting a new transaction. Intended to use to handle usurped
	/// transactions.
	SubmitTx(PendingTxSubmission<ChainApi>),

	/// Represents the action of removing a subtree of transactions.
	RemoveSubtree(PendingTxRemoval<ChainApi>),
}

/// Represents a task awaiting execution, to be performed immediately prior to the view insertion
/// into the view store.
struct PendingPreInsertTask<ChainApi>
where
	ChainApi: graph::ChainApi,
{
	/// The action to be applied when inserting a new view.
	action: PreInsertAction<ChainApi>,
	/// Indicates if the action was already applied to all the views in the view_store.
	/// If true, it can be removed after inserting any new view.
	processed: bool,
}

impl<ChainApi> PendingPreInsertTask<ChainApi>
where
	ChainApi: graph::ChainApi,
{
	/// Creates new unprocessed instance of pending transaction submission.
	fn new_submission_action(
		xt: ExtrinsicFor<ChainApi>,
		source: TimedTransactionSource,
		watched: bool,
	) -> Self {
		Self {
			processed: false,
			action: PreInsertAction::SubmitTx(PendingTxSubmission { xt, source, watched }),
		}
	}

	/// Creates new unprocessed instance of pending transaction removal.
	fn new_removal_action(
		xt_hash: ExtrinsicHash<ChainApi>,
		listener: RemovalListener<ChainApi>,
	) -> Self {
		Self {
			processed: false,
			action: PreInsertAction::RemoveSubtree(PendingTxRemoval {
				xt_hash,
				listener_action: listener,
			}),
		}
	}

	/// Marks a task as done for every view present in view store. Basically means that can be
	/// removed on new view insertion.
	fn mark_processed(&mut self) {
		self.processed = true;
	}
}

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
	/// The map used to synchronize replacement of transactions between maintain and dropped
	/// notifcication threads. It is meant to assure that replaced transaction is also removed from
	/// newly built views in maintain process.
	///
	/// The map's key is hash of actionable extrinsic (to avoid duplicated entries).
	pending_txs_tasks: RwLock<HashMap<ExtrinsicHash<ChainApi>, PendingPreInsertTask<ChainApi>>>,
}

/// Type alias to outcome of submission to `ViewStore`.
pub(super) type ViewStoreSubmitOutcome<ChainApi> =
	BaseSubmitOutcome<ChainApi, TxStatusStream<ChainApi>>;

impl<ChainApi: graph::ChainApi> From<ValidatedPoolSubmitOutcome<ChainApi>>
	for ViewStoreSubmitOutcome<ChainApi>
{
	fn from(value: ValidatedPoolSubmitOutcome<ChainApi>) -> Self {
		Self::new(value.hash(), value.priority())
	}
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
			pending_txs_tasks: Default::default(),
		}
	}

	/// Imports a bunch of unverified extrinsics to every active view.
	pub(super) async fn submit(
		&self,
		xts: impl IntoIterator<Item = (TimedTransactionSource, ExtrinsicFor<ChainApi>)> + Clone,
	) -> HashMap<Block::Hash, Vec<Result<ViewStoreSubmitOutcome<ChainApi>, ChainApi::Error>>> {
		let submit_futures = {
			let active_views = self.active_views.read();
			active_views
				.iter()
				.map(|(_, view)| {
					let view = view.clone();
					let xts = xts.clone();
					async move {
						(
							view.at.hash,
							view.submit_many(xts)
								.await
								.into_iter()
								.map(|r| r.map(Into::into))
								.collect::<Vec<_>>(),
						)
					}
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
	) -> Result<ViewStoreSubmitOutcome<ChainApi>, ChainApi::Error> {
		let active_views = self
			.active_views
			.read()
			.iter()
			.map(|(_, view)| view.clone())
			.collect::<Vec<_>>();

		let tx_hash = self.api.hash_and_length(&xt).0;

		let result = active_views
			.iter()
			.map(|view| view.submit_local(xt.clone()))
			.find_or_first(Result::is_ok);

		match result {
			Some(Err(err)) => {
				log::trace!(target: LOG_TARGET, "[{:?}] submit_local: err: {}", tx_hash, err);
				Err(err)
			},
			None => Ok(ViewStoreSubmitOutcome::new(tx_hash, None)),
			Some(Ok(r)) => Ok(r.into()),
		}
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
		source: TimedTransactionSource,
		xt: ExtrinsicFor<ChainApi>,
	) -> Result<ViewStoreSubmitOutcome<ChainApi>, ChainApi::Error> {
		let tx_hash = self.api.hash_and_length(&xt).0;
		let Some(external_watcher) = self.listener.create_external_watcher_for_tx(tx_hash) else {
			return Err(PoolError::AlreadyImported(Box::new(tx_hash)).into())
		};
		let submit_and_watch_futures = {
			let active_views = self.active_views.read();
			active_views
				.iter()
				.map(|(_, view)| {
					let view = view.clone();
					let xt = xt.clone();
					let source = source.clone();
					async move {
						match view.submit_and_watch(source, xt).await {
							Ok(mut result) => {
								self.listener.add_view_watcher_for_tx(
									tx_hash,
									view.at.hash,
									result.expect_watcher().into_stream().boxed(),
								);
								Ok(result)
							},
							Err(e) => Err(e),
						}
					}
				})
				.collect::<Vec<_>>()
		};
		let result = futures::future::join_all(submit_and_watch_futures)
			.await
			.into_iter()
			.find_or_first(Result::is_ok);

		match result {
			Some(Err(err)) => {
				log::trace!(target: LOG_TARGET, "[{:?}] submit_and_watch: err: {}", tx_hash, err);
				return Err(err);
			},
			Some(Ok(result)) =>
				Ok(ViewStoreSubmitOutcome::from(result).with_watcher(external_watcher)),
			None => Ok(ViewStoreSubmitOutcome::new(tx_hash, None).with_watcher(external_watcher)),
		}
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
			.map(|at| self.futures_at(at))
			.flatten()
			.unwrap_or_default()
	}

	/// Returns a list of future transactions in the view at given block hash.
	pub(super) fn futures_at(
		&self,
		at: Block::Hash,
	) -> Option<Vec<Transaction<ExtrinsicHash<ChainApi>, ExtrinsicFor<ChainApi>>>> {
		self.get_view_at(at, true)
			.map(|(v, _)| v.pool.validated_pool().pool.read().futures().cloned().collect())
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
	/// The `most_recent_view` is updated with the reference to the newly inserted view.
	///
	/// If there are any pending tx replacments, they are applied to the new view.
	pub(super) async fn insert_new_view(
		&self,
		view: Arc<View<ChainApi>>,
		tree_route: &TreeRoute<Block>,
	) {
		self.apply_pending_tx_replacements(view.clone()).await;

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
			let active_views = self.active_views.read();
			let inactive_views = self.inactive_views.read();

			active_views
				.iter()
				.filter(|(hash, v)| !match finalized_number {
					Err(_) | Ok(None) => **hash == finalized_hash,
					Ok(Some(n)) if v.at.number == n => **hash == finalized_hash,
					Ok(Some(n)) => v.at.number > n,
				})
				.map(|(_, v)| removed_views.push(v.at.hash))
				.for_each(drop);

			inactive_views
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

		let mut dropped_views = vec![];
		//clean up older then finalized
		{
			let mut active_views = self.active_views.write();
			let mut inactive_views = self.inactive_views.write();
			active_views.retain(|hash, v| {
				let retain = match finalized_number {
					Err(_) | Ok(None) => *hash == finalized_hash,
					Ok(Some(n)) if v.at.number == n => *hash == finalized_hash,
					Ok(Some(n)) => v.at.number > n,
				};
				if !retain {
					dropped_views.push(*hash);
				}
				retain
			});

			inactive_views.retain(|hash, v| {
				let retain = match finalized_number {
					Err(_) | Ok(None) => false,
					Ok(Some(n)) => v.at.number >= n,
				};
				if !retain {
					dropped_views.push(*hash);
				}
				retain
			});

			log::trace!(target:LOG_TARGET,"handle_finalized: inactive_views: {:?}", inactive_views.keys());
		}

		log::trace!(target:LOG_TARGET,"handle_finalized: dropped_views: {:?}", dropped_views);

		self.listener.remove_stale_controllers();
		self.dropped_stream_controller.remove_finalized_txs(finalized_xts.clone());

		self.listener.remove_view(finalized_hash);
		for view in dropped_views {
			self.listener.remove_view(view);
			self.dropped_stream_controller.remove_view(view);
		}

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

	/// Replaces an existing transaction in the view_store with a new one.
	///
	/// Attempts to replace a transaction identified by `replaced` with a new transaction `xt`.
	///
	/// Before submitting a transaction to the views, the new *unprocessed* transaction replacement
	/// record will be inserted into a pending replacement map. Once the submission to all the views
	/// is accomplished, the record is marked as *processed*.
	///
	/// This map is later applied in `insert_new_view` method executed from different thread.
	///
	/// If the transaction is already being replaced, it will simply return without making
	/// changes.
	pub(super) async fn replace_transaction(
		&self,
		source: TimedTransactionSource,
		xt: ExtrinsicFor<ChainApi>,
		replaced: ExtrinsicHash<ChainApi>,
		watched: bool,
	) {
		if let Entry::Vacant(entry) = self.pending_txs_tasks.write().entry(replaced) {
			entry.insert(PendingPreInsertTask::new_submission_action(
				xt.clone(),
				source.clone(),
				watched,
			));
		} else {
			return
		};

		let xt_hash = self.api.hash_and_length(&xt).0;
		log::trace!(target:LOG_TARGET,"[{replaced:?}] replace_transaction wtih {xt_hash:?}, w:{watched}");

		self.replace_transaction_in_views(source, xt, xt_hash, replaced, watched).await;

		if let Some(replacement) = self.pending_txs_tasks.write().get_mut(&replaced) {
			replacement.mark_processed();
		}
	}

	/// Applies pending transaction replacements to the specified view.
	///
	/// After application, all already processed replacements are removed.
	async fn apply_pending_tx_replacements(&self, view: Arc<View<ChainApi>>) {
		let mut futures = vec![];
		for replacement in self.pending_txs_tasks.read().values() {
			match replacement.action {
				PreInsertAction::SubmitTx(ref submission) => {
					let xt_hash = self.api.hash_and_length(&submission.xt).0;
					futures.push(self.replace_transaction_in_view(
						view.clone(),
						submission.source.clone(),
						submission.xt.clone(),
						xt_hash,
						submission.watched,
					));
				},
				PreInsertAction::RemoveSubtree(ref removal) => {
					view.remove_subtree(removal.xt_hash, &*removal.listener_action);
				},
			}
		}
		let _results = futures::future::join_all(futures).await;
		self.pending_txs_tasks.write().retain(|_, r| r.processed);
	}

	/// Submits `xt` to the given view.
	///
	/// For watched transaction stream is added to the listener.
	async fn replace_transaction_in_view(
		&self,
		view: Arc<View<ChainApi>>,
		source: TimedTransactionSource,
		xt: ExtrinsicFor<ChainApi>,
		xt_hash: ExtrinsicHash<ChainApi>,
		watched: bool,
	) {
		if watched {
			match view.submit_and_watch(source, xt).await {
				Ok(mut result) => {
					self.listener.add_view_watcher_for_tx(
						xt_hash,
						view.at.hash,
						result.expect_watcher().into_stream().boxed(),
					);
				},
				Err(e) => {
					log::trace!(
						target:LOG_TARGET,
						"[{:?}] replace_transaction: submit_and_watch to {} failed {}",
						xt_hash, view.at.hash, e
					);
				},
			}
		} else {
			if let Some(Err(e)) = view.submit_many(std::iter::once((source, xt))).await.pop() {
				log::trace!(
					target:LOG_TARGET,
					"[{:?}] replace_transaction: submit to {} failed {}",
					xt_hash, view.at.hash, e
				);
			}
		}
	}

	/// Sends `xt` to every view (both active and inactive) containing `replaced` extrinsics.
	///
	/// It is assumed that transaction is already known by the pool. Intended to ba called when `xt`
	/// is replacing `replaced` extrinsic.
	async fn replace_transaction_in_views(
		&self,
		source: TimedTransactionSource,
		xt: ExtrinsicFor<ChainApi>,
		xt_hash: ExtrinsicHash<ChainApi>,
		replaced: ExtrinsicHash<ChainApi>,
		watched: bool,
	) {
		if watched && !self.listener.contains_tx(&xt_hash) {
			log::trace!(
				target:LOG_TARGET,
				"error: replace_transaction_in_views: no listener for watched transaction {:?}",
				xt_hash,
			);
			return;
		}

		let submit_futures = {
			let active_views = self.active_views.read();
			let inactive_views = self.inactive_views.read();
			active_views
				.iter()
				.chain(inactive_views.iter())
				.filter(|(_, view)| view.is_imported(&replaced))
				.map(|(_, view)| {
					self.replace_transaction_in_view(
						view.clone(),
						source.clone(),
						xt.clone(),
						xt_hash,
						watched,
					)
				})
				.collect::<Vec<_>>()
		};
		let _results = futures::future::join_all(submit_futures).await;
	}

	/// Removes a transaction subtree from every view in the view_store, starting from the given
	/// transaction hash.
	///
	/// This function traverses the dependency graph of transactions and removes the specified
	/// transaction along with all its descendant transactions from every view.
	///
	/// A `listener_action` callback function is invoked for every transaction that is removed,
	/// providing a reference to the pool's listener and the hash of the removed transaction. This
	/// allows to trigger the required events. Note that listener may be called multiple times for
	/// the same hash.
	///
	/// Function will also schedule view pre-insertion actions to ensure that transactions will be
	/// removed from newly created view.
	///
	/// Returns a vector containing the hashes of all removed transactions, including the root
	/// transaction specified by `tx_hash`. Vector contains only unique hashes.
	pub(super) fn remove_transaction_subtree<F>(
		&self,
		xt_hash: ExtrinsicHash<ChainApi>,
		listener_action: F,
	) -> Vec<ExtrinsicHash<ChainApi>>
	where
		F: Fn(&mut crate::graph::Listener<ChainApi>, ExtrinsicHash<ChainApi>)
			+ Clone
			+ Send
			+ Sync
			+ 'static,
	{
		if let Entry::Vacant(entry) = self.pending_txs_tasks.write().entry(xt_hash) {
			entry.insert(PendingPreInsertTask::new_removal_action(
				xt_hash,
				Arc::from(listener_action.clone()),
			));
		};

		let mut seen = HashSet::new();

		let removed = self
			.active_views
			.read()
			.iter()
			.chain(self.inactive_views.read().iter())
			.filter(|(_, view)| view.is_imported(&xt_hash))
			.flat_map(|(_, view)| view.remove_subtree(xt_hash, &listener_action))
			.filter(|xt_hash| seen.insert(*xt_hash))
			.collect();

		if let Some(removal_action) = self.pending_txs_tasks.write().get_mut(&xt_hash) {
			removal_action.mark_processed();
		}

		removed
	}
}
