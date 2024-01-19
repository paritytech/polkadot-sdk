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

use futures::{stream, StreamExt};
use log::info;
use sc_transaction_pool_api::{BlockHash, TransactionPool, TxHash};
use sp_runtime::traits::{Block as BlockT, Extrinsic, Hash as HashT};
use std::{collections::HashMap, pin::Pin};
use tokio::sync::mpsc;
use tokio_stream::StreamMap;

type TxStatusStream<T> =
	Pin<Box<sc_transaction_pool_api::TransactionStatusStream<TxHash<T>, BlockHash<T>>>>;

enum ViewEvent<PoolApi: TransactionPool> {
	ViewAdded(BlockHash<PoolApi>, TxStatusStream<PoolApi>),
}

pub struct MultiViewListener<PoolApi: TransactionPool> {
	controllers: HashMap<TxHash<PoolApi>, mpsc::Sender<ViewEvent<PoolApi>>>,
}

impl<PoolApi> MultiViewListener<PoolApi>
where
	PoolApi: TransactionPool + 'static,
	<<PoolApi as TransactionPool>::Block as BlockT>::Hash: Unpin,
{
	pub fn new() -> Self {
		Self { controllers: Default::default() }
	}
	//should be called when tx is first submitted
	pub(crate) fn create_external_watcher_for_tx(
		&mut self,
		tx_hash: TxHash<PoolApi>,
	) -> Option<TxStatusStream<PoolApi>> {
		if self.controllers.contains_key(&tx_hash) {
			return None;
		}

		let (tx, rx) = mpsc::channel(32);
		self.controllers.insert(tx_hash, tx);

		let mut stream_map: StreamMap<BlockHash<PoolApi>, TxStatusStream<PoolApi>> =
			StreamMap::new();
		stream_map.insert(Default::default(), stream::pending().boxed());
		let fused = futures::StreamExt::fuse(stream_map);

		Some(
			futures::stream::unfold((fused, rx), move |(mut fused, mut rx)| async move {
				loop {
					tokio::select! {
					cmd = rx.recv() => {
						if let Some(ViewEvent::ViewAdded(h,stream)) = cmd {
							fused.get_mut().insert(h, stream);
						}
					},
					v =  futures::StreamExt::select_next_some(&mut fused) => {
						info!(
							"got value: {v:#?} streams:{:#?}",
							fused.get_ref().keys().collect::<Vec<_>>()
						);
						return Some((v.1, (fused, rx)));
					}
					};
				}
			})
			.boxed(),
		)
	}

	//should be called after submitting tx to every view
	pub(crate) async fn add_view_watcher_for_tx(
		&self,
		tx_hash: TxHash<PoolApi>,
		block_hash: BlockHash<PoolApi>,
		stream: TxStatusStream<PoolApi>,
	) {
		if let Some(tx) = self.controllers.get(&tx_hash) {
			//todo: unwrap / error handling
			tx.send(ViewEvent::ViewAdded(block_hash, stream)).await.unwrap();
		}
	}
}
