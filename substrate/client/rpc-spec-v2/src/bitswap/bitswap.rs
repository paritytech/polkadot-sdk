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
//! - <https://github.com/paritytech/json-rpc-interface-spec/blob/main/src/api/bitswap_unstable_get.md>
//! - <https://github.com/paritytech/json-rpc-interface-spec/blob/main/src/api/bitswap_unstable_stream.md>
//! - <https://github.com/paritytech/json-rpc-interface-spec/blob/main/src/api/bitswap_unstable_unstream.md>

use crate::{
	bitswap::{
		api::{BitswapApiServer, StreamEvent},
		error::Error,
	},
	SubscriptionTaskExecutor,
};
use cid::Cid;
use jsonrpsee::{core::RpcResult, types::ErrorObject, PendingSubscriptionSink};
use sc_client_api::BlockBackend;
use sc_rpc::utils::Subscription;
use sp_core::H256;
use sp_runtime::traits::Block as BlockT;
use std::{collections::HashSet, sync::Arc};

/// Log target for this file. Filterable independently of the rest of `rpc-spec-v2`, matching
/// the `rpc-spec-v2::<module>` convention from `archive/archive.rs:58`.
const LOG_TARGET: &str = "rpc-spec-v2::bitswap";

// Standard multihash codes.
// See <https://github.com/multiformats/multicodec/blob/master/table.csv>
const SHA2_256: u64 = 0x12;
const BLAKE2B_256: u64 = 0xb220;

/// Maximum number of CIDs accepted by `bitswap_unstable_stream` in a single subscription. Bounds
/// worst-case response at ≤128 MiB (64 × 2 MiB/chunk). Spec only requires ≥16.
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
	fn bitswap_unstable_get(&self, cid_str: String) -> RpcResult<String> {
		log::trace!(target: LOG_TARGET, "bitswap_unstable_get cid={cid_str}");
		let digest = match parse_and_validate_cid(&cid_str) {
			Ok(d) => d,
			Err(e) => {
				log::trace!(target: LOG_TARGET, "bitswap_unstable_get reject cid={cid_str} reason=invalid_cid: {e}");
				return Err(e.into());
			},
		};

		match lookup_by_digest(&self.client, self.sync_oracle.is_major_syncing(), digest) {
			Ok(value) => {
				log::trace!(
					target: LOG_TARGET,
					"bitswap_unstable_get hit cid={cid_str} bytes={}",
					// `value` is hex-encoded with `0x` prefix; payload bytes = (len-2)/2.
					value.len().saturating_sub(2) / 2,
				);
				Ok(value)
			},
			Err(e) => {
				log::trace!(target: LOG_TARGET, "bitswap_unstable_get miss cid={cid_str} reason={e}");
				Err(e.into())
			},
		}
	}

	fn bitswap_unstable_stream(&self, pending: PendingSubscriptionSink, cids: Vec<String>) {
		let client = self.client.clone();
		let sync_oracle = self.sync_oracle.clone();

		let fut = async move {
			// TODO: per-CID `FailRetryBackoff` is correct for misses during sync, but a
			// smarter implementation would attempt a peer-side Bitswap fetch and write
			// the result back to the local DB. Needs a coherence story for concurrent
			// writes during major sync.

			let cids_len = cids.len();
			log::trace!(target: LOG_TARGET, "bitswap_unstable_stream open cids={cids_len}");

			// Top-level validation. Per the spec, all three structural rejections happen
			// before the subscription is accepted, so no events are ever emitted on these
			// paths.
			if cids.is_empty() {
				log::trace!(target: LOG_TARGET, "bitswap_unstable_stream reject reason=empty_cids");
				pending.reject(Error::EmptyCids).await;
				return;
			}
			if cids_len > MAX_CIDS_PER_REQUEST {
				log::trace!(
					target: LOG_TARGET,
					"bitswap_unstable_stream reject reason=too_many_cids got={cids_len} max={MAX_CIDS_PER_REQUEST}",
				);
				pending
					.reject(Error::TooManyCids { max: MAX_CIDS_PER_REQUEST, got: cids_len })
					.await;
				return;
			}
			let parsed = match parse_and_dedup(cids) {
				Ok(p) => p,
				Err(e) => {
					log::trace!(target: LOG_TARGET, "bitswap_unstable_stream reject reason=duplicate_cids");
					pending.reject(e).await;
					return;
				},
			};
			let is_major_syncing = sync_oracle.is_major_syncing();

			let Ok(sink) = pending.accept().await.map(Subscription::from) else { return };

			for (cid_str, parse_result) in parsed {
				let event = match parse_result {
					Ok(digest) => match lookup_by_digest(&client, is_major_syncing, digest) {
						Ok(value) => {
							log::trace!(
								target: LOG_TARGET,
								"bitswap_unstable_stream item cid={cid_str} bytes={}",
								// `value` is hex-encoded with `0x` prefix; payload bytes = (len-2)/2.
								value.len().saturating_sub(2) / 2,
							);
							StreamEvent::StreamItem { cid: cid_str, value }
						},
						Err(e) => {
							log::trace!(
								target: LOG_TARGET,
								"bitswap_unstable_stream item-error cid={cid_str} reason={e}",
							);
							stream_item_error(cid_str, e)
						},
					},
					Err(e) => {
						log::trace!(
							target: LOG_TARGET,
							"bitswap_unstable_stream item-error cid={cid_str} reason=invalid_cid: {e}",
						);
						stream_item_error(cid_str, e)
					},
				};
				// A send error means the client unsubscribed or disconnected. The spec says
				// `streamDone` must NOT be emitted on cancellation, so we just exit.
				if sink.send(&event).await.is_err() {
					log::trace!(target: LOG_TARGET, "bitswap_unstable_stream cancelled (sink closed)");
					return;
				}
			}

			// All per-CID events emitted successfully — send the end-of-stream marker.
			// Ignore a send error here: the client may have unsubscribed in the window
			// between the final per-CID event and this `streamDone`, and that's fine.
			let _ = sink.send(&StreamEvent::StreamDone).await;
			log::trace!(target: LOG_TARGET, "bitswap_unstable_stream done cids={cids_len}");
		};

		sc_rpc::utils::spawn_subscription_task(&self.executor, fut);
	}
}

/// Render an [`Error`] as a [`StreamEvent::StreamItemError`] for the given CID, using the
/// standard JSON-RPC error mapping defined in [`super::error`].
fn stream_item_error(cid: String, e: Error) -> StreamEvent {
	let obj = ErrorObject::from(e);
	StreamEvent::StreamItemError { cid, code: obj.code(), message: obj.message().to_string() }
}

/// Parse all input CIDs and reject duplicates. Two-stage detection:
/// 1. Literal-string dedup catches identical inputs before any parsing.
/// 2. Digest dedup catches different strings that decode to the same data.
///
/// **Invariant**: when this returns `Ok(out)`, no two `Ok(_)` entries in `out` carry
/// the same `H256` digest. Detection short-circuits on the first collision via
/// `Err(Error::DuplicateCids)` *before* the duplicate is appended to `out`.
fn parse_and_dedup(cids: Vec<String>) -> Result<Vec<(String, Result<H256, Error>)>, Error> {
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

/// DB lookup for a parsed digest.
///
/// On a miss, distinguishes "not yet synced" (`MajorSyncing` → `-32812 FailRetryBackoff`,
/// transient: caller should retry with backoff) from "permanently absent"
/// (`NotFound` → `-32810 Fail`).
fn lookup_by_digest<Block, Client>(
	client: &Arc<Client>,
	is_major_syncing: bool,
	digest: H256,
) -> Result<String, Error>
where
	Block: BlockT,
	Client: BlockBackend<Block> + Send + Sync + 'static,
{
	match client.indexed_transaction(digest) {
		Ok(Some(data)) => Ok(crate::hex_string(&data)),
		Ok(None) => {
			if is_major_syncing {
				Err(Error::MajorSyncing)
			} else {
				Err(Error::NotFound)
			}
		},
		Err(err) => {
			log::warn!(target: LOG_TARGET, "Indexed transaction fetch failed: {err:?}");
			Err(Error::Internal(err))
		},
	}
}
