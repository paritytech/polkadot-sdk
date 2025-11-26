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

use cid::multihash::Code as Code;
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

	/// Supported multihash codes for CID validation.
	/// Uses `Vec` instead of `HashSet` or `BTreeSet` because `cid::multihash::Code` doesn't implement
	/// `Hash` or `Ord`. O(n) lookup is acceptable in our case (3 values).
	supported_hash_codes: Vec<Code>,
}

impl<Block: BlockT> BitswapServer<Block> {
	/// Create new [`BitswapServer`].
	pub fn new(
		client: Arc<dyn BlockBackend<Block> + Send + Sync>,
	) -> (Pin<Box<dyn Future<Output = ()> + Send>>, Config) {
		let (config, handle) = Config::new();
		let supported_hash_codes = vec![
			Code::Blake2b256,
			Code::Sha2_256,
			Code::Keccak256,
		];
		let code_names: Vec<&str> = supported_hash_codes.iter().map(|c| Self::code_to_name(c)).collect();
		log::debug!(
			target: LOG_TARGET,
			"BitswapServer initialized with supported multihash codes: {:?}",
			code_names
		);
		let bitswap = Self { client, handle, supported_hash_codes };

		(Box::pin(async move { bitswap.run().await }), config)
	}

	/// Convert a multihash code to its name.
	fn code_to_name(code: &Code) -> &'static str {
		match code {
			Code::Blake2b256 => "Blake2b256",
			Code::Sha2_256 => "Sha2_256",
			Code::Keccak256 => "Keccak256",
			_ => "Unknown",
		}
	}

	async fn run(mut self) {
		log::debug!(target: LOG_TARGET, "starting bitswap server");

		while let Some(event) = self.handle.next().await {
			match event {
				BitswapEvent::Request { peer, cids } => {
					log::debug!(target: LOG_TARGET, "handle bitswap request from {peer:?} for {cids:?}");

					let response: Vec<ResponseType> = cids
						.into_iter()
						.filter(|(cid, _)| {
							let version_num: u64 = cid.version().into();
							let size = cid.hash().size() as usize;
							let code = cid.hash().code();
							let cid_str = cid.to_string();
							self.is_valid_cid(version_num, size, code, cid_str)
						})
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

	/// Takes extracted values instead of the CID directly to avoid version conflicts:
	/// `litep2p` (direct dependency) uses `cid = "0.9.0"`, which exports `CidGeneric<64>`
	/// This crate (`sc-network`) directly depends on `cid = "0.11.1"`, which exports `Cid<64>`
	/// Those (^) are different types (even though they're the same thing) and cause type conflicts.
	fn is_valid_cid(&self, version_num: u64, size: usize, code: u64, cid_str: String) -> bool {
		if version_num == 0 {
			log::trace!(
				target: LOG_TARGET,
				"Unsupported CID version for cid: {cid_str}"
			);
			return false;
		}

		if size != 32 {
			log::warn!(
				target: LOG_TARGET,
				"Unsupported multihash size: {size} for cid: {cid_str}, supports only 32!"
			);
			return false;
		}

		// Convert u64 code to Code enum for comparison
		let code_enum = match Code::try_from(code) {
			Ok(c) => c,
			Err(_) => {
				log::warn!(
					target: LOG_TARGET,
					"Unknown multihash algorithm code: {code} for cid: {cid_str}"
				);
				return false;
			},
		};

		if !self.supported_hash_codes.contains(&code_enum) {
			let supported_names: Vec<&str> =
				self.supported_hash_codes.iter().map(|c| Self::code_to_name(c)).collect();
			log::warn!(
				target: LOG_TARGET,
				"Unsupported multihash algorithm: {} ({code}) for cid: {cid_str}, supports only {:?}!",
				Self::code_to_name(&code_enum),
				supported_names
			);
			return false;
		}

		true
	}
}
