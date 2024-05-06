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

const LOG_TARGET: &str = "txpool::mvlistener";

use crate::graph::{BlockHash, ChainApi, ExtrinsicHash as TxHash};
use futures::{stream, StreamExt};
use log::trace;
use sc_transaction_pool_api::{TransactionStatus, TransactionStatusStream};
use sp_runtime::traits::{Block as BlockT, Extrinsic, Hash as HashT};
use std::{
	collections::{HashMap, HashSet},
	pin::Pin,
};
use tokio::sync::mpsc;
use tokio_stream::StreamMap;

// pub type TransactionStatusStream<Hash, BlockHash> =
// 	dyn Stream<Item = TransactionStatus<Hash, BlockHash>> + Send;
pub type TxStatusStream<T> = Pin<Box<TransactionStatusStream<TxHash<T>, BlockHash<T>>>>;

enum ListenerAction<PoolApi: ChainApi> {
	ViewAdded(BlockHash<PoolApi>, TxStatusStream<PoolApi>),
	InvalidateTransaction,
}

impl<PoolApi> std::fmt::Debug for ListenerAction<PoolApi>
where
	PoolApi: ChainApi,
{
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			ListenerAction::ViewAdded(h, _) => write!(f, "ListenerAction::ViewAdded({})", h),
			ListenerAction::InvalidateTransaction => {
				write!(f, "ListenerAction::InvalidateTransaction")
			},
		}
	}
}

pub struct MultiViewListener<PoolApi: ChainApi> {
	//todo: rwlock not needed here (mut?)
	controllers:
		tokio::sync::RwLock<HashMap<TxHash<PoolApi>, mpsc::Sender<ListenerAction<PoolApi>>>>,
}

struct ExternalWatcherContext<PoolApi: ChainApi> {
	tx_hash: TxHash<PoolApi>,
	fused: futures::stream::Fuse<StreamMap<BlockHash<PoolApi>, TxStatusStream<PoolApi>>>,
	rx: mpsc::Receiver<ListenerAction<PoolApi>>,
	terminate: bool,
	future_seen: bool,
	ready_seen: bool,
	breadcast_seen: bool,

	inblock: HashSet<BlockHash<PoolApi>>,
	views_keeping_tx_valid: HashSet<BlockHash<PoolApi>>,
}

impl<PoolApi: ChainApi> ExternalWatcherContext<PoolApi>
where
	<<PoolApi as ChainApi>::Block as BlockT>::Hash: Unpin,
{
	fn new(tx_hash: TxHash<PoolApi>, rx: mpsc::Receiver<ListenerAction<PoolApi>>) -> Self {
		let mut stream_map: StreamMap<BlockHash<PoolApi>, TxStatusStream<PoolApi>> =
			StreamMap::new();
		stream_map.insert(Default::default(), stream::pending().boxed());
		Self {
			tx_hash,
			fused: futures::StreamExt::fuse(stream_map),
			rx,
			terminate: false,
			future_seen: false,
			ready_seen: false,
			breadcast_seen: false,
			views_keeping_tx_valid: Default::default(),
			inblock: Default::default(),
		}
	}

	fn handle(
		&mut self,
		status: &TransactionStatus<TxHash<PoolApi>, BlockHash<PoolApi>>,
		hash: BlockHash<PoolApi>,
	) -> bool {
		// todo: full termination logic: count invalid status events
		// self.terminate = matches!(status,TransactionStatus::Finalized(_));

		trace!(
			target: LOG_TARGET, "[{:?}] handle event from {hash:?}: {status:?} views:{:#?}", self.tx_hash,
			self.fused.get_ref().keys().collect::<Vec<_>>()
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
			TransactionStatus::Broadcast(_) => true,
			TransactionStatus::InBlock((block, _)) => self.inblock.insert(*block),
			TransactionStatus::Retracted(_) => {
				//todo: remove panic
				panic!("retracted? shall not happen")
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
		let keys =
			HashSet::<BlockHash<PoolApi>>::from_iter(self.fused.get_ref().keys().map(Clone::clone));
		trace!(
		target: LOG_TARGET, "[{:?}] got invalidate_transaction: views:{:#?}", self.tx_hash,
		self.fused.get_ref().keys().collect::<Vec<_>>()
		);
		if self.views_keeping_tx_valid.is_disjoint(&keys) {
			self.terminate = true;
			true
		} else {
			false
		}
	}

	fn add_stream(&mut self, block_hash: BlockHash<PoolApi>, stream: TxStatusStream<PoolApi>) {
		trace!(target: LOG_TARGET, "[{:?}] ViewAdded view: {:?} views:{:?}", self.tx_hash, block_hash, self.fused.get_ref().keys().collect::<Vec<_>>());
		self.fused.get_mut().insert(block_hash, stream);
		trace!(target: LOG_TARGET, "[{:?}] after: ViewAdded view: {:?} views:{:?}", self.tx_hash, block_hash, self.fused.get_ref().keys().collect::<Vec<_>>());
	}
}

impl<PoolApi> MultiViewListener<PoolApi>
where
	PoolApi: ChainApi + 'static,
	<<PoolApi as ChainApi>::Block as BlockT>::Hash: Unpin,
{
	pub fn new() -> Self {
		Self { controllers: Default::default() }
	}
	//should be called when tx is first submitted
	//is async needed (bc of rwlock)
	pub(crate) async fn create_external_watcher_for_tx(
		&self,
		tx_hash: TxHash<PoolApi>,
	) -> Option<TxStatusStream<PoolApi>> {
		if self.controllers.read().await.contains_key(&tx_hash) {
			return None;
		}

		trace!(target: LOG_TARGET, "[{:?}] create_external_watcher_for_tx", tx_hash);

		//todo: bounded?
		let (tx, rx) = mpsc::channel(32);
		//todo: controllers cannot grow - remove staff at some point!
		self.controllers.write().await.insert(tx_hash, tx);

		let ctx = ExternalWatcherContext::new(tx_hash, rx);

		Some(
			futures::stream::unfold(ctx, |mut ctx| async move {
				if ctx.terminate {
					return None
				}
				loop {
					tokio::select! {
					biased;
					v =  futures::StreamExt::select_next_some(&mut ctx.fused) => {
						log::trace!(target: LOG_TARGET, "[{:?}] select::map views:{:?}", ctx.tx_hash, ctx.fused.get_ref().keys().collect::<Vec<_>>());
						let (view_hash, status) = v;

						if ctx.handle(&status, view_hash) {
							log::debug!(target: LOG_TARGET, "[{:?}] sending out: {status:?}", ctx.tx_hash);
							return Some((status, ctx));
						}
					},
					cmd = ctx.rx.recv() => {
						log::trace!(target: LOG_TARGET, "[{:?}] select::rx views:{:?}", ctx.tx_hash, ctx.fused.get_ref().keys().collect::<Vec<_>>());
						match cmd {
							Some(ListenerAction::ViewAdded(h,stream)) => {
								ctx.add_stream(h, stream);
							},
							Some(ListenerAction::InvalidateTransaction) => {
								if ctx.handle_invalidate_transaction() {
									log::debug!(target: LOG_TARGET, "[{:?}] sending out: Invalid", ctx.tx_hash);
									return Some((TransactionStatus::Invalid, ctx))
								}
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

	//should be called after submitting tx to every view
	//todo: should be async?
	pub(crate) async fn add_view_watcher_for_tx(
		&self,
		tx_hash: TxHash<PoolApi>,
		block_hash: BlockHash<PoolApi>,
		stream: TxStatusStream<PoolApi>,
	) {
		let mut controllers = self.controllers.write().await;
		if let Some(tx) = controllers.get(&tx_hash) {
			match tx.send(ListenerAction::ViewAdded(block_hash, stream)).await {
				Err(mpsc::error::SendError(e)) => {
					trace!(target: LOG_TARGET, "[{:?}] add_view_watcher_for_tx: SendError: {:?}", tx_hash, e);
					controllers.remove(&tx_hash);
				},
				Ok(_) => {},
			}
		}
	}

	pub(crate) async fn invalidate_transactions(&self, invalid_hashes: Vec<TxHash<PoolApi>>) {
		use futures::future::FutureExt;
		let mut controllers = self.controllers.write().await;

		let futs = invalid_hashes.into_iter().filter_map(|tx_hash| {
			if let Some(tx) = controllers.get(&tx_hash) {
				trace!(target: LOG_TARGET, "[{:?}] invalidate_transaction", tx_hash);
				Some(
					tx.send(ListenerAction::InvalidateTransaction)
						.map(move |result| (result, tx_hash)),
				)
			} else {
				None
			}
		});

		futures::future::join_all(futs)
			.await
			.into_iter()
			.for_each(|result| match result.0 {
				Err(mpsc::error::SendError(e)) => {
					trace!(target: LOG_TARGET, "invalidate_transaction: SendError: {:?}", e);
					controllers.remove(&result.1);
				},
				Ok(_) => {},
			});
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use futures::StreamExt;
	use sp_core::H256;

	type MultiViewListener = super::MultiViewListener<crate::tests::TestApi>;

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
		let external_watcher = listener.create_external_watcher_for_tx(tx_hash).await.unwrap();

		let view_stream = futures::stream::iter(events.clone());

		listener.add_view_watcher_for_tx(tx_hash, block_hash, view_stream.boxed()).await;

		let out = external_watcher.collect::<Vec<_>>().await;
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
		let external_watcher = listener.create_external_watcher_for_tx(tx_hash).await.unwrap();

		let view_stream0 = futures::stream::iter(events0.clone());
		let view_stream1 = futures::stream::iter(events1.clone());

		listener
			.add_view_watcher_for_tx(tx_hash, block_hash0, view_stream0.boxed())
			.await;
		listener
			.add_view_watcher_for_tx(tx_hash, block_hash1, view_stream1.boxed())
			.await;

		let out = external_watcher.collect::<Vec<_>>().await;
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
		let external_watcher = listener.create_external_watcher_for_tx(tx_hash).await.unwrap();

		let view_stream0 = futures::stream::iter(events0.clone());
		let view_stream1 = futures::stream::iter(events1.clone());

		listener
			.add_view_watcher_for_tx(tx_hash, block_hash0, view_stream0.boxed())
			.await;
		listener
			.add_view_watcher_for_tx(tx_hash, block_hash1, view_stream1.boxed())
			.await;

		listener.invalidate_transactions(vec![tx_hash]).await;

		let out = external_watcher.collect::<Vec<_>>().await;
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
		let external_watcher_tx0 = listener.create_external_watcher_for_tx(tx0_hash).await.unwrap();
		let external_watcher_tx1 = listener.create_external_watcher_for_tx(tx1_hash).await.unwrap();

		let view0_tx0_stream = futures::stream::iter(events0_tx0.clone());
		let view0_tx1_stream = futures::stream::iter(events0_tx1.clone());

		let view1_tx0_stream = futures::stream::iter(events1_tx0.clone());
		let view1_tx1_stream = futures::stream::iter(events1_tx1.clone());

		listener
			.add_view_watcher_for_tx(tx0_hash, block_hash0, view0_tx0_stream.boxed())
			.await;
		listener
			.add_view_watcher_for_tx(tx0_hash, block_hash1, view1_tx0_stream.boxed())
			.await;
		listener
			.add_view_watcher_for_tx(tx1_hash, block_hash0, view0_tx1_stream.boxed())
			.await;
		listener
			.add_view_watcher_for_tx(tx1_hash, block_hash1, view1_tx1_stream.boxed())
			.await;

		listener.invalidate_transactions(vec![tx0_hash]).await;
		listener.invalidate_transactions(vec![tx1_hash]).await;

		let out_tx0 = external_watcher_tx0.collect::<Vec<_>>().await;
		let out_tx1 = external_watcher_tx1.collect::<Vec<_>>().await;
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
		let external_watcher = listener.create_external_watcher_for_tx(tx_hash).await.unwrap();

		let view_stream0 = futures::stream::iter(events0.clone()).chain(stream::pending().boxed());
		let view_stream1 = futures::stream::iter(events1.clone()).chain(stream::pending().boxed());

		listener
			.add_view_watcher_for_tx(tx_hash, block_hash0, view_stream0.boxed())
			.await;
		listener
			.add_view_watcher_for_tx(tx_hash, block_hash1, view_stream1.boxed())
			.await;

		listener.invalidate_transactions(vec![tx_hash]).await;

		// stream is pending, we need to fetch 3 events
		let out = external_watcher.take(3).collect::<Vec<_>>().await;
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
		let external_watcher = listener.create_external_watcher_for_tx(tx_hash).await.unwrap();

		let view_stream0 = futures::stream::iter(events0.clone()).chain(stream::pending().boxed());

		// Note: this generates actual Invalid event.
		// Invalid event from View's stream is intentionally ignored.
		listener.invalidate_transactions(vec![tx_hash]).await;

		listener
			.add_view_watcher_for_tx(tx_hash, block_hash0, view_stream0.boxed())
			.await;

		let out = external_watcher.collect::<Vec<_>>().await;
		log::info!("out: {:#?}", out);

		assert!(out.iter().all(|v| vec![TransactionStatus::Invalid].contains(v)));
		assert_eq!(out.len(), 1);
	}
}
