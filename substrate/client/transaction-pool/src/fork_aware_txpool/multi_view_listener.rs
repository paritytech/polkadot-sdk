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

//! Multi view listener. Combines streams from many views into single transaction status stream.

use crate::{
	graph::{self, BlockHash, ExtrinsicHash},
	LOG_TARGET,
};
use futures::{stream, stream::Fuse, StreamExt};
use log::{debug, info, trace};
use sc_transaction_pool_api::{TransactionStatus, TransactionStatusStream, TxIndex};
use sc_utils::mpsc;
use sp_runtime::traits::Block as BlockT;
use std::{
	collections::{HashMap, HashSet},
	pin::Pin,
};
use tokio_stream::StreamMap;

type Controller<T> = mpsc::TracingUnboundedSender<T>;
type CommandReceiver<T> = mpsc::TracingUnboundedReceiver<T>;

/// The stream of transaction events.
///
/// It can represent both view's stream and external watcher stream.
pub type TxStatusStream<T> = Pin<Box<TransactionStatusStream<ExtrinsicHash<T>, BlockHash<T>>>>;

enum ControllerCommand<ChainApi: graph::ChainApi> {
	AddView(BlockHash<ChainApi>, TxStatusStream<ChainApi>),
	RemoveView(BlockHash<ChainApi>),
	TransactionInvalidated,
	FinalizeTransaction(BlockHash<ChainApi>, TxIndex),
	TransactionBroadcasted(Vec<String>),
	TransactionDropped,
}

impl<ChainApi> std::fmt::Debug for ControllerCommand<ChainApi>
where
	ChainApi: graph::ChainApi,
{
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			ControllerCommand::AddView(h, _) => write!(f, "ListenerAction::AddView({h})"),
			ControllerCommand::RemoveView(h) => write!(f, "ListenerAction::RemoveView({h})"),
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

/// This struct allows to create and control listener for single transactions.
///
/// For every transaction the view's stream generating its own events can be added. The events are
/// flattened and sent out to the external listener.
///
/// The listner allows to add and remove view's stream (per transaction).
/// The listener allows also to invalidate and finalize transcation.
pub struct MultiViewListener<ChainApi: graph::ChainApi> {
	controllers: parking_lot::RwLock<
		HashMap<ExtrinsicHash<ChainApi>, Controller<ControllerCommand<ChainApi>>>,
	>,
}

/// External watcher context.
///
/// Aggregates and implements the logic of converting single view's events to the external
/// events. This context is used to unfold external watcher stream.
struct ExternalWatcherContext<ChainApi: graph::ChainApi> {
	tx_hash: ExtrinsicHash<ChainApi>,
	status_stream_map:
		futures::stream::Fuse<StreamMap<BlockHash<ChainApi>, TxStatusStream<ChainApi>>>,
	command_receiver: Fuse<CommandReceiver<ControllerCommand<ChainApi>>>,
	terminate: bool,
	future_seen: bool,
	ready_seen: bool,

	inblock: HashSet<BlockHash<ChainApi>>,
	views_keeping_tx_valid: HashSet<BlockHash<ChainApi>>,
}

impl<ChainApi: graph::ChainApi> ExternalWatcherContext<ChainApi>
where
	<<ChainApi as graph::ChainApi>::Block as BlockT>::Hash: Unpin,
{
	fn new(
		tx_hash: ExtrinsicHash<ChainApi>,
		command_receiver: Fuse<CommandReceiver<ControllerCommand<ChainApi>>>,
	) -> Self {
		let mut stream_map: StreamMap<BlockHash<ChainApi>, TxStatusStream<ChainApi>> =
			StreamMap::new();
		//note: do not terminate stream-map if input streams (views) are all done:
		stream_map.insert(Default::default(), stream::pending().boxed());
		Self {
			tx_hash,
			status_stream_map: futures::StreamExt::fuse(stream_map),
			command_receiver,
			terminate: false,
			future_seen: false,
			ready_seen: false,
			views_keeping_tx_valid: Default::default(),
			inblock: Default::default(),
		}
	}

	fn handle(
		&mut self,
		status: &TransactionStatus<ExtrinsicHash<ChainApi>, BlockHash<ChainApi>>,
		hash: BlockHash<ChainApi>,
	) -> bool {
		trace!(
			target: LOG_TARGET, "[{:?}] handle event from {hash:?}: {status:?} views:{:#?}", self.tx_hash,
			self.status_stream_map.get_ref().keys().collect::<Vec<_>>()
		);
		match status {
			TransactionStatus::Future => {
				self.views_keeping_tx_valid.insert(hash);
				if self.ready_seen || self.future_seen {
					false
				} else {
					self.future_seen = true;
					true
				}
			},
			TransactionStatus::Ready => {
				self.views_keeping_tx_valid.insert(hash);
				if self.ready_seen {
					false
				} else {
					self.ready_seen = true;
					true
				}
			},
			TransactionStatus::Broadcast(_) => {
				//todo: remove panic
				panic!("broadcast shall not happen");
				// false
			},
			TransactionStatus::InBlock((block, _)) => self.inblock.insert(*block),
			TransactionStatus::Retracted(_) => {
				//todo: remove panic
				panic!("retracted? shall not happen");
				// false
			},
			TransactionStatus::FinalityTimeout(_) => true,
			TransactionStatus::Finalized(_) => {
				self.terminate = true;
				true
			},
			TransactionStatus::Usurped(_) |
			TransactionStatus::Dropped |
			TransactionStatus::Invalid => false,
		}
	}

	fn handle_invalidate_transaction(&mut self) -> bool {
		let keys = HashSet::<BlockHash<ChainApi>>::from_iter(
			self.status_stream_map.get_ref().keys().map(Clone::clone),
		);
		trace!(
			target: LOG_TARGET,
			"[{:?}] got invalidate_transaction: views:{:#?}", self.tx_hash,
			self.status_stream_map.get_ref().keys().collect::<Vec<_>>()
		);
		if self.views_keeping_tx_valid.is_disjoint(&keys) {
			self.terminate = true;
			true
		} else {
			//todo:
			// - add debug / metrics,
			// - handle corner case:  this may happen when tx is invalid for mempool, but somehow
			//   some view still sees it as ready/future. In that case we don't send the invalid
			//   event, as transaction can still be included. Probably we should set some flag here
			//   and allow for invalid sent from the view.
			false
		}
	}

	fn add_stream(&mut self, block_hash: BlockHash<ChainApi>, stream: TxStatusStream<ChainApi>) {
		self.status_stream_map.get_mut().insert(block_hash, stream);
		trace!(target: LOG_TARGET, "[{:?}] AddView view: {:?} views:{:?}", self.tx_hash, block_hash, self.status_stream_map.get_ref().keys().collect::<Vec<_>>());
	}

	fn remove_view(&mut self, block_hash: BlockHash<ChainApi>) {
		self.status_stream_map.get_mut().remove(&block_hash);
		trace!(target: LOG_TARGET, "[{:?}] RemoveView view: {:?} views:{:?}", self.tx_hash, block_hash, self.status_stream_map.get_ref().keys().collect::<Vec<_>>());
	}
}

impl<ChainApi> MultiViewListener<ChainApi>
where
	ChainApi: graph::ChainApi + 'static,
	<<ChainApi as graph::ChainApi>::Block as BlockT>::Hash: Unpin,
{
	/// Creates new instance.
	pub fn new() -> Self {
		Self { controllers: Default::default() }
	}

	/// Creates an external watcher for given transaction.
	pub(crate) fn create_external_watcher_for_tx(
		&self,
		tx_hash: ExtrinsicHash<ChainApi>,
	) -> Option<TxStatusStream<ChainApi>> {
		let mut controllers = self.controllers.write();
		if controllers.contains_key(&tx_hash) {
			return None;
		}

		trace!(target: LOG_TARGET, "[{:?}] create_external_watcher_for_tx", tx_hash);

		let (tx, rx) = mpsc::tracing_unbounded("txpool-multi-view-listener", 32);
		controllers.insert(tx_hash, tx);

		let ctx = ExternalWatcherContext::new(tx_hash, rx.fuse());

		Some(
			futures::stream::unfold(ctx, |mut ctx| async move {
				if ctx.terminate {
					return None
				}
				loop {
					tokio::select! {
					biased;
					v =  futures::StreamExt::select_next_some(&mut ctx.status_stream_map) => {
						log::trace!(target: LOG_TARGET, "[{:?}] select::map views:{:?}", ctx.tx_hash, ctx.status_stream_map.get_ref().keys().collect::<Vec<_>>());
						let (view_hash, status) = v;

						if ctx.handle(&status, view_hash) {
							log::debug!(target: LOG_TARGET, "[{:?}] sending out: {status:?}", ctx.tx_hash);
							return Some((status, ctx));
						}
					},
					cmd = ctx.command_receiver.next() => {
						log::trace!(target: LOG_TARGET, "[{:?}] select::rx views:{:?}", ctx.tx_hash, ctx.status_stream_map.get_ref().keys().collect::<Vec<_>>());
						match cmd {
							Some(ControllerCommand::AddView(h,stream)) => {
								ctx.add_stream(h, stream);
							},
							Some(ControllerCommand::RemoveView(h)) => {
								ctx.remove_view(h);
							},
							Some(ControllerCommand::TransactionInvalidated) => {
								if ctx.handle_invalidate_transaction() {
									log::debug!(target: LOG_TARGET, "[{:?}] sending out: Invalid", ctx.tx_hash);
									return Some((TransactionStatus::Invalid, ctx))
								}
							},
							Some(ControllerCommand::FinalizeTransaction(block, index)) => {
								log::debug!(target: LOG_TARGET, "[{:?}] sending out: Finalized", ctx.tx_hash);
								ctx.terminate = true;
								return Some((TransactionStatus::Finalized((block, index)), ctx))
							},
							Some(ControllerCommand::TransactionBroadcasted(peers)) => {
								log::debug!(target: LOG_TARGET, "[{:?}] sending out: Broadcasted", ctx.tx_hash);
								return Some((TransactionStatus::Broadcast(peers), ctx))
							},
							Some(ControllerCommand::TransactionDropped) => {
								log::debug!(target: LOG_TARGET, "[{:?}] sending out: Dropped", ctx.tx_hash);
								ctx.terminate = true;
								return Some((TransactionStatus::Dropped, ctx))
							},
							None => {},
						}
					},
					};
				}
			})
			.boxed(),
		)
	}

	/// Adds a view's stream for particular transaction.
	pub(crate) fn add_view_watcher_for_tx(
		&self,
		tx_hash: ExtrinsicHash<ChainApi>,
		block_hash: BlockHash<ChainApi>,
		stream: TxStatusStream<ChainApi>,
	) {
		let mut controllers = self.controllers.write();
		if let Some(tx) = controllers.get(&tx_hash) {
			match tx.unbounded_send(ControllerCommand::AddView(block_hash, stream)) {
				Err(e) => {
					debug!(target: LOG_TARGET, "[{:?}] add_view_watcher_for_tx: send message failed: {:?}", tx_hash, e);
					controllers.remove(&tx_hash);
				},
				Ok(_) => {},
			}
		}
	}

	/// Remove given view's stream from every transaction stream.
	pub(crate) fn remove_view(&self, block_hash: BlockHash<ChainApi>) {
		let mut controllers = self.controllers.write();
		let mut invalid_controllers = Vec::new();
		for (tx_hash, sender) in controllers.iter() {
			match sender.unbounded_send(ControllerCommand::RemoveView(block_hash)) {
				Err(e) => {
					log::debug!(target: LOG_TARGET, "[{:?}] remove_view: send message failed: {:?}", tx_hash, e);
					invalid_controllers.push(*tx_hash);
				},
				Ok(_) => {},
			}
		}
		invalid_controllers.into_iter().for_each(|tx_hash| {
			controllers.remove(&tx_hash);
		});
	}

	/// Invalidate given transaction.
	///
	/// This will send invalidated event to the external watcher.
	pub(crate) fn invalidate_transactions(&self, invalid_hashes: Vec<ExtrinsicHash<ChainApi>>) {
		let mut controllers = self.controllers.write();

		for tx_hash in invalid_hashes {
			if let Some(tx) = controllers.get(&tx_hash) {
				trace!(target: LOG_TARGET, "[{:?}] invalidate_transaction", tx_hash);
				match tx.unbounded_send(ControllerCommand::TransactionInvalidated) {
					Err(e) => {
						debug!(target: LOG_TARGET, "[{:?}] invalidate_transaction: send message failed: {:?}", tx_hash, e);
						controllers.remove(&tx_hash);
					},
					Ok(_) => {},
				}
			}
		}
	}

	/// Send `Broadcasted` event to listeners of transactions.
	pub(crate) fn transactions_broadcasted(
		&self,
		propagated: HashMap<ExtrinsicHash<ChainApi>, Vec<String>>,
	) {
		// pub fn on_broadcasted(&self, propagated: HashMap<ExtrinsicHash<B>, Vec<String>>) {
		let mut controllers = self.controllers.write();

		for (tx_hash, peers) in propagated {
			if let Some(tx) = controllers.get(&tx_hash) {
				trace!(target: LOG_TARGET, "[{:?}] transaction_broadcasted", tx_hash);
				match tx.unbounded_send(ControllerCommand::TransactionBroadcasted(peers)) {
					Err(e) => {
						debug!(target: LOG_TARGET, "[{:?}] transactions_broadcasted: send message failed: {:?}", tx_hash, e);
						controllers.remove(&tx_hash);
					},
					Ok(_) => {},
				}
			}
		}
	}

	/// Send `Dropped` event to listeners of transactions.
	pub(crate) fn transactions_dropped(&self, dropped: &Vec<ExtrinsicHash<ChainApi>>) {
		// pub fn on_broadcasted(&self, propagated: HashMap<ExtrinsicHash<B>, Vec<String>>) {
		let mut controllers = self.controllers.write();

		info!(target: LOG_TARGET, "mvl::transactions_dropped: {:?}", dropped);
		for tx_hash in dropped {
			if let Some(tx) = controllers.get(&tx_hash) {
				info!(target: LOG_TARGET, "[{:?}] transaction_dropped", tx_hash);
				match tx.unbounded_send(ControllerCommand::TransactionDropped) {
					Err(e) => {
						debug!(target: LOG_TARGET, "[{:?}] transactions_dropped: send message failed: {:?}", tx_hash, e);
						controllers.remove(&tx_hash);
					},
					Ok(_) => {},
				}
			}
		}
	}

	/// Finalize given transaction at given block.
	///
	/// This will send finalize event to the external watcher.
	pub(crate) fn finalize_transaction(
		&self,
		tx_hash: ExtrinsicHash<ChainApi>,
		block: BlockHash<ChainApi>,
		idx: TxIndex,
	) {
		let mut controllers = self.controllers.write();

		if let Some(tx) = controllers.get(&tx_hash) {
			trace!(target: LOG_TARGET, "[{:?}] finalize_transaction", tx_hash);
			let result = tx.unbounded_send(ControllerCommand::FinalizeTransaction(block, idx));
			if let Err(e) = result {
				debug!(target: LOG_TARGET, "[{:?}] finalize_transaction: send message failed: {:?}", tx_hash, e);
				controllers.remove(&tx_hash);
			}
		};
	}

	/// Removes stale controllers
	pub(crate) fn remove_stale_controllers(&self) {
		let mut controllers = self.controllers.write();
		controllers.retain(|_, c| !c.is_closed());
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::common::tests::TestApi;
	use futures::StreamExt;
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
		log::info!("out: {:#?}", out);
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

		log::info!("out: {:#?}", out);
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

		listener.invalidate_transactions(vec![tx_hash]);

		let out = handle.await.unwrap();
		log::info!("out: {:#?}", out);
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

		listener.invalidate_transactions(vec![tx0_hash]);
		listener.invalidate_transactions(vec![tx1_hash]);

		let out_tx0 = handle0.await.unwrap();
		let out_tx1 = handle1.await.unwrap();

		log::info!("out_tx0: {:#?}", out_tx0);
		log::info!("out_tx1: {:#?}", out_tx1);
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

		listener.invalidate_transactions(vec![tx_hash]);

		let out = handle.await.unwrap();
		log::info!("out: {:#?}", out);

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
		listener.invalidate_transactions(vec![tx_hash]);

		listener.add_view_watcher_for_tx(tx_hash, block_hash0, view_stream0.boxed());

		let out = handle.await.unwrap();
		log::info!("out: {:#?}", out);

		assert!(out.iter().all(|v| vec![TransactionStatus::Invalid].contains(v)));
		assert_eq!(out.len(), 1);
	}
}
