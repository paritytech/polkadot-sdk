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

//! Bitswap-based fetcher for indexed-transaction blobs.
//!
//! Thin adapter over [`sc_network_bitswap::BitswapHandle`]: builds the per-want CIDs,
//! submits them in chunks of [`MAX_CIDS_PER_REQUEST`] and collects the outcomes. Peer
//! selection, timeouts, retries and hash verification live in the bitswap service.

use crate::RenewWant;
use cid::{multihash::Multihash, Cid};
use futures::future;
use sc_network_bitswap::{BitswapError, BitswapRequest, FetchOutcome, MAX_CIDS_PER_REQUEST};
use sp_runtime::traits::Block as BlockT;
use sp_transaction_storage_proof::ContentHash;
use std::{
	collections::HashMap,
	sync::{Arc, OnceLock},
};

const LOG_TARGET: &str = "storage-chain-fetcher";

/// Late-bound bitswap handle slot, populated by the node after `build_network`.
///
/// [`crate::StorageChainBlockImport`] is constructed before `build_network` runs; the
/// `OnceLock` carries the handle across that boundary.
pub type BitswapHandleSlot = Arc<OnceLock<Arc<dyn BitswapRequest>>>;

/// Infrastructure-level fetch failure surfaced to [`crate::StorageChainBlockImport`].
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
	/// The bitswap handle has not been set, either because `build_network` has not finished
	/// yet or because bitswap is not configured (`--ipfs-server` not enabled).
	#[error("bitswap handle not yet set; storage-chain blocks cannot be fetched before build_network completes")]
	BitswapHandleUnset,
	/// CID construction failed for the given (hashing, hash) pair.
	#[error("failed to construct multihash for CID: {0}")]
	Multihash(String),
	/// The bitswap service rejected the request at admission, or shut down mid-stream.
	#[error("bitswap service error: {0}")]
	Bitswap(#[from] BitswapError),
}

/// Fetcher that resolves indexed-transaction hashes via bitswap.
///
/// Holds the late-bound [`BitswapRequest`] slot. The block-import path holds one
/// of these and calls [`Self::fetch_many`] for each batch of missing renew hashes.
///
/// Cloning is cheap: the only field is an `Arc`.
pub struct IndexedTransactionFetcher<Block: BlockT> {
	bitswap: BitswapHandleSlot,
	_phantom: std::marker::PhantomData<Block>,
}

impl<Block: BlockT> Clone for IndexedTransactionFetcher<Block> {
	fn clone(&self) -> Self {
		Self { bitswap: self.bitswap.clone(), _phantom: std::marker::PhantomData }
	}
}

impl<Block: BlockT> IndexedTransactionFetcher<Block> {
	/// Build a new fetcher backed by the given late-bound bitswap handle slot.
	pub fn new(bitswap: BitswapHandleSlot) -> Self {
		Self { bitswap, _phantom: std::marker::PhantomData }
	}

	/// Resolve a batch of indexed-transaction hashes via bitswap. Each want carries the
	/// runtime-declared `cid_codec` so the request CID matches what the producing runtime
	/// announced. Returns only successfully fetched entries.
	pub(crate) async fn fetch_many(
		&self,
		wants: &[RenewWant],
	) -> Result<HashMap<ContentHash, Vec<u8>>, FetchError> {
		if wants.is_empty() {
			return Ok(HashMap::new());
		}
		let handle = self.bitswap.get().ok_or(FetchError::BitswapHandleUnset)?;

		let mut by_cid: HashMap<Cid, ContentHash> = HashMap::with_capacity(wants.len());
		let mut cids: Vec<Cid> = Vec::with_capacity(wants.len());
		for want in wants {
			let mh = Multihash::<64>::wrap(want.hashing.multihash_code(), &want.hash)
				.map_err(|e| FetchError::Multihash(e.to_string()))?;
			let cid = Cid::new_v1(want.cid_codec, mh);
			by_cid.insert(cid, want.hash);
			cids.push(cid);
		}

		let receivers = future::try_join_all(
			cids.chunks(MAX_CIDS_PER_REQUEST)
				.map(|chunk| handle.request_stream(chunk.to_vec())),
		)
		.await?;

		// Every receiver is sized to buffer all its outcomes, so the requests progress
		// concurrently regardless of drain order.
		let mut acquired: HashMap<ContentHash, Vec<u8>> = HashMap::with_capacity(wants.len());
		for mut rx in receivers {
			while let Some(item) = rx.recv().await {
				match item {
					Ok((cid, FetchOutcome::Block(bytes))) => {
						if let Some(hash) = by_cid.get(&cid) {
							log::debug!(
								target: LOG_TARGET,
								"bitswap fetched {} bytes for {hash:?}",
								bytes.len(),
							);
							acquired.insert(*hash, bytes);
						}
					},
					Ok((cid, FetchOutcome::Missing)) => {
						log::debug!(target: LOG_TARGET, "bitswap returned Missing for {cid}");
					},
					Err(BitswapError::ServiceClosed) => {
						log::warn!(
							target: LOG_TARGET,
							"bitswap service closed mid-stream; returning partial result",
						);
						return Ok(acquired);
					},
					Err(other) => return Err(FetchError::Bitswap(other)),
				}
			}
		}

		Ok(acquired)
	}
}
