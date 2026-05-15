// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

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

//! Bitswap-based fetcher for indexed-transaction blobs. Owns the late-bound network and
//! peer-source handles; rotates across connected peers per batch.

use async_trait::async_trait;
use cid::{multihash::Multihash, Cid};
use futures::channel::oneshot;
use sc_network::{
	bitswap::{request_bitswap_blocks, FetchOutcome, MAX_WANTED_BLOCKS, RAW_CODEC},
	NetworkRequest, PeerId,
};
use sc_network_sync::SyncingService;
use sp_runtime::traits::Block as BlockT;
use sp_transaction_storage_proof::{ContentHash, HashingAlgorithm};
use std::{
	collections::HashMap,
	sync::{Arc, OnceLock},
	time::Duration,
};

const LOG_TARGET: &str = "storage-chain-fetcher";
const BITSWAP_PER_PEER_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_PEERS_PER_IMPORT: usize = 8;

/// Source of currently-connected sync peer IDs. Abstracted so the fetcher can be unit-tested
/// without spinning up a full `SyncingService`. The production blanket impl on
/// `SyncingService<Block>` calls `peers_info()` and projects to the peer-id column.
#[async_trait]
pub trait BitswapPeerSource: Send + Sync {
	async fn current_peers(&self) -> Result<Vec<PeerId>, oneshot::Canceled>;
}

#[async_trait]
impl<B: BlockT> BitswapPeerSource for SyncingService<B> {
	async fn current_peers(&self) -> Result<Vec<PeerId>, oneshot::Canceled> {
		Ok(self.peers_info().await?.into_iter().map(|(peer, _)| peer).collect())
	}
}

/// Late-bound network request handle, populated by the omni-node after build_network.
pub type NetworkHandle = Arc<OnceLock<Arc<dyn NetworkRequest + Send + Sync>>>;
/// Late-bound peer-source handle populated after `build_network` returns.
pub type SyncingHandle = Arc<OnceLock<Arc<dyn BitswapPeerSource + Send + Sync>>>;

/// Infrastructure-level fetch failure.
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
	#[error("network handle not yet set; storage-chain blocks cannot be fetched before build_network completes")]
	NetworkHandleUnset,
	#[error("sync handle not yet set; storage-chain blocks cannot be fetched before build_network completes")]
	SyncingHandleUnset,
	#[error("failed to construct multihash for CID: {0}")]
	Multihash(String),
}

/// Fetcher that resolves indexed-transaction hashes via bitswap.
///
/// Owns the late-bound network/sync handles plus the per-peer iteration policy. The block-import
/// path holds one of these and calls [`Self::fetch_many`] for each batch of missing renew hashes.
///
/// Cloning is cheap: every field is an `Arc`-equivalent.
pub struct IndexedTransactionFetcher<Block: BlockT> {
	network: NetworkHandle,
	peer_source: SyncingHandle,
	_phantom: std::marker::PhantomData<Block>,
}

impl<Block: BlockT> Clone for IndexedTransactionFetcher<Block> {
	fn clone(&self) -> Self {
		Self {
			network: self.network.clone(),
			peer_source: self.peer_source.clone(),
			_phantom: std::marker::PhantomData,
		}
	}
}

impl<Block: BlockT> IndexedTransactionFetcher<Block> {
	/// Build a new fetcher backed by the given late-bound handles.
	pub fn new(network: NetworkHandle, peer_source: SyncingHandle) -> Self {
		Self { network, peer_source, _phantom: std::marker::PhantomData }
	}

	/// Resolve a batch of `(content_hash, hashing)` pairs via bitswap across up to
	/// [`MAX_PEERS_PER_IMPORT`] peers, sending one multi-entry `WANT-BLOCK` request per peer.
	///
	/// Returns only successfully fetched entries; `Missing`/`DontHave` outcomes from each peer
	/// fall through to the next peer in the candidate list. The caller detects partial fill
	/// by comparing `result.len()` against `wants.len()`.
	pub async fn fetch_many(
		&self,
		wants: &[(ContentHash, HashingAlgorithm)],
	) -> Result<HashMap<ContentHash, Vec<u8>>, FetchError> {
		if wants.is_empty() {
			return Ok(HashMap::new());
		}
		let network = self.network.get().ok_or(FetchError::NetworkHandleUnset)?;
		let peer_source = self.peer_source.get().ok_or(FetchError::SyncingHandleUnset)?;

		let peers = match peer_source.current_peers().await {
			Ok(peers) => peers,
			Err(_) => {
				log::warn!(target: LOG_TARGET, "current_peers() channel cancelled");
				return Ok(HashMap::new());
			},
		};
		if peers.is_empty() {
			log::debug!(
				target: LOG_TARGET,
				"no connected sync peers, cannot fetch via bitswap yet",
			);
			return Ok(HashMap::new());
		}

		// Build per-want CIDs once; reuse across peers and chunks.
		let cids: Vec<(ContentHash, Cid)> = wants
			.iter()
			.map(|(hash, algo)| Ok::<_, FetchError>((*hash, cid_for(*hash, *algo)?)))
			.collect::<Result<_, _>>()?;
		let mut remaining = cids;
		let mut acquired: HashMap<ContentHash, Vec<u8>> = HashMap::new();

		for peer in peers.into_iter().take(MAX_PEERS_PER_IMPORT) {
			if remaining.is_empty() {
				break;
			}
			let from_peer = try_fetch_from_peer(network.as_ref(), peer, &remaining).await;
			acquired.extend(from_peer);
			remaining.retain(|(hash, _)| !acquired.contains_key(hash));
		}

		Ok(acquired)
	}
}

/// Try every chunk of `wants` against a single peer in sequence. Returns whatever blocks the
/// peer actually served. A timeout or per-chunk error aborts the remaining chunks for this peer
/// and lets the caller move on to the next one.
async fn try_fetch_from_peer<N: NetworkRequest + ?Sized>(
	network: &N,
	peer: PeerId,
	wants: &[(ContentHash, Cid)],
) -> HashMap<ContentHash, Vec<u8>> {
	let mut acquired: HashMap<ContentHash, Vec<u8>> = HashMap::new();
	for chunk in wants.chunks(MAX_WANTED_BLOCKS) {
		let cids: Vec<Cid> = chunk.iter().map(|(_, cid)| *cid).collect();
		match with_timeout(request_bitswap_blocks(network, peer, &cids), BITSWAP_PER_PEER_TIMEOUT)
			.await
		{
			None => {
				log::debug!(
					target: LOG_TARGET,
					"request_bitswap_blocks to {peer:?}: timeout (chunk size {})",
					chunk.len(),
				);
				return acquired;
			},
			Some(Err(e)) => {
				log::debug!(target: LOG_TARGET, "request_bitswap_blocks to {peer:?}: {e:?}");
				return acquired;
			},
			Some(Ok(per_cid)) => {
				for (hash, cid) in chunk {
					if let Some(FetchOutcome::Block(data)) = per_cid.get(cid) {
						log::debug!(
							target: LOG_TARGET,
							"fetched {} bytes for {:?} from {peer:?}",
							data.len(),
							hash,
						);
						acquired.insert(*hash, data.clone());
					}
				}
			},
		}
	}
	acquired
}

/// Build a CIDv1 over RAW_CODEC with the supplied hash + algorithm's multihash code.
fn cid_for(hash: ContentHash, algo: HashingAlgorithm) -> Result<Cid, FetchError> {
	let mh = Multihash::<64>::wrap(algo.multihash_code(), &hash)
		.map_err(|e| FetchError::Multihash(e.to_string()))?;
	Ok(Cid::new_v1(RAW_CODEC, mh))
}

async fn with_timeout<F, T>(fut: F, timeout: Duration) -> Option<T>
where
	F: std::future::Future<Output = T>,
{
	use futures::FutureExt;
	futures::select! {
		v = fut.fuse() => Some(v),
		_ = futures_timer::Delay::new(timeout).fuse() => None,
	}
}
