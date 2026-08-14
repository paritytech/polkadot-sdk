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

use crate::{IfDisconnected, NetworkRequest, ProtocolName};

use cid::{multihash::Multihash as CidMultihash, Cid, Version as CidVersion};
use log::{debug, trace, warn};
use prost::Message;
use sc_network_types::PeerId;
use std::collections::{HashMap, HashSet};

use super::{
	is_cid_supported,
	schema::bitswap::{
		message::{
			wantlist::{Entry, WantType as ProtoWantType},
			BlockPresence, BlockPresenceType, Wantlist,
		},
		Message as BitswapMessage,
	},
	Prefix, LOG_TARGET, MAX_WANTED_BLOCKS, PROTOCOL_NAME,
};

/// Const from <https://github.com/multiformats/multicodec/blame/master/table.csv>
/// Multihash code for BLAKE2b-256.
pub const BLAKE2B_256_MULTIHASH_CODE: u64 = 0xb220;
/// Multihash code for SHA2-256.
pub const SHA2_256_MULTIHASH_CODE: u64 = 0x12;
/// Multihash code for Keccak-256.
pub const KECCAK_256_MULTIHASH_CODE: u64 = 0x1b;

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

/// Multihash type with a 64-byte digest capacity.
type Multihash = CidMultihash<64>;

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

/// Send one `WANT-BLOCK` request for `cids` to `peer` and classify the response.
///
/// Returned blocks are verified by recomputing the CID from the response prefix and bytes.
/// Blocks whose recomputed CID was not requested are ignored.
///
/// Errors if `cids` is empty, larger than [`MAX_WANTED_BLOCKS`], contains an unsupported CID,
/// or contains a duplicate CID.
///
/// Note: This is a temporary API that shall be superseeded by a better abstraction such as
///  <https://github.com/paritytech/polkadot-sdk/issues/12052>
pub async fn request_bitswap_blocks<N>(
	network: &N,
	peer: PeerId,
	cids: &[Cid],
) -> Result<HashMap<Cid, FetchOutcome>, BitswapError>
where
	N: NetworkRequest + ?Sized,
{
	validate_cids(cids)?;

	let wanted: HashSet<Cid> = cids.iter().copied().collect();
	let response = send_request(network, peer, cids).await?;
	Ok(classify_response(response, &wanted, peer))
}

/// Like [`request_bitswap_blocks`], but does not recompute or verify the hash of received bytes.
///
/// Use this when the requester must fetch by CID-shaped identifiers before it can verify the
/// returned bytes through an external authority. The response is matched by request order and
/// CID prefix only; integrity verification is delegated to the caller.
///
/// Note: This is a temporary API that shall be superseeded by a better abstraction such as
///  <https://github.com/paritytech/polkadot-sdk/issues/12052>
pub async fn request_bitswap_blocks_unverified<N>(
	network: &N,
	peer: PeerId,
	cids: &[Cid],
) -> Result<HashMap<Cid, FetchOutcome>, BitswapError>
where
	N: NetworkRequest + ?Sized,
{
	validate_cids(cids)?;

	let response = send_request(network, peer, cids).await?;
	Ok(classify_response_unverified(response, cids, peer))
}

/// Dispatch a bitswap WANT request to `peer` and decode the response.
async fn send_request<N>(
	network: &N,
	peer: PeerId,
	cids: &[Cid],
) -> Result<BitswapMessage, BitswapError>
where
	N: NetworkRequest + ?Sized,
{
	let entries: Vec<Entry> = cids
		.iter()
		.copied()
		.map(|cid| Entry {
			block: cid.to_bytes(),
			want_type: ProtoWantType::Block as i32,
			send_dont_have: true,
			..Default::default()
		})
		.collect();
	let request =
		BitswapMessage { wantlist: Some(Wantlist { entries, full: false }), ..Default::default() };

	trace!(
		target: LOG_TARGET,
		"client: sending Bitswap wantlist for {} CIDs to {peer}, protocol {PROTOCOL_NAME}",
		cids.len(),
	);

	let payload = match network
		.request(
			peer,
			ProtocolName::from(PROTOCOL_NAME),
			request.encode_to_vec(),
			None,
			IfDisconnected::TryConnect,
		)
		.await
	{
		Ok((payload, _)) => payload,
		Err(err) => {
			debug!(target: LOG_TARGET, "client: batch request to {peer} rejected by network: {err:?}");
			return Err(BitswapError::RequestFailed(err.to_string()));
		},
	};

	BitswapMessage::decode(&payload[..]).map_err(|err| {
		debug!(target: LOG_TARGET, "client: failed to decode batch response from {peer}: {err}");
		BitswapError::DecodeError(err.to_string())
	})
}

/// Classify the response by verifying each block's CID against the wanted set.
///
/// Every wanted CID is recorded exactly once: as [`FetchOutcome::Block`] if the peer delivered
/// bytes whose recomputed CID is in `wanted`, otherwise as [`FetchOutcome::Missing`]. Presence
/// frames (`HAVE` / `DONT_HAVE`) are logged for diagnostics but do not change the outcome.
fn classify_response(
	response: BitswapMessage,
	wanted: &HashSet<Cid>,
	peer: PeerId,
) -> HashMap<Cid, FetchOutcome> {
	let mut result: HashMap<Cid, FetchOutcome> = HashMap::with_capacity(wanted.len());

	for block in response.payload {
		let Ok(cid) = cid_from_block_prefix(&block.prefix, &block.data).inspect_err(|err| {
			debug!(target: LOG_TARGET, "client: malformed block prefix from {peer}: {err:?}");
		}) else {
			continue;
		};
		if !wanted.contains(&cid) {
			debug!(target: LOG_TARGET, "client: {peer} returned unsolicited block for CID {cid}");
			continue;
		}
		debug!(target: LOG_TARGET, "client: {peer} returned {} bytes for CID {cid}", block.data.len());
		result.insert(cid, FetchOutcome::Block(block.data));
	}

	log_presences(response.block_presences, wanted, peer);

	for cid in wanted {
		result.entry(*cid).or_insert(FetchOutcome::Missing);
	}

	result
}

/// Classify an unverified response via order-based correlation.
///
/// Every wanted CID is recorded exactly once: as [`FetchOutcome::Block`] if the peer delivered
/// bytes whose declared prefix matches a requested CID at the corresponding position in the
/// wantlist, otherwise as [`FetchOutcome::Missing`].
fn classify_response_unverified(
	response: BitswapMessage,
	cids: &[Cid],
	peer: PeerId,
) -> HashMap<Cid, FetchOutcome> {
	let mut result: HashMap<Cid, FetchOutcome> = HashMap::with_capacity(cids.len());
	let wanted_set: HashSet<Cid> = cids.iter().copied().collect();
	let mut dont_have_cids: HashSet<Cid> = HashSet::with_capacity(cids.len());

	for presence in response.block_presences {
		let Ok(cid) = Cid::read_bytes(presence.cid.as_slice()).inspect_err(|err| {
			debug!(target: LOG_TARGET, "client: malformed presence CID from {peer}: {err}");
		}) else {
			continue;
		};
		if !wanted_set.contains(&cid) {
			debug!(target: LOG_TARGET, "client: {peer} returned unsolicited presence for CID {cid}");
			continue;
		}
		if presence.r#type == BlockPresenceType::DontHave as i32 {
			debug!(target: LOG_TARGET, "client: {peer} DONT_HAVE for CID {cid}");
			dont_have_cids.insert(cid);
		} else if presence.r#type == BlockPresenceType::Have as i32 {
			debug!(target: LOG_TARGET, "client: {peer} HAVE for CID {cid}");
		} else {
			warn!(
				target: LOG_TARGET,
				"client: {peer} unexpected presence type {} for CID {cid}",
				presence.r#type,
			);
		}
	}

	// Unverified payloads cannot be matched by recomputing their CID from bytes, so attribute
	// each block to the next requested CID (skipping any the peer already said it doesn't have)
	// whose CID metadata matches the payload prefix.
	let mut expected_payload_order =
		cids.iter().copied().filter(|cid| !dont_have_cids.contains(cid));

	for block in response.payload {
		let Some(expected_cid) = expected_payload_order.next() else {
			debug!(target: LOG_TARGET, "client: {peer} returned more payload blocks than expected; dropping extras");
			break;
		};
		let Ok(prefix) = decode_prefix(&block.prefix).inspect_err(|err| {
			debug!(target: LOG_TARGET, "client: malformed block prefix from {peer}: {err:?}");
		}) else {
			break;
		};
		if !prefix_matches_cid(&prefix, &expected_cid) {
			debug!(
				target: LOG_TARGET,
				"client: {peer} returned block with prefix {:?} but expected CID {expected_cid}; \
				 stopping payload attribution",
				prefix,
			);
			break;
		}
		debug!(
			target: LOG_TARGET,
			"client: {peer} returned {} unverified bytes for CID {expected_cid}",
			block.data.len(),
		);
		result.insert(expected_cid, FetchOutcome::Block(block.data.clone()));
	}

	for cid in cids {
		result.entry(*cid).or_insert(FetchOutcome::Missing);
	}

	result
}

/// Log per-CID presence frames for diagnostics. Presence does not influence the public outcome.
fn log_presences(presences: Vec<BlockPresence>, wanted: &HashSet<Cid>, peer: PeerId) {
	for presence in presences {
		let Ok(cid) = Cid::read_bytes(presence.cid.as_slice()).inspect_err(|err| {
			debug!(target: LOG_TARGET, "client: malformed presence CID from {peer}: {err}");
		}) else {
			continue;
		};
		if !wanted.contains(&cid) {
			debug!(target: LOG_TARGET, "client: {peer} returned unsolicited presence for CID {cid}");
			continue;
		}
		if presence.r#type == BlockPresenceType::DontHave as i32 {
			debug!(target: LOG_TARGET, "client: {peer} DONT_HAVE for CID {cid}");
		} else if presence.r#type == BlockPresenceType::Have as i32 {
			debug!(target: LOG_TARGET, "client: {peer} HAVE for CID {cid}");
		} else {
			debug!(
				target: LOG_TARGET,
				"client: {peer} unexpected presence type {} for CID {cid}",
				presence.r#type,
			);
		}
	}
}

/// Check that a decoded prefix matches a CID's version, codec, and multihash metadata.
fn prefix_matches_cid(prefix: &Prefix, cid: &Cid) -> bool {
	prefix.version == cid.version() &&
		prefix.codec == cid.codec() &&
		prefix.mh_type == cid.hash().code() &&
		prefix.mh_len == cid.hash().size()
}

/// Reconstruct a CID from a block's prefix bytes and payload data.
fn cid_from_block_prefix(prefix: &[u8], data: &[u8]) -> Result<Cid, BitswapError> {
	let prefix = decode_prefix(prefix)?;
	if prefix.version != CidVersion::V1 {
		return Err(BitswapError::UnsupportedCidVersion { version: prefix.version.into() });
	}

	let hash = hash_for_multihash_code(prefix.mh_type, data)
		.ok_or(BitswapError::UnsupportedHashing { multihash_code: prefix.mh_type })?;
	let multihash = Multihash::wrap(prefix.mh_type, &hash)
		.map_err(|err| BitswapError::DecodeError(err.to_string()))?;
	Ok(Cid::new_v1(prefix.codec, multihash))
}

/// Compute a 32-byte hash for the given multihash code.
fn hash_for_multihash_code(multihash_code: u64, data: &[u8]) -> Option<[u8; 32]> {
	match multihash_code {
		BLAKE2B_256_MULTIHASH_CODE => Some(sp_crypto_hashing::blake2_256(data)),
		SHA2_256_MULTIHASH_CODE => Some(sp_crypto_hashing::sha2_256(data)),
		KECCAK_256_MULTIHASH_CODE => Some(sp_crypto_hashing::keccak_256(data)),
		_ => None,
	}
}

/// Decode varint-encoded CID prefix bytes.
fn decode_prefix(mut bytes: &[u8]) -> Result<Prefix, BitswapError> {
	let mut read_varint = || -> Result<u64, BitswapError> {
		let (v, rest) = unsigned_varint::decode::u64(bytes)
			.map_err(|err| BitswapError::DecodeError(err.to_string()))?;
		bytes = rest;
		Ok(v)
	};

	let version = read_varint()?;
	let codec = read_varint()?;
	let mh_type = read_varint()?;
	let mh_len = read_varint()?;

	if !bytes.is_empty() {
		return Err(BitswapError::DecodeError("bitswap block prefix had trailing bytes".into()));
	}

	let version = CidVersion::try_from(version)
		.map_err(|_| BitswapError::UnsupportedCidVersion { version })?;
	let mh_len = u8::try_from(mh_len).map_err(|_| {
		BitswapError::DecodeError(format!("multihash length {mh_len} does not fit into u8"))
	})?;

	Ok(Prefix { version, codec, mh_type, mh_len })
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
	/// CID version is unsupported for this bitswap client.
	UnsupportedCidVersion {
		/// The unsupported CID version number.
		version: u64,
	},
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{OutboundFailure, RequestFailure};
	use futures::channel::oneshot;
	use sc_network_types::PeerId;
	use std::{collections::VecDeque, sync::Mutex};

	use super::super::{
		is_supported_multihash_code,
		schema::bitswap::message::{Block as MessageBlock, BlockPresence, BlockPresenceType},
		RAW_CODEC,
	};

	/// Build a raw-codec CID from a 32-byte digest and supported multihash code.
	fn raw_cid_from_digest(multihash_code: u64, digest: [u8; 32]) -> Result<Cid, BitswapError> {
		if !is_supported_multihash_code(multihash_code) {
			return Err(BitswapError::UnsupportedHashing { multihash_code });
		}
		let multihash = CidMultihash::wrap(multihash_code, &digest)
			.map_err(|e| BitswapError::DecodeError(e.to_string()))?;
		Ok(Cid::new_v1(RAW_CODEC, multihash))
	}

	struct StubSender {
		responses: Mutex<VecDeque<Result<Vec<u8>, RequestFailure>>>,
		requests: Mutex<Vec<Vec<u8>>>,
	}

	impl StubSender {
		fn new(responses: impl IntoIterator<Item = Result<Vec<u8>, RequestFailure>>) -> Self {
			Self {
				responses: Mutex::new(responses.into_iter().collect()),
				requests: Mutex::new(Vec::new()),
			}
		}

		fn pop_request(&self) -> BitswapMessage {
			let bytes = self.requests.lock().unwrap().pop().expect("request should be recorded");
			BitswapMessage::decode(bytes.as_slice()).expect("request should decode")
		}
	}

	#[async_trait::async_trait]
	impl NetworkRequest for StubSender {
		async fn request(
			&self,
			_target: PeerId,
			_protocol: ProtocolName,
			request: Vec<u8>,
			_fallback_request: Option<(Vec<u8>, ProtocolName)>,
			_connect: IfDisconnected,
		) -> Result<(Vec<u8>, ProtocolName), RequestFailure> {
			self.requests.lock().unwrap().push(request);
			self.responses
				.lock()
				.unwrap()
				.pop_front()
				.expect("StubSender: no canned response queued")
				.map(|bytes| (bytes, ProtocolName::from(PROTOCOL_NAME)))
		}

		fn start_request(
			&self,
			_peer: PeerId,
			_protocol: ProtocolName,
			payload: Vec<u8>,
			_fallback_request: Option<(Vec<u8>, ProtocolName)>,
			tx: oneshot::Sender<Result<(Vec<u8>, ProtocolName), RequestFailure>>,
			_connect: IfDisconnected,
		) {
			self.requests.lock().unwrap().push(payload);
			let resp = self
				.responses
				.lock()
				.unwrap()
				.pop_front()
				.expect("StubSender: no canned response queued");
			let _ = tx.send(resp.map(|bytes| (bytes, ProtocolName::from(PROTOCOL_NAME))));
		}
	}

	fn prefix_for(multihash_code: u64) -> Vec<u8> {
		Prefix { version: CidVersion::V1, codec: RAW_CODEC, mh_type: multihash_code, mh_len: 32 }
			.to_bytes()
	}

	fn cid_for_data(multihash_code: u64, data: &[u8]) -> Cid {
		raw_cid_from_digest(multihash_code, hash_for_multihash_code(multihash_code, data).unwrap())
			.unwrap()
	}

	fn cid_for_digest(multihash_code: u64, digest: [u8; 32]) -> Cid {
		raw_cid_from_digest(multihash_code, digest).unwrap()
	}

	fn encode_response(blocks: &[(u64, Vec<u8>)], presences: &[(Cid, i32)]) -> Vec<u8> {
		let payload = blocks
			.iter()
			.map(|(multihash_code, data)| MessageBlock {
				prefix: prefix_for(*multihash_code),
				data: data.clone(),
			})
			.collect();
		let block_presences = presences
			.iter()
			.map(|(cid, ptype)| BlockPresence { cid: cid.to_bytes(), r#type: *ptype })
			.collect();
		BitswapMessage { payload, block_presences, ..Default::default() }.encode_to_vec()
	}

	#[tokio::test]
	async fn request_bitswap_blocks_returns_blocks_for_all_wanted() {
		let data_a = b"hash-a-payload".to_vec();
		let data_b = b"hash-b-payload".to_vec();
		let data_c = b"hash-c-payload".to_vec();
		let cid_a = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data_a);
		let cid_b = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data_b);
		let cid_c = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data_c);

		let response = encode_response(
			&[
				(BLAKE2B_256_MULTIHASH_CODE, data_a.clone()),
				(BLAKE2B_256_MULTIHASH_CODE, data_b.clone()),
				(BLAKE2B_256_MULTIHASH_CODE, data_c.clone()),
			],
			&[],
		);
		let stub = StubSender::new([Ok(response)]);

		let result = request_bitswap_blocks(&stub, PeerId::random(), &[cid_a, cid_b, cid_c])
			.await
			.expect("request_bitswap_blocks should succeed");

		assert_eq!(result.len(), 3);
		assert!(matches!(result.get(&cid_a), Some(FetchOutcome::Block(d)) if *d == data_a));
		assert!(matches!(result.get(&cid_b), Some(FetchOutcome::Block(d)) if *d == data_b));
		assert!(matches!(result.get(&cid_c), Some(FetchOutcome::Block(d)) if *d == data_c));
	}

	#[tokio::test]
	async fn request_bitswap_blocks_dont_have_is_surfaced_as_missing() {
		let data_a = b"a".to_vec();
		let data_b = b"b".to_vec();
		let cid_a = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data_a);
		let cid_b = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data_b);
		let cid_c = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, b"c-not-served");

		let response = encode_response(
			&[
				(BLAKE2B_256_MULTIHASH_CODE, data_a.clone()),
				(BLAKE2B_256_MULTIHASH_CODE, data_b.clone()),
			],
			&[(cid_c, BlockPresenceType::DontHave as i32)],
		);
		let stub = StubSender::new([Ok(response)]);

		let result = request_bitswap_blocks(&stub, PeerId::random(), &[cid_a, cid_b, cid_c])
			.await
			.unwrap();

		assert_eq!(result.len(), 3);
		assert!(matches!(result.get(&cid_a), Some(FetchOutcome::Block(_))));
		assert!(matches!(result.get(&cid_b), Some(FetchOutcome::Block(_))));
		assert!(matches!(result.get(&cid_c), Some(FetchOutcome::Missing)));
	}

	#[tokio::test]
	async fn request_bitswap_blocks_corrupted_data_dropped_as_unsolicited() {
		let real_data = b"real-payload".to_vec();
		let wanted_cid = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &real_data);
		let corrupted_data = b"i-am-not-the-real-payload".to_vec();
		let response = encode_response(&[(BLAKE2B_256_MULTIHASH_CODE, corrupted_data)], &[]);
		let stub = StubSender::new([Ok(response)]);

		let result = request_bitswap_blocks(&stub, PeerId::random(), &[wanted_cid]).await.unwrap();

		assert_eq!(result.len(), 1);
		assert!(matches!(result.get(&wanted_cid), Some(FetchOutcome::Missing)));
	}

	#[tokio::test]
	async fn request_bitswap_blocks_encodes_only_want_block_entries() {
		let cid_a = cid_for_digest(BLAKE2B_256_MULTIHASH_CODE, [1u8; 32]);
		let cid_b = cid_for_digest(BLAKE2B_256_MULTIHASH_CODE, [2u8; 32]);
		let stub = StubSender::new([Ok(BitswapMessage::default().encode_to_vec())]);

		let result = request_bitswap_blocks(&stub, PeerId::random(), &[cid_a, cid_b])
			.await
			.expect("block-only request must encode");

		assert!(matches!(result.get(&cid_a), Some(FetchOutcome::Missing)));
		assert!(matches!(result.get(&cid_b), Some(FetchOutcome::Missing)));

		let request = stub.pop_request();
		let entries = request.wantlist.expect("wantlist should be present").entries;
		assert_eq!(entries.len(), 2);
		assert_eq!(entries[0].want_type, ProtoWantType::Block as i32);
		assert_eq!(entries[1].want_type, ProtoWantType::Block as i32);
	}

	#[tokio::test]
	async fn request_bitswap_blocks_have_presence_alone_is_missing() {
		let cid = cid_for_digest(BLAKE2B_256_MULTIHASH_CODE, [3u8; 32]);
		let response = encode_response(&[], &[(cid, BlockPresenceType::Have as i32)]);
		let stub = StubSender::new([Ok(response)]);

		let result = request_bitswap_blocks(&stub, PeerId::random(), &[cid])
			.await
			.expect("HAVE-only response should classify successfully");

		assert_eq!(result.len(), 1);
		assert!(matches!(result.get(&cid), Some(FetchOutcome::Missing)));
	}

	#[tokio::test]
	async fn request_bitswap_blocks_unverified_accepts_bytes_without_hash_recompute() {
		let data = b"sha2-digest-but-blake2b-request-prefix".to_vec();
		let cid = cid_for_digest(BLAKE2B_256_MULTIHASH_CODE, sp_crypto_hashing::sha2_256(&data));
		let response = encode_response(&[(BLAKE2B_256_MULTIHASH_CODE, data.clone())], &[]);
		let stub = StubSender::new([Ok(response)]);

		let result = request_bitswap_blocks_unverified(&stub, PeerId::random(), &[cid])
			.await
			.expect("unverified fetch should not recompute hashes");

		assert_eq!(result.len(), 1);
		assert!(matches!(result.get(&cid), Some(FetchOutcome::Block(d)) if *d == data));
	}

	#[tokio::test]
	async fn request_bitswap_blocks_unverified_dont_have_returned_as_missing() {
		let cid = cid_for_digest(
			BLAKE2B_256_MULTIHASH_CODE,
			sp_crypto_hashing::sha2_256(b"pruned-unverified-payload"),
		);
		let response = encode_response(&[], &[(cid, BlockPresenceType::DontHave as i32)]);
		let stub = StubSender::new([Ok(response)]);

		let result = request_bitswap_blocks_unverified(&stub, PeerId::random(), &[cid])
			.await
			.expect("unverified DONT_HAVE should classify successfully");

		assert_eq!(result.len(), 1);
		assert!(matches!(result.get(&cid), Some(FetchOutcome::Missing)));
	}

	#[tokio::test]
	async fn request_bitswap_blocks_unverified_empty_wants_errors() {
		let stub = StubSender::new(std::iter::empty());

		let err = request_bitswap_blocks_unverified(&stub, PeerId::random(), &[])
			.await
			.expect_err("empty wantlist must error");
		assert!(matches!(err, BitswapError::DecodeError(msg) if msg == "empty wantlist"));
	}

	#[tokio::test]
	async fn request_bitswap_blocks_duplicate_cids_error() {
		let cid = cid_for_digest(BLAKE2B_256_MULTIHASH_CODE, [9u8; 32]);
		let stub = StubSender::new(std::iter::empty());

		let err = request_bitswap_blocks(&stub, PeerId::random(), &[cid, cid])
			.await
			.expect_err("two wants for the same CID are ambiguous");
		assert!(matches!(err, BitswapError::DecodeError(msg) if msg.starts_with("duplicate CID")));
	}

	#[tokio::test]
	async fn request_bitswap_blocks_unverified_multi_want_all_served_in_request_order() {
		let data_a = b"first-unverified-payload".to_vec();
		let data_b = b"second-unverified-payload".to_vec();
		let data_c = b"third-unverified-payload".to_vec();
		let cid_a =
			cid_for_digest(BLAKE2B_256_MULTIHASH_CODE, sp_crypto_hashing::sha2_256(&data_a));
		let cid_b =
			cid_for_digest(BLAKE2B_256_MULTIHASH_CODE, sp_crypto_hashing::keccak_256(&data_b));
		let cid_c = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data_c);

		let response = encode_response(
			&[
				(BLAKE2B_256_MULTIHASH_CODE, data_a.clone()),
				(BLAKE2B_256_MULTIHASH_CODE, data_b.clone()),
				(BLAKE2B_256_MULTIHASH_CODE, data_c.clone()),
			],
			&[],
		);
		let stub = StubSender::new([Ok(response)]);

		let result =
			request_bitswap_blocks_unverified(&stub, PeerId::random(), &[cid_a, cid_b, cid_c])
				.await
				.expect("multi-want unverified must succeed via positional correlation");

		assert_eq!(result.len(), 3);
		assert!(matches!(result.get(&cid_a), Some(FetchOutcome::Block(d)) if *d == data_a));
		assert!(matches!(result.get(&cid_b), Some(FetchOutcome::Block(d)) if *d == data_b));
		assert!(matches!(result.get(&cid_c), Some(FetchOutcome::Block(d)) if *d == data_c));
	}

	#[tokio::test]
	async fn request_bitswap_blocks_unverified_dont_have_skips_position_in_payload_order() {
		let data = b"second-payload-after-dont-have".to_vec();
		let dont_have_cid = cid_for_digest(BLAKE2B_256_MULTIHASH_CODE, [4u8; 32]);
		let block_cid =
			cid_for_digest(BLAKE2B_256_MULTIHASH_CODE, sp_crypto_hashing::sha2_256(&data));
		let response = encode_response(
			&[(BLAKE2B_256_MULTIHASH_CODE, data.clone())],
			&[(dont_have_cid, BlockPresenceType::DontHave as i32)],
		);
		let stub = StubSender::new([Ok(response)]);

		let result =
			request_bitswap_blocks_unverified(&stub, PeerId::random(), &[dont_have_cid, block_cid])
				.await
				.expect("unverified mixed presence/payload should classify successfully");

		assert_eq!(result.len(), 2);
		assert!(matches!(result.get(&dont_have_cid), Some(FetchOutcome::Missing)));
		assert!(matches!(result.get(&block_cid), Some(FetchOutcome::Block(d)) if *d == data));
	}

	#[tokio::test]
	async fn request_bitswap_blocks_dispatches_per_entry_multihash() {
		let data_b2 = b"blake2b-payload".to_vec();
		let data_sha = b"sha2-256-payload".to_vec();
		let data_kec = b"keccak-256-payload".to_vec();
		let cid_b2 = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data_b2);
		let cid_sha = cid_for_data(SHA2_256_MULTIHASH_CODE, &data_sha);
		let cid_kec = cid_for_data(KECCAK_256_MULTIHASH_CODE, &data_kec);

		let response = encode_response(
			&[
				(BLAKE2B_256_MULTIHASH_CODE, data_b2.clone()),
				(SHA2_256_MULTIHASH_CODE, data_sha.clone()),
				(KECCAK_256_MULTIHASH_CODE, data_kec.clone()),
			],
			&[],
		);
		let stub = StubSender::new([Ok(response)]);

		let result = request_bitswap_blocks(&stub, PeerId::random(), &[cid_b2, cid_sha, cid_kec])
			.await
			.unwrap();

		assert_eq!(result.len(), 3);
		assert!(matches!(result.get(&cid_b2), Some(FetchOutcome::Block(d)) if *d == data_b2));
		assert!(matches!(result.get(&cid_sha), Some(FetchOutcome::Block(d)) if *d == data_sha));
		assert!(matches!(result.get(&cid_kec), Some(FetchOutcome::Block(d)) if *d == data_kec));
	}

	#[tokio::test]
	async fn request_bitswap_blocks_over_cap_errors() {
		let wants: Vec<_> = (0..(MAX_WANTED_BLOCKS + 1) as u8)
			.map(|i| {
				let mut h = [0u8; 32];
				h[0] = i;
				cid_for_digest(BLAKE2B_256_MULTIHASH_CODE, h)
			})
			.collect();
		let stub = StubSender::new(std::iter::empty());

		let err = request_bitswap_blocks(&stub, PeerId::random(), &wants)
			.await
			.expect_err("over-cap wantlist must error");
		assert!(matches!(err, BitswapError::DecodeError(_)));
	}

	#[tokio::test]
	async fn request_bitswap_blocks_at_exactly_max_wanted_blocks_succeeds() {
		let mut wants = Vec::with_capacity(MAX_WANTED_BLOCKS);
		let mut blocks = Vec::with_capacity(MAX_WANTED_BLOCKS);
		for i in 0..MAX_WANTED_BLOCKS {
			let data = format!("payload-{i}").into_bytes();
			wants.push(cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data));
			blocks.push((BLAKE2B_256_MULTIHASH_CODE, data));
		}

		let response = encode_response(&blocks, &[]);
		let stub = StubSender::new([Ok(response)]);

		let result = request_bitswap_blocks(&stub, PeerId::random(), &wants)
			.await
			.expect("exactly MAX_WANTED_BLOCKS must succeed");

		assert_eq!(result.len(), MAX_WANTED_BLOCKS);
		for cid in &wants {
			assert!(matches!(result.get(cid), Some(FetchOutcome::Block(_))));
		}
	}

	#[tokio::test]
	async fn request_bitswap_blocks_block_beats_presence_for_same_cid() {
		let data = b"both-block-and-presence".to_vec();
		let cid = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data);

		let response = encode_response(
			&[(BLAKE2B_256_MULTIHASH_CODE, data.clone())],
			&[(cid, BlockPresenceType::DontHave as i32)],
		);
		let stub = StubSender::new([Ok(response)]);

		let result = request_bitswap_blocks(&stub, PeerId::random(), &[cid]).await.unwrap();

		assert_eq!(result.len(), 1);
		assert!(matches!(result.get(&cid), Some(FetchOutcome::Block(d)) if *d == data));
	}

	#[tokio::test]
	async fn request_bitswap_blocks_response_decode_failure() {
		let stub = StubSender::new([Ok(vec![0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff])]);
		let cid = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, b"any");

		let err = request_bitswap_blocks(&stub, PeerId::random(), &[cid])
			.await
			.expect_err("malformed response bytes must surface as DecodeError");
		assert!(matches!(err, BitswapError::DecodeError(_)));
	}

	#[tokio::test]
	async fn request_bitswap_blocks_request_failure_propagates() {
		struct FailingSender;
		#[async_trait::async_trait]
		impl NetworkRequest for FailingSender {
			async fn request(
				&self,
				_target: PeerId,
				_protocol: ProtocolName,
				_request: Vec<u8>,
				_fallback_request: Option<(Vec<u8>, ProtocolName)>,
				_connect: IfDisconnected,
			) -> Result<(Vec<u8>, ProtocolName), RequestFailure> {
				Err(RequestFailure::Network(OutboundFailure::ConnectionClosed))
			}

			fn start_request(
				&self,
				_peer: PeerId,
				_protocol: ProtocolName,
				_payload: Vec<u8>,
				_fallback_request: Option<(Vec<u8>, ProtocolName)>,
				tx: oneshot::Sender<Result<(Vec<u8>, ProtocolName), RequestFailure>>,
				_connect: IfDisconnected,
			) {
				drop(tx);
			}
		}

		let cid = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, b"any");
		let err = request_bitswap_blocks(&FailingSender, PeerId::random(), &[cid])
			.await
			.expect_err("request failure must surface as RequestFailed");
		assert!(matches!(err, BitswapError::RequestFailed(_)));
	}

	#[tokio::test]
	async fn request_bitswap_blocks_unsupported_multihash_in_block_dropped() {
		let wanted_data = b"wanted".to_vec();
		let wanted_cid = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &wanted_data);
		const UNSUPPORTED_MH_CODE: u64 = 0x99;
		let bad_prefix = Prefix {
			version: CidVersion::V1,
			codec: RAW_CODEC,
			mh_type: UNSUPPORTED_MH_CODE,
			mh_len: 32,
		}
		.to_bytes();

		let mut payload_msg = BitswapMessage::default();
		payload_msg.payload =
			vec![MessageBlock { prefix: bad_prefix, data: b"some-bytes".to_vec() }];
		let response = payload_msg.encode_to_vec();
		let stub = StubSender::new([Ok(response)]);

		let result = request_bitswap_blocks(&stub, PeerId::random(), &[wanted_cid]).await.unwrap();

		assert_eq!(result.len(), 1);
		assert!(matches!(result.get(&wanted_cid), Some(FetchOutcome::Missing)));
	}

	#[test]
	fn cid_from_block_prefix_rejects_cid_v0_as_unsupported() {
		let prefix = Prefix {
			version: CidVersion::V0,
			codec: RAW_CODEC,
			mh_type: BLAKE2B_256_MULTIHASH_CODE,
			mh_len: 32,
		}
		.to_bytes();

		let err =
			cid_from_block_prefix(&prefix, b"payload").expect_err("CIDv0 must be unsupported");
		assert!(matches!(err, BitswapError::UnsupportedCidVersion { version: 0 }));
	}
}
