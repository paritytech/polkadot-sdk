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

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Bitswap-based fetcher for indexed-transaction blobs.
//!
//! Thin adapter over [`sc_network::bitswap::BitswapHandle`]. Peer selection,
//! per-peer timeouts, retries and hash verification all live in the bitswap actor
//! itself. This fetcher's only jobs are:
//!
//! 1. building the per-want CIDs from the runtime-declared (hash, hashing, codec)
//!    triples,
//! 2. chunking by [`sc_network::bitswap::MAX_CIDS_PER_REQUEST`] and submitting all
//!    chunks concurrently,
//! 3. draining the per-chunk streams into a `HashMap<ContentHash, Vec<u8>>`.

use cid::{multihash::Multihash, Cid};
use futures::{future, stream::FuturesUnordered, StreamExt};
use sc_network::bitswap::{
	BitswapError, BitswapRequest, FetchOutcome, MAX_CIDS_PER_REQUEST,
};
use sp_runtime::traits::Block as BlockT;
use sp_transaction_storage_proof::{ContentHash, HashingAlgorithm};
use std::{
	collections::HashMap,
	sync::{Arc, OnceLock},
};

const LOG_TARGET: &str = "storage-chain-fetcher";

/// Late-bound bitswap handle slot, populated by the omni-node after `build_network`.
///
/// The slot exists because [`crate::StorageChainBlockImport`] is constructed before
/// `build_network` runs (the block import is consumed when building the import queue,
/// which is in turn consumed by `build_network`). The handle becomes available only
/// once the network service has been built; the `OnceLock` carries it across that
/// boundary without changing the block-import's public API.
pub type BitswapHandleSlot = Arc<OnceLock<Arc<dyn BitswapRequest>>>;

/// Infrastructure-level fetch failure surfaced to [`crate::StorageChainBlockImport`].
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
	/// The bitswap handle has not been set yet (called before `build_network` finished)
	/// or bitswap is not configured (`--ipfs-server` not enabled, or libp2p backend in use).
	#[error("bitswap handle not yet set; storage-chain blocks cannot be fetched before build_network completes")]
	BitswapHandleUnset,
	/// CID construction failed for the given (hashing, hash) pair. Bug indicator.
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

	/// Resolve a batch of indexed-transaction hashes via bitswap.
	///
	/// Each want carries the runtime-declared `cid_codec` so the request CID's
	/// codec matches what the producing runtime announced.
	///
	/// Returns only successfully fetched entries. A short result means the caller
	/// (the block import) will surface a `ConsensusError` and the import will be
	/// retried later.
	pub async fn fetch_many(
		&self,
		wants: &[(ContentHash, HashingAlgorithm, u64)],
	) -> Result<HashMap<ContentHash, Vec<u8>>, FetchError> {
		if wants.is_empty() {
			return Ok(HashMap::new());
		}
		let handle = self.bitswap.get().ok_or(FetchError::BitswapHandleUnset)?;

		let mut by_cid: HashMap<Cid, ContentHash> = HashMap::with_capacity(wants.len());
		let mut cids: Vec<Cid> = Vec::with_capacity(wants.len());
		for (hash, algo, codec) in wants {
			let mh = Multihash::<64>::wrap(algo.multihash_code(), hash)
				.map_err(|e| FetchError::Multihash(e.to_string()))?;
			let cid = Cid::new_v1(*codec, mh);
			by_cid.insert(cid, *hash);
			cids.push(cid);
		}

		let chunks: Vec<Vec<Cid>> = cids
			.chunks(MAX_CIDS_PER_REQUEST)
			.map(<[Cid]>::to_vec)
			.collect();

		let receivers = future::try_join_all(
			chunks.into_iter().map(|chunk| handle.request_stream(chunk)),
		)
		.await?;

		let mut acquired: HashMap<ContentHash, Vec<u8>> = HashMap::with_capacity(wants.len());
		let mut streams: FuturesUnordered<_> = receivers
			.into_iter()
			.map(|mut rx| async move {
				let mut out = Vec::new();
				while let Some(item) = rx.recv().await {
					out.push(item);
				}
				out
			})
			.collect();

		while let Some(items) = streams.next().await {
			for item in items {
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
						log::debug!(
							target: LOG_TARGET,
							"bitswap returned Missing for {cid}",
						);
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
