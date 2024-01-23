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
use std::{collections::HashMap, pin::Pin};
use tokio::sync::mpsc;
use tokio_stream::StreamMap;

pub type TxStatusStream<T> = Pin<Box<TransactionStatusStream<TxHash<T>, BlockHash<T>>>>;

enum ViewEvent<PoolApi: ChainApi> {
	ViewAdded(BlockHash<PoolApi>, TxStatusStream<PoolApi>),
}

pub struct MultiViewListener<PoolApi: ChainApi> {
	//todo: rwlock not needed here (mut?)
	controllers: tokio::sync::RwLock<HashMap<TxHash<PoolApi>, mpsc::Sender<ViewEvent<PoolApi>>>>,
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

		let (tx, rx) = mpsc::channel(32);
		self.controllers.write().await.insert(tx_hash, tx);

		let mut stream_map: StreamMap<BlockHash<PoolApi>, TxStatusStream<PoolApi>> =
			StreamMap::new();
		stream_map.insert(Default::default(), stream::pending().boxed());
		let fused = futures::StreamExt::fuse(stream_map);

		Some(
			futures::stream::unfold(
				(fused, rx, false),
				|(mut fused, mut rx, terminate)| async move {
					if terminate {
						return None
					}
					loop {
						tokio::select! {
						biased;
						v =  futures::StreamExt::select_next_some(&mut fused) => {
							trace!(
								target: LOG_TARGET, "got value: {v:#?} streams:{:#?}",
								fused.get_ref().keys().collect::<Vec<_>>()
							);
							let (hash, status) = v;

							// todo: full termination logic: count invalid status events
							let terminate = matches!(status,TransactionStatus::Finalized(_));
							return Some((status, (fused, rx, terminate)));
						}
						cmd = rx.recv() => {
							if let Some(ViewEvent::ViewAdded(h,stream)) = cmd {
								trace!(target: LOG_TARGET, "create_external_watcher_for_tx: got viewEvent {:#?}", h);
								fused.get_mut().insert(h, stream);
							}
						},
						};
					}
				},
			)
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
		if let Some(tx) = self.controllers.write().await.get(&tx_hash) {
			//todo: unwrap / error handling
			trace!(target: LOG_TARGET, "add_view_watcher_for_tx {:#?}: sent viewEvent", tx_hash);
			tx.send(ViewEvent::ViewAdded(block_hash, stream)).await.unwrap();
		}
	}
}
