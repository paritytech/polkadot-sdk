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
use prometheus_endpoint::{register, Counter, PrometheusError, Registry, U64};

use sc_client_api::BlockBackend;
use sp_runtime::traits::Block as BlockT;

use std::{future::Future, pin::Pin, sync::Arc};

/// Logging target for the file.
const LOG_TARGET: &str = "sub-libp2p::bitswap";

/// Prometheus metrics for the Bitswap server.
#[derive(Clone)]
struct Metrics {
	/// Total incoming requests.
	requests_received: Counter<U64>,
	/// Total CIDs requested.
	cids_requested: Counter<U64>,
	/// Blocks found and sent.
	blocks_sent: Counter<U64>,
	/// Bytes of block data sent.
	blocks_sent_bytes: Counter<U64>,
	/// CIDs not found (DontHave responses).
	blocks_not_found: Counter<U64>,
}

impl Metrics {
	fn register(registry: &Registry) -> Result<Self, PrometheusError> {
		Ok(Self {
			requests_received: register(
				Counter::new(
					"substrate_bitswap_requests_received_total",
					"Total number of Bitswap requests received",
				)?,
				registry,
			)?,
			cids_requested: register(
				Counter::new(
					"substrate_bitswap_cids_requested_total",
					"Total number of CIDs requested via Bitswap",
				)?,
				registry,
			)?,
			blocks_sent: register(
				Counter::new(
					"substrate_bitswap_blocks_sent_total",
					"Total number of blocks sent via Bitswap",
				)?,
				registry,
			)?,
			blocks_sent_bytes: register(
				Counter::new(
					"substrate_bitswap_blocks_sent_bytes_total",
					"Total bytes of block data sent via Bitswap",
				)?,
				registry,
			)?,
			blocks_not_found: register(
				Counter::new(
					"substrate_bitswap_blocks_not_found_total",
					"Total number of CIDs not found (DontHave responses)",
				)?,
				registry,
			)?,
		})
	}
}

pub struct BitswapServer<Block: BlockT> {
	/// Bitswap handle.
	handle: BitswapHandle,

	/// Blockchain client.
	client: Arc<dyn BlockBackend<Block> + Send + Sync>,

	/// Prometheus metrics.
	metrics: Option<Metrics>,
}

impl<Block: BlockT> BitswapServer<Block> {
	/// Create new [`BitswapServer`].
	pub fn new(
		client: Arc<dyn BlockBackend<Block> + Send + Sync>,
		prometheus_registry: Option<&Registry>,
	) -> (Pin<Box<dyn Future<Output = ()> + Send>>, Config) {
		let (config, handle) = Config::new();
		let metrics = prometheus_registry
			.map(Metrics::register)
			.transpose()
			.unwrap_or_else(|e| {
				log::warn!(target: LOG_TARGET, "Failed to register bitswap metrics: {e}");
				None
			});
		let bitswap = Self { client, handle, metrics };

		(Box::pin(async move { bitswap.run().await }), config)
	}

	async fn run(mut self) {
		log::debug!(target: LOG_TARGET, "starting bitswap server");

		while let Some(event) = self.handle.next().await {
			match event {
				BitswapEvent::Request { peer, cids } => {
					log::debug!(target: LOG_TARGET, "handle bitswap request from {peer:?} for {cids:?}");

					if let Some(ref metrics) = self.metrics {
						metrics.requests_received.inc();
						metrics.cids_requested.inc_by(cids.len() as u64);
					}

					let response: Vec<ResponseType> = cids
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

									if let Some(ref metrics) = self.metrics {
										match want_type {
											WantType::Block => {
												metrics.blocks_sent.inc();
												metrics
													.blocks_sent_bytes
													.inc_by(transaction.len() as u64);
											},
											_ => {},
										}
									}

									match want_type {
										WantType::Block => {
											ResponseType::Block { cid, block: transaction }
										},
										_ => ResponseType::Presence {
											cid,
											presence: BlockPresenceType::Have,
										},
									}
								},
								None => {
									log::trace!(target: LOG_TARGET, "missing cid {cid:?}, hash {hash:?}");

									if let Some(ref metrics) = self.metrics {
										metrics.blocks_not_found.inc();
									}

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
				BitswapEvent::Response { peer, responses } => {
					// We're a server, not a client - ignore incoming responses
					log::trace!(
						target: LOG_TARGET,
						"ignoring bitswap response from {peer:?} with {} entries",
						responses.len()
					);
				},
			}
		}
	}
}
