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

//! Fetches indexed-transaction blobs through the shared Bitswap service.

use crate::RenewWant;
use cid::{multihash::Multihash, Cid};
use sc_network_bitswap::{BitswapError, BitswapRequest};
use sp_transaction_storage_proof::ContentHash;
use std::{
	collections::HashMap,
	sync::{Arc, OnceLock},
	time::Duration,
};

const LOG_TARGET: &str = "storage-chain-fetcher";

/// Base time budget for a single [`IndexedTransactionFetcher::fetch_many`] call.
const FETCH_TIMEOUT_BASE: Duration = Duration::from_secs(30);
/// Additional budget per requested CID: large wantlists queue behind the bitswap
/// service's dispatch window and need proportionally more time.
const FETCH_TIMEOUT_PER_CID: Duration = Duration::from_millis(100);
/// Hard cap so a hopeless fetch cannot stall block import for too long.
const FETCH_TIMEOUT_MAX: Duration = Duration::from_secs(600);

fn fetch_timeout(cid_count: usize) -> Duration {
	let per_cid = FETCH_TIMEOUT_PER_CID.saturating_mul(cid_count.min(u32::MAX as usize) as u32);
	FETCH_TIMEOUT_BASE.saturating_add(per_cid).min(FETCH_TIMEOUT_MAX)
}

/// Late-bound slot for the bitswap handle.
///
/// Allows constructing a fetcher before the handle exists; fetches fail with
/// [`FetchError::BitswapUnavailable`] until the slot is populated.
pub type BitswapHandleSlot = Arc<OnceLock<Arc<dyn BitswapRequest>>>;

/// Infrastructure-level fetch failure surfaced to [`crate::StorageChainBlockImport`].
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
	/// No bitswap handle is available: bitswap is disabled on this node, or the network
	/// has not been initialized yet.
	#[error("bitswap unavailable: disabled on this node, or network not yet initialized")]
	BitswapUnavailable,
	/// CID construction failed for the given (hashing, hash) pair.
	#[error("failed to construct multihash for CID: {0}")]
	Multihash(String),
	/// The bitswap service rejected the request at admission, or shut down mid-stream.
	#[error("bitswap service error: {0}")]
	Bitswap(#[from] BitswapError),
}

/// Fetcher that resolves indexed-transaction hashes via bitswap.
#[derive(Clone)]
pub struct IndexedTransactionFetcher {
	bitswap: BitswapHandleSlot,
}

impl IndexedTransactionFetcher {
	/// Build a new fetcher backed by the given late-bound bitswap handle slot.
	pub fn new(bitswap: BitswapHandleSlot) -> Self {
		Self { bitswap }
	}

	/// Resolve a batch of indexed-transaction hashes via bitswap. Each want carries the
	/// runtime-declared `cid_codec` so the request CID matches what the producing runtime
	/// announced. Returns only successfully fetched entries; entries unresolved when the
	/// time budget expires are simply absent.
	pub(crate) async fn fetch_many(
		&self,
		wants: &[RenewWant],
	) -> Result<HashMap<ContentHash, Vec<u8>>, FetchError> {
		if wants.is_empty() {
			return Ok(HashMap::new());
		}
		let handle = self.bitswap.get().ok_or(FetchError::BitswapUnavailable)?;

		let mut by_cid: HashMap<Cid, ContentHash> = HashMap::with_capacity(wants.len());
		let mut cids: Vec<Cid> = Vec::with_capacity(wants.len());
		for want in wants {
			let mh = Multihash::<64>::wrap(want.hashing.multihash_code(), &want.hash)
				.map_err(|e| FetchError::Multihash(e.to_string()))?;
			let cid = Cid::new_v1(want.cid_codec, mh);
			by_cid.insert(cid, want.hash);
			cids.push(cid);
		}

		let mut rx = match handle.request_stream(cids) {
			Ok(rx) => rx,
			// Transient congestion: degrade to an empty partial result instead of failing
			// the import.
			Err(BitswapError::Overloaded) => {
				log::debug!(target: LOG_TARGET, "bitswap service overloaded, deferring fetch");
				return Ok(HashMap::new());
			},
			Err(other) => return Err(FetchError::Bitswap(other)),
		};

		let deadline = tokio::time::Instant::now() + fetch_timeout(wants.len());
		let mut acquired: HashMap<ContentHash, Vec<u8>> = HashMap::with_capacity(wants.len());
		loop {
			match tokio::time::timeout_at(deadline, rx.recv()).await {
				Ok(Some(Ok((cid, bytes)))) => {
					if let Some(hash) = by_cid.get(&cid) {
						log::debug!(
							target: LOG_TARGET,
							"bitswap fetched {} bytes for {hash:?}",
							bytes.len(),
						);
						acquired.insert(*hash, bytes);
					}
				},
				Ok(Some(Err(BitswapError::ServiceClosed))) => {
					log::warn!(
						target: LOG_TARGET,
						"bitswap service closed mid-stream; returning partial result",
					);
					return Ok(acquired);
				},
				Ok(Some(Err(BitswapError::Overloaded))) => {
					log::debug!(
						target: LOG_TARGET,
						"bitswap service overloaded; returning partial result",
					);
					return Ok(acquired);
				},
				Ok(Some(Err(other))) => return Err(FetchError::Bitswap(other)),
				// The stream closed: every CID was delivered.
				Ok(None) => break,
				// Time budget expired. Dropping the receiver cancels the remaining wants.
				Err(_) => {
					log::debug!(
						target: LOG_TARGET,
						"bitswap fetch timed out with {}/{} entries resolved",
						acquired.len(),
						wants.len(),
					);
					break;
				},
			}
		}

		Ok(acquired)
	}
}
