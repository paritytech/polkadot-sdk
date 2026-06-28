// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Substrate.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate. If not, see <https://www.gnu.org/licenses/>.

use cid::Cid;
use futures::channel::oneshot;
use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc;

use super::{
	is_cid_supported, schema::bitswap::message::wantlist::WantType as ProtoWantType,
	MAX_WANTED_BLOCKS,
};

/// Const from <https://github.com/multiformats/multicodec/blame/master/table.csv>
/// Multihash code for BLAKE2b-256.
pub const BLAKE2B_256_MULTIHASH_CODE: u64 = 0xb220;
/// Multihash code for SHA2-256.
pub const SHA2_256_MULTIHASH_CODE: u64 = 0x12;
/// Multihash code for Keccak-256.
pub const KECCAK_256_MULTIHASH_CODE: u64 = 0x1b;

/// Whether the service should re-hash block bytes to confirm they match the requested CID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VerificationMode {
	/// Re-hash received block bytes and confirm the digest matches the requested CID.
	/// Blocks that fail verification are recorded as [`FetchOutcome::Missing`].
	Verified,
	/// Trust the peer-declared CID without recomputing the hash.
	/// Integrity verification is delegated to the caller.
	Unverified,
}

/// Per-CID outcome from a Bitswap block request.
///
/// The public contract is intentionally narrow: either the peer delivered the bytes for the CID
/// or it did not. A peer signalling `DONT_HAVE` and a peer staying silent for a CID are both
/// surfaced as [`FetchOutcome::Missing`]; callers needing a different policy must implement it
/// over [`FetchOutcome`].
#[derive(Debug)]
pub enum FetchOutcome {
	/// Peer returned bytes for the requested CID.
	Block(Vec<u8>),
	/// Peer did not deliver bytes for this CID.
	///
	/// Covers the peer explicitly answering `DONT_HAVE`, the peer answering `HAVE` without bytes,
	/// and the peer not acknowledging the CID at all. From the caller's perspective these are
	/// equivalent: no block was delivered.
	Missing,
}

pub(crate) type BitswapResponse = Result<HashMap<Cid, FetchOutcome>, BitswapError>;

/// An outbound request queued for [`BitswapService`] to fulfil.
///
/// `response_tx` is a oneshot sender the service uses to return the collected
/// [`BitswapResponse`] once all requested CIDs have been resolved or the request times out.
pub(crate) struct BitswapRequest {
	pub(crate) cids: Vec<(Cid, ProtoWantType)>,
	pub(crate) response_tx: oneshot::Sender<BitswapResponse>,
	pub(crate) verification: VerificationMode,
}

/// A cloneable handle for submitting block requests to the [`BitswapService`].
#[derive(Clone, Debug)]
pub struct BitswapClient {
	pub(crate) request_tx: mpsc::Sender<BitswapRequest>,
}

impl BitswapClient {
	/// Request blocks, verifying each response by recomputing the CID from the received bytes.
	///
	/// Blocks whose recomputed CID does not match what was requested are recorded as
	/// [`FetchOutcome::Missing`]. Errors if `cids` is empty, larger than [`MAX_WANTED_BLOCKS`],
	/// contains an unsupported CID, or contains a duplicate CID.
	pub async fn request_blocks(&self, cids: &[Cid]) -> BitswapResponse {
		validate_cids(cids)?;
		self.send(cids, VerificationMode::Verified).await
	}

	/// Like [`BitswapClient::request_blocks`], but does not recompute or verify the hash.
	///
	/// Use when the requester must fetch blocks before it can verify them through an external
	/// authority. Integrity verification is delegated to the caller.
	pub async fn request_blocks_unverified(&self, cids: &[Cid]) -> BitswapResponse {
		validate_cids(cids)?;
		self.send(cids, VerificationMode::Unverified).await
	}

	async fn send(&self, cids: &[Cid], verification: VerificationMode) -> BitswapResponse {
		let (response_tx, response_rx) = oneshot::channel();
		let cids = cids.iter().map(|cid| (*cid, ProtoWantType::Block)).collect();
		let _ = self.request_tx.send(BitswapRequest { cids, response_tx, verification }).await;
		response_rx
			.await
			.map_err(|err| BitswapError::RequestFailed(err.to_string()))
			.and_then(|r| r)
	}
}

/// Validate the wantlist length is within bounds.
fn validate_wantlist_size(len: usize) -> Result<(), BitswapError> {
	if len == 0 {
		return Err(BitswapError::DecodeError("empty wantlist".into()));
	}
	if len > MAX_WANTED_BLOCKS {
		return Err(BitswapError::DecodeError(format!(
			"wantlist too large: {len} > {MAX_WANTED_BLOCKS}",
		)));
	}
	Ok(())
}

/// Validate CIDs: enforce length, CID support, and CID uniqueness.
fn validate_cids(cids: &[Cid]) -> Result<(), BitswapError> {
	validate_wantlist_size(cids.len())?;

	let mut seen: HashSet<Cid> = HashSet::with_capacity(cids.len());
	for cid in cids {
		if !is_cid_supported(cid) {
			return Err(BitswapError::UnsupportedHashing { multihash_code: cid.hash().code() });
		}
		if !seen.insert(*cid) {
			return Err(BitswapError::DecodeError(format!("duplicate CID in wantlist: {cid}")));
		}
	}

	Ok(())
}

/// Bitswap client errors.
#[derive(Debug)]
pub enum BitswapError {
	/// Failed to decode or validate a bitswap payload.
	DecodeError(String),
	/// Request/response exchange failed.
	RequestFailed(String),
	/// Block prefix declared an unsupported multihash code.
	UnsupportedHashing {
		/// The unrecognised IPFS multihash code.
		multihash_code: u64,
	},
}

#[cfg(test)]
mod tests {
	use super::*;
	use cid::multihash::Multihash as CidMultihash;
	use std::collections::HashMap;
	use tokio::sync::mpsc;

	use super::super::{is_supported_multihash_code, RAW_CODEC};

	fn make_cid(code: u64, digest: [u8; 32]) -> Cid {
		let mh = CidMultihash::<64>::wrap(code, &digest).unwrap();
		Cid::new_v1(RAW_CODEC, mh)
	}

	fn make_client() -> (BitswapClient, mpsc::Receiver<BitswapRequest>) {
		let (tx, rx) = mpsc::channel(8);
		(BitswapClient { request_tx: tx }, rx)
	}

	#[tokio::test]
	async fn request_blocks_empty_wantlist_errors() {
		let (client, _rx) = make_client();
		let err = client.request_blocks(&[]).await.expect_err("empty wantlist must error");
		assert!(matches!(err, BitswapError::DecodeError(msg) if msg == "empty wantlist"));
	}

	#[tokio::test]
	async fn request_blocks_unverified_empty_wantlist_errors() {
		let (client, _rx) = make_client();
		let err = client
			.request_blocks_unverified(&[])
			.await
			.expect_err("empty wantlist must error");
		assert!(matches!(err, BitswapError::DecodeError(msg) if msg == "empty wantlist"));
	}

	#[tokio::test]
	async fn request_blocks_over_cap_errors() {
		let (client, _rx) = make_client();
		let wants: Vec<Cid> = (0..=MAX_WANTED_BLOCKS)
			.map(|i| {
				let mut digest = [0u8; 32];
				digest[..4].copy_from_slice(&(i as u32).to_le_bytes());
				make_cid(BLAKE2B_256_MULTIHASH_CODE, digest)
			})
			.collect();
		let err = client.request_blocks(&wants).await.expect_err("over-cap wantlist must error");
		assert!(matches!(err, BitswapError::DecodeError(_)));
	}

	#[tokio::test]
	async fn request_blocks_at_max_wanted_blocks_passes_validation() {
		let (client, mut rx) = make_client();
		let wants: Vec<Cid> = (0..MAX_WANTED_BLOCKS)
			.map(|i| {
				let mut digest = [0u8; 32];
				digest[..4].copy_from_slice(&(i as u32).to_le_bytes());
				make_cid(BLAKE2B_256_MULTIHASH_CODE, digest)
			})
			.collect();

		let handle = tokio::spawn(async move {
			let req = rx.recv().await.unwrap();
			let result = req.cids.iter().map(|(c, _)| (*c, FetchOutcome::Missing)).collect();
			let _ = req.response_tx.send(Ok(result));
		});

		let response = client.request_blocks(&wants).await.unwrap();
		assert_eq!(response.len(), MAX_WANTED_BLOCKS);
		handle.await.unwrap();
	}

	#[tokio::test]
	async fn request_blocks_unsupported_multihash_errors() {
		let (client, _rx) = make_client();
		const UNSUPPORTED: u64 = 0x99;
		assert!(!is_supported_multihash_code(UNSUPPORTED));
		let mh = CidMultihash::<64>::wrap(UNSUPPORTED, &[1u8; 32]).unwrap();
		let cid = Cid::new_v1(RAW_CODEC, mh);
		let err = client.request_blocks(&[cid]).await.expect_err("unsupported CID must error");
		assert!(matches!(err, BitswapError::UnsupportedHashing { multihash_code: 0x99 }));
	}

	#[tokio::test]
	async fn request_blocks_duplicate_cid_errors() {
		let (client, _rx) = make_client();
		let cid = make_cid(BLAKE2B_256_MULTIHASH_CODE, [1u8; 32]);
		let err = client.request_blocks(&[cid, cid]).await.expect_err("duplicate CID must error");
		assert!(matches!(err, BitswapError::DecodeError(msg) if msg.starts_with("duplicate CID")));
	}

	#[tokio::test]
	async fn request_blocks_returns_block_outcomes() {
		let (client, mut rx) = make_client();
		let cid_a = make_cid(BLAKE2B_256_MULTIHASH_CODE, [1u8; 32]);
		let cid_b = make_cid(SHA2_256_MULTIHASH_CODE, [2u8; 32]);
		let data_a = b"block-a-data".to_vec();
		let data_b = b"block-b-data".to_vec();

		let (da, db) = (data_a.clone(), data_b.clone());
		let handle = tokio::spawn(async move {
			let req = rx.recv().await.unwrap();
			let mut result = HashMap::new();
			result.insert(cid_a, FetchOutcome::Block(da));
			result.insert(cid_b, FetchOutcome::Block(db));
			let _ = req.response_tx.send(Ok(result));
		});

		let response = client.request_blocks(&[cid_a, cid_b]).await.unwrap();
		assert_eq!(response.len(), 2);
		assert!(matches!(response.get(&cid_a), Some(FetchOutcome::Block(d)) if *d == data_a));
		assert!(matches!(response.get(&cid_b), Some(FetchOutcome::Block(d)) if *d == data_b));
		handle.await.unwrap();
	}

	#[tokio::test]
	async fn request_blocks_missing_cids_surfaced_as_missing() {
		let (client, mut rx) = make_client();
		let cid_a = make_cid(BLAKE2B_256_MULTIHASH_CODE, [1u8; 32]);
		let cid_b = make_cid(BLAKE2B_256_MULTIHASH_CODE, [2u8; 32]);
		let data_a = b"only-a-served".to_vec();

		let da = data_a.clone();
		let handle = tokio::spawn(async move {
			let req = rx.recv().await.unwrap();
			let mut result = HashMap::new();
			result.insert(cid_a, FetchOutcome::Block(da));
			result.insert(cid_b, FetchOutcome::Missing);
			let _ = req.response_tx.send(Ok(result));
		});

		let response = client.request_blocks(&[cid_a, cid_b]).await.unwrap();
		assert!(matches!(response.get(&cid_a), Some(FetchOutcome::Block(d)) if *d == data_a));
		assert!(matches!(response.get(&cid_b), Some(FetchOutcome::Missing)));
		handle.await.unwrap();
	}

	#[tokio::test]
	async fn request_blocks_service_error_propagates() {
		let (client, mut rx) = make_client();
		let cid = make_cid(BLAKE2B_256_MULTIHASH_CODE, [1u8; 32]);

		let handle = tokio::spawn(async move {
			let req = rx.recv().await.unwrap();
			let _ = req.response_tx.send(Err(BitswapError::RequestFailed("timed out".into())));
		});

		let err = client.request_blocks(&[cid]).await.expect_err("error must propagate");
		assert!(matches!(err, BitswapError::RequestFailed(msg) if msg == "timed out"));
		handle.await.unwrap();
	}

	#[tokio::test]
	async fn request_blocks_channel_closed_errors() {
		let (client, rx) = make_client();
		let cid = make_cid(BLAKE2B_256_MULTIHASH_CODE, [1u8; 32]);
		drop(rx);

		let err = client.request_blocks(&[cid]).await.expect_err("closed channel must error");
		assert!(matches!(err, BitswapError::RequestFailed(_)));
	}

	#[tokio::test]
	async fn request_blocks_sends_verified_mode() {
		let (client, mut rx) = make_client();
		let cid = make_cid(BLAKE2B_256_MULTIHASH_CODE, [1u8; 32]);

		let handle = tokio::spawn(async move {
			let req = rx.recv().await.unwrap();
			assert_eq!(req.verification, VerificationMode::Verified);
			let _ = req.response_tx.send(Ok(HashMap::new()));
		});

		let _ = client.request_blocks(&[cid]).await;
		handle.await.unwrap();
	}

	#[tokio::test]
	async fn request_blocks_unverified_sends_unverified_mode() {
		let (client, mut rx) = make_client();
		let cid = make_cid(BLAKE2B_256_MULTIHASH_CODE, [1u8; 32]);

		let handle = tokio::spawn(async move {
			let req = rx.recv().await.unwrap();
			assert_eq!(req.verification, VerificationMode::Unverified);
			let _ = req.response_tx.send(Ok(HashMap::new()));
		});

		let _ = client.request_blocks_unverified(&[cid]).await;
		handle.await.unwrap();
	}

	#[tokio::test]
	async fn request_blocks_all_entries_use_want_block_type() {
		let (client, mut rx) = make_client();
		let cid_a = make_cid(BLAKE2B_256_MULTIHASH_CODE, [1u8; 32]);
		let cid_b = make_cid(SHA2_256_MULTIHASH_CODE, [2u8; 32]);
		let cid_c = make_cid(KECCAK_256_MULTIHASH_CODE, [3u8; 32]);

		let handle = tokio::spawn(async move {
			let req = rx.recv().await.unwrap();
			assert_eq!(req.cids.len(), 3);
			for (_, want_type) in &req.cids {
				assert_eq!(*want_type as i32, ProtoWantType::Block as i32);
			}
			let _ = req.response_tx.send(Ok(HashMap::new()));
		});

		let _ = client.request_blocks(&[cid_a, cid_b, cid_c]).await;
		handle.await.unwrap();
	}

	#[test]
	fn validate_cids_rejects_unsupported_multihash() {
		const UNSUPPORTED: u64 = 0x99;
		assert!(!is_supported_multihash_code(UNSUPPORTED));
		let mh = CidMultihash::<64>::wrap(UNSUPPORTED, &[0u8; 32]).unwrap();
		let cid = Cid::new_v1(RAW_CODEC, mh);
		let err = validate_cids(&[cid]).expect_err("unsupported multihash must reject");
		assert!(matches!(err, BitswapError::UnsupportedHashing { multihash_code: 0x99 }));
	}
}
