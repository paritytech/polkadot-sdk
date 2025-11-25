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

use cid::multihash::Code;
use sc_client_api::BlockBackend;
use sp_runtime::traits::Block as BlockT;

use std::{collections::HashSet, future::Future, pin::Pin, sync::Arc};

/// Logging target for the file.
const LOG_TARGET: &str = "sub-libp2p::bitswap";

pub struct BitswapServer<Block: BlockT> {
	/// Bitswap handle.
	handle: BitswapHandle,

	/// Blockchain client.
	client: Arc<dyn BlockBackend<Block> + Send + Sync>,

	/// Supported multihash codes for CID validation.
    supported_hash_codes: HashSet<u64>,
}

impl<Block: BlockT> BitswapServer<Block> {
	/// Convert a multihash code to its name.
	fn code_to_name(code: u64) -> &'static str {
		match code {
			c if c == u64::from(Code::Blake2b256) => "Blake2b256",
			c if c == u64::from(Code::Sha2_256) => "Sha2_256",
			c if c == u64::from(Code::Keccak256) => "Keccak256",
			_ => "Unknown",
		}
	}

	/// Validate a CID and return None if it's invalid.
	fn validate_cid(&self, cid: &cid::Cid) -> Option<()> {
		let version_num: u64 = cid.version().into();
		if version_num == 0 {
			log::trace!(
				target: LOG_TARGET,
				"Unsupported CID version {:?} for cid: {cid}",
				cid.version()
			);
			return None;
		}

		let size = cid.hash().size();
		if size != 32 {
			log::warn!(
				target: LOG_TARGET,
				"Unsupported multihash size: {size} for cid: {cid}, supports only 32!"
			);
			return None;
		}

		let code = cid.hash().code();
		if !self.supported_hash_codes.contains(&code) {
			let supported_names: Vec<&str> =
				self.supported_hash_codes.iter().map(|&c| Self::code_to_name(c)).collect();
			log::warn!(
				target: LOG_TARGET,
				"Unsupported multihash algorithm: {} ({code}) for cid: {cid}, supports only {:?}!",
				Self::code_to_name(code),
				supported_names
			);
			return None;
		}

		Some(())
	}

	/// Create new [`BitswapServer`].
	pub fn new(
		client: Arc<dyn BlockBackend<Block> + Send + Sync>,
	) -> (Pin<Box<dyn Future<Output = ()> + Send>>, Config) {
		let (config, handle) = Config::new();
		let supported_hash_codes = HashSet::from([
			u64::from(Code::Blake2b256),
			u64::from(Code::Sha2_256),
			u64::from(Code::Keccak256),
		]);
		let code_names: Vec<&str> = supported_hash_codes.iter().map(|&c| Self::code_to_name(c)).collect();
		log::debug!(
			target: LOG_TARGET,
			"BitswapServer initialized with supported multihash codes: {:?}",
			code_names
		);
		let bitswap = Self { client, handle, supported_hash_codes };

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
						.filter_map(|(cid, want_type)| {
							
							// Validate and filter out invalid CIDs before processing.
							self.validate_cid(&cid)?;
			
							let mut hash = Block::Hash::default();
							hash.as_mut().copy_from_slice(&cid.hash().digest()[0..32]);
							let transaction = match self.client.indexed_transaction(hash) {
								Ok(ex) => ex,
								Err(error) => {
									log::error!(target: LOG_TARGET, "error retrieving transaction {hash}: {error}");
									None
								},
							};

							Some(match transaction {
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
							})
						})
						.collect();

					self.handle.send_response(peer, response).await;
				},
			}
		}
	}
}
