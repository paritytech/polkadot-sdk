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

use crate::bitswap::is_cid_supported;
use futures::StreamExt;
use litep2p::protocol::libp2p::bitswap::{
	BitswapEvent, BitswapHandle, BlockPresenceType, Config, ResponseType, WantType,
};

use sc_client_api::BlockBackend;
use sp_runtime::traits::Block as BlockT;

use std::{future::Future, pin::Pin, sync::Arc};

/// Logging target for the file.
const LOG_TARGET: &str = "sub-libp2p::bitswap";

/// Maximum payload size for batching bitswap responses (8 MB).
/// We use a slightly smaller value than the wire limit to account for protobuf encoding overhead.
const MAX_RESPONSE_SIZE: usize = 7_800_000;

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

					let responses: Vec<ResponseType> = cids
						.into_iter()
						.filter(|(cid, _)| is_cid_supported(&cid))
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

					// Batch responses to avoid exceeding the bitswap message size limit.
					// Each batch is sent as a separate message.
					let mut current_batch: Vec<ResponseType> = Vec::new();
					let mut current_size: usize = 0;

					for response in responses {
						let response_size = match &response {
							ResponseType::Block { block, .. } => block.len() + 64, // block data + CID overhead
							ResponseType::Presence { .. } => 64, // CID + presence type
						};

						// If adding this response would exceed the limit, send current batch first
						if !current_batch.is_empty() && current_size + response_size > MAX_RESPONSE_SIZE
						{
							log::trace!(
								target: LOG_TARGET,
								"sending bitswap batch of {} responses ({} bytes) to {peer:?}",
								current_batch.len(),
								current_size
							);
							self.handle.send_response(peer, std::mem::take(&mut current_batch)).await;
							current_size = 0;
						}

						current_size += response_size;
						current_batch.push(response);
					}

					// Send any remaining responses
					if !current_batch.is_empty() {
						log::trace!(
							target: LOG_TARGET,
							"sending final bitswap batch of {} responses ({} bytes) to {peer:?}",
							current_batch.len(),
							current_size
						);
						self.handle.send_response(peer, current_batch).await;
					}
				},
			}
		}
	}
}
