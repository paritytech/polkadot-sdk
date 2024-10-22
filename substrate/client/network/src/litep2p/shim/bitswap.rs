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

//! Shim for litep2p's Bitswap implementation to make it work with `sc-network`.

use futures::StreamExt;
use litep2p::protocol::libp2p::bitswap::{
	BitswapEvent, BitswapHandle, BlockPresenceType, Config, ResponseType, WantType,
};

use sc_client_api::BlockBackend;
use sp_runtime::traits::Block as BlockT;

use std::{future::Future, pin::Pin, sync::Arc};

/// Logging target for the file.
const LOG_TARGET: &str = "sub-libp2p::bitswap";

pub struct BitswapServer<Block: BlockT> {
	/// Bitswap handle.
	handle: BitswapHandle,

	/// Blockchain client.
	client: Arc<dyn BlockBackend<Block> + Send + Sync>,
}

impl<Block: BlockT> BitswapServer<Block> {
	/// Create new [`BitswapServer`].
	pub fn new(
		client: Arc<dyn BlockBackend<Block> + Send + Sync>,
	) -> (Pin<Box<dyn Future<Output = ()> + Send>>, Config) {
		let (config, handle) = Config::new();
		let bitswap = Self { client, handle };

		(Box::pin(async move { bitswap.run().await }), config)
	}

	async fn run(mut self) {
		log::debug!(target: LOG_TARGET, "starting bitswap server");

		while let Some(event) = self.handle.next().await {
			match event {
				BitswapEvent::Request { peer, cids } => {
					log::debug!(target: LOG_TARGET, "handle bitswap request from {peer:?} for {cids:?}");

					let response: Vec<ResponseType> = cids
						.into_iter()
						.map(|(cid, want_type)| {
							let mut hash = Block::Hash::default();
							hash.as_mut().copy_from_slice(&cid.hash().digest()[0..32]);
							let transaction = match self.client.indexed_transaction(hash) {
								Ok(ex) => ex,
								Err(error) => {
									log::error!(target: LOG_TARGET, "error retrieving transaction {hash}: {error}");
									None
								},
							};

							match transaction {
								Some(transaction) => {
									log::trace!(target: LOG_TARGET, "found cid {cid:?}, hash {hash:?}");

									match want_type {
										WantType::Block =>
											ResponseType::Block { cid, block: transaction },
										_ => ResponseType::Presence {
											cid,
											presence: BlockPresenceType::Have,
										},
									}
								},
								None => {
									log::trace!(target: LOG_TARGET, "missing cid {cid:?}, hash {hash:?}");

									ResponseType::Presence {
										cid,
										presence: BlockPresenceType::DontHave,
									}
								},
							}
						})
						.collect();

					self.handle.send_response(peer, response).await;
				},
			}
		}
	}
}
