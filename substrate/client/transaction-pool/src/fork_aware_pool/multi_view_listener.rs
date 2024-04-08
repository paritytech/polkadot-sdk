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

//! Multi view listener. Combines streams from many views into single pool watcher.

const LOG_TARGET: &str = "txpool::mvlistener";

use crate::graph::{BlockHash, ChainApi, ExtrinsicHash as TxHash};
use futures::{stream, StreamExt};
use log::trace;
use sc_transaction_pool_api::{TransactionStatus, TransactionStatusStream};
use sp_runtime::traits::{Block as BlockT, Extrinsic, Hash as HashT};
use std::{
	collections::{HashMap, HashSet},
	pin::Pin,
	sync::TryLockError,
};
use tokio::sync::mpsc;
use tokio_stream::StreamMap;

pub type TxStatusStream<T> = Pin<Box<TransactionStatusStream<TxHash<T>, BlockHash<T>>>>;

enum ViewEvent<PoolApi: ChainApi> {
	ViewAdded(BlockHash<PoolApi>, TxStatusStream<PoolApi>),
	Invalid,
}

impl<PoolApi> std::fmt::Debug for ViewEvent<PoolApi>
where
	PoolApi: ChainApi,
{
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			ViewEvent::ViewAdded(h, _) => write!(f, "ViewEvent::ViewAdded({})", h),
			ViewEvent::Invalid => write!(f, "ViewEvent::Invalid"),
		}
	}
}

pub struct MultiViewListener<PoolApi: ChainApi> {
	//todo: rwlock not needed here (mut?)
	controllers: tokio::sync::RwLock<HashMap<TxHash<PoolApi>, mpsc::Sender<ViewEvent<PoolApi>>>>,
}

struct ExternalWatcherContext<PoolApi: ChainApi> {
	fused: futures::stream::Fuse<StreamMap<BlockHash<PoolApi>, TxStatusStream<PoolApi>>>,
	rx: mpsc::Receiver<ViewEvent<PoolApi>>,
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
	fn new(rx: mpsc::Receiver<ViewEvent<PoolApi>>) -> Self {
		let mut stream_map: StreamMap<BlockHash<PoolApi>, TxStatusStream<PoolApi>> =
			StreamMap::new();
		stream_map.insert(Default::default(), stream::pending().boxed());
		Self {
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
		trace!(target: LOG_TARGET, "create_external_watcher_for_tx: 1: {}", tx_hash);
		if self.controllers.read().await.contains_key(&tx_hash) {
			return None;
		}
		trace!(target: LOG_TARGET, "create_external_watcher_for_tx: 2: {}", tx_hash);

		//todo: bounded?
		let (tx, rx) = mpsc::channel(32);
		self.controllers.write().await.insert(tx_hash, tx);

		let ctx = ExternalWatcherContext::new(rx);

		Some(
			futures::stream::unfold(ctx, |mut ctx| async move {
				if ctx.terminate {
					return None
				}
				loop {
					tokio::select! {
					biased;
					v =  futures::StreamExt::select_next_some(&mut ctx.fused) => {
						trace!(
							target: LOG_TARGET, "got value: {v:#?} streams:{:#?}",
							ctx.fused.get_ref().keys().collect::<Vec<_>>()
						);
						let (hash, status) = v;

						if ctx.handle(&status, hash) {
							return Some((status, ctx));
						}
					},
					cmd = ctx.rx.recv() => {
						match cmd {
							Some(ViewEvent::ViewAdded(h,stream)) => {
								trace!(target: LOG_TARGET, "got viewEvent added {:#?}", h);
								ctx.fused.get_mut().insert(h, stream);
							},
							Some(ViewEvent::Invalid) => {
								let keys = HashSet::<BlockHash<PoolApi>>::from_iter(ctx.fused.get_ref().keys().map(Clone::clone));
								trace!(
									target: LOG_TARGET, "got Invalid: streams:{:#?}",
									ctx.fused.get_ref().keys().collect::<Vec<_>>()
									);
								if ctx.views_keeping_tx_valid.is_disjoint(&keys) {
									ctx.terminate = true;
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
			trace!(target: LOG_TARGET, "add_view_watcher_for_tx {:#?}: sent viewEvent", tx_hash);
			match tx.send(ViewEvent::ViewAdded(block_hash, stream)).await {
				Err(mpsc::error::SendError(e)) => {
					trace!(target: LOG_TARGET, "add_view_watcher_for_tx: SendError: {:?}", e);
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
				trace!(target: LOG_TARGET, "invalidate_transaction {:#?}", tx_hash);
				Some(tx.send(ViewEvent::Invalid).map(move |result| (result, tx_hash)))
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
