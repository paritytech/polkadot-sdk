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

use crate::RenewWant;
use async_trait::async_trait;
use cid::{multihash::Multihash, Cid};
use futures::channel::oneshot;
use rand::seq::SliceRandom;
use sc_network::{
	bitswap::{request_bitswap_blocks, FetchOutcome, MAX_WANTED_BLOCKS},
	NetworkRequest, PeerId,
};
use sc_network_sync::SyncingService;
use sp_runtime::traits::Block as BlockT;
use sp_transaction_storage_proof::ContentHash;
use std::{
	collections::HashMap,
	sync::{Arc, OnceLock},
	time::Duration,
};

const LOG_TARGET: &str = "storage-chain-fetcher";
const BITSWAP_PER_PEER_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_PEERS_PER_IMPORT: usize = 8;

/// Source of currently-connected sync peer IDs.
#[async_trait]
pub trait BitswapPeerSource: Send + Sync {
	async fn current_peers(&self) -> Result<Vec<PeerId>, oneshot::Canceled>;
}

#[async_trait]
impl<B: BlockT> BitswapPeerSource for SyncingService<B> {
	async fn current_peers(&self) -> Result<Vec<PeerId>, oneshot::Canceled> {
		Ok(self
			.peers_info()
			.await?
			.into_iter()
			.filter_map(|(peer, info)| info.roles.is_full().then_some(peer))
			.collect())
	}
}

/// Late-bound network request handle, populated once the network is built.
pub type NetworkHandle = Arc<OnceLock<Arc<dyn NetworkRequest + Send + Sync>>>;
/// Late-bound peer-source handle, populated once the network is built.
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

	/// Resolve a batch of indexed-transaction renew wants via bitswap, rotating across up to
	/// `MAX_PEERS_PER_IMPORT` peers. Returns only successfully fetched entries.
	pub(crate) async fn fetch_many(
		&self,
		wants: &[RenewWant],
	) -> Result<HashMap<ContentHash, Vec<u8>>, FetchError> {
		if wants.is_empty() {
			return Ok(HashMap::new());
		}
		let network = self.network.get().ok_or(FetchError::NetworkHandleUnset)?;
		let peer_source = self.peer_source.get().ok_or(FetchError::SyncingHandleUnset)?;

		let Ok(mut peers) = peer_source.current_peers().await else {
			log::warn!(target: LOG_TARGET, "current_peers() channel cancelled");
			return Ok(HashMap::new());
		};
		if peers.is_empty() {
			log::debug!(
				target: LOG_TARGET,
				"no connected sync peers, cannot fetch via bitswap yet",
			);
			return Ok(HashMap::new());
		}
		// Shuffle peers to not end up with always the same peers.
		peers.shuffle(&mut rand::thread_rng());

		// Build per-want CIDs once; reuse across peers and chunks.
		let cids: Vec<(ContentHash, Cid)> = wants
			.iter()
			.map(|w| {
				let mh = Multihash::<64>::wrap(w.hashing.multihash_code(), &w.hash)
					.map_err(|e| FetchError::Multihash(e.to_string()))?;
				Ok::<_, FetchError>((w.hash, Cid::new_v1(w.cid_codec, mh)))
			})
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
