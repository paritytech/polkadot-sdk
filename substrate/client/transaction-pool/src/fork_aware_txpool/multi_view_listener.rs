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

//! `MultiViewListener` and `ExternalWatcherContext` manage view streams and status updates for
//! transactions, providing control commands to manage transaction states, and create external
//! aggregated streams of transaction events.

use crate::{
	fork_aware_txpool::stream_map_util::next_event,
	graph::{self, BlockHash, ExtrinsicHash},
	LOG_TARGET,
};
use futures::StreamExt;
use log::{debug, trace};
use sc_transaction_pool_api::{TransactionStatus, TransactionStatusStream, TxIndex};
use sc_utils::mpsc;
use sp_runtime::traits::Block as BlockT;
use std::{
	collections::{hash_map::Entry, HashMap, HashSet},
	pin::Pin,
};
use tokio_stream::StreamMap;

/// A side channel allowing to control the external stream instance (one per transaction) with
/// [`ControllerCommand`].
///
/// Set of instances of [`Controller`] lives within the [`MultiViewListener`].
type Controller<T> = mpsc::TracingUnboundedSender<T>;

/// A receiver of [`ControllerCommand`] instances allowing to control the external stream.
///
/// Lives within the [`ExternalWatcherContext`] instance.
type CommandReceiver<T> = mpsc::TracingUnboundedReceiver<T>;

/// The stream of the transaction events.
///
/// It can represent both a single view's stream and an external watcher stream.
pub type TxStatusStream<T> = Pin<Box<TransactionStatusStream<ExtrinsicHash<T>, BlockHash<T>>>>;

/// Commands to control the single external stream living within the multi view listener.
enum ControllerCommand<ChainApi: graph::ChainApi> {
	/// Adds a new stream of transaction statuses originating in the view associated with a
	/// specific block hash.
	AddViewStream(BlockHash<ChainApi>, TxStatusStream<ChainApi>),

	/// Removes an existing view's stream associated with a specific block hash.
	RemoveViewStream(BlockHash<ChainApi>),

	/// Marks a transaction as invalidated.
	///
	/// If all pre-conditions are met, an external invalid event will be sent out.
	TransactionInvalidated,

	/// Notifies that a transaction was finalized in a specific block hash and transaction index.
	///
	/// Send out an external finalized event.
	FinalizeTransaction(BlockHash<ChainApi>, TxIndex),

	/// Notifies that a transaction was broadcasted with a list of peer addresses.
	///
	/// Sends out an external broadcasted event.
	TransactionBroadcasted(Vec<String>),

	/// Notifies that a transaction was dropped from the pool.
	///
	/// If all preconditions are met, an external dropped event will be sent out.
	TransactionDropped,
}

impl<ChainApi> std::fmt::Debug for ControllerCommand<ChainApi>
where
	ChainApi: graph::ChainApi,
{
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			ControllerCommand::AddViewStream(h, _) => write!(f, "ListenerAction::AddView({h})"),
			ControllerCommand::RemoveViewStream(h) => write!(f, "ListenerAction::RemoveView({h})"),
			ControllerCommand::TransactionInvalidated => {
				write!(f, "ListenerAction::TransactionInvalidated")
			},
			ControllerCommand::FinalizeTransaction(h, i) => {
				write!(f, "ListenerAction::FinalizeTransaction({h},{i})")
			},
			ControllerCommand::TransactionBroadcasted(_) => {
				write!(f, "ListenerAction::TransactionBroadcasted(...)")
			},
			ControllerCommand::TransactionDropped => {
				write!(f, "ListenerAction::TransactionDropped")
			},
		}
	}
}

/// This struct allows to create and control listener for multiple transactions.
///
/// For every transaction the view's stream generating its own events can be added. The events are
/// flattened and sent out to the external listener. (The *external*  term here means that it can be
/// exposed to [`sc_transaction_pool_api::TransactionPool`] API client e.g. over RPC.)
///
/// The listener allows to add and remove view's stream (per transaction).
///
/// The listener provides a side channel that allows triggering specific events (finalized, dropped,
/// invalid) independently of the view's stream.
pub struct MultiViewListener<ChainApi: graph::ChainApi> {
	/// Provides the set of controllers for the events streams corresponding to individual
	/// transactions identified by transaction hashes.
	controllers: parking_lot::RwLock<
		HashMap<ExtrinsicHash<ChainApi>, Controller<ControllerCommand<ChainApi>>>,
	>,
}

/// The external stream unfolding context.
///
/// This context is used to unfold the external events stream for a single transaction, it
/// facilitates the logic of converting single view's events to the external events stream.
struct ExternalWatcherContext<ChainApi: graph::ChainApi> {
	/// The hash of the transaction being monitored within this context.
	tx_hash: ExtrinsicHash<ChainApi>,
	/// A stream map of transaction status streams coming from individual views, keyed by
	/// block hash associated with view.
	status_stream_map: StreamMap<BlockHash<ChainApi>, TxStatusStream<ChainApi>>,
	/// A receiver for controller commands.
	command_receiver: CommandReceiver<ControllerCommand<ChainApi>>,
	/// A flag indicating whether the context should terminate.
	terminate: bool,
	/// A flag indicating if a `Future` status has been encountered.
	future_seen: bool,
	/// A flag indicating if a `Ready` status has been encountered.
	ready_seen: bool,

	/// A hash set of block hashes from views that consider the transaction valid.
	views_keeping_tx_valid: HashSet<BlockHash<ChainApi>>,
}

impl<ChainApi: graph::ChainApi> ExternalWatcherContext<ChainApi>
where
	<<ChainApi as graph::ChainApi>::Block as BlockT>::Hash: Unpin,
{
	/// Creates new `ExternalWatcherContext` for particular transaction identified by `tx_hash`
	///
	/// The `command_receiver` is a side channel for receiving controller's commands.
	fn new(
		tx_hash: ExtrinsicHash<ChainApi>,
		command_receiver: CommandReceiver<ControllerCommand<ChainApi>>,
	) -> Self {
		Self {
			tx_hash,
			status_stream_map: StreamMap::new(),
			command_receiver,
			terminate: false,
			future_seen: false,
			ready_seen: false,
			views_keeping_tx_valid: Default::default(),
		}
	}

	/// Handles various transaction status updates and manages internal states based on the status.
	///
	/// Function may set the context termination flag, which will close the stream.
	///
	/// Returns `Some` with the `event` to forward or `None`.
	fn handle(
		&mut self,
		status: TransactionStatus<ExtrinsicHash<ChainApi>, BlockHash<ChainApi>>,
		hash: BlockHash<ChainApi>,
	) -> Option<TransactionStatus<ExtrinsicHash<ChainApi>, BlockHash<ChainApi>>> {
		trace!(
			target: LOG_TARGET, "[{:?}] mvl handle event from {hash:?}: {status:?} views:{:?}", self.tx_hash,
			self.status_stream_map.keys().collect::<Vec<_>>()
		);
		match status {
			TransactionStatus::Future => {
				self.views_keeping_tx_valid.insert(hash);
				if self.ready_seen || self.future_seen {
					None
				} else {
					self.future_seen = true;
					Some(status)
				}
			},
			TransactionStatus::Ready => {
				self.views_keeping_tx_valid.insert(hash);
				if self.ready_seen {
					None
				} else {
					self.ready_seen = true;
					Some(status)
				}
			},
			TransactionStatus::Broadcast(_) => None,
			TransactionStatus::InBlock((..)) => {
				self.views_keeping_tx_valid.insert(hash);
				if !(self.ready_seen || self.future_seen) {
					self.ready_seen = true;
					Some(status)
				} else {
					Some(status)
				}
			},
			TransactionStatus::Retracted(_) => None,
			TransactionStatus::FinalityTimeout(_) => Some(status),
			TransactionStatus::Finalized(_) => {
				self.terminate = true;
				Some(status)
			},
			TransactionStatus::Usurped(_) |
			TransactionStatus::Dropped |
			TransactionStatus::Invalid => None,
		}
	}

	/// Handles transaction invalidation sent via side channel.
	///
	/// Function may set the context termination flag, which will close the stream.
	///
	/// Returns true if the event should be sent out, and false if the invalidation request should
	/// be skipped.
	fn handle_invalidate_transaction(&mut self) -> bool {
		let keys = HashSet::<BlockHash<ChainApi>>::from_iter(
			self.status_stream_map.keys().map(Clone::clone),
		);
		trace!(
			target: LOG_TARGET,
			"[{:?}] got invalidate_transaction: views:{:?}", self.tx_hash,
			self.status_stream_map.keys().collect::<Vec<_>>()
		);
		if self.views_keeping_tx_valid.is_disjoint(&keys) {
			self.terminate = true;
			true
		} else {
			//todo [#5477]
			// - handle corner case:  this may happen when tx is invalid for mempool, but somehow
			//   some view still sees it as ready/future. In that case we don't send the invalid
			//   event, as transaction can still be included. Probably we should set some flag here
			//   and allow for invalid sent from the view.
			// - add debug / metrics,
			false
		}
	}

	/// Adds a new transaction status stream.
	///
	/// Inserts a new view's transaction status stream associated with a specific block hash into
	/// the stream map.
	fn add_stream(&mut self, block_hash: BlockHash<ChainApi>, stream: TxStatusStream<ChainApi>) {
		self.status_stream_map.insert(block_hash, stream);
		trace!(target: LOG_TARGET, "[{:?}] AddView view: {:?} views:{:?}", self.tx_hash, block_hash, self.status_stream_map.keys().collect::<Vec<_>>());
	}

	/// Removes an existing transaction status stream.
	///
	/// Removes a transaction status stream associated with a specific block hash from the
	/// stream map.
	fn remove_view(&mut self, block_hash: BlockHash<ChainApi>) {
		self.status_stream_map.remove(&block_hash);
		trace!(target: LOG_TARGET, "[{:?}] RemoveView view: {:?} views:{:?}", self.tx_hash, block_hash, self.status_stream_map.keys().collect::<Vec<_>>());
	}
}

impl<ChainApi> MultiViewListener<ChainApi>
where
	ChainApi: graph::ChainApi + 'static,
	<<ChainApi as graph::ChainApi>::Block as BlockT>::Hash: Unpin,
{
	/// Creates new instance of `MultiViewListener`.
	pub fn new() -> Self {
		Self { controllers: Default::default() }
	}

	/// Creates an external aggregated stream of events for given transaction.
	///
	/// This method initializes an `ExternalWatcherContext` for the provided transaction hash, sets
	/// up the necessary communication channels, and unfolds an external (meaning that it can be
	/// exposed to [`sc_transaction_pool_api::TransactionPool`] API client e.g. rpc) stream of
	/// transaction status events. If an external watcher is already present for the given
	/// transaction, it returns `None`.
	pub(crate) fn create_external_watcher_for_tx(
		&self,
		tx_hash: ExtrinsicHash<ChainApi>,
	) -> Option<TxStatusStream<ChainApi>> {
		let mut controllers = self.controllers.write();
		if controllers.contains_key(&tx_hash) {
			return None
		}

		trace!(target: LOG_TARGET, "[{:?}] create_external_watcher_for_tx", tx_hash);

		let (tx, rx) = mpsc::tracing_unbounded("txpool-multi-view-listener", 32);
		controllers.insert(tx_hash, tx);

		let ctx = ExternalWatcherContext::new(tx_hash, rx);

		Some(
			futures::stream::unfold(ctx, |mut ctx| async move {
				if ctx.terminate {
					return None
				}
				loop {
					tokio::select! {
						biased;
						Some((view_hash, status)) =  next_event(&mut ctx.status_stream_map) => {
							if let Some(new_status) = ctx.handle(status, view_hash) {
								log::trace!(target: LOG_TARGET, "[{:?}] mvl sending out: {new_status:?}", ctx.tx_hash);
								return Some((new_status, ctx))
							}
						},
						cmd = ctx.command_receiver.next() => {
							log::trace!(target: LOG_TARGET, "[{:?}] select::rx views:{:?}",
								ctx.tx_hash,
								ctx.status_stream_map.keys().collect::<Vec<_>>()
							);
							match cmd? {
								ControllerCommand::AddViewStream(h,stream) => {
									ctx.add_stream(h, stream);
								},
								ControllerCommand::RemoveViewStream(h) => {
									ctx.remove_view(h);
								},
								ControllerCommand::TransactionInvalidated => {
									if ctx.handle_invalidate_transaction() {
										log::trace!(target: LOG_TARGET, "[{:?}] mvl sending out: Invalid", ctx.tx_hash);
										return Some((TransactionStatus::Invalid, ctx))
									}
								},
								ControllerCommand::FinalizeTransaction(block, index) => {
									log::trace!(target: LOG_TARGET, "[{:?}] mvl sending out: Finalized", ctx.tx_hash);
									ctx.terminate = true;
									return Some((TransactionStatus::Finalized((block, index)), ctx))
								},
								ControllerCommand::TransactionBroadcasted(peers) => {
									log::trace!(target: LOG_TARGET, "[{:?}] mvl sending out: Broadcasted", ctx.tx_hash);
									return Some((TransactionStatus::Broadcast(peers), ctx))
								},
								ControllerCommand::TransactionDropped => {
									log::trace!(target: LOG_TARGET, "[{:?}] mvl sending out: Dropped", ctx.tx_hash);
									ctx.terminate = true;
									return Some((TransactionStatus::Dropped, ctx))
								},
							}
						},
					};
				}
			})
			.boxed(),
		)
	}

	/// Adds a view's transaction status stream for particular transaction.
	///
	/// This method sends a `AddViewStream` command to the controller of each transaction to
	/// remove the view's stream corresponding to the given block hash.
	pub(crate) fn add_view_watcher_for_tx(
		&self,
		tx_hash: ExtrinsicHash<ChainApi>,
		block_hash: BlockHash<ChainApi>,
		stream: TxStatusStream<ChainApi>,
	) {
		let mut controllers = self.controllers.write();

		if let Entry::Occupied(mut tx) = controllers.entry(tx_hash) {
			if let Err(e) = tx
				.get_mut()
				.unbounded_send(ControllerCommand::AddViewStream(block_hash, stream))
			{
				trace!(target: LOG_TARGET, "[{:?}] add_view_watcher_for_tx: send message failed: {:?}", tx_hash, e);
				tx.remove();
			}
		}
	}

	/// Removes a view's stream associated with a specific view hash across all transactions.
	///
	/// This method sends a `RemoveViewStream` command to the controller of each transaction to
	/// remove the view's stream corresponding to the given block hash.
	pub(crate) fn remove_view(&self, block_hash: BlockHash<ChainApi>) {
		self.controllers.write().retain(|tx_hash, sender| {
			sender
				.unbounded_send(ControllerCommand::RemoveViewStream(block_hash))
				.map_err(|e| {
					log::trace!(target: LOG_TARGET, "[{:?}] remove_view: send message failed: {:?}", tx_hash, e);
					e
				})
				.is_ok()
		});
	}

	/// Invalidate given transaction.
	///
	/// This method sends a `TransactionInvalidated` command to the controller of each transaction
	/// provided to process the invalidation request.
	///
	/// The external event will be sent if no view is referencing the transaction as `Ready` or
	/// `Future`.
	pub(crate) fn invalidate_transactions(&self, invalid_hashes: &[ExtrinsicHash<ChainApi>]) {
		let mut controllers = self.controllers.write();
		invalid_hashes.iter().for_each(|tx_hash| {
			if let Entry::Occupied(mut tx) = controllers.entry(*tx_hash) {
				trace!(target: LOG_TARGET, "[{:?}] invalidate_transaction", tx_hash);
				if let Err(e) =
					tx.get_mut().unbounded_send(ControllerCommand::TransactionInvalidated)
				{
					trace!(target: LOG_TARGET, "[{:?}] invalidate_transaction: send message failed: {:?}", tx_hash, e);
					tx.remove();
				}
			}
		});
	}

	/// Send `Broadcasted` event to listeners of all transactions.
	///
	/// This method sends a `TransactionBroadcasted` command to the controller of each transaction
	/// provided prompting the external `Broadcasted` event.
	pub(crate) fn transactions_broadcasted(
		&self,
		propagated: HashMap<ExtrinsicHash<ChainApi>, Vec<String>>,
	) {
		let mut controllers = self.controllers.write();
		propagated.into_iter().for_each(|(tx_hash, peers)| {
			if let Entry::Occupied(mut tx) = controllers.entry(tx_hash) {
				trace!(target: LOG_TARGET, "[{:?}] transaction_broadcasted", tx_hash);
				if let Err(e) = tx.get_mut().unbounded_send(ControllerCommand::TransactionBroadcasted(peers)) {
					trace!(target: LOG_TARGET, "[{:?}] transactions_broadcasted: send message failed: {:?}", tx_hash, e);
					tx.remove();
				}
			}
		});
	}

	/// Send `Dropped` event to listeners of transactions.
	///
	/// This method sends a `TransactionDropped` command to the controller of each requested
	/// transaction prompting and external `Broadcasted` event.
	pub(crate) fn transactions_dropped(&self, dropped: &[ExtrinsicHash<ChainApi>]) {
		let mut controllers = self.controllers.write();
		debug!(target: LOG_TARGET, "mvl::transactions_dropped: {:?}", dropped);
		for tx_hash in dropped {
			if let Some(tx) = controllers.remove(&tx_hash) {
				debug!(target: LOG_TARGET, "[{:?}] transaction_dropped", tx_hash);
				if let Err(e) = tx.unbounded_send(ControllerCommand::TransactionDropped) {
					trace!(target: LOG_TARGET, "[{:?}] transactions_dropped: send message failed: {:?}", tx_hash, e);
				};
			}
		}
	}

	/// Send `Finalized` event for given transaction at given block.
	///
	/// This will send `Finalized` event to the external watcher.
	pub(crate) fn finalize_transaction(
		&self,
		tx_hash: ExtrinsicHash<ChainApi>,
		block: BlockHash<ChainApi>,
		idx: TxIndex,
	) {
		let mut controllers = self.controllers.write();
		if let Some(tx) = controllers.remove(&tx_hash) {
			trace!(target: LOG_TARGET, "[{:?}] finalize_transaction", tx_hash);
			if let Err(e) = tx.unbounded_send(ControllerCommand::FinalizeTransaction(block, idx)) {
				trace!(target: LOG_TARGET, "[{:?}] finalize_transaction: send message failed: {:?}", tx_hash, e);
			}
		};
	}

	/// Removes stale controllers.
	pub(crate) fn remove_stale_controllers(&self) {
		self.controllers.write().retain(|_, c| !c.is_closed());
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::common::tests::TestApi;
	use futures::{stream, StreamExt};
	use sp_core::H256;

	type MultiViewListener = super::MultiViewListener<TestApi>;

	#[tokio::test]
	async fn test01() {
		sp_tracing::try_init_simple();
		let listener = MultiViewListener::new();

		let block_hash = H256::repeat_byte(0x01);
		let events = vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((block_hash, 0)),
			TransactionStatus::Finalized((block_hash, 0)),
		];

		let tx_hash = H256::repeat_byte(0x0a);
		let external_watcher = listener.create_external_watcher_for_tx(tx_hash).unwrap();
		let handle = tokio::spawn(async move { external_watcher.collect::<Vec<_>>().await });

		let view_stream = futures::stream::iter(events.clone());

		listener.add_view_watcher_for_tx(tx_hash, block_hash, view_stream.boxed());

		let out = handle.await.unwrap();
		assert_eq!(out, events);
		log::debug!("out: {:#?}", out);
	}

	#[tokio::test]
	async fn test02() {
		sp_tracing::try_init_simple();
		let listener = MultiViewListener::new();

		let block_hash0 = H256::repeat_byte(0x01);
		let events0 = vec![
			TransactionStatus::Future,
			TransactionStatus::Ready,
			TransactionStatus::InBlock((block_hash0, 0)),
		];

		let block_hash1 = H256::repeat_byte(0x02);
		let events1 = vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((block_hash1, 0)),
			TransactionStatus::Finalized((block_hash1, 0)),
		];

		let tx_hash = H256::repeat_byte(0x0a);
		let external_watcher = listener.create_external_watcher_for_tx(tx_hash).unwrap();

		let view_stream0 = futures::stream::iter(events0.clone());
		let view_stream1 = futures::stream::iter(events1.clone());

		let handle = tokio::spawn(async move { external_watcher.collect::<Vec<_>>().await });

		listener.add_view_watcher_for_tx(tx_hash, block_hash0, view_stream0.boxed());
		listener.add_view_watcher_for_tx(tx_hash, block_hash1, view_stream1.boxed());

		let out = handle.await.unwrap();

		log::debug!("out: {:#?}", out);
		assert!(out.iter().all(|v| vec![
			TransactionStatus::Future,
			TransactionStatus::Ready,
			TransactionStatus::InBlock((block_hash0, 0)),
			TransactionStatus::InBlock((block_hash1, 0)),
			TransactionStatus::Finalized((block_hash1, 0)),
		]
		.contains(v)));
		assert_eq!(out.len(), 5);
	}

	#[tokio::test]
	async fn test03() {
		sp_tracing::try_init_simple();
		let listener = MultiViewListener::new();

		let block_hash0 = H256::repeat_byte(0x01);
		let events0 = vec![
			TransactionStatus::Future,
			TransactionStatus::Ready,
			TransactionStatus::InBlock((block_hash0, 0)),
		];

		let block_hash1 = H256::repeat_byte(0x02);
		let events1 = vec![TransactionStatus::Future];

		let tx_hash = H256::repeat_byte(0x0a);
		let external_watcher = listener.create_external_watcher_for_tx(tx_hash).unwrap();
		let handle = tokio::spawn(async move { external_watcher.collect::<Vec<_>>().await });

		let view_stream0 = futures::stream::iter(events0.clone());
		let view_stream1 = futures::stream::iter(events1.clone());

		listener.add_view_watcher_for_tx(tx_hash, block_hash0, view_stream0.boxed());
		listener.add_view_watcher_for_tx(tx_hash, block_hash1, view_stream1.boxed());

		listener.invalidate_transactions(&[tx_hash]);

		let out = handle.await.unwrap();
		log::debug!("out: {:#?}", out);
		assert!(out.iter().all(|v| vec![
			TransactionStatus::Future,
			TransactionStatus::Ready,
			TransactionStatus::InBlock((block_hash0, 0)),
			TransactionStatus::Invalid
		]
		.contains(v)));
		assert_eq!(out.len(), 4);
	}

	#[tokio::test]
	async fn test032() {
		sp_tracing::try_init_simple();
		let listener = MultiViewListener::new();

		let block_hash0 = H256::repeat_byte(0x01);
		let events0_tx0 = vec![TransactionStatus::Future];
		let events0_tx1 = vec![TransactionStatus::Ready];

		let block_hash1 = H256::repeat_byte(0x02);
		let events1_tx0 =
			vec![TransactionStatus::Ready, TransactionStatus::InBlock((block_hash1, 0))];
		let events1_tx1 = vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((block_hash1, 1)),
			TransactionStatus::Finalized((block_hash1, 1)),
		];

		let tx0_hash = H256::repeat_byte(0x0a);
		let tx1_hash = H256::repeat_byte(0x0b);
		let external_watcher_tx0 = listener.create_external_watcher_for_tx(tx0_hash).unwrap();
		let external_watcher_tx1 = listener.create_external_watcher_for_tx(tx1_hash).unwrap();

		let handle0 = tokio::spawn(async move { external_watcher_tx0.collect::<Vec<_>>().await });
		let handle1 = tokio::spawn(async move { external_watcher_tx1.collect::<Vec<_>>().await });

		let view0_tx0_stream = futures::stream::iter(events0_tx0.clone());
		let view0_tx1_stream = futures::stream::iter(events0_tx1.clone());

		let view1_tx0_stream = futures::stream::iter(events1_tx0.clone());
		let view1_tx1_stream = futures::stream::iter(events1_tx1.clone());

		listener.add_view_watcher_for_tx(tx0_hash, block_hash0, view0_tx0_stream.boxed());
		listener.add_view_watcher_for_tx(tx0_hash, block_hash1, view1_tx0_stream.boxed());
		listener.add_view_watcher_for_tx(tx1_hash, block_hash0, view0_tx1_stream.boxed());
		listener.add_view_watcher_for_tx(tx1_hash, block_hash1, view1_tx1_stream.boxed());

		listener.invalidate_transactions(&[tx0_hash]);
		listener.invalidate_transactions(&[tx1_hash]);

		let out_tx0 = handle0.await.unwrap();
		let out_tx1 = handle1.await.unwrap();

		log::debug!("out_tx0: {:#?}", out_tx0);
		log::debug!("out_tx1: {:#?}", out_tx1);
		assert!(out_tx0.iter().all(|v| vec![
			TransactionStatus::Future,
			TransactionStatus::Ready,
			TransactionStatus::InBlock((block_hash1, 0)),
			TransactionStatus::Invalid
		]
		.contains(v)));

		assert!(out_tx1.iter().all(|v| vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((block_hash1, 1)),
			TransactionStatus::Finalized((block_hash1, 1))
		]
		.contains(v)));
		assert_eq!(out_tx0.len(), 4);
		assert_eq!(out_tx1.len(), 3);
	}

	#[tokio::test]
	async fn test04() {
		sp_tracing::try_init_simple();
		let listener = MultiViewListener::new();

		let block_hash0 = H256::repeat_byte(0x01);
		let events0 = vec![
			TransactionStatus::Future,
			TransactionStatus::Ready,
			TransactionStatus::InBlock((block_hash0, 0)),
		];

		let block_hash1 = H256::repeat_byte(0x02);
		let events1 = vec![TransactionStatus::Future];

		let tx_hash = H256::repeat_byte(0x0a);
		let external_watcher = listener.create_external_watcher_for_tx(tx_hash).unwrap();

		//views will keep transaction valid, invalidation shall not happen
		let view_stream0 = futures::stream::iter(events0.clone()).chain(stream::pending().boxed());
		let view_stream1 = futures::stream::iter(events1.clone()).chain(stream::pending().boxed());

		let handle = tokio::spawn(async move {
			// views are still there, we need to fetch 3 events
			external_watcher.take(3).collect::<Vec<_>>().await
		});

		listener.add_view_watcher_for_tx(tx_hash, block_hash0, view_stream0.boxed());
		listener.add_view_watcher_for_tx(tx_hash, block_hash1, view_stream1.boxed());

		listener.invalidate_transactions(&[tx_hash]);

		let out = handle.await.unwrap();
		log::debug!("out: {:#?}", out);

		// invalid shall not be sent
		assert!(out.iter().all(|v| vec![
			TransactionStatus::Future,
			TransactionStatus::Ready,
			TransactionStatus::InBlock((block_hash0, 0)),
		]
		.contains(v)));
		assert_eq!(out.len(), 3);
	}

	#[tokio::test]
	async fn test05() {
		sp_tracing::try_init_simple();
		let listener = MultiViewListener::new();

		let block_hash0 = H256::repeat_byte(0x01);
		let events0 = vec![TransactionStatus::Invalid];

		let tx_hash = H256::repeat_byte(0x0a);
		let external_watcher = listener.create_external_watcher_for_tx(tx_hash).unwrap();
		let handle = tokio::spawn(async move { external_watcher.collect::<Vec<_>>().await });

		let view_stream0 = futures::stream::iter(events0.clone()).chain(stream::pending().boxed());

		// Note: this generates actual Invalid event.
		// Invalid event from View's stream is intentionally ignored.
		listener.invalidate_transactions(&[tx_hash]);

		listener.add_view_watcher_for_tx(tx_hash, block_hash0, view_stream0.boxed());

		let out = handle.await.unwrap();
		log::debug!("out: {:#?}", out);

		assert!(out.iter().all(|v| vec![TransactionStatus::Invalid].contains(v)));
		assert_eq!(out.len(), 1);
	}
}
