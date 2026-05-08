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

//! Implementation of the bitswap RPC methods.
//!
//! See the JSON-RPC interface spec:
//! - <https://github.com/paritytech/json-rpc-interface-spec/blob/main/src/api/bitswap_v1_get.md>
//! - <https://github.com/paritytech/json-rpc-interface-spec/blob/main/src/api/bitswap_v1_getMany.md>
//! - <https://github.com/paritytech/json-rpc-interface-spec/blob/main/src/api/bitswap_v1_stream.md>

use crate::{
	bitswap::{
		api::{BitswapApiServer, BlockResult},
		error::Error,
	},
	SubscriptionTaskExecutor,
};
use cid::Cid;
use jsonrpsee::{core::RpcResult, PendingSubscriptionSink};
use sc_client_api::BlockBackend;
use sc_rpc::utils::Subscription;
use sp_core::H256;
use sp_runtime::traits::Block as BlockT;
use std::{collections::HashSet, sync::Arc};

/// Log target for this file.
const LOG_TARGET: &str = "rpc-spec-v2";

// Standard multihash codes.
// See <https://github.com/multiformats/multicodec/blob/master/table.csv>
const SHA2_256: u64 = 0x12;
const BLAKE2B_256: u64 = 0xb220;

/// Maximum number of CIDs accepted by `bitswap_v1_getMany` and `bitswap_v1_stream`
/// in a single request. Bounds worst-case response at ≤128 MiB (64 × 2 MiB/chunk).
pub const MAX_CIDS_PER_REQUEST: usize = 64;

/// Bitswap RPC implementation.
pub struct Bitswap<Block, Client> {
	client: Arc<Client>,
	sync_oracle: Arc<dyn sp_consensus::SyncOracle + Send + Sync>,
	executor: SubscriptionTaskExecutor,
	_phantom: std::marker::PhantomData<Block>,
}

impl<Block, Client> Bitswap<Block, Client> {
	/// Creates a new [`Bitswap`] instance.
	pub fn new(
		client: Arc<Client>,
		sync_oracle: Arc<dyn sp_consensus::SyncOracle + Send + Sync>,
		executor: SubscriptionTaskExecutor,
	) -> Self {
		Self { client, sync_oracle, executor, _phantom: std::marker::PhantomData }
	}
}

/// Parse a CID string and validate it (CIDv1, sha2-256 or blake2b-256, 32-byte digest).
fn parse_and_validate_cid(cid_str: &str) -> Result<H256, Error> {
	let cid = Cid::try_from(cid_str).map_err(|e| Error::InvalidCid(format!("{e}")))?;

	// Only CIDv1 version is supported according to the spec.
	if cid.version() != cid::Version::V1 {
		return Err(Error::InvalidCid("Only CIDv1 is supported".into()));
	}

	let hash = cid.hash();

	// Only sha2-256 & blake2b-256 hash functions are supported according to the spec.
	if hash.code() != SHA2_256 && hash.code() != BLAKE2B_256 {
		return Err(Error::InvalidCid(
			"Only sha2-256 & blake2b-256 hash functions are supported".into(),
		));
	}

	// `H256::from_slice` panics below if the size is incorrect, so double-check the size is
	// correct, even though we checked the hash function type above.
	if hash.size() != 32 {
		return Err(Error::InvalidCid("Only 256-bit hash digests are supported".into()));
	}

	Ok(H256::from_slice(hash.digest()))
}

impl<Block, Client> BitswapApiServer for Bitswap<Block, Client>
where
	Block: BlockT,
	Client: BlockBackend<Block> + Send + Sync + 'static,
{
	fn bitswap_v1_get(&self, cid_str: String) -> RpcResult<String> {
		let digest = parse_and_validate_cid(&cid_str)?;

		match self.client.indexed_transaction(digest) {
			Ok(Some(data)) => Ok(crate::hex_string(&data)),
			Ok(None) => {
				if self.sync_oracle.is_major_syncing() {
					Err(Error::MajorSyncing.into())
				} else {
					Err(Error::NotFound.into())
				}
			},
			Err(err) => {
				// Note: this never happens in practice, because `indexed_transaction`
				// implementation in `substrate/client/db` always returns Ok(_), and is only
				// needed to handle possible future API changes.
				log::warn!(target: LOG_TARGET, "Indexed transaction fetch failed: {err:?}");

				Err(Error::Internal(err).into())
			},
		}
	}

	fn bitswap_v1_get_many(
		&self,
		cids: Vec<String>,
	) -> RpcResult<Vec<(String, BlockResult)>> {
		// TODO: per-CID `FailRetryBackoff` is correct for misses during sync, but a
		// smarter implementation would attempt a peer-side Bitswap fetch and write
		// the result back to the local DB. Needs a coherence story for concurrent
		// writes during major sync.
		if cids.len() > MAX_CIDS_PER_REQUEST {
			return Err(Error::TooManyCids { max: MAX_CIDS_PER_REQUEST, got: cids.len() }.into());
		}

		let parsed = parse_and_dedup(cids)?;
		let is_major_syncing = self.sync_oracle.is_major_syncing();

		Ok(parsed
			.into_iter()
			.map(|(cid_str, parse_result)| {
				let result = match parse_result {
					Ok(digest) => lookup_by_digest(&self.client, is_major_syncing, digest),
					Err(e) => BlockResult::from(e),
				};
				(cid_str, result)
			})
			.collect())
	}

	fn bitswap_v1_stream(&self, pending: PendingSubscriptionSink, cids: Vec<String>) {
		let client = self.client.clone();
		let sync_oracle = self.sync_oracle.clone();

		let fut = async move {
			// TODO: see `bitswap_v1_get_many`.
			if cids.len() > MAX_CIDS_PER_REQUEST {
				pending
					.reject(Error::TooManyCids { max: MAX_CIDS_PER_REQUEST, got: cids.len() })
					.await;
				return;
			}

			let parsed = match parse_and_dedup(cids) {
				Ok(p) => p,
				Err(e) => {
					pending.reject(e).await;
					return;
				},
			};
			let is_major_syncing = sync_oracle.is_major_syncing();

			let Ok(sink) = pending.accept().await.map(Subscription::from) else { return };

			for (cid_str, parse_result) in parsed {
				let result = match parse_result {
					Ok(digest) => lookup_by_digest(&client, is_major_syncing, digest),
					Err(e) => BlockResult::from(e),
				};
				if sink.send(&(cid_str, result)).await.is_err() {
					return;
				}
			}
		};

		sc_rpc::utils::spawn_subscription_task(&self.executor, fut);
	}
}

/// Parse all input CIDs and reject duplicates. Two-stage detection:
/// 1. Literal-string dedup catches identical inputs before any parsing.
/// 2. Digest dedup catches different strings that decode to the same data.
///
/// **Invariant**: when this returns `Ok(out)`, no two `Ok(_)` entries in `out` carry
/// the same `H256` digest. Detection short-circuits on the first collision via
/// `Err(Error::DuplicateCids)` *before* the duplicate is appended to `out`.
fn parse_and_dedup(
	cids: Vec<String>,
) -> Result<Vec<(String, Result<H256, Error>)>, Error> {
	let mut seen_strings: HashSet<String> = HashSet::with_capacity(cids.len());
	let mut seen_digests: HashSet<H256> = HashSet::with_capacity(cids.len());
	let mut out = Vec::with_capacity(cids.len());

	for cid_str in cids {
		// Stage A: literal-string dedup. Catches `[A, A]` and `["bad", "bad"]` —
		// no parsing required.
		if !seen_strings.insert(cid_str.clone()) {
			return Err(Error::DuplicateCids);
		}
		let parsed = parse_and_validate_cid(&cid_str);
		// Stage B: digest dedup. Catches `[A, B]` where A and B are distinct strings
		// encoding the same data.
		if let Ok(digest) = &parsed {
			if !seen_digests.insert(*digest) {
				return Err(Error::DuplicateCids);
			}
		}
		out.push((cid_str, parsed));
	}

	Ok(out)
}

/// DB lookup for a parsed digest. Returns the spec-shaped per-CID outcome.
///
/// On a miss, distinguishes "not yet synced" (`MajorSyncing` → `-32812 FailRetryBackoff`,
/// transient: caller should retry with backoff) from "permanently absent"
/// (`NotFound` → `-32810 Fail`). Mirrors the existing `bitswap_v1_get` semantics.
///
/// `is_major_syncing` is a snapshot taken once by the caller and reused across
/// the whole batch — every CID in a single batch gets a consistent "sync moment".
fn lookup_by_digest<Block, Client>(
	client: &Arc<Client>,
	is_major_syncing: bool,
	digest: H256,
) -> BlockResult
where
	Block: BlockT,
	Client: BlockBackend<Block> + Send + Sync + 'static,
{
	match client.indexed_transaction(digest) {
		Ok(Some(data)) => BlockResult::Ok(crate::hex_string(&data)),
		Ok(None) =>
			if is_major_syncing {
				BlockResult::from(Error::MajorSyncing)
			} else {
				BlockResult::from(Error::NotFound)
			},
		Err(err) => {
			log::warn!(target: LOG_TARGET, "Indexed transaction fetch failed: {err:?}");
			BlockResult::from(Error::NotFound)
		},
	}
}
