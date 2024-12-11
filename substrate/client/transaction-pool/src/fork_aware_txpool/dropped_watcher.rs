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

//! Multi-view pool dropped events listener provides means to combine streams from multiple pool
//! views into a single event stream. It allows management of dropped transaction events, adding new
//! views, and removing views as needed, ensuring that transactions which are no longer referenced
//! by any view are detected and properly notified.

use crate::{
	common::log_xt::log_xt_trace,
	fork_aware_txpool::stream_map_util::next_event,
	graph::{self, BlockHash, ExtrinsicHash},
	LOG_TARGET,
};
use futures::stream::StreamExt;
use log::{debug, trace};
use sc_transaction_pool_api::TransactionStatus;
use sc_utils::mpsc;
use sp_runtime::traits::Block as BlockT;
use std::{
	collections::{
		hash_map::{Entry, OccupiedEntry},
		HashMap, HashSet,
	},
	fmt::{self, Debug, Formatter},
	pin::Pin,
};
use tokio_stream::StreamMap;

/// Represents a transaction that was removed from the transaction pool, including the reason of its
/// removal.
#[derive(Debug, PartialEq)]
pub struct DroppedTransaction<Hash> {
	/// Hash of the dropped extrinsic.
	pub tx_hash: Hash,
	/// Reason of the transaction being dropped.
	pub reason: DroppedReason<Hash>,
}

impl<Hash> DroppedTransaction<Hash> {
	fn new_usurped(tx_hash: Hash, by: Hash) -> Self {
		Self { reason: DroppedReason::Usurped(by), tx_hash }
	}

	fn new_enforced_by_limts(tx_hash: Hash) -> Self {
		Self { reason: DroppedReason::LimitsEnforced, tx_hash }
	}
}

/// Provides reason of why transactions was dropped.
#[derive(Debug, PartialEq)]
pub enum DroppedReason<Hash> {
	/// Transaction was replaced by other transaction (e.g. because of higher priority).
	Usurped(Hash),
	/// Transaction was dropped because of internal pool limits being enforced.
	LimitsEnforced,
}

/// Dropped-logic related event from the single view.
pub type ViewStreamEvent<C> = crate::graph::DroppedByLimitsEvent<ExtrinsicHash<C>, BlockHash<C>>;

/// Dropped-logic stream of events coming from the single view.
type ViewStream<C> = Pin<Box<dyn futures::Stream<Item = ViewStreamEvent<C>> + Send>>;

/// Stream of extrinsic hashes that were dropped by the views and have no references by existing
/// views.
pub(crate) type StreamOfDropped<C> =
	Pin<Box<dyn futures::Stream<Item = DroppedTransaction<ExtrinsicHash<C>>> + Send>>;

/// A type alias for a sender used as the controller of the [`MultiViewDropWatcherContext`].
/// Used to send control commands from the [`MultiViewDroppedWatcherController`] to
/// [`MultiViewDropWatcherContext`].
type Controller<T> = mpsc::TracingUnboundedSender<T>;

/// A type alias for a receiver used as the commands receiver in the
/// [`MultiViewDropWatcherContext`].
type CommandReceiver<T> = mpsc::TracingUnboundedReceiver<T>;

/// Commands to control the instance of dropped transactions stream [`StreamOfDropped`].
enum Command<ChainApi>
where
	ChainApi: graph::ChainApi,
{
	/// Adds a new stream of dropped-related events originating in a view with a specific block
	/// hash
	AddView(BlockHash<ChainApi>, ViewStream<ChainApi>),
	/// Removes an existing view's stream associated with a specific block hash.
	RemoveView(BlockHash<ChainApi>),
	/// Removes referencing views for given extrinsic hashes.
	///
	/// Intended to ba called on finalization.
	RemoveFinalizedTxs(Vec<ExtrinsicHash<ChainApi>>),
}

impl<ChainApi> Debug for Command<ChainApi>
where
	ChainApi: graph::ChainApi,
{
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		match self {
			Command::AddView(..) => write!(f, "AddView"),
			Command::RemoveView(..) => write!(f, "RemoveView"),
			Command::RemoveFinalizedTxs(..) => write!(f, "RemoveFinalizedTxs"),
		}
	}
}

/// Manages the state and logic for handling events related to dropped transactions across multiple
/// views.
///
/// This struct maintains a mapping of active views and their corresponding streams, as well as the
/// state of each transaction with respect to these views.
struct MultiViewDropWatcherContext<ChainApi>
where
	ChainApi: graph::ChainApi,
{
	/// A map that associates the views identified by corresponding block hashes with their streams
	/// of dropped-related events. This map is used to keep track of active views and their event
	/// streams.
	stream_map: StreamMap<BlockHash<ChainApi>, ViewStream<ChainApi>>,
	/// A receiver for commands to control the state of the stream, allowing the addition and
	/// removal of views. This is used to dynamically update which views are being tracked.
	command_receiver: CommandReceiver<Command<ChainApi>>,
	/// For each transaction hash we keep the set of hashes representing the views that see this
	/// transaction as ready or in_block.
	///
	/// Even if all views referencing a ready transactions are removed, we still want to keep
	/// transaction, there can be a fork which sees the transaction as ready.
	///
	/// Once transaction is dropped, dropping view is removed from the set.
	ready_transaction_views: HashMap<ExtrinsicHash<ChainApi>, HashSet<BlockHash<ChainApi>>>,
	/// For each transaction hash we keep the set of hashes representing the views that see this
	/// transaction as future.
	///
	/// Once all views referencing a future transactions are removed, the future can be dropped.
	///
	/// Once transaction is dropped, dropping view is removed from the set.
	future_transaction_views: HashMap<ExtrinsicHash<ChainApi>, HashSet<BlockHash<ChainApi>>>,

	/// Transactions that need to be notified as dropped.
	pending_dropped_transactions: Vec<ExtrinsicHash<ChainApi>>,
}

impl<C> MultiViewDropWatcherContext<C>
where
	C: graph::ChainApi + 'static,
	<<C as graph::ChainApi>::Block as BlockT>::Hash: Unpin,
{
	/// Provides the ready or future `HashSet` containing views referencing given transaction.
	fn transaction_views(
		&mut self,
		tx_hash: ExtrinsicHash<C>,
	) -> Option<OccupiedEntry<ExtrinsicHash<C>, HashSet<BlockHash<C>>>> {
		if let Entry::Occupied(views_keeping_tx_valid) = self.ready_transaction_views.entry(tx_hash)
		{
			return Some(views_keeping_tx_valid)
		}
		if let Entry::Occupied(views_keeping_tx_valid) =
			self.future_transaction_views.entry(tx_hash)
		{
			return Some(views_keeping_tx_valid)
		}
		None
	}

	/// Processes the command and updates internal state accordingly.
	fn handle_command(&mut self, cmd: Command<C>) {
		match cmd {
			Command::AddView(key, stream) => {
				trace!(
					target: LOG_TARGET,
					"dropped_watcher: Command::AddView {key:?} views:{:?}",
					self.stream_map.keys().collect::<Vec<_>>()
				);
				self.stream_map.insert(key, stream);
			},
			Command::RemoveView(key) => {
				trace!(
					target: LOG_TARGET,
					"dropped_watcher: Command::RemoveView {key:?} views:{:?}",
					self.stream_map.keys().collect::<Vec<_>>()
				);
				self.stream_map.remove(&key);
				self.ready_transaction_views.iter_mut().for_each(|(tx_hash, views)| {
					trace!(
						target: LOG_TARGET,
						"[{:?}] dropped_watcher: Command::RemoveView ready views: {:?}",
						tx_hash,
						views
					);
					views.remove(&key);
				});

				self.future_transaction_views.iter_mut().for_each(|(tx_hash, views)| {
					trace!(
						target: LOG_TARGET,
						"[{:?}] dropped_watcher: Command::RemoveView future views: {:?}",
						tx_hash,
						views
					);
					views.remove(&key);
					if views.is_empty() {
						self.pending_dropped_transactions.push(*tx_hash);
					}
				});
			},
			Command::RemoveFinalizedTxs(xts) => {
				log_xt_trace!(
					target: LOG_TARGET,
					xts.clone(),
					"[{:?}] dropped_watcher: finalized xt removed"
				);
				xts.iter().for_each(|xt| {
					self.ready_transaction_views.remove(xt);
					self.future_transaction_views.remove(xt);
				});
			},
		}
	}

	/// Processes a `ViewStreamEvent` from a specific view and updates the internal state
	/// accordingly.
	///
	/// If the event indicates that a transaction has been dropped and is no longer referenced by
	/// any active views, the transaction hash is returned. Otherwise `None` is returned.
	fn handle_event(
		&mut self,
		block_hash: BlockHash<C>,
		event: ViewStreamEvent<C>,
	) -> Option<DroppedTransaction<ExtrinsicHash<C>>> {
		trace!(
			target: LOG_TARGET,
			"dropped_watcher: handle_event: event:{event:?} from:{block_hash:?} future_views:{:?} ready_views:{:?} stream_map views:{:?}, ",
			self.future_transaction_views.get(&event.0),
			self.ready_transaction_views.get(&event.0),
			self.stream_map.keys().collect::<Vec<_>>(),
		);
		let (tx_hash, status) = event;
		match status {
			TransactionStatus::Future => {
				self.future_transaction_views.entry(tx_hash).or_default().insert(block_hash);
			},
			TransactionStatus::Ready | TransactionStatus::InBlock(..) => {
				// note: if future transaction was once seens as the ready we may want to treat it
				// as ready transactions. Unreferenced future transactions are more likely to be
				// removed when the last referencing view is removed then ready transactions.
				// Transcaction seen as ready is likely quite close to be included in some
				// future fork.
				if let Some(mut views) = self.future_transaction_views.remove(&tx_hash) {
					views.insert(block_hash);
					self.ready_transaction_views.insert(tx_hash, views);
				} else {
					self.ready_transaction_views.entry(tx_hash).or_default().insert(block_hash);
				}
			},
			TransactionStatus::Dropped => {
				if let Some(mut views_keeping_tx_valid) = self.transaction_views(tx_hash) {
					views_keeping_tx_valid.get_mut().remove(&block_hash);
					if views_keeping_tx_valid.get().is_empty() {
						return Some(DroppedTransaction::new_enforced_by_limts(tx_hash))
					}
				} else {
					debug!("[{:?}] dropped_watcher: removing (non-tracked) tx", tx_hash);
					return Some(DroppedTransaction::new_enforced_by_limts(tx_hash))
				}
			},
			TransactionStatus::Usurped(by) =>
				return Some(DroppedTransaction::new_usurped(tx_hash, by)),
			_ => {},
		};
		None
	}

	/// Gets pending dropped transactions if any.
	fn get_pending_dropped_transaction(&mut self) -> Option<DroppedTransaction<ExtrinsicHash<C>>> {
		while let Some(tx_hash) = self.pending_dropped_transactions.pop() {
			// never drop transaction that was seen as ready. It may not have a referencing
			// view now, but such fork can appear.
			if self.ready_transaction_views.get(&tx_hash).is_some() {
				continue
			}

			if let Some(views) = self.future_transaction_views.get(&tx_hash) {
				if views.is_empty() {
					self.future_transaction_views.remove(&tx_hash);
					return Some(DroppedTransaction::new_enforced_by_limts(tx_hash))
				}
			}
		}
		None
	}

	/// Creates a new `StreamOfDropped` and its associated event stream controller.
	///
	/// This method initializes the internal structures and unfolds the stream of dropped
	/// transactions. Returns a tuple containing this stream and the controller for managing
	/// this stream.
	fn event_stream() -> (StreamOfDropped<C>, Controller<Command<C>>) {
		//note: 64 allows to avoid warning messages during execution of unit tests.
		const CHANNEL_SIZE: usize = 64;
		let (sender, command_receiver) = sc_utils::mpsc::tracing_unbounded::<Command<C>>(
			"tx-pool-dropped-watcher-cmd-stream",
			CHANNEL_SIZE,
		);

		let ctx = Self {
			stream_map: StreamMap::new(),
			command_receiver,
			ready_transaction_views: Default::default(),
			future_transaction_views: Default::default(),
			pending_dropped_transactions: Default::default(),
		};

		let stream_map = futures::stream::unfold(ctx, |mut ctx| async move {
			loop {
				if let Some(dropped) = ctx.get_pending_dropped_transaction() {
					debug!("dropped_watcher: sending out (pending): {dropped:?}");
					return Some((dropped, ctx));
				}
				tokio::select! {
					biased;
					Some(event) = next_event(&mut ctx.stream_map) => {
						if let Some(dropped) = ctx.handle_event(event.0, event.1) {
							debug!("dropped_watcher: sending out: {dropped:?}");
							return Some((dropped, ctx));
						}
					},
					cmd = ctx.command_receiver.next() => {
						ctx.handle_command(cmd?);
					}

				}
			}
		})
		.boxed();

		(stream_map, sender)
	}
}

/// The controller for manipulating the state of the [`StreamOfDropped`].
///
/// This struct provides methods to add and remove streams associated with views to and from the
/// stream.
pub struct MultiViewDroppedWatcherController<ChainApi: graph::ChainApi> {
	/// A controller allowing to update the state of the associated [`StreamOfDropped`].
	controller: Controller<Command<ChainApi>>,
}

impl<ChainApi: graph::ChainApi> Clone for MultiViewDroppedWatcherController<ChainApi> {
	fn clone(&self) -> Self {
		Self { controller: self.controller.clone() }
	}
}

impl<ChainApi> MultiViewDroppedWatcherController<ChainApi>
where
	ChainApi: graph::ChainApi + 'static,
	<<ChainApi as graph::ChainApi>::Block as BlockT>::Hash: Unpin,
{
	/// Creates new [`StreamOfDropped`] and its controller.
	pub fn new() -> (MultiViewDroppedWatcherController<ChainApi>, StreamOfDropped<ChainApi>) {
		let (stream_map, ctrl) = MultiViewDropWatcherContext::<ChainApi>::event_stream();
		(Self { controller: ctrl }, stream_map.boxed())
	}

	/// Notifies the [`StreamOfDropped`] that new view was created.
	pub fn add_view(&self, key: BlockHash<ChainApi>, view: ViewStream<ChainApi>) {
		let _ = self.controller.unbounded_send(Command::AddView(key, view)).map_err(|e| {
			trace!(target: LOG_TARGET, "dropped_watcher: add_view {key:?} send message failed: {e}");
		});
	}

	/// Notifies the [`StreamOfDropped`] that the view was destroyed and shall be removed the
	/// stream map.
	pub fn remove_view(&self, key: BlockHash<ChainApi>) {
		let _ = self.controller.unbounded_send(Command::RemoveView(key)).map_err(|e| {
			trace!(target: LOG_TARGET, "dropped_watcher: remove_view {key:?} send message failed: {e}");
		});
	}

	/// Removes status info for finalized transactions.
	pub fn remove_finalized_txs(
		&self,
		xts: impl IntoIterator<Item = ExtrinsicHash<ChainApi>> + Clone,
	) {
		let _ = self
			.controller
			.unbounded_send(Command::RemoveFinalizedTxs(xts.into_iter().collect()))
			.map_err(|e| {
				trace!(target: LOG_TARGET, "dropped_watcher: remove_finalized_txs send message failed: {e}");
			});
	}
}

#[cfg(test)]
mod dropped_watcher_tests {
	use super::*;
	use crate::common::tests::TestApi;
	use futures::{stream::pending, FutureExt, StreamExt};
	use sp_core::H256;

	type MultiViewDroppedWatcher = super::MultiViewDroppedWatcherController<TestApi>;

	#[tokio::test]
	async fn test01() {
		sp_tracing::try_init_simple();
		let (watcher, output_stream) = MultiViewDroppedWatcher::new();

		let block_hash = H256::repeat_byte(0x01);
		let tx_hash = H256::repeat_byte(0x0a);

		let view_stream = futures::stream::iter(vec![
			(tx_hash, TransactionStatus::Ready),
			(tx_hash, TransactionStatus::Dropped),
		])
		.boxed();

		watcher.add_view(block_hash, view_stream);
		let handle = tokio::spawn(async move { output_stream.take(1).collect::<Vec<_>>().await });
		assert_eq!(handle.await.unwrap(), vec![DroppedTransaction::new_enforced_by_limts(tx_hash)]);
	}

	#[tokio::test]
	async fn test02() {
		sp_tracing::try_init_simple();
		let (watcher, mut output_stream) = MultiViewDroppedWatcher::new();

		let block_hash0 = H256::repeat_byte(0x01);
		let block_hash1 = H256::repeat_byte(0x02);
		let tx_hash = H256::repeat_byte(0x0a);

		let view_stream0 = futures::stream::iter(vec![(tx_hash, TransactionStatus::Future)])
			.chain(pending())
			.boxed();
		let view_stream1 = futures::stream::iter(vec![
			(tx_hash, TransactionStatus::Ready),
			(tx_hash, TransactionStatus::Dropped),
		])
		.boxed();

		watcher.add_view(block_hash0, view_stream0);

		assert!(output_stream.next().now_or_never().is_none());
		watcher.add_view(block_hash1, view_stream1);
		assert!(output_stream.next().now_or_never().is_none());
	}

	#[tokio::test]
	async fn test03() {
		sp_tracing::try_init_simple();
		let (watcher, output_stream) = MultiViewDroppedWatcher::new();

		let block_hash0 = H256::repeat_byte(0x01);
		let block_hash1 = H256::repeat_byte(0x02);
		let tx_hash0 = H256::repeat_byte(0x0a);
		let tx_hash1 = H256::repeat_byte(0x0b);

		let view_stream0 = futures::stream::iter(vec![(tx_hash0, TransactionStatus::Future)])
			.chain(pending())
			.boxed();
		let view_stream1 = futures::stream::iter(vec![
			(tx_hash1, TransactionStatus::Ready),
			(tx_hash1, TransactionStatus::Dropped),
		])
		.boxed();

		watcher.add_view(block_hash0, view_stream0);
		watcher.add_view(block_hash1, view_stream1);
		let handle = tokio::spawn(async move { output_stream.take(1).collect::<Vec<_>>().await });
		assert_eq!(
			handle.await.unwrap(),
			vec![DroppedTransaction::new_enforced_by_limts(tx_hash1)]
		);
	}

	#[tokio::test]
	async fn test04() {
		sp_tracing::try_init_simple();
		let (watcher, mut output_stream) = MultiViewDroppedWatcher::new();

		let block_hash0 = H256::repeat_byte(0x01);
		let block_hash1 = H256::repeat_byte(0x02);
		let tx_hash = H256::repeat_byte(0x0b);

		let view_stream0 = futures::stream::iter(vec![
			(tx_hash, TransactionStatus::Future),
			(tx_hash, TransactionStatus::InBlock((block_hash1, 0))),
		])
		.boxed();
		let view_stream1 = futures::stream::iter(vec![
			(tx_hash, TransactionStatus::Ready),
			(tx_hash, TransactionStatus::Dropped),
		])
		.boxed();

		watcher.add_view(block_hash0, view_stream0);
		assert!(output_stream.next().now_or_never().is_none());
		watcher.remove_view(block_hash0);

		watcher.add_view(block_hash1, view_stream1);
		let handle = tokio::spawn(async move { output_stream.take(1).collect::<Vec<_>>().await });
		assert_eq!(handle.await.unwrap(), vec![DroppedTransaction::new_enforced_by_limts(tx_hash)]);
	}

	#[tokio::test]
	async fn test05() {
		sp_tracing::try_init_simple();
		let (watcher, mut output_stream) = MultiViewDroppedWatcher::new();
		assert!(output_stream.next().now_or_never().is_none());

		let block_hash0 = H256::repeat_byte(0x01);
		let block_hash1 = H256::repeat_byte(0x02);
		let tx_hash = H256::repeat_byte(0x0b);

		let view_stream0 = futures::stream::iter(vec![
			(tx_hash, TransactionStatus::Future),
			(tx_hash, TransactionStatus::InBlock((block_hash1, 0))),
		])
		.boxed();
		watcher.add_view(block_hash0, view_stream0);
		assert!(output_stream.next().now_or_never().is_none());

		let view_stream1 = futures::stream::iter(vec![
			(tx_hash, TransactionStatus::Ready),
			(tx_hash, TransactionStatus::InBlock((block_hash0, 0))),
		])
		.boxed();

		watcher.add_view(block_hash1, view_stream1);
		assert!(output_stream.next().now_or_never().is_none());
		assert!(output_stream.next().now_or_never().is_none());
		assert!(output_stream.next().now_or_never().is_none());
		assert!(output_stream.next().now_or_never().is_none());
		assert!(output_stream.next().now_or_never().is_none());

		let tx_hash = H256::repeat_byte(0x0c);
		let view_stream2 = futures::stream::iter(vec![
			(tx_hash, TransactionStatus::Future),
			(tx_hash, TransactionStatus::Dropped),
		])
		.boxed();
		let block_hash2 = H256::repeat_byte(0x03);
		watcher.add_view(block_hash2, view_stream2);
		let handle = tokio::spawn(async move { output_stream.take(1).collect::<Vec<_>>().await });
		assert_eq!(handle.await.unwrap(), vec![DroppedTransaction::new_enforced_by_limts(tx_hash)]);
	}
}
