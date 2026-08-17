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
	import_notification_sink::MultiViewImportNotificationSink,
	multi_view_listener::{MultiViewListener, TxStatusStream},
	view::{View, ViewPoolObserver},
};
use crate::{
	fork_aware_txpool::dropped_watcher::MultiViewDroppedWatcherController,
	graph::{
		self,
		base_pool::{TimedTransactionSource, Transaction},
		BaseSubmitOutcome, BlockHash, ExtrinsicFor, ExtrinsicHash, TransactionFor,
		ValidatedPoolSubmitOutcome,
	},
	ReadyIteratorFor, ValidateTransactionPriority, LOG_TARGET,
};
use parking_lot::RwLock;
use sc_transaction_pool_api::{
	error::{Error as PoolError, IntoPoolError},
	PoolStatus, TransactionTag as Tag, TxInvalidityReportMap,
};
use sp_blockchain::{HashAndNumber, TreeRoute};
use sp_runtime::{
	generic::BlockId,
	traits::{Block as BlockT, Header, One, Saturating},
	transaction_validity::{InvalidTransaction, TransactionValidityError},
};
use std::{
	collections::{hash_map::Entry, HashMap, HashSet},
	sync::Arc,
	time::Instant,
};
use tracing::{debug, instrument, trace, warn, Level};

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
}

/// Helper type representing the callback allowing to trigger per-transaction events on
/// `ValidatedPool`'s listener.
type RemovalCallback<ChainApi> = Arc<
	dyn Fn(
			&mut crate::graph::EventDispatcher<ChainApi, ViewPoolObserver<ChainApi>>,
			ExtrinsicHash<ChainApi>,
		) + Send
		+ Sync,
>;

/// Helper struct to maintain the context for pending transaction removal, executed for
/// newly inserted views.
struct PendingTxRemoval<ChainApi>
where
	ChainApi: graph::ChainApi,
{
	/// Hash of the transaction that will be removed,
	xt_hash: ExtrinsicHash<ChainApi>,
	/// Action that shall be executed on underlying `ValidatedPool`'s listener.
	listener_action: RemovalCallback<ChainApi>,
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
	fn new_submission_action(xt: ExtrinsicFor<ChainApi>, source: TimedTransactionSource) -> Self {
		Self {
			processed: false,
			action: PreInsertAction::SubmitTx(PendingTxSubmission { xt, source }),
		}
	}

	/// Creates new unprocessed instance of pending transaction removal.
	fn new_removal_action(
		xt_hash: ExtrinsicHash<ChainApi>,
		listener: RemovalCallback<ChainApi>,
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
	/// Most recent view processed by tx-pool. Used in the API functions that were not changed to
	/// add `at` parameter.
	pub(super) most_recent_view: RwLock<Option<Arc<View<ChainApi>>>>,
	/// The controller of multi view dropped stream.
	pub(super) dropped_stream_controller: MultiViewDroppedWatcherController<ChainApi>,
	/// Util providing an aggregated stream of transactions that were imported to ready queue in
	/// any view. Reference kept here for clean up purposes.
	pub(super) import_notification_sink:
		MultiViewImportNotificationSink<Block::Hash, ExtrinsicHash<ChainApi>>,
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
		import_notification_sink: MultiViewImportNotificationSink<
			Block::Hash,
			ExtrinsicHash<ChainApi>,
		>,
	) -> Self {
		Self {
			api,
			active_views: Default::default(),
			inactive_views: Default::default(),
			listener,
			most_recent_view: RwLock::from(None),
			dropped_stream_controller,
			import_notification_sink,
			pending_txs_tasks: Default::default(),
		}
	}

	/// Imports a bunch of unverified extrinsics to every active view.
	///
	/// The results of submissions to individual views are merged into a single result for every
	/// extrinsic, preserving the input order. Refer to [`Self::merge_submission_results`] for
	/// details on how the individual views' results (including `AlreadyImported` errors
	/// interpreted against `preexisting_views`) are combined.
	pub(super) async fn submit(
		&self,
		xts: impl IntoIterator<Item = (TimedTransactionSource, ExtrinsicFor<ChainApi>)> + Clone,
		preexisting_views: &HashSet<Block::Hash>,
	) -> Vec<Result<ViewStoreSubmitOutcome<ChainApi>, ChainApi::Error>> {
		let tx_hashes = xts
			.clone()
			.into_iter()
			.map(|(_, xt)| self.api.hash_and_length(&xt).0)
			.collect::<Vec<_>>();
		let submit_futures = {
			let active_views = self.active_views.read();
			active_views
				.values()
				.map(|view| {
					let view = view.clone();
					let xts = xts.clone();
					async move {
						(
							view.at.hash,
							view.submit_many(xts, ValidateTransactionPriority::Submitted)
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

		let mut per_view_results = results
			.into_iter()
			.map(|(view_hash, results)| {
				debug_assert_eq!(results.len(), tx_hashes.len());
				(view_hash, results.into_iter())
			})
			.collect::<Vec<_>>();

		tx_hashes
			.into_iter()
			.map(|tx_hash| {
				let row = per_view_results.iter_mut().map(|(view_hash, results)| {
					(
						*view_hash,
						results
							.next()
							.expect("all views return one result per extrinsic; qed"),
					)
				});
				self.merge_submission_results(tx_hash, row, preexisting_views)
			})
			.collect()
	}

	/// Synchronously imports single unverified extrinsics into every active view.
	///
	/// `preexisting_views` shall contain the hashes of the views that were active before the
	/// transaction was inserted into the mempool (see [`Self::active_view_hashes`]). Refer to
	/// [`Self::merge_submission_results`] for details on how the individual views' results are
	/// combined into the final result.
	pub(super) fn submit_local(
		&self,
		xt: ExtrinsicFor<ChainApi>,
		preexisting_views: &HashSet<Block::Hash>,
	) -> Result<ViewStoreSubmitOutcome<ChainApi>, ChainApi::Error> {
		let active_views = self.active_views.read().values().cloned().collect::<Vec<_>>();

		let tx_hash = self.api.hash_and_length(&xt).0;

		let results = active_views
			.iter()
			.map(|view| (view.at.hash, view.submit_local(xt.clone()).map(Into::into)))
			.collect::<Vec<_>>();

		self.merge_submission_results(tx_hash, results, preexisting_views)
	}

	/// Import a single extrinsic into every active view and provide the submission outcome.
	///
	/// The external watcher is not created here — the caller is expected to create it (using
	/// `MultiViewListener::create_external_watcher_for_tx`) before inserting the transaction
	/// into the mempool.
	///
	/// `preexisting_views` shall contain the hashes of the views that were active before the
	/// transaction was inserted into the mempool (see [`Self::active_view_hashes`]). It is used
	/// to interpret `AlreadyImported` errors reported by individual views. Refer to
	/// [`Self::merge_submission_results`] for details on how the final result is computed.
	#[instrument(level = Level::TRACE, skip_all, target = "txpool", name = "view_store::sumbit_and_watch")]
	pub(super) async fn submit_and_watch(
		&self,
		_at: Block::Hash,
		source: TimedTransactionSource,
		xt: ExtrinsicFor<ChainApi>,
		preexisting_views: &HashSet<Block::Hash>,
	) -> Result<ViewStoreSubmitOutcome<ChainApi>, ChainApi::Error> {
		let tx_hash = self.api.hash_and_length(&xt).0;
		let submit_futures = {
			let active_views = self.active_views.read();
			active_views
				.values()
				.map(|view| {
					let view = view.clone();
					let xt = xt.clone();
					let source = source.clone();
					async move {
						(
							view.at.hash,
							view.submit_one(source, xt, ValidateTransactionPriority::Submitted)
								.await,
						)
					}
				})
				.collect::<Vec<_>>()
		};
		let results = futures::future::join_all(submit_futures).await;

		self.merge_submission_results(
			tx_hash,
			results.into_iter().map(|(view_hash, result)| (view_hash, result.map(Into::into))),
			preexisting_views,
		)
	}

	/// Merges the per-view submission results of a single extrinsic into the final result.
	///
	/// `preexisting_views` shall contain the hashes of the views that were active before the
	/// transaction was inserted into the mempool (see [`Self::active_view_hashes`]). It is used
	/// to interpret `AlreadyImported` errors reported by individual views:
	/// - an `AlreadyImported` error is ignored if reported by a view not contained in
	///   `preexisting_views`: such a view was created during the submission process and the
	///   transaction was already imported into it from the mempool (by
	///   `update_view_with_mempool`),
	/// - an `AlreadyImported` error reported by a view contained in `preexisting_views`
	///   indicates an inconsistency between the mempool and the views: the transaction was
	///   present in the view while it was absent from the mempool. A warning is logged whenever
	///   this is detected, regardless of the final result.
	///
	/// The final result is computed from the individual views' results as follows:
	/// - if any view accepted the transaction, the outcome of the first successful submission is
	///   returned,
	/// - if all views reported errors:
	///   - if `AlreadyImported` was reported by any view contained in `preexisting_views`, the
	///     `AlreadyImported` error is returned (this rule takes precedence over the following
	///     one),
	///   - if `AlreadyImported` was reported only by views not contained in
	///     `preexisting_views`, the errors are ignored and a successful outcome (with unknown
	///     priority) is returned,
	///   - otherwise, the first reported error is returned,
	/// - if there are no active views, a successful outcome (with unknown priority) is returned.
	fn merge_submission_results(
		&self,
		tx_hash: ExtrinsicHash<ChainApi>,
		results: impl IntoIterator<
			Item = (Block::Hash, Result<ViewStoreSubmitOutcome<ChainApi>, ChainApi::Error>),
		>,
		preexisting_views: &HashSet<Block::Hash>,
	) -> Result<ViewStoreSubmitOutcome<ChainApi>, ChainApi::Error> {
		let mut first_outcome = None;
		let mut already_imported_in_preexisting_view = false;
		let mut already_imported_in_new_view = false;
		let mut first_error = None;
		let mut no_views = true;

		for (view_hash, result) in results {
			no_views = false;
			match result {
				Ok(outcome) =>
					if first_outcome.is_none() {
						first_outcome = Some(outcome);
					},
				Err(error) => match error.into_pool_error() {
					Ok(PoolError::AlreadyImported(_)) =>
						if preexisting_views.contains(&view_hash) {
							warn!(
								target: LOG_TARGET,
								?tx_hash,
								?view_hash,
								"transaction already present in a pre-existing view while it \
								 was not in the mempool - views/mempool inconsistency"
							);
							already_imported_in_preexisting_view = true;
						} else {
							trace!(
								target: LOG_TARGET,
								?tx_hash,
								?view_hash,
								"transaction already imported into a view created during \
								 submission, ignoring"
							);
							already_imported_in_new_view = true;
						},
					Ok(pool_error) =>
						if first_error.is_none() {
							first_error = Some(pool_error.into());
						},
					Err(error) =>
						if first_error.is_none() {
							first_error = Some(error);
						},
				},
			}
		}

		if let Some(outcome) = first_outcome {
			return Ok(outcome)
		}
		if already_imported_in_preexisting_view {
			return Err(PoolError::AlreadyImported(Box::new(tx_hash)).into())
		}
		if already_imported_in_new_view || no_views {
			return Ok(ViewStoreSubmitOutcome::new(tx_hash, None))
		}
		Err(first_error.expect("all results are non-AlreadyImported errors; qed"))
	}

	/// Returns the pool status for every active view.
	pub(super) fn status(&self) -> HashMap<Block::Hash, PoolStatus> {
		self.active_views.read().iter().map(|(h, v)| (*h, v.status())).collect()
	}

	/// Returns true if there are no active views.
	pub(super) fn is_empty(&self) -> bool {
		self.active_views.read().is_empty() && self.inactive_views.read().is_empty()
	}

	/// Returns the hashes of all currently active views.
	///
	/// The returned set is a snapshot of the `active_views` collection taken at the time of the
	/// call, allowing callers to determine at a later point which views were already present and
	/// which were added afterwards (e.g. refer to [`Self::submit_and_watch`]).
	pub(super) fn active_view_hashes(&self) -> HashSet<Block::Hash> {
		self.active_views.read().keys().cloned().collect()
	}

	/// Searches in the view store for the first descendant view by iterating through the fork of
	/// the `at` block, up to the provided `block_number`.
	///
	/// Returns with a maybe pair of a view and a set of enacted blocks when the first view is
	/// found.
	pub(super) fn find_view_descendent_up_to_number(
		&self,
		at: &HashAndNumber<Block>,
		up_to: <<Block as BlockT>::Header as Header>::Number,
	) -> Option<(Arc<View<ChainApi>>, Vec<Block::Hash>)> {
		let mut enacted_blocks = Vec::new();
		let mut at_hash = at.hash;
		let mut at_number = at.number;

		// Search for a view that can be used to get and return an approximate ready
		// transaction set.
		while at_number >= up_to {
			// Found a view, stop searching.
			if let Some((view, _)) = self.get_view_at(at_hash, true) {
				return Some((view, enacted_blocks));
			}

			enacted_blocks.push(at_hash);

			// Move up into the fork.
			let header = self.api.block_header(at_hash).ok().flatten()?;
			at_hash = *header.parent_hash();
			at_number = at_number.saturating_sub(One::one());
		}

		None
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
		let ready_iterator =
			self.most_recent_view.read().as_ref().map(|v| v.pool.validated_pool().ready());

		if let Some(ready_iterator) = ready_iterator {
			return Box::new(ready_iterator);
		} else {
			return Box::new(std::iter::empty());
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
			.as_ref()
			.and_then(|view| self.futures_at(view.at.hash))
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
		debug!(
			target: LOG_TARGET,
			?finalized_hash,
			?tree_route,
			"finalize_route"
		);
		let mut finalized_transactions = Vec::new();

		for block in tree_route.iter().chain(std::iter::once(&finalized_hash)) {
			let extrinsics = self
				.api
				.block_body(*block)
				.await
				.unwrap_or_else(|error| {
					warn!(
						target: LOG_TARGET,
						%error,
						"Finalize route: error request"
					);
					None
				})
				.unwrap_or_default()
				.iter()
				.map(|e| self.api.hash_and_length(&e).0)
				.collect::<Vec<_>>();

			extrinsics
				.iter()
				.enumerate()
				.for_each(|(i, tx_hash)| self.listener.transaction_finalized(*tx_hash, *block, i));

			finalized_transactions.extend(extrinsics);
		}

		debug!(
			target: LOG_TARGET,
			"finalize_route: done"
		);
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
	/// Refer to [`Self::insert_new_view_sync`] more details.
	/// If there are any pending tx replacments, they are applied to the new view.
	#[instrument(level = Level::TRACE, skip_all, target = "txpool", name = "view_store::insert_new_view")]
	pub(super) async fn insert_new_view(
		&self,
		view: Arc<View<ChainApi>>,
		tree_route: &TreeRoute<Block>,
	) {
		self.apply_pending_tx_replacements(view.clone()).await;

		let start = Instant::now();
		self.insert_new_view_sync(view, tree_route);

		debug!(
			target: LOG_TARGET,
			inactive_views = ?self.inactive_views.read().keys(),
			duration = ?start.elapsed(),
			"insert_new_view"
		);
	}

	/// Inserts new view into the view store.
	///
	/// All the views associated with the blocks which are on enacted path (including common
	/// ancestor) will be:
	/// - moved to the inactive views set (`inactive_views`),
	/// - removed from the multi view listeners.
	///
	/// The `most_recent_view` is updated with the reference to the newly inserted view.
	pub(super) fn insert_new_view_sync(
		&self,
		view: Arc<View<ChainApi>>,
		tree_route: &TreeRoute<Block>,
	) {
		// note: most_recent_view must be synced with changes in in/active_views.
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
			most_recent_view_lock.replace(view.clone());
		};
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
				return Some((view.clone(), true));
			}
		};
		None
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
		// clean up older then finalized
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

			debug!(
				target: LOG_TARGET,
				inactive_views = ?inactive_views.keys(),
				?dropped_views,
				"handle_finalized"
			);
		}

		self.listener.remove_stale_controllers();
		self.dropped_stream_controller.remove_transactions(finalized_xts.clone());

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
				.values()
				.map(|view| {
					let view = view.clone();
					async move { view.finish_revalidation().await }
				})
				.collect::<Vec<_>>()
		};
		debug!(
			target: LOG_TARGET,
			duration = ?start.elapsed(),
			"finish_background_revalidations before"
		);
		futures::future::join_all(finish_revalidation_futures).await;
		debug!(
			target: LOG_TARGET,
			duration = ?start.elapsed(),
			"finish_background_revalidations after"
		);
	}

	/// Reports invalid transactions to the view store.
	///
	/// This function accepts an array of tuples, each containing a transaction hash and an
	/// optional error encountered during the transaction execution at a specific (also optional)
	/// block.
	///
	/// Removal operation applies to provided transactions. Their descendants can be removed from
	/// the view, but will not be invalidated or banned.
	///
	/// Invalid future and stale transaction will be removed only from given `at` view, and will be
	/// kept in the view_store. Such transaction will not be reported in returned vector. They
	/// also will not be banned from re-entering the pool. No event will be triggered.
	///
	/// For other errors, the transaction will be removed from the view_store, and it will be
	/// included in the returned vector. Additionally, transactions provided as input will be banned
	/// from re-entering the pool.
	///
	/// If the tuple's error is None, the transaction will be forcibly removed from the view_store,
	/// banned and included into the returned vector.
	///
	/// For every transaction removed from the view_store (excluding descendants) an Invalid event
	/// is triggered.
	///
	/// Returns the list of actually removed transactions from the mempool, which were included in
	/// the provided input list.
	pub(crate) fn report_invalid(
		&self,
		at: Option<Block::Hash>,
		invalid_tx_errors: TxInvalidityReportMap<ExtrinsicHash<ChainApi>>,
	) -> Vec<TransactionFor<ChainApi>> {
		let mut remove_from_view = vec![];
		let mut remove_from_pool = vec![];

		invalid_tx_errors.into_iter().for_each(|(hash, e)| match e {
			Some(TransactionValidityError::Invalid(
				InvalidTransaction::Future | InvalidTransaction::Stale,
			)) => {
				remove_from_view.push(hash);
			},
			_ => {
				remove_from_pool.push(hash);
			},
		});

		// transaction removed from view, won't be included into the final result, as they may still
		// be in the pool.
		at.map(|at| {
			self.get_view_at(at, true)
				.map(|(view, _)| view.remove_subtree(&remove_from_view, false, |_, _| {}))
		});

		let mut removed = vec![];
		for tx_hash in &remove_from_pool {
			let removed_from_pool = self.remove_transaction_subtree(*tx_hash, |_, _| {});
			removed_from_pool
				.iter()
				.find(|tx| tx.hash == *tx_hash)
				.map(|tx| removed.push(tx.clone()));
		}

		self.listener.transactions_invalidated(&remove_from_pool);

		removed
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
	) {
		if let Entry::Vacant(entry) = self.pending_txs_tasks.write().entry(replaced) {
			entry.insert(PendingPreInsertTask::new_submission_action(xt.clone(), source.clone()));
		} else {
			return;
		};

		let tx_hash = self.api.hash_and_length(&xt).0;
		trace!(
			target: LOG_TARGET,
			?replaced,
			?tx_hash,
			"replace_transaction"
		);
		self.replace_transaction_in_views(source, xt, tx_hash, replaced).await;

		if let Some(replacement) = self.pending_txs_tasks.write().get_mut(&replaced) {
			replacement.mark_processed();
		}
	}

	/// Applies pending transaction replacements to the specified view.
	///
	/// After application, all already processed replacements are removed.
	#[instrument(level = Level::TRACE, skip_all, target = "txpool", name = "view_store::apply_pending_tx_replacements")]
	async fn apply_pending_tx_replacements(&self, view: Arc<View<ChainApi>>) {
		let start = Instant::now();
		let mut futures = vec![];
		let mut replace_count = 0;
		let mut remove_count = 0;

		for replacement in self.pending_txs_tasks.read().values() {
			match replacement.action {
				PreInsertAction::SubmitTx(ref submission) => {
					replace_count += 1;
					let xt_hash = self.api.hash_and_length(&submission.xt).0;
					futures.push(self.replace_transaction_in_view(
						view.clone(),
						submission.source.clone(),
						submission.xt.clone(),
						xt_hash,
					));
				},
				PreInsertAction::RemoveSubtree(ref removal) => {
					remove_count += 1;
					view.remove_subtree(&[removal.xt_hash], true, &*removal.listener_action);
				},
			}
		}
		let _ = futures::future::join_all(futures).await;
		self.pending_txs_tasks.write().retain(|_, r| !r.processed);
		debug!(
			target: LOG_TARGET,
			at_hash = ?view.at.hash,
			replace_count,
			remove_count,
			duration = ?start.elapsed(),
			count = ?self.pending_txs_tasks.read().len(),
			"apply_pending_tx_replacements"
		);
	}

	/// Submits `xt` to the given view.
	///
	/// For watched transaction stream is added to the listener.
	async fn replace_transaction_in_view(
		&self,
		view: Arc<View<ChainApi>>,
		source: TimedTransactionSource,
		xt: ExtrinsicFor<ChainApi>,
		tx_hash: ExtrinsicHash<ChainApi>,
	) {
		if let Err(error) =
			view.submit_one(source, xt, ValidateTransactionPriority::Maintained).await
		{
			trace!(
				target: LOG_TARGET,
				?tx_hash,
				at_hash = ?view.at.hash,
				%error,
				"replace_transaction: submit failed"
			);
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
		tx_hash: ExtrinsicHash<ChainApi>,
		replaced: ExtrinsicHash<ChainApi>,
	) {
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
						tx_hash,
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
	) -> Vec<TransactionFor<ChainApi>>
	where
		F: Fn(
				&mut crate::graph::EventDispatcher<ChainApi, ViewPoolObserver<ChainApi>>,
				ExtrinsicHash<ChainApi>,
			) + Clone
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
			.flat_map(|(_, view)| view.remove_subtree(&[xt_hash], true, &listener_action))
			.filter_map(|xt| seen.insert(xt.hash).then(|| xt.clone()))
			.collect();

		if let Some(removal_action) = self.pending_txs_tasks.write().get_mut(&xt_hash) {
			removal_action.mark_processed();
		}

		removed
	}

	/// Clears stale views when blockchain finality stalls.
	///
	/// This function removes outdated active and inactive views based on the block height
	/// difference compared to the current block's height. Views are considered stale and
	/// purged from the `ViewStore` if their height difference from the current block `at`
	/// exceeds the specified `threshold`.
	///
	/// If any views are removed, corresponding cleanup operations are performed on multi-view
	/// stream controllers to ensure views are also removed there.
	pub(crate) fn finality_stall_view_cleanup(&self, at: &HashAndNumber<Block>, threshold: usize) {
		let mut dropped_views = vec![];
		{
			let mut active_views = self.active_views.write();
			let mut inactive_views = self.inactive_views.write();
			let mut f = |hash: &BlockHash<ChainApi>, v: &View<ChainApi>| -> bool {
				let diff = at.number.saturating_sub(v.at.number);
				if diff.into() > threshold.into() {
					dropped_views.push(*hash);
					false
				} else {
					true
				}
			};

			active_views.retain(|h, v| f(h, v));
			inactive_views.retain(|h, v| f(h, v));
		}

		if !dropped_views.is_empty() {
			for view in dropped_views {
				self.listener.remove_view(view);
				self.dropped_stream_controller.remove_view(view);
			}
		}
	}

	/// Returns provides tags of given transactions in the views associated to the given set of
	/// blocks.
	pub(crate) fn provides_tags_from_inactive_views(
		&self,
		block_hashes: Vec<&HashAndNumber<Block>>,
		mut xts_hashes: Vec<ExtrinsicHash<ChainApi>>,
	) -> HashMap<ExtrinsicHash<ChainApi>, Vec<Tag>> {
		let mut provides_tags_map = HashMap::new();

		block_hashes.into_iter().for_each(|hn| {
			// Get tx provides tags from given view's pool.
			if let Some((view, _)) = self.get_view_at(hn.hash, true) {
				let provides_tags = view.pool.validated_pool().extrinsics_tags(&xts_hashes);
				let xts_provides_tags = xts_hashes
					.iter()
					.zip(provides_tags.into_iter())
					.filter_map(|(hash, maybe_tags)| maybe_tags.map(|tags| (*hash, tags)))
					.collect::<HashMap<ExtrinsicHash<ChainApi>, Vec<Tag>>>();

				// Remove txs that have been resolved.
				xts_hashes.retain(|xth| !xts_provides_tags.contains_key(xth));

				// Collect the (extrinsic hash, tags) pairs in a map.
				provides_tags_map.extend(xts_provides_tags);
			}
		});

		provides_tags_map
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		common::tests::TestApi,
		fork_aware_txpool::{
			dropped_watcher::MultiViewDroppedWatcherController,
			import_notification_sink::MultiViewImportNotificationSink,
			multi_view_listener::MultiViewListener, view::View,
		},
		graph::base_pool::TimedTransactionSource,
		ValidateTransactionPriority,
	};
	use futures::StreamExt;
	use substrate_test_runtime::{AccountId, Block, ExtrinsicBuilder, Transfer, H256};
	use substrate_test_runtime_client::Sr25519Keyring::Alice;

	type TestViewStore = ViewStore<TestApi, Block>;

	fn create_test_view_store(
		api: Arc<TestApi>,
	) -> (TestViewStore, futures::future::BoxFuture<'static, ()>) {
		let (listener, listener_task) = MultiViewListener::new_with_worker(Default::default());
		let listener = Arc::new(listener);
		let (dropped_controller, _dropped_stream) = MultiViewDroppedWatcherController::new();
		let (import_sink, _import_task) = MultiViewImportNotificationSink::new_with_worker();
		let view_store = ViewStore::new(api, listener, dropped_controller, import_sink);
		(view_store, listener_task)
	}

	/// Creates a view store with a single active view at block 0 that already contains the
	/// returned transaction (mimicking an import done by `update_view_with_mempool`).
	async fn create_test_view_store_with_tx_in_view(
	) -> (TestViewStore, H256, ExtrinsicFor<TestApi>) {
		let api = Arc::new(TestApi::default());
		let (view_store, listener_task) = create_test_view_store(api.clone());

		// Spawn the listener task so the external watcher stream works.
		let _listener_handle = tokio::spawn(listener_task);

		// Create a view at block 0.
		let block_hash = H256::from_low_u64_be(0);
		let at = sp_blockchain::HashAndNumber { hash: block_hash, number: 0 };
		let (view, _dropped_stream, aggregated_stream) =
			View::new(api.clone(), at, Default::default(), Default::default(), true.into());
		let view = Arc::new(view);

		// Create a test transaction (nonce 0 is "ready" at block 0).
		let xt = ExtrinsicBuilder::new_transfer(Transfer {
			from: Alice.into(),
			to: AccountId::from_h256(H256::from_low_u64_be(2)),
			amount: 5,
			nonce: 0,
		})
		.build();
		let xt = Arc::from(xt);

		// Submit the tx to the view first — this is what update_view_with_mempool does.
		view.submit_one(
			TimedTransactionSource::new_external(false),
			Arc::clone(&xt),
			ValidateTransactionPriority::Maintained,
		)
		.await
		.expect("initial submit to view should succeed");

		// Connect the view's aggregated stream to the listener and insert into active_views.
		view_store
			.listener
			.add_view_aggregated_stream(block_hash, aggregated_stream.boxed());
		view_store.active_views.write().insert(block_hash, view);

		(view_store, block_hash, xt)
	}

	/// An `AlreadyImported` error from a view created during the submission process (not
	/// contained in `preexisting_views`) is the benign maintain race — it shall be ignored and
	/// the submission shall succeed.
	#[tokio::test]
	async fn submit_and_watch_ignores_already_imported_from_new_view() {
		sp_tracing::try_init_simple();
		let (view_store, block_hash, xt) = create_test_view_store_with_tx_in_view().await;

		// The view holding the tx is not in the pre-existing set, i.e. it was created during
		// the submission process and got the tx from the mempool.
		let preexisting_views = HashSet::default();

		let result = view_store
			.submit_and_watch(
				block_hash,
				TimedTransactionSource::new_external(false),
				xt,
				&preexisting_views,
			)
			.await;

		assert!(
			result.is_ok(),
			"AlreadyImported from a view created during submission shall be ignored, got: {:?}",
			result.err().map(|e| e.to_string())
		);
	}

	/// An `AlreadyImported` error from a view that existed before the submission started
	/// indicates a views/mempool inconsistency — the error shall be propagated.
	#[tokio::test]
	async fn submit_and_watch_rejects_already_imported_from_preexisting_view() {
		sp_tracing::try_init_simple();
		let (view_store, block_hash, xt) = create_test_view_store_with_tx_in_view().await;

		// The view holding the tx was active before the submission started.
		let preexisting_views = HashSet::from([block_hash]);

		let result = view_store
			.submit_and_watch(
				block_hash,
				TimedTransactionSource::new_external(false),
				xt,
				&preexisting_views,
			)
			.await;

		assert!(
			result.is_err(),
			"AlreadyImported from a pre-existing view indicates views/mempool inconsistency \
			 and shall be propagated"
		);
	}

	// Direct unit tests of the per-transaction merging rules (successor of the coverage
	// provided by the former `reduce_multiview_result` tests).

	type MergeResults = Vec<(H256, Result<ViewStoreSubmitOutcome<TestApi>, PoolError>)>;

	fn merge_test_view_store() -> TestViewStore {
		let api = Arc::new(TestApi::default());
		let (view_store, _listener_task) = create_test_view_store(api);
		view_store
	}

	fn h(n: u64) -> H256 {
		H256::from_low_u64_be(n)
	}

	/// If any view accepted the transaction, the outcome of the first successful submission is
	/// returned.
	#[test]
	fn merge_submission_results_first_success_wins() {
		let view_store = merge_test_view_store();
		let tx_hash = h(1);
		let results: MergeResults = vec![
			(h(0x13), Err(PoolError::ImmediatelyDropped)),
			(h(0x14), Ok(ViewStoreSubmitOutcome::new(tx_hash, Some(10)))),
			(h(0x15), Ok(ViewStoreSubmitOutcome::new(tx_hash, Some(20)))),
		];
		let result = view_store
			.merge_submission_results(tx_hash, results, &HashSet::default())
			.expect("first success wins");
		assert_eq!(result.hash(), tx_hash);
		assert_eq!(result.priority(), Some(10));
	}

	/// If all views reported errors and none of them is `AlreadyImported`, the first reported
	/// error is returned.
	#[test]
	fn merge_submission_results_returns_first_error() {
		let view_store = merge_test_view_store();
		let tx_hash = h(1);
		let results: MergeResults = vec![
			(h(0x13), Err(PoolError::ImmediatelyDropped)),
			(h(0x14), Err(PoolError::TemporarilyBanned)),
		];
		let error = view_store
			.merge_submission_results(tx_hash, results, &HashSet::default())
			.err()
			.expect("all views failed");
		assert!(matches!(error.into_pool_error(), Ok(PoolError::ImmediatelyDropped)));
	}

	/// With no active views the submission succeeds with unknown priority.
	#[test]
	fn merge_submission_results_no_views_is_success() {
		let view_store = merge_test_view_store();
		let tx_hash = h(1);
		let result = view_store
			.merge_submission_results(tx_hash, MergeResults::default(), &HashSet::default())
			.expect("no views is success");
		assert_eq!(result.hash(), tx_hash);
		assert_eq!(result.priority(), None);
	}

	/// `AlreadyImported` reported only by views created during the submission process is
	/// ignored, even if other views reported unrelated errors.
	#[test]
	fn merge_submission_results_ignores_already_imported_from_new_views() {
		let view_store = merge_test_view_store();
		let tx_hash = h(1);
		let results: MergeResults = vec![
			(h(0x13), Err(PoolError::AlreadyImported(Box::new(tx_hash)))),
			(h(0x14), Err(PoolError::ImmediatelyDropped)),
		];
		let result = view_store
			.merge_submission_results(tx_hash, results, &HashSet::default())
			.expect("AlreadyImported from a view created during submission is benign");
		assert_eq!(result.hash(), tx_hash);
		assert_eq!(result.priority(), None);
	}

	/// `AlreadyImported` reported by a pre-existing view takes precedence over the benign
	/// `AlreadyImported` reported by a view created during the submission process.
	#[test]
	fn merge_submission_results_preexisting_already_imported_takes_precedence() {
		let view_store = merge_test_view_store();
		let tx_hash = h(1);
		let results: MergeResults = vec![
			(h(0x13), Err(PoolError::AlreadyImported(Box::new(tx_hash)))),
			(h(0x14), Err(PoolError::AlreadyImported(Box::new(tx_hash)))),
		];
		let error = view_store
			.merge_submission_results(tx_hash, results, &HashSet::from([h(0x13)]))
			.err()
			.expect("AlreadyImported from a pre-existing view shall be propagated");
		assert!(matches!(error.into_pool_error(), Ok(PoolError::AlreadyImported(_))));
	}

	/// A successful submission to any view wins over the views/mempool inconsistency detected
	/// in another view (which is still logged).
	#[test]
	fn merge_submission_results_success_wins_over_preexisting_already_imported() {
		let view_store = merge_test_view_store();
		let tx_hash = h(1);
		let results: MergeResults = vec![
			(h(0x13), Err(PoolError::AlreadyImported(Box::new(tx_hash)))),
			(h(0x14), Ok(ViewStoreSubmitOutcome::new(tx_hash, Some(7)))),
		];
		let result = view_store
			.merge_submission_results(tx_hash, results, &HashSet::from([h(0x13)]))
			.expect("success wins");
		assert_eq!(result.priority(), Some(7));
	}
}
